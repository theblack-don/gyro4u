//! Steam Controller (2026, Valve "Triton"/Ibex) HID support.
//!
//! Protocol layout follows SDL3 `release-3.4.20`
//! (`SDL_hidapi_steam_triton.c` and `steam/controller_structs.h`) and the
//! Linux `hid-steam.c` reference. No GPL implementation code is copied here.

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Error, Result, bail};
use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{
    common::BatteryInfo,
    inputs::{
        BTN_CIRCLE, BTN_CREATE, BTN_CROSS, BTN_L1, BTN_L2, BTN_L3, BTN_OPTIONS, BTN_R1, BTN_R2,
        BTN_R3, BTN_SQUARE, BTN_TRIANGLE, ControllerState, DPAD_E, DPAD_N, DPAD_NE, DPAD_NEUTRAL,
        DPAD_NW, DPAD_S, DPAD_SE, DPAD_SW, DPAD_W, EXTRA_BTN_L4, EXTRA_BTN_L5, EXTRA_BTN_LEFT_GRIP,
        EXTRA_BTN_LEFT_PAD, EXTRA_BTN_LEFT_PAD_TOUCH, EXTRA_BTN_LEFT_STICK_TOUCH,
        EXTRA_BTN_LEFT_TRIGGER_CLICK, EXTRA_BTN_QAM, EXTRA_BTN_R4, EXTRA_BTN_R5,
        EXTRA_BTN_RIGHT_GRIP, EXTRA_BTN_RIGHT_PAD, EXTRA_BTN_RIGHT_PAD_TOUCH,
        EXTRA_BTN_RIGHT_STICK_TOUCH, EXTRA_BTN_RIGHT_TRIGGER_CLICK, EXTRA_BTN_STEAM, SteamPadState,
    },
};

pub const STEAM_VID: u16 = 0x28de;
pub const STEAM_PID_WIRED: u16 = 0x1302;
pub const STEAM_PID_BLE: u16 = 0x1303;
pub const STEAM_PID_PUCK: u16 = 0x1304;
pub const STEAM_PID_RECEIVER: u16 = 0x1305;

pub const REPORT_INPUT: u8 = 0x42;
pub const REPORT_BATTERY: u8 = 0x43;
pub const REPORT_INPUT_BLE: u8 = 0x45;
pub const REPORT_WIRELESS_EVENT_X: u8 = 0x46;
pub const REPORT_INPUT_TIMESTAMP: u8 = 0x47;
pub const REPORT_WIRELESS_EVENT: u8 = 0x79;
pub const REPORT_PAD_HAPTIC: u8 = 0x82;

pub const HAPTIC_GAIN_MIN_DB: i8 = -23;
pub const HAPTIC_GAIN_MAX_DB: i8 = 24;

const FEATURE_REPORT_ID: u8 = 1;
const FEATURE_REPORT_SIZE: usize = 64;

const CMD_CLEAR_DIGITAL_MAPPINGS: u8 = 0x81;
const CMD_SET_DEFAULT_DIGITAL_MAPPINGS: u8 = 0x85;
const CMD_SET_SETTINGS_VALUES: u8 = 0x87;

const SETTING_LIZARD_MODE: u8 = 9;
const SETTING_IMU_MODE: u8 = 48;
const IMU_MODE_RAW_ACCEL: u16 = 0x0008;
const IMU_MODE_RAW_GYRO: u16 = 0x0010;

// 2 s matches the hardware-tested reference; re-verify on all transports.
const LIZARD_KEEPALIVE: Duration = Duration::from_secs(2);
const WIRELESS_DISCONNECT: u8 = 1;
const WIRELESS_CONNECT: u8 = 2;
const WIRELESS_PAIR: u8 = 3;

static LAST_OPEN_ERROR_LOG: AtomicU64 = AtomicU64::new(0);
static LAST_PROBE_ERROR_LOG: AtomicU64 = AtomicU64::new(0);
static LAST_INACTIVE_SLOT_LOG: AtomicU64 = AtomicU64::new(0);

pub const TRITON_GYRO_COUNTS_PER_DPS: f32 = 32768.0 / 2000.0;
pub const TRITON_ACCEL_COUNTS_PER_G: f32 = 32768.0 / 2.0;
const TIMESTAMP_UNIT_US_INPUT: f32 = 1.0;
const TIMESTAMP_UNIT_US_TIMESTAMPED: f32 = 32.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SteamTransport {
    Wired,
    Ble,
    Puck { slot: u8 },
    Receiver { slot: u8 },
}

impl SteamTransport {
    pub fn label(self) -> String {
        match self {
            Self::Wired => "USB".to_string(),
            Self::Ble => "Bluetooth LE".to_string(),
            Self::Puck { slot } => format!("Wireless puck slot {slot}"),
            Self::Receiver { slot } => format!("Integrated receiver slot {slot}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SteamWirelessEvent {
    Connect,
    Disconnect,
    Pair,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SteamPadSide {
    Left = 0,
    Right = 1,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SteamPadHaptic {
    Tick = 1,
    Click = 2,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedReport {
    State(Box<ControllerState>),
    Battery(BatteryInfo),
    Wireless(SteamWirelessEvent),
    Ignored,
    Malformed,
}

/// Liveness evidence from a single raw report. `Unknown` must never be treated
/// as absent; an idle slot, permission error, or busy handle proves nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportLiveness {
    Live,
    NotLive,
    Unknown,
}

pub fn report_liveness(data: &[u8]) -> ReportLiveness {
    match data.first() {
        Some(&REPORT_INPUT) | Some(&REPORT_INPUT_BLE) | Some(&REPORT_INPUT_TIMESTAMP) => {
            ReportLiveness::Live
        }
        Some(&REPORT_BATTERY) if data.len() >= 3 => ReportLiveness::Live,
        Some(&REPORT_WIRELESS_EVENT) => match data.get(1) {
            Some(&WIRELESS_CONNECT) => ReportLiveness::Live,
            Some(&WIRELESS_DISCONNECT) => ReportLiveness::NotLive,
            _ => ReportLiveness::Unknown,
        },
        _ => ReportLiveness::Unknown,
    }
}

pub fn is_steam_vid(vid: u16) -> bool {
    vid == STEAM_VID
}

pub fn is_steam_pid(pid: u16) -> bool {
    matches!(
        pid,
        STEAM_PID_WIRED | STEAM_PID_BLE | STEAM_PID_PUCK | STEAM_PID_RECEIVER
    )
}

/// The 2026 wired controller exposes one unified interface, BLE exposes one HID
/// interface, and pucks expose controller slots 2-5 plus non-controller pogo.
pub fn is_controller_interface(pid: u16, interface_number: i32) -> bool {
    match pid {
        STEAM_PID_WIRED | STEAM_PID_BLE => true,
        STEAM_PID_PUCK | STEAM_PID_RECEIVER => (2..=5).contains(&interface_number),
        _ => false,
    }
}

pub fn transport_for(pid: u16, interface_number: i32) -> SteamTransport {
    match pid {
        STEAM_PID_BLE => SteamTransport::Ble,
        STEAM_PID_PUCK => SteamTransport::Puck {
            slot: interface_number.clamp(2, 5) as u8,
        },
        STEAM_PID_RECEIVER => SteamTransport::Receiver {
            slot: interface_number.clamp(2, 5) as u8,
        },
        _ => SteamTransport::Wired,
    }
}

pub fn lizard_mode_feature_report(enabled: bool) -> [u8; FEATURE_REPORT_SIZE] {
    let value = u16::from(enabled);
    feature_settings_report(&[(SETTING_LIZARD_MODE, value)])
}

pub fn imu_mode_feature_report(enabled: bool) -> [u8; FEATURE_REPORT_SIZE] {
    let value = if enabled {
        IMU_MODE_RAW_ACCEL | IMU_MODE_RAW_GYRO
    } else {
        0
    };
    feature_settings_report(&[(SETTING_IMU_MODE, value)])
}

/// Builds the 4-byte pad-local haptic output report (0x82). Side/command
/// mapping and latch behavior still require hardware verification.
pub fn haptic_command_report(side: SteamPadSide, haptic: SteamPadHaptic, gain_db: i8) -> [u8; 4] {
    [
        REPORT_PAD_HAPTIC,
        side as u8,
        haptic as u8,
        gain_db.clamp(HAPTIC_GAIN_MIN_DB, HAPTIC_GAIN_MAX_DB) as u8,
    ]
}

pub fn clear_digital_mappings_report() -> [u8; FEATURE_REPORT_SIZE] {
    let mut buf = [0u8; FEATURE_REPORT_SIZE];
    buf[0] = FEATURE_REPORT_ID;
    buf[1] = CMD_CLEAR_DIGITAL_MAPPINGS;
    buf
}

pub fn restore_digital_mappings_report() -> [u8; FEATURE_REPORT_SIZE] {
    let mut buf = [0u8; FEATURE_REPORT_SIZE];
    buf[0] = FEATURE_REPORT_ID;
    buf[1] = CMD_SET_DEFAULT_DIGITAL_MAPPINGS;
    buf
}

fn feature_settings_report(settings: &[(u8, u16)]) -> [u8; FEATURE_REPORT_SIZE] {
    let mut buf = [0u8; FEATURE_REPORT_SIZE];
    buf[0] = FEATURE_REPORT_ID;
    buf[1] = CMD_SET_SETTINGS_VALUES;
    buf[2] = (settings.len() * 3) as u8;
    let mut offset = 3;
    for (setting, value) in settings {
        buf[offset] = *setting;
        let [lo, hi] = value.to_le_bytes();
        buf[offset + 1] = lo;
        buf[offset + 2] = hi;
        offset += 3;
    }
    buf
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
        *data.get(offset + 2)?,
        *data.get(offset + 3)?,
    ]))
}

fn bit(buttons: u32, index: u32) -> bool {
    buttons & (1 << index) != 0
}

fn neg_i16(value: i16) -> i16 {
    (-(value as i32)).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn stick_axis(value: i16, invert: bool) -> u8 {
    let value = if invert {
        -(value as i32)
    } else {
        value as i32
    };
    ((value + 32768) >> 8).clamp(0, 255) as u8
}

fn trigger_axis(value: i16) -> u8 {
    let clamped = (value as i32).clamp(0, 32767) as f32;
    (clamped / 32767.0 * 255.0).round() as u8
}

fn dpad_from_bits(bits: u8) -> u8 {
    match bits {
        1 => DPAD_N,
        3 => DPAD_NE,
        2 => DPAD_E,
        6 => DPAD_SE,
        4 => DPAD_S,
        12 => DPAD_SW,
        8 => DPAD_W,
        9 => DPAD_NW,
        _ => DPAD_NEUTRAL,
    }
}

fn standard_buttons(buttons: u32) -> u32 {
    // SDL3's TRITON enum names are misleading; follow SDL emitted semantics and
    // hid-steam: bit 6 is Menu (BTN_START), bit 14 is View (BTN_SELECT).
    let mut out = 0;
    if bit(buttons, 0) {
        out |= BTN_CROSS; // A
    }
    if bit(buttons, 1) {
        out |= BTN_CIRCLE; // B
    }
    if bit(buttons, 2) {
        out |= BTN_SQUARE; // X
    }
    if bit(buttons, 3) {
        out |= BTN_TRIANGLE; // Y
    }
    if bit(buttons, 5) {
        out |= BTN_R3;
    }
    if bit(buttons, 6) {
        out |= BTN_OPTIONS; // Menu
    }
    if bit(buttons, 9) {
        out |= BTN_R1;
    }
    if bit(buttons, 14) {
        out |= BTN_CREATE; // View
    }
    if bit(buttons, 15) {
        out |= BTN_L3;
    }
    if bit(buttons, 19) {
        out |= BTN_L1;
    }
    if bit(buttons, 23) {
        out |= BTN_R2; // right trigger click
    }
    if bit(buttons, 27) {
        out |= BTN_L2; // left trigger click
    }
    out
}

fn extra_buttons(buttons: u32) -> u32 {
    let mut out = 0;
    if bit(buttons, 4) {
        out |= EXTRA_BTN_QAM;
    }
    if bit(buttons, 7) {
        out |= EXTRA_BTN_R4;
    }
    if bit(buttons, 8) {
        out |= EXTRA_BTN_R5;
    }
    if bit(buttons, 16) {
        out |= EXTRA_BTN_STEAM;
    }
    if bit(buttons, 17) {
        out |= EXTRA_BTN_L4;
    }
    if bit(buttons, 18) {
        out |= EXTRA_BTN_L5;
    }
    if bit(buttons, 20) {
        out |= EXTRA_BTN_RIGHT_STICK_TOUCH;
    }
    if bit(buttons, 21) {
        out |= EXTRA_BTN_RIGHT_PAD_TOUCH;
    }
    if bit(buttons, 22) {
        out |= EXTRA_BTN_RIGHT_PAD;
    }
    if bit(buttons, 23) {
        out |= EXTRA_BTN_RIGHT_TRIGGER_CLICK;
    }
    if bit(buttons, 24) {
        out |= EXTRA_BTN_LEFT_STICK_TOUCH;
    }
    if bit(buttons, 25) {
        out |= EXTRA_BTN_LEFT_PAD_TOUCH;
    }
    if bit(buttons, 26) {
        out |= EXTRA_BTN_LEFT_PAD;
    }
    if bit(buttons, 27) {
        out |= EXTRA_BTN_LEFT_TRIGGER_CLICK;
    }
    if bit(buttons, 28) {
        out |= EXTRA_BTN_RIGHT_GRIP;
    }
    if bit(buttons, 29) {
        out |= EXTRA_BTN_LEFT_GRIP;
    }
    out
}

fn parse_state_42(data: &[u8]) -> Option<ControllerState> {
    if data.len() < 46 {
        return None;
    }
    let buttons = u32_at(data, 2)?;
    let mut state = ControllerState {
        left_x: stick_axis(i16_at(data, 10)?, false),
        left_y: stick_axis(i16_at(data, 12)?, true),
        right_x: stick_axis(i16_at(data, 14)?, false),
        right_y: stick_axis(i16_at(data, 16)?, true),
        l2: trigger_axis(i16_at(data, 6)?),
        r2: trigger_axis(i16_at(data, 8)?),
        buttons: standard_buttons(buttons),
        dpad: dpad_from_bits(
            (bit(buttons, 13) as u8)
                | ((bit(buttons, 11) as u8) << 1)
                | ((bit(buttons, 10) as u8) << 2)
                | ((bit(buttons, 12) as u8) << 3),
        ),
        gyro: [
            i16_at(data, 40)?,
            i16_at(data, 44)?,
            neg_i16(i16_at(data, 42)?),
        ],
        accel: [
            i16_at(data, 34)?,
            i16_at(data, 38)?,
            neg_i16(i16_at(data, 36)?),
        ],
        sensor_timestamp: u32_at(data, 30)?,
        extra_buttons: extra_buttons(buttons),
        steam_left_pad: SteamPadState {
            touch: bit(buttons, 25),
            click: bit(buttons, 26),
            x: i16_at(data, 18)?,
            y: i16_at(data, 20)?,
            pressure: u16_at(data, 22)?,
        },
        steam_right_pad: SteamPadState {
            touch: bit(buttons, 21),
            click: bit(buttons, 22),
            x: i16_at(data, 24)?,
            y: i16_at(data, 26)?,
            pressure: u16_at(data, 28)?,
        },
        gyro_counts_per_dps: TRITON_GYRO_COUNTS_PER_DPS,
        accel_counts_per_g: TRITON_ACCEL_COUNTS_PER_G,
        timestamp_unit_us: TIMESTAMP_UNIT_US_INPUT,
        ..ControllerState::default()
    };
    if bit(buttons, 31) {
        state.buttons |= crate::inputs::BTN_MUTE;
    }
    Some(state)
}

fn parse_state_timestamped(data: &[u8]) -> Option<ControllerState> {
    if data.len() < 46 {
        return None;
    }
    let buttons = u32_at(data, 2)?;
    let mut state = ControllerState {
        left_x: stick_axis(i16_at(data, 10)?, false),
        left_y: stick_axis(i16_at(data, 12)?, true),
        right_x: stick_axis(i16_at(data, 14)?, false),
        right_y: stick_axis(i16_at(data, 16)?, true),
        l2: trigger_axis(i16_at(data, 6)?),
        r2: trigger_axis(i16_at(data, 8)?),
        buttons: standard_buttons(buttons),
        dpad: dpad_from_bits(
            (bit(buttons, 13) as u8)
                | ((bit(buttons, 11) as u8) << 1)
                | ((bit(buttons, 10) as u8) << 2)
                | ((bit(buttons, 12) as u8) << 3),
        ),
        gyro: [
            i16_at(data, 40)?,
            i16_at(data, 44)?,
            neg_i16(i16_at(data, 42)?),
        ],
        accel: [
            i16_at(data, 34)?,
            i16_at(data, 38)?,
            neg_i16(i16_at(data, 36)?),
        ],
        sensor_timestamp: u16_at(data, 32)? as u32,
        extra_buttons: extra_buttons(buttons),
        steam_left_pad: SteamPadState {
            touch: bit(buttons, 25),
            click: bit(buttons, 26),
            x: i16_at(data, 20)?,
            y: i16_at(data, 22)?,
            pressure: u16_at(data, 24)?,
        },
        steam_right_pad: SteamPadState {
            touch: bit(buttons, 21),
            click: bit(buttons, 22),
            x: i16_at(data, 26)?,
            y: i16_at(data, 28)?,
            pressure: u16_at(data, 30)?,
        },
        gyro_counts_per_dps: TRITON_GYRO_COUNTS_PER_DPS,
        accel_counts_per_g: TRITON_ACCEL_COUNTS_PER_G,
        timestamp_unit_us: TIMESTAMP_UNIT_US_TIMESTAMPED,
        ..ControllerState::default()
    };
    if bit(buttons, 31) {
        state.buttons |= crate::inputs::BTN_MUTE;
    }
    Some(state)
}

fn battery_status(charge_state: u8) -> String {
    match charge_state {
        1 => "Discharging",
        2 => "Charging",
        4 => "Full",
        _ => "Unknown",
    }
    .to_string()
}

fn parse_battery_report(data: &[u8]) -> Option<BatteryInfo> {
    let charge_state = *data.get(1)?;
    let level = *data.get(2)?;
    Some(BatteryInfo {
        capacity: level.min(100),
        status: battery_status(charge_state),
    })
}

fn wireless_event(state: u8) -> Option<SteamWirelessEvent> {
    match state {
        WIRELESS_CONNECT => Some(SteamWirelessEvent::Connect),
        WIRELESS_DISCONNECT => Some(SteamWirelessEvent::Disconnect),
        WIRELESS_PAIR => Some(SteamWirelessEvent::Pair),
        _ => None,
    }
}

/// Decode one HID input report, including its report-id byte.
pub fn decode_report(data: &[u8]) -> DecodedReport {
    let Some(&id) = data.first() else {
        return DecodedReport::Ignored;
    };
    match id {
        REPORT_INPUT => match parse_state_42(data) {
            Some(state) => DecodedReport::State(Box::new(state)),
            None => DecodedReport::Malformed,
        },
        REPORT_INPUT_BLE | REPORT_INPUT_TIMESTAMP => match parse_state_timestamped(data) {
            Some(state) => DecodedReport::State(Box::new(state)),
            None => DecodedReport::Malformed,
        },
        REPORT_BATTERY => match parse_battery_report(data) {
            Some(battery) => DecodedReport::Battery(battery),
            None => DecodedReport::Malformed,
        },
        REPORT_WIRELESS_EVENT | REPORT_WIRELESS_EVENT_X => match data.get(1) {
            Some(&state) => match wireless_event(state) {
                Some(event) => DecodedReport::Wireless(event),
                None => DecodedReport::Ignored,
            },
            None => DecodedReport::Malformed,
        },
        _ => DecodedReport::Ignored,
    }
}

pub struct PolledSteamInput {
    pub state: ControllerState,
    pub battery: Option<BatteryInfo>,
    pub disconnected: bool,
}

/// Extends the 16-bit 32 µs timestamp used by report 0x45/0x47 into a
/// monotonic 32-bit tick counter. The raw counter wraps every ~2.1 seconds.
#[derive(Default)]
pub struct TimestampExtender {
    last: Option<u16>,
    extended: u32,
}

impl TimestampExtender {
    pub fn extend(&mut self, raw: u16) -> u32 {
        match self.last {
            Some(previous) => {
                let delta = raw.wrapping_sub(previous) as u32;
                self.extended = self.extended.wrapping_add(delta);
            }
            None => self.extended = raw as u32,
        }
        self.last = Some(raw);
        self.extended
    }

    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// Controls whether closing a handle is allowed to write device-global state.
/// A daemon taking over the same controller must be able to use its own handle
/// without the GUI's closing handle restoring lizard/IMU settings.
#[derive(Clone, Copy, Debug)]
struct SessionWritePolicy {
    restore_on_drop: bool,
    motion_writes: bool,
}

impl Default for SessionWritePolicy {
    fn default() -> Self {
        Self {
            restore_on_drop: true,
            motion_writes: true,
        }
    }
}

impl SessionWritePolicy {
    fn set_restore_on_drop(&mut self, enabled: bool) {
        self.restore_on_drop = enabled;
    }

    fn prepare_handoff(&mut self) {
        self.restore_on_drop = false;
        self.motion_writes = false;
    }

    fn restores_on_drop(self) -> bool {
        self.restore_on_drop
    }

    fn motion_writes_allowed(self) -> bool {
        self.motion_writes
    }
}

pub struct SteamController {
    device: HidDevice,
    product_id: u16,
    serial: String,
    identity: String,
    transport: SteamTransport,
    update_mode: Arc<AtomicBool>,
    motion_enabled: bool,
    policy: SessionWritePolicy,
    last_battery: Option<BatteryInfo>,
    last_lizard_update: Instant,
    lizard_warned: bool,
    timestamps: TimestampExtender,
}

impl SteamController {
    pub fn new(api: &HidApi, selector: Option<&str>) -> Result<Self> {
        // hidapi reports one entry per HID usage; probe each physical
        // interface only once.
        let mut seen = HashSet::new();
        let mut candidates: Vec<&DeviceInfo> = api
            .device_list()
            .filter(|info| {
                is_steam_vid(info.vendor_id())
                    && is_steam_pid(info.product_id())
                    && is_controller_interface(info.product_id(), info.interface_number())
            })
            .filter(|info| {
                seen.insert((
                    info.path().to_string_lossy().into_owned(),
                    info.interface_number(),
                ))
            })
            .collect();

        if let Some(selector) = selector {
            candidates.retain(|info| device_matches(info, selector));
            if candidates.is_empty() {
                bail!(
                    "Steam Controller '{}' not found. Check the connection and try refreshing.",
                    selector
                );
            }
        } else if candidates.is_empty() {
            bail!(
                "No Steam Controller found. Connect the 2026 controller via USB, \
                 Bluetooth, or a puck/receiver slot."
            );
        }

        candidates.sort_by_key(|info| match info.product_id() {
            STEAM_PID_WIRED => 0,
            STEAM_PID_BLE => 1,
            _ => 2 + info.interface_number(),
        });

        let mut last_error: Option<Error> = None;
        for info in candidates {
            let product_id = info.product_id();
            let path = info.path().to_string_lossy().to_string();
            let serial = info
                .serial_number()
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown")
                .to_string();
            let identity = if serial != "Unknown" {
                serial.clone()
            } else {
                path
            };
            let transport = transport_for(product_id, info.interface_number());
            let device = match info
                .open_device(api)
                .with_context(|| format!("Cannot open Steam Controller ({})", transport.label()))
            {
                Ok(device) => device,
                Err(e) => {
                    if crate::util::rate_limited_gate(&LAST_OPEN_ERROR_LOG, Duration::from_secs(30))
                    {
                        eprintln!(
                            "[gyro4u] cannot open Steam Controller ({}): {e}",
                            transport.label()
                        );
                    }
                    last_error = Some(e);
                    continue;
                }
            };

            // Puck/receiver slots exist even when no controller is awake in
            // them. Probe for a live report before selecting the slot.
            if matches!(
                transport,
                SteamTransport::Puck { .. } | SteamTransport::Receiver { .. }
            ) {
                let mut probe = [0u8; 128];
                let first = device.read_timeout(&mut probe, 20);
                let alive = match first {
                    Ok(n) if n > 0 => true,
                    Ok(_) => {
                        // Some firmware variants only start streaming state
                        // after lizard mode is switched off.
                        let _ = device.send_feature_report(&clear_digital_mappings_report());
                        let _ = device.send_feature_report(&lizard_mode_feature_report(false));
                        matches!(device.read_timeout(&mut probe, 20), Ok(n) if n > 0)
                    }
                    Err(e) => {
                        if crate::util::rate_limited_gate(
                            &LAST_PROBE_ERROR_LOG,
                            Duration::from_secs(30),
                        ) {
                            eprintln!(
                                "[gyro4u] Steam Controller probe failed ({}): {e}",
                                transport.label()
                            );
                        }
                        last_error = Some(e.into());
                        continue;
                    }
                };
                if !alive {
                    let _ = device.send_feature_report(&lizard_mode_feature_report(true));
                    if crate::util::rate_limited_gate(
                        &LAST_INACTIVE_SLOT_LOG,
                        Duration::from_secs(60),
                    ) {
                        eprintln!(
                            "[gyro4u] Steam {} has no live controller",
                            transport.label()
                        );
                    }
                    continue;
                }
            }

            let mut controller = Self {
                device,
                product_id,
                serial,
                identity,
                transport,
                update_mode: Arc::new(AtomicBool::new(false)),
                motion_enabled: false,
                policy: SessionWritePolicy::default(),
                last_battery: None,
                last_lizard_update: Instant::now() - LIZARD_KEEPALIVE,
                lizard_warned: false,
                timestamps: TimestampExtender::default(),
            };

            controller.disable_lizard_mode();
            // Start with raw IMU streaming off regardless of what a previous
            // session left behind.
            let _ = controller.write_feature(&imu_mode_feature_report(false));
            controller.motion_enabled = false;
            return Ok(controller);
        }

        if let Some(error) = last_error {
            Err(error)
        } else {
            bail!("No active controller found in the selected puck/receiver slot")
        }
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn vendor_id(&self) -> u16 {
        STEAM_VID
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn transport(&self) -> SteamTransport {
        self.transport
    }

    pub fn is_bluetooth(&self) -> bool {
        self.product_id == STEAM_PID_BLE
    }

    pub fn set_update_mode(&self, active: bool) {
        self.update_mode.store(active, Ordering::SeqCst);
    }

    fn write_feature(&self, report: &[u8; FEATURE_REPORT_SIZE]) -> Result<()> {
        self.device
            .send_feature_report(report)
            .context("Failed to send Steam Controller feature report")
    }

    pub fn haptic_command(
        &mut self,
        side: SteamPadSide,
        haptic: SteamPadHaptic,
        gain_db: i8,
    ) -> Result<()> {
        let report = haptic_command_report(side, haptic, gain_db);
        let written = self
            .device
            .write(&report)
            .context("Failed to send Steam Controller pad haptic report")?;
        if written != report.len() {
            bail!(
                "Short Steam Controller pad haptic write: {written} of {} bytes",
                report.len()
            );
        }
        Ok(())
    }

    fn disable_lizard_mode(&mut self) {
        let mut error = None;
        if let Err(e) = self.write_feature(&clear_digital_mappings_report()) {
            error = Some(e);
        }
        if let Err(e) = self.write_feature(&lizard_mode_feature_report(false)) {
            error = Some(e);
        }
        if let Some(e) = error
            && !self.lizard_warned
        {
            eprintln!(
                "[gyro4u] Steam Controller: cannot disable lizard mode: {e}. \
                 Steam Input or another client may own the device."
            );
            self.lizard_warned = true;
        }
        self.last_lizard_update = Instant::now();
    }

    fn maintain_lizard_mode(&mut self) {
        if self.last_lizard_update.elapsed() < LIZARD_KEEPALIVE {
            return;
        }
        self.disable_lizard_mode();
    }

    fn restore_normal_mode(&mut self) {
        let _ = self.write_feature(&imu_mode_feature_report(false));
        let _ = self.write_feature(&restore_digital_mappings_report());
        let _ = self.write_feature(&lizard_mode_feature_report(true));
    }

    pub fn set_motion(&mut self, enabled: bool) -> Result<()> {
        if !self.policy.motion_writes_allowed() || self.motion_enabled == enabled {
            return Ok(());
        }
        self.write_feature(&imu_mode_feature_report(enabled))?;
        self.motion_enabled = enabled;
        Ok(())
    }

    /// Re-send the IMU mode report even when the cached state already matches.
    pub fn force_motion(&mut self, enabled: bool) -> Result<()> {
        if !self.policy.motion_writes_allowed() {
            return Ok(());
        }
        self.write_feature(&imu_mode_feature_report(enabled))?;
        self.motion_enabled = enabled;
        Ok(())
    }

    pub fn set_restore_on_drop(&mut self, enabled: bool) {
        self.policy.set_restore_on_drop(enabled);
    }

    /// Release the device without restoring lizard/IMU state or accepting
    /// further motion writes; used when a daemon takes over the device.
    pub fn prepare_handoff(&mut self) {
        self.policy.prepare_handoff();
    }

    fn normalize_timestamp(&mut self, state: &mut ControllerState) {
        if (state.timestamp_unit_us - TIMESTAMP_UNIT_US_TIMESTAMPED).abs() < f32::EPSILON {
            state.sensor_timestamp = self.timestamps.extend(state.sensor_timestamp as u16);
        } else {
            self.timestamps.reset();
        }
    }

    fn read_report(&self, timeout_ms: i32, buf: &mut [u8]) -> Result<usize> {
        self.device
            .read_timeout(buf, timeout_ms)
            .context("Failed to read Steam Controller input report")
    }

    pub fn poll_latest_input(&mut self) -> Result<Option<PolledSteamInput>> {
        if self.update_mode.load(Ordering::Relaxed) {
            return Ok(None);
        }
        self.maintain_lizard_mode();

        let mut buf = [0u8; 128];
        let mut latest_state = None;
        let mut battery = None;
        let mut disconnected = false;

        for _ in 0..128 {
            let n = self.read_report(0, &mut buf)?;
            if n == 0 {
                break;
            }
            match decode_report(&buf[..n]) {
                DecodedReport::State(state) => latest_state = Some(*state),
                DecodedReport::Battery(info) => battery = Some(info),
                DecodedReport::Wireless(SteamWirelessEvent::Disconnect) => disconnected = true,
                DecodedReport::Wireless(_) | DecodedReport::Ignored | DecodedReport::Malformed => {}
            }
        }

        if let Some(info) = battery {
            self.last_battery = Some(info);
        }

        let latest_state = latest_state.map(|mut state| {
            self.normalize_timestamp(&mut state);
            state
        });

        if latest_state.is_none() && !disconnected {
            return Ok(None);
        }

        Ok(Some(PolledSteamInput {
            state: latest_state.unwrap_or_default(),
            battery: self.last_battery.clone(),
            disconnected,
        }))
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        if self.update_mode.load(Ordering::Relaxed) {
            bail!("Controller is in update mode");
        }
        self.maintain_lizard_mode();
        let mut buf = [0u8; 128];
        for _ in 0..4 {
            let n = self.read_report(16, &mut buf)?;
            if n == 0 {
                continue;
            }
            match decode_report(&buf[..n]) {
                DecodedReport::State(state) => {
                    let mut state = *state;
                    self.normalize_timestamp(&mut state);
                    return Ok(state);
                }
                DecodedReport::Battery(info) => self.last_battery = Some(info),
                DecodedReport::Wireless(SteamWirelessEvent::Disconnect) => {
                    bail!("Steam Controller wireless connection lost")
                }
                _ => {}
            }
        }
        bail!("Timeout reading Steam Controller state")
    }

    pub fn get_battery(&mut self) -> Result<BatteryInfo> {
        if let Some(info) = self.last_battery.clone() {
            return Ok(info);
        }
        let mut buf = [0u8; 128];
        for _ in 0..16 {
            let n = self.read_report(0, &mut buf)?;
            if n == 0 {
                break;
            }
            match decode_report(&buf[..n]) {
                DecodedReport::Battery(info) => {
                    self.last_battery = Some(info.clone());
                    return Ok(info);
                }
                DecodedReport::Wireless(SteamWirelessEvent::Disconnect) => {
                    bail!("Steam Controller wireless connection lost")
                }
                _ => {}
            }
        }
        bail!("No Steam Controller battery report received yet")
    }

    pub fn get_firmware_info(&self) -> Result<(u16, String, String)> {
        bail!("Firmware updates are not supported for the Steam Controller in gyro4u")
    }

    pub fn set_lightbar(&mut self, _r: u8, _g: u8, _b: u8, _brightness: u8) -> Result<()> {
        Ok(())
    }

    pub fn set_lightbar_enabled(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn set_player_leds(&mut self, _leds: u8) -> Result<()> {
        Ok(())
    }

    pub fn set_mic(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn set_mic_led(&mut self, _state: crate::common::MicLedState) -> Result<()> {
        Ok(())
    }

    pub fn set_trigger_effects(
        &mut self,
        _left: Option<(u8, [u8; 10])>,
        _right: Option<(u8, [u8; 10])>,
    ) -> Result<()> {
        Ok(())
    }

    pub fn set_trigger_effect(
        &mut self,
        _left: bool,
        _right: bool,
        _mode: u8,
        _params: &[u8],
    ) -> Result<()> {
        Ok(())
    }

    pub fn set_trigger_off(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn set_speaker(&mut self, _mode: &str) -> Result<()> {
        Ok(())
    }

    pub fn set_volume(&mut self, _volume: u8) -> Result<()> {
        Ok(())
    }

    pub fn set_vibration(&mut self, _rumble: u8, _trigger: u8) -> Result<()> {
        Ok(())
    }

    pub fn set_rumble(&mut self, _left: u8, _right: u8) -> Result<()> {
        Ok(())
    }

    pub fn set_haptics(&mut self, _samples: &[i8]) -> Result<()> {
        Ok(())
    }
}

impl Drop for SteamController {
    fn drop(&mut self) {
        if self.policy.restores_on_drop() {
            self.restore_normal_mode();
        }
    }
}

fn device_matches(info: &DeviceInfo, selector: &str) -> bool {
    info.serial_number().is_some_and(|s| s == selector) || info.path().to_string_lossy() == selector
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_42() -> Vec<u8> {
        let mut data = vec![0u8; 54];
        data[0] = REPORT_INPUT;
        data[1] = 7;
        data
    }

    fn report_45() -> Vec<u8> {
        let mut data = vec![0u8; 46];
        data[0] = REPORT_INPUT_BLE;
        data[1] = 9;
        data
    }

    fn set_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn set_i16(data: &mut [u8], offset: usize, value: i16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn state(data: Vec<u8>) -> ControllerState {
        match decode_report(&data) {
            DecodedReport::State(state) => *state,
            other => panic!("expected state, got {other:?}"),
        }
    }

    #[test]
    fn decodes_all_standard_button_bits() {
        let cases = [
            (0, BTN_CROSS),
            (1, BTN_CIRCLE),
            (2, BTN_SQUARE),
            (3, BTN_TRIANGLE),
            (5, BTN_R3),
            (6, BTN_OPTIONS),
            (9, BTN_R1),
            (14, BTN_CREATE),
            (15, BTN_L3),
            (19, BTN_L1),
            (23, BTN_R2),
            (27, BTN_L2),
            (31, crate::inputs::BTN_MUTE),
        ];
        for (index, expected) in cases {
            let mut data = report_42();
            set_u32(&mut data, 2, 1 << index);
            assert_eq!(state(data).buttons, expected, "button bit {index}");
        }
    }

    #[test]
    fn menu_and_view_bits_match_device_labels() {
        use crate::inputs::{Button, ControllerKind};

        let mut menu = report_42();
        set_u32(&mut menu, 2, 1 << 6);
        let menu = state(menu);
        assert!(Button::Options.is_pressed(&menu));
        assert!(!Button::Create.is_pressed(&menu));
        assert_eq!(
            Button::Options.device_label(ControllerKind::SteamController),
            "Menu"
        );

        let mut view = report_42();
        set_u32(&mut view, 2, 1 << 14);
        let view = state(view);
        assert!(Button::Create.is_pressed(&view));
        assert!(!Button::Options.is_pressed(&view));
        assert_eq!(
            Button::Create.device_label(ControllerKind::SteamController),
            "View"
        );
    }

    #[test]
    fn decodes_all_extra_button_bits() {
        let cases = [
            (4, EXTRA_BTN_QAM),
            (7, EXTRA_BTN_R4),
            (8, EXTRA_BTN_R5),
            (16, EXTRA_BTN_STEAM),
            (17, EXTRA_BTN_L4),
            (18, EXTRA_BTN_L5),
            (20, EXTRA_BTN_RIGHT_STICK_TOUCH),
            (21, EXTRA_BTN_RIGHT_PAD_TOUCH),
            (22, EXTRA_BTN_RIGHT_PAD),
            (23, EXTRA_BTN_RIGHT_TRIGGER_CLICK),
            (24, EXTRA_BTN_LEFT_STICK_TOUCH),
            (25, EXTRA_BTN_LEFT_PAD_TOUCH),
            (26, EXTRA_BTN_LEFT_PAD),
            (27, EXTRA_BTN_LEFT_TRIGGER_CLICK),
            (28, EXTRA_BTN_RIGHT_GRIP),
            (29, EXTRA_BTN_LEFT_GRIP),
        ];
        for (index, expected) in cases {
            let mut data = report_42();
            set_u32(&mut data, 2, 1 << index);
            assert_eq!(state(data).extra_buttons, expected, "extra bit {index}");
        }
    }

    #[test]
    fn parser_and_button_enum_agree() {
        use crate::inputs::{ALL_BUTTONS, Button, ControllerKind as Kind};
        let steam_buttons: Vec<Button> = ALL_BUTTONS
            .iter()
            .copied()
            .filter(|b| {
                b.to_extra_mask()
                    .is_some_and(|mask| mask <= crate::inputs::EXTRA_BTN_RIGHT_TRIGGER_CLICK)
            })
            .collect();
        assert_eq!(steam_buttons.len(), 16);
        // The global extra space also holds 8BitDo and Flydigi buttons, which
        // Steam reports cannot produce.
        assert_eq!(
            ALL_BUTTONS
                .iter()
                .filter(|b| b.to_extra_mask().is_some())
                .count(),
            29
        );
        for button in steam_buttons {
            let mask = button.to_extra_mask().unwrap();
            // Toggle every raw bit and check that the decoded state exposes
            // this button, which also guards against mask/enum drift.
            let found = (0..30).any(|index| {
                let mut probe = report_42();
                set_u32(&mut probe, 2, 1 << index);
                button.is_pressed(&state(probe))
            });
            assert!(found, "{button:?} ({mask:#x}) was not decodable");
        }
        assert!(
            !Button::Steam
                .device_label(Kind::SteamController)
                .contains("PS")
        );
    }

    #[test]
    fn decodes_dpad_directions_and_diagonals() {
        let cases = [
            (1 << 13, DPAD_N),
            ((1 << 13) | (1 << 11), DPAD_NE),
            (1 << 11, DPAD_E),
            ((1 << 11) | (1 << 10), DPAD_SE),
            (1 << 10, DPAD_S),
            ((1 << 10) | (1 << 12), DPAD_SW),
            (1 << 12, DPAD_W),
            ((1 << 12) | (1 << 13), DPAD_NW),
            ((1 << 13) | (1 << 10), DPAD_NEUTRAL),
            ((1 << 11) | (1 << 12), DPAD_NEUTRAL),
        ];
        for (bits, expected) in cases {
            let mut data = report_42();
            set_u32(&mut data, 2, bits);
            assert_eq!(state(data).dpad, expected, "bits {bits:#x}");
        }
    }

    #[test]
    fn decodes_axis_extrema() {
        for (raw, x_expected, y_expected) in [
            (i16::MIN, 0, 255),
            (-16384, 64, 192),
            (0, 128, 128),
            (16384, 192, 64),
            (i16::MAX, 255, 0),
        ] {
            let mut data = report_42();
            set_i16(&mut data, 10, raw);
            set_i16(&mut data, 12, raw);
            set_i16(&mut data, 14, raw);
            set_i16(&mut data, 16, raw);
            let s = state(data);
            assert_eq!(s.left_x, x_expected, "x raw {raw}");
            assert_eq!(s.right_x, x_expected, "x raw {raw}");
            assert_eq!(s.left_y, y_expected, "y raw {raw}");
            assert_eq!(s.right_y, y_expected, "y raw {raw}");
        }

        let mut data = report_42();
        set_i16(&mut data, 6, 0);
        set_i16(&mut data, 8, 32767);
        let s = state(data);
        assert_eq!(s.l2, 0);
        assert_eq!(s.r2, 255);
    }

    #[test]
    fn decodes_simultaneous_inputs() {
        let mut data = report_42();
        set_u32(
            &mut data,
            2,
            (1 << 0) | (1 << 3) | (1 << 19) | (1 << 10) | (1 << 11) | (1 << 29),
        );
        set_i16(&mut data, 10, 12000);
        set_i16(&mut data, 12, -12000);
        set_i16(&mut data, 6, 20000);
        let s = state(data);
        assert_eq!(s.buttons, BTN_CROSS | BTN_TRIANGLE | BTN_L1);
        assert_eq!(s.dpad, DPAD_SE);
        assert!(s.extra_buttons & EXTRA_BTN_LEFT_GRIP != 0);
        assert!(s.left_x > 128);
        assert!(s.left_y > 128);
        assert!(s.l2 > 150);
    }

    #[test]
    fn decodes_both_report_variants_with_pad_positions() {
        let mut classic = report_42();
        set_u32(&mut classic, 2, (1 << 25) | (1 << 21));
        set_i16(&mut classic, 18, -1234);
        set_i16(&mut classic, 20, 4321);
        set_u16(&mut classic, 22, 12345);
        set_i16(&mut classic, 24, 2222);
        set_i16(&mut classic, 26, -3333);
        set_u16(&mut classic, 28, 23456);
        set_u32(&mut classic, 30, 1_234_567);
        let s = state(classic);
        assert!(s.steam_left_pad.touch);
        assert!(s.steam_right_pad.touch);
        assert_eq!(s.steam_left_pad.x, -1234);
        assert_eq!(s.steam_left_pad.y, 4321);
        assert_eq!(s.steam_left_pad.pressure, 12345);
        assert_eq!(s.steam_right_pad.x, 2222);
        assert_eq!(s.sensor_timestamp, 1_234_567);
        assert_eq!(s.timestamp_unit_us, 1.0);

        let mut timestamped = report_45();
        set_u32(&mut timestamped, 2, (1 << 25) | (1 << 21) | (1 << 26));
        set_u16(&mut timestamped, 18, 0xbeef);
        set_i16(&mut timestamped, 20, -1234);
        set_i16(&mut timestamped, 22, 4321);
        set_u16(&mut timestamped, 24, 12345);
        set_i16(&mut timestamped, 26, 2222);
        set_i16(&mut timestamped, 28, -3333);
        set_u16(&mut timestamped, 30, 23456);
        set_u16(&mut timestamped, 32, 0xabcd);
        let s = state(timestamped);
        assert!(s.steam_left_pad.touch);
        assert!(s.steam_left_pad.click);
        assert_eq!(s.steam_left_pad.x, -1234);
        assert_eq!(s.steam_left_pad.y, 4321);
        assert_eq!(s.steam_left_pad.pressure, 12345);
        assert_eq!(s.steam_right_pad.x, 2222);
        assert_eq!(s.steam_right_pad.y, -3333);
        assert_eq!(s.steam_right_pad.pressure, 23456);
        assert_eq!(s.sensor_timestamp, 0xabcd);
        assert_eq!(s.timestamp_unit_us, 32.0);
    }

    #[test]
    fn timestamped_firmware_variant_0x47_is_supported() {
        let mut data = report_45();
        data[0] = REPORT_INPUT_TIMESTAMP;
        set_u32(&mut data, 2, 1 << 0);
        set_u16(&mut data, 32, 1234);
        let s = state(data);
        assert_eq!(s.buttons, BTN_CROSS);
        assert_eq!(s.sensor_timestamp, 1234);
        assert_eq!(s.timestamp_unit_us, 32.0);
    }

    #[test]
    fn touch_release_keeps_coordinates_but_clears_touch() {
        let mut data = report_42();
        set_u32(&mut data, 2, 1 << 25);
        set_i16(&mut data, 18, 100);
        set_i16(&mut data, 20, 200);
        let touched = state(data.clone());
        assert!(touched.steam_left_pad.touch);

        set_u32(&mut data, 2, 0);
        let released = state(data);
        assert!(!released.steam_left_pad.touch);
        assert_eq!(released.steam_left_pad.x, 100);
        assert_eq!(released.steam_left_pad.y, 200);
    }

    #[test]
    fn truncation_never_panics() {
        for len in 0..46 {
            let mut data = report_42();
            data.truncate(len);
            assert!(
                matches!(
                    decode_report(&data),
                    DecodedReport::Malformed | DecodedReport::Ignored
                ),
                "len {len}"
            );
            if len > 0 {
                assert_eq!(decode_report(&data), DecodedReport::Malformed, "len {len}");
            }
        }
        for len in 1..46 {
            let mut data = report_45();
            data.truncate(len);
            assert_eq!(
                decode_report(&data),
                DecodedReport::Malformed,
                "0x45 len {len}"
            );
        }
        for len in 0..3 {
            let mut data = vec![0u8; 15];
            data[0] = REPORT_BATTERY;
            data.truncate(len);
            assert!(matches!(
                decode_report(&data),
                DecodedReport::Malformed | DecodedReport::Ignored
            ));
        }
        for len in 0..2 {
            let mut data = vec![0u8; 2];
            data[0] = REPORT_WIRELESS_EVENT;
            data.truncate(len);
            assert!(matches!(
                decode_report(&data),
                DecodedReport::Malformed | DecodedReport::Ignored
            ));
        }
    }

    #[test]
    fn malformed_and_unknown_reports_are_ignored_safely() {
        assert_eq!(decode_report(&[]), DecodedReport::Ignored);
        assert_eq!(decode_report(&[0xff, 0, 0, 0]), DecodedReport::Ignored);
        assert_eq!(
            decode_report(&[REPORT_INPUT, 0, 0, 0, 0, 0]),
            DecodedReport::Malformed
        );
    }

    #[test]
    fn decodes_battery_and_wireless_events() {
        let mut data = vec![0u8; 15];
        data[0] = REPORT_BATTERY;
        data[1] = 2;
        data[2] = 87;
        assert_eq!(
            decode_report(&data),
            DecodedReport::Battery(BatteryInfo {
                capacity: 87,
                status: "Charging".to_string(),
            })
        );

        for (raw, expected) in [
            (1, SteamWirelessEvent::Disconnect),
            (2, SteamWirelessEvent::Connect),
            (3, SteamWirelessEvent::Pair),
        ] {
            for id in [REPORT_WIRELESS_EVENT, REPORT_WIRELESS_EVENT_X] {
                assert_eq!(decode_report(&[id, raw]), DecodedReport::Wireless(expected));
            }
        }
    }

    #[test]
    fn report_liveness_classifies_input_and_battery_evidence() {
        assert_eq!(report_liveness(&[]), ReportLiveness::Unknown);
        for id in [REPORT_INPUT, REPORT_INPUT_BLE, REPORT_INPUT_TIMESTAMP] {
            assert_eq!(report_liveness(&[id]), ReportLiveness::Live);
            assert_eq!(report_liveness(&[id, 0, 0, 0]), ReportLiveness::Live);
        }
        assert_eq!(
            report_liveness(&[REPORT_BATTERY, 2, 87]),
            ReportLiveness::Live
        );
        assert_eq!(report_liveness(&[REPORT_BATTERY]), ReportLiveness::Unknown);
        assert_eq!(
            report_liveness(&[REPORT_BATTERY, 2]),
            ReportLiveness::Unknown
        );
    }

    #[test]
    fn report_liveness_handles_connect_disconnect_and_unknown_wireless_state() {
        assert_eq!(
            report_liveness(&[REPORT_WIRELESS_EVENT, WIRELESS_CONNECT]),
            ReportLiveness::Live
        );
        assert_eq!(
            report_liveness(&[REPORT_WIRELESS_EVENT, WIRELESS_DISCONNECT]),
            ReportLiveness::NotLive
        );
        assert_eq!(
            report_liveness(&[REPORT_WIRELESS_EVENT, WIRELESS_PAIR]),
            ReportLiveness::Unknown
        );
        assert_eq!(
            report_liveness(&[REPORT_WIRELESS_EVENT, 0xff]),
            ReportLiveness::Unknown
        );
        assert_eq!(
            report_liveness(&[REPORT_WIRELESS_EVENT]),
            ReportLiveness::Unknown
        );
        assert_eq!(report_liveness(&[0xff, 2]), ReportLiveness::Unknown);
    }

    #[test]
    fn battery_charge_state_labels() {
        for (raw, label) in [
            (1, "Discharging"),
            (2, "Charging"),
            (4, "Full"),
            (9, "Unknown"),
        ] {
            let mut data = vec![0u8; 15];
            data[0] = REPORT_BATTERY;
            data[1] = raw;
            data[2] = 50;
            match decode_report(&data) {
                DecodedReport::Battery(info) => assert_eq!(info.status, label),
                other => panic!("expected battery, got {other:?}"),
            }
        }
    }

    #[test]
    fn full_quaternion_report_is_accepted() {
        let mut data = report_42();
        data.resize(54, 0);
        set_u32(&mut data, 2, 1 << 0);
        assert_eq!(state(data).buttons, BTN_CROSS);
    }

    #[test]
    fn motion_axes_are_normalized_to_linux_axis_order() {
        let mut data = report_42();
        set_i16(&mut data, 34, 1000);
        set_i16(&mut data, 36, 2000);
        set_i16(&mut data, 38, 3000);
        set_i16(&mut data, 40, 4000);
        set_i16(&mut data, 42, 5000);
        set_i16(&mut data, 44, 6000);
        let s = state(data);
        assert_eq!(s.accel, [1000, 3000, -2000]);
        assert_eq!(s.gyro, [4000, 6000, -5000]);
        assert_eq!(s.gyro_counts_per_dps, TRITON_GYRO_COUNTS_PER_DPS);
        assert_eq!(s.accel_counts_per_g, 16384.0);
    }

    #[test]
    fn timestamp_extender_handles_u16_rollover() {
        let mut extender = TimestampExtender::default();
        assert_eq!(extender.extend(65500), 65500);
        assert_eq!(extender.extend(20), 65556);
        assert_eq!(extender.extend(40), 65576);
        extender.reset();
        assert_eq!(extender.extend(5), 5);
    }

    #[test]
    fn sequence_number_rollover_is_accepted() {
        for seq in [0u8, 1, 254, 255] {
            let mut data = report_42();
            data[1] = seq;
            assert!(matches!(decode_report(&data), DecodedReport::State(_)));
        }
    }

    #[test]
    fn controller_interface_filter_matches_puck_slots() {
        assert!(is_controller_interface(STEAM_PID_WIRED, 0));
        assert!(is_controller_interface(STEAM_PID_BLE, 0));
        for slot in 2..=5 {
            assert!(is_controller_interface(STEAM_PID_PUCK, slot));
            assert!(is_controller_interface(STEAM_PID_RECEIVER, slot));
        }
        assert!(!is_controller_interface(STEAM_PID_PUCK, 0));
        assert!(!is_controller_interface(STEAM_PID_PUCK, 1));
        assert!(!is_controller_interface(STEAM_PID_PUCK, 6));
    }

    #[test]
    fn feature_reports_match_sdl_layout() {
        let off = lizard_mode_feature_report(false);
        assert_eq!(off[0], 1);
        assert_eq!(off[1], CMD_SET_SETTINGS_VALUES);
        assert_eq!(off[2], 3);
        assert_eq!(off[3], SETTING_LIZARD_MODE);
        assert_eq!(off[4..6], [0, 0]);

        let on = lizard_mode_feature_report(true);
        assert_eq!(on[4..6], [1, 0]);

        let imu = imu_mode_feature_report(true);
        assert_eq!(imu[3], SETTING_IMU_MODE);
        assert_eq!(u16::from_le_bytes([imu[4], imu[5]]), 0x18);

        assert_eq!(
            clear_digital_mappings_report()[1],
            CMD_CLEAR_DIGITAL_MAPPINGS
        );
        assert_eq!(
            restore_digital_mappings_report()[1],
            CMD_SET_DEFAULT_DIGITAL_MAPPINGS
        );
    }

    #[test]
    fn pad_haptic_report_bytes_and_gain_clamps() {
        assert_eq!(
            haptic_command_report(SteamPadSide::Left, SteamPadHaptic::Tick, 0),
            [REPORT_PAD_HAPTIC, 0, 1, 0]
        );
        assert_eq!(
            haptic_command_report(SteamPadSide::Right, SteamPadHaptic::Click, -9),
            [REPORT_PAD_HAPTIC, 1, 2, (-9i8) as u8]
        );
        assert_eq!(
            haptic_command_report(SteamPadSide::Left, SteamPadHaptic::Tick, -128),
            [REPORT_PAD_HAPTIC, 0, 1, 233]
        );
        assert_eq!(
            haptic_command_report(SteamPadSide::Right, SteamPadHaptic::Tick, 127),
            [REPORT_PAD_HAPTIC, 1, 1, 24]
        );
    }

    #[test]
    fn handoff_disables_restore_and_motion_writes() {
        let mut policy = SessionWritePolicy::default();
        assert!(policy.restores_on_drop());
        assert!(policy.motion_writes_allowed());

        policy.set_restore_on_drop(false);
        assert!(!policy.restores_on_drop());
        assert!(policy.motion_writes_allowed());

        policy.prepare_handoff();
        assert!(!policy.restores_on_drop());
        assert!(!policy.motion_writes_allowed());
    }

    #[test]
    fn transport_labels_are_transport_specific() {
        assert_eq!(transport_for(STEAM_PID_WIRED, 0).label(), "USB");
        assert_eq!(transport_for(STEAM_PID_BLE, 0).label(), "Bluetooth LE");
        assert_eq!(
            transport_for(STEAM_PID_PUCK, 3).label(),
            "Wireless puck slot 3"
        );
        assert_eq!(
            transport_for(STEAM_PID_RECEIVER, 4).label(),
            "Integrated receiver slot 4"
        );
    }

    #[test]
    fn pad_normalization_covers_center_and_edges() {
        let center = SteamPadState {
            x: 0,
            y: 0,
            pressure: 16384,
            ..Default::default()
        };
        assert_eq!(center.normalized(), (0.5, 0.5));
        assert_eq!(center.pressure_normalized(), 0.5);
        let upper_left = SteamPadState {
            x: i16::MIN,
            y: i16::MAX,
            pressure: 65535,
            ..Default::default()
        };
        let (x, y) = upper_left.normalized();
        assert!(x < 0.01);
        assert!(y < 0.01);
    }
}
