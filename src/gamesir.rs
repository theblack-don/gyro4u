//! GameSir Tarantula Pro over its 2.4 GHz dongle.
//!
//! The dongle exposes interface 1 as vendor HID (usage page 0xFFA0) carrying
//! keyboard, mouse, consumer, and vendor reports. Vendor OUT frames use report
//! 0x07 with `[len][object][op][payload...]` and vendor IN frames use report
//! 0x06. Enabling raw mode starts a 27-byte state frame at roughly 73 Hz. The
//! report has no timestamp, so sample timestamps come from the host wall clock
//! in microseconds.
//!
//! The driver estimates and subtracts the gyro's startup bias. Keep the
//! controller still during the roughly 0.5 s connect calibration; if it moves,
//! the estimate is skipped and the raw bias remains.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use hidapi::{DeviceInfo as HidDeviceInfo, HidApi, HidDevice};

use crate::{
    common::BatteryInfo,
    inputs::{
        BTN_CIRCLE, BTN_CREATE, BTN_CROSS, BTN_GAMESIR_C1, BTN_GAMESIR_C2, BTN_GAMESIR_C3,
        BTN_GAMESIR_C4, BTN_GAMESIR_L4, BTN_GAMESIR_R4, BTN_GAMESIR_SHARE, BTN_GAMESIR_T1,
        BTN_GAMESIR_T2, BTN_GAMESIR_T3, BTN_L1, BTN_L2, BTN_L3, BTN_OPTIONS, BTN_PS, BTN_R1,
        BTN_R2, BTN_R3, BTN_SQUARE, BTN_TRIANGLE, ControllerKind, ControllerState, DPAD_E, DPAD_N,
        DPAD_NE, DPAD_NEUTRAL, DPAD_NW, DPAD_S, DPAD_SE, DPAD_SW, DPAD_W,
    },
};

pub const GAMESIR_VID: u16 = 0x3537;
pub const TARANTULA_PRO_PID: u16 = 0x103e;
pub const GAMESIR_VENDOR_USAGE_PAGE: u16 = 0xffa0;
pub const GAMESIR_INTERFACE: i32 = 1;

pub const GAMESIR_REPORT_ID: u8 = 0x06;
pub const GAMESIR_OUTPUT_REPORT: u8 = 0x07;
pub const GAMESIR_OBJECT: u8 = 0x0a;
pub const GAMESIR_OP_GET_INFO: u8 = 0x01;
pub const GAMESIR_OP_SET_RAW: u8 = 0x02;
pub const GAMESIR_OP_STATE: u8 = 0x03;
pub const GAMESIR_OP_RUMBLE: u8 = 0x04;

pub const GAMESIR_OUTPUT_LEN: usize = 32;
pub const GAMESIR_STATE_LEN: usize = 27;
pub const GAMESIR_INFO_LEN: usize = 15;
pub const GAMESIR_ACK_LEN: usize = 5;
pub const GAMESIR_TRIGGER_THRESHOLD: u8 = 30;

pub const GAMESIR_GYRO_COUNTS_PER_DPS: f32 = 14.29;
pub const GAMESIR_ACCEL_COUNTS_PER_G: f32 = 4096.0;

const REPLY_TIMEOUT: Duration = Duration::from_millis(300);
const STREAM_TIMEOUT: Duration = Duration::from_millis(700);
const BATTERY_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const INPUT_REPORT_LEN: usize = 64;
const GYRO_CALIBRATION_TIMEOUT: Duration = Duration::from_millis(1200);
const GYRO_CALIBRATION_SAMPLES: usize = 32;
const GYRO_BIAS_MIN_SAMPLES: usize = 16;
const GYRO_BIAS_MAX_RANGE: i32 = 100;
const GYRO_BIAS_MAX_MEAN_MAGNITUDE: f64 = 400.0;

pub fn is_gamesir_vid(vid: u16) -> bool {
    vid == GAMESIR_VID
}

pub fn is_supported_pid(pid: u16) -> bool {
    pid == TARANTULA_PRO_PID
}

pub fn is_controller_interface(pid: u16, interface_number: i32) -> bool {
    is_supported_pid(pid) && interface_number == GAMESIR_INTERFACE
}

/// Why a visible GameSir device is skipped. Supported handles return None.
pub fn skip_reason(pid: u16) -> Option<&'static str> {
    if is_supported_pid(pid) {
        None
    } else if pid == 0x1050 {
        Some(
            "wired GameSir Tarantula Pro is not supported yet; connect through the 2.4 GHz \
             dongle (3537:103e) in XInput/PC mode",
        )
    } else {
        Some(
            "unsupported GameSir model; only the Tarantula Pro over its 2.4 GHz dongle is supported",
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub device_id: u8,
    pub firmware: u16,
    pub mac: [u8; 6],
    pub battery: u8,
    pub device_type: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedReport {
    State(Box<ControllerState>),
    Ack { object: u8, op: u8, status: u8 },
    Info(DeviceInfo),
    Ignored,
    Malformed,
}

/// Build one zero-padded 32-byte output frame including its report ID.
pub fn out_frame(object: u8, op: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![GAMESIR_OUTPUT_REPORT, (payload.len() as u8) + 3, object, op];
    bytes.extend_from_slice(payload);
    assert!(bytes.len() <= GAMESIR_OUTPUT_LEN);
    bytes.resize(GAMESIR_OUTPUT_LEN, 0);
    bytes
}

pub fn set_raw_state_report(mode: u8) -> Vec<u8> {
    out_frame(GAMESIR_OBJECT, GAMESIR_OP_SET_RAW, &[mode])
}

pub fn get_info_report() -> Vec<u8> {
    out_frame(GAMESIR_OBJECT, GAMESIR_OP_GET_INFO, &[])
}

pub fn rumble_report(left: u8, right: u8) -> Vec<u8> {
    out_frame(GAMESIR_OBJECT, GAMESIR_OP_RUMBLE, &[left, right, 0, 0])
}

/// Parse a `06 0e 0a 01 ...` GetInfo reply.
pub fn parse_info_reply(data: &[u8]) -> Option<DeviceInfo> {
    if data.len() < GAMESIR_INFO_LEN
        || data[0] != GAMESIR_REPORT_ID
        || data[2] != GAMESIR_OBJECT
        || data[3] != GAMESIR_OP_GET_INFO
    {
        return None;
    }
    Some(DeviceInfo {
        device_id: data[4],
        firmware: u16::from_be_bytes([data[5], data[6]]),
        mac: [data[7], data[8], data[9], data[10], data[11], data[12]],
        battery: data[13],
        device_type: data[14],
    })
}

pub fn battery_from_raw(raw: u8) -> BatteryInfo {
    if raw == 255 {
        BatteryInfo {
            capacity: 100,
            status: "Charging".to_string(),
        }
    } else {
        BatteryInfo {
            capacity: raw.min(100),
            status: "Discharging".to_string(),
        }
    }
}

fn i16_be_at(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

/// Stick axes are passed through unchanged: the hardware already reports Y with
/// 0 at the top, matching the project convention.
fn stick_axes(ls_x: u8, ls_y: u8, rs_x: u8, rs_y: u8) -> (u8, u8, u8, u8) {
    (ls_x, ls_y, rs_x, rs_y)
}

/// Isolated so the hardware motion axis order and signs can be corrected after
/// a live pass. The output stays in DualSense `[pitch, yaw, roll]` order.
fn motion_axes(hw_gyro: [i16; 3], hw_accel: [i16; 3]) -> ([i16; 3], [i16; 3]) {
    (
        [hw_gyro[0], hw_gyro[2], hw_gyro[1].saturating_neg()],
        [hw_accel[0], hw_accel[1], hw_accel[2]],
    )
}

/// Estimate a stationary gyro bias, rejecting moving or implausible samples.
fn estimate_gyro_bias(samples: &[[i16; 3]]) -> Option<[i16; 3]> {
    if samples.len() < GYRO_BIAS_MIN_SAMPLES {
        return None;
    }
    let mut min = [i32::MAX; 3];
    let mut max = [i32::MIN; 3];
    let mut sum = [0i32; 3];
    for sample in samples {
        for (axis, value) in sample.iter().enumerate() {
            let value = i32::from(*value);
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
            sum[axis] += value;
        }
    }
    if (0..3).any(|axis| max[axis] - min[axis] > GYRO_BIAS_MAX_RANGE) {
        return None;
    }
    let count = samples.len() as f64;
    let means = sum.map(|total| f64::from(total) / count);
    if means.iter().map(|mean| mean * mean).sum::<f64>()
        > GYRO_BIAS_MAX_MEAN_MAGNITUDE * GYRO_BIAS_MAX_MEAN_MAGNITUDE
    {
        return None;
    }
    Some(means.map(|mean| mean.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16))
}

fn apply_gyro_bias(state: &mut ControllerState, bias: [i16; 3]) {
    for (axis, offset) in state.gyro.iter_mut().zip(bias) {
        *axis = axis.saturating_sub(offset);
    }
}

fn dpad_from_bits(secondary: u8) -> u8 {
    let horizontal = (secondary & 0x20 != 0) as i8 - (secondary & 0x10 != 0) as i8;
    let vertical = (secondary & 0x80 != 0) as i8 - (secondary & 0x40 != 0) as i8;
    match (horizontal, vertical) {
        (0, -1) => DPAD_N,
        (1, -1) => DPAD_NE,
        (1, 0) => DPAD_E,
        (1, 1) => DPAD_SE,
        (0, 1) => DPAD_S,
        (-1, 1) => DPAD_SW,
        (-1, 0) => DPAD_W,
        (-1, -1) => DPAD_NW,
        _ => DPAD_NEUTRAL,
    }
}

fn parse_state(data: &[u8], timestamp: u32) -> Option<ControllerState> {
    if data.len() < GAMESIR_STATE_LEN {
        return None;
    }

    let primary = data[10];
    let secondary = data[11];
    let extra = data[12];
    let extra2 = data[13];
    let l2 = data[8];
    let r2 = data[9];

    let mut buttons = 0;
    if primary & 0x02 != 0 {
        buttons |= BTN_CROSS;
    }
    if primary & 0x01 != 0 {
        buttons |= BTN_CIRCLE;
    }
    if primary & 0x08 != 0 {
        buttons |= BTN_SQUARE;
    }
    if primary & 0x04 != 0 {
        buttons |= BTN_TRIANGLE;
    }
    if primary & 0x10 != 0 {
        buttons |= BTN_L1;
    }
    if primary & 0x20 != 0 {
        buttons |= BTN_L2;
    }
    if primary & 0x40 != 0 {
        buttons |= BTN_L3;
    }
    if primary & 0x80 != 0 {
        buttons |= BTN_R1;
    }
    if secondary & 0x01 != 0 {
        buttons |= BTN_R2;
    }
    if secondary & 0x02 != 0 {
        buttons |= BTN_R3;
    }
    if secondary & 0x04 != 0 {
        buttons |= BTN_CREATE;
    }
    if secondary & 0x08 != 0 {
        buttons |= BTN_OPTIONS;
    }
    if extra & 0x01 != 0 {
        buttons |= BTN_PS;
    }
    if extra & 0x02 != 0 {
        buttons |= BTN_GAMESIR_SHARE;
    }
    if extra2 & 0x01 != 0 {
        buttons |= BTN_GAMESIR_L4;
    }
    if extra2 & 0x02 != 0 {
        buttons |= BTN_GAMESIR_R4;
    }
    if extra2 & 0x04 != 0 {
        buttons |= BTN_GAMESIR_T1;
    }
    if extra2 & 0x08 != 0 {
        buttons |= BTN_GAMESIR_T2;
    }
    if extra2 & 0x10 != 0 {
        buttons |= BTN_GAMESIR_T3;
    }
    if extra2 & 0x20 != 0 {
        buttons |= BTN_GAMESIR_C1;
    }
    if extra2 & 0x40 != 0 {
        buttons |= BTN_GAMESIR_C2;
    }
    if extra2 & 0x80 != 0 {
        buttons |= BTN_GAMESIR_C3;
    }
    if data[14] & 0x01 != 0 {
        buttons |= BTN_GAMESIR_C4;
    }
    if l2 >= GAMESIR_TRIGGER_THRESHOLD {
        buttons |= BTN_L2;
    }
    if r2 >= GAMESIR_TRIGGER_THRESHOLD {
        buttons |= BTN_R2;
    }

    let hw_gyro = [
        i16_be_at(data, 21)?,
        i16_be_at(data, 23)?,
        i16_be_at(data, 25)?,
    ];
    let hw_accel = [
        i16_be_at(data, 15)?,
        i16_be_at(data, 17)?,
        i16_be_at(data, 19)?,
    ];
    let (gyro, accel) = motion_axes(hw_gyro, hw_accel);
    let (left_x, left_y, right_x, right_y) = stick_axes(data[4], data[5], data[6], data[7]);

    Some(ControllerState {
        left_x,
        left_y,
        right_x,
        right_y,
        l2,
        r2,
        buttons,
        dpad: dpad_from_bits(secondary),
        gyro,
        accel,
        sensor_timestamp: timestamp,
        gyro_counts_per_dps: GAMESIR_GYRO_COUNTS_PER_DPS,
        accel_counts_per_g: GAMESIR_ACCEL_COUNTS_PER_G,
        timestamp_unit_us: 1.0,
        ..ControllerState::default()
    })
}

/// Decode one input report. Dispatch uses the report ID plus object/op; the
/// frame's own length byte is never trusted for routing.
pub fn decode_report(data: &[u8], timestamp: u32) -> DecodedReport {
    if data.first() != Some(&GAMESIR_REPORT_ID) {
        return DecodedReport::Ignored;
    }
    if data.len() < 4 {
        return DecodedReport::Malformed;
    }
    match (data[2], data[3]) {
        (GAMESIR_OBJECT, GAMESIR_OP_STATE) => match parse_state(data, timestamp) {
            Some(state) => DecodedReport::State(Box::new(state)),
            None => DecodedReport::Malformed,
        },
        (GAMESIR_OBJECT, GAMESIR_OP_SET_RAW) => {
            if data.len() < GAMESIR_ACK_LEN {
                DecodedReport::Malformed
            } else {
                DecodedReport::Ack {
                    object: data[2],
                    op: data[3],
                    status: data[4],
                }
            }
        }
        (GAMESIR_OBJECT, GAMESIR_OP_GET_INFO) => match parse_info_reply(data) {
            Some(info) => DecodedReport::Info(info),
            None => DecodedReport::Malformed,
        },
        _ => DecodedReport::Ignored,
    }
}

pub struct PolledGameSirInput {
    pub state: ControllerState,
    pub battery: Option<BatteryInfo>,
}

pub struct GameSirTarantulaPro {
    device: HidDevice,
    product_id: u16,
    serial: String,
    identity: String,
    restore_on_drop: bool,
    last_battery: Option<BatteryInfo>,
    last_battery_refresh: Option<Instant>,
    last_state_at: Instant,
    /// Wall-clock base for sample timestamps; the report has no timestamp.
    sensor_start: Instant,
    gyro_bias: [i16; 3],
}

impl GameSirTarantulaPro {
    pub fn new(api: &HidApi, selector: Option<&str>) -> Result<Self> {
        let mut candidates: Vec<&HidDeviceInfo> = Vec::new();
        for info in api.device_list().filter(|info| {
            is_gamesir_vid(info.vendor_id())
                && is_supported_pid(info.product_id())
                && info.interface_number() == GAMESIR_INTERFACE
        }) {
            let path = info.path().to_string_lossy();
            match candidates.iter_mut().find(|existing| {
                existing.interface_number() == info.interface_number()
                    && existing.path().to_string_lossy() == path
            }) {
                Some(existing) => {
                    // Prefer the vendor usage page when the same hidraw is
                    // repeated for keyboard, mouse, and consumer collections.
                    if existing.usage_page() != GAMESIR_VENDOR_USAGE_PAGE
                        && info.usage_page() == GAMESIR_VENDOR_USAGE_PAGE
                    {
                        *existing = info;
                    }
                }
                None => candidates.push(info),
            }
        }

        if let Some(selector) = selector {
            candidates.retain(|info| device_matches(info, selector));
            if candidates.is_empty() {
                bail!(
                    "GameSir Tarantula Pro '{}' not found. Connect it through its 2.4 GHz \
                     dongle in XInput/PC mode.",
                    selector
                );
            }
        } else if candidates.is_empty() {
            bail!(
                "No GameSir Tarantula Pro found. Connect it through its 2.4 GHz dongle in \
                 XInput/PC mode."
            );
        }

        let mut last_error = None;
        for info in candidates {
            let device = match info.open_device(api) {
                Ok(device) => device,
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            };
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
            let mut driver = Self {
                device,
                product_id,
                serial,
                identity,
                restore_on_drop: true,
                last_battery: None,
                last_battery_refresh: None,
                last_state_at: Instant::now(),
                sensor_start: Instant::now(),
                gyro_bias: [0; 3],
            };

            if let Ok(info) = driver.read_get_info() {
                driver.last_battery = Some(battery_from_raw(info.battery));
                driver.last_battery_refresh = Some(Instant::now());
            }
            if let Err(error) = driver.enable_raw_state() {
                let _ = driver.write_report(&set_raw_state_report(0));
                return Err(error);
            }
            driver.calibrate_gyro_bias();
            return Ok(driver);
        }

        if let Some(error) = last_error {
            return Err(anyhow::Error::new(error).context("Cannot open GameSir Tarantula Pro"));
        }
        bail!(
            "No GameSir Tarantula Pro found. Connect it through its 2.4 GHz dongle in \
             XInput/PC mode."
        )
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn vendor_id(&self) -> u16 {
        GAMESIR_VID
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn kind(&self) -> ControllerKind {
        ControllerKind::GameSirTarantulaPro
    }

    pub fn set_update_mode(&self, _active: bool) {}

    fn write_report(&self, report: &[u8]) -> Result<()> {
        let written = self
            .device
            .write(report)
            .context("Failed to send GameSir Tarantula Pro output report")?;
        if written != report.len() {
            bail!(
                "Short GameSir Tarantula Pro output write: {written} of {} bytes",
                report.len()
            );
        }
        Ok(())
    }

    fn read_report(&self, timeout_ms: i32, buf: &mut [u8]) -> Result<usize> {
        self.device
            .read_timeout(buf, timeout_ms)
            .context("Failed to read GameSir Tarantula Pro input report")
    }

    fn read_get_info(&mut self) -> Result<DeviceInfo> {
        self.write_report(&get_info_report())?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut buf = [0u8; INPUT_REPORT_LEN];
        while Instant::now() < deadline {
            let n = self.read_report(20, &mut buf)?;
            if n == 0 {
                continue;
            }
            if let Some(info) = parse_info_reply(&buf[..n]) {
                return Ok(info);
            }
        }
        bail!("Timed out waiting for GameSir Tarantula Pro GetInfo reply")
    }

    fn wait_for_ack(&mut self) -> Result<()> {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut buf = [0u8; INPUT_REPORT_LEN];
        while Instant::now() < deadline {
            let n = self.read_report(20, &mut buf)?;
            if n == 0 {
                continue;
            }
            if let DecodedReport::Ack { status, .. } = decode_report(&buf[..n], 0) {
                if status != 0 {
                    bail!("GameSir Tarantula Pro rejected raw mode (status {status})");
                }
                return Ok(());
            }
        }
        bail!("Timed out waiting for GameSir Tarantula Pro raw-mode ACK")
    }

    fn enable_raw_state(&mut self) -> Result<()> {
        self.write_report(&set_raw_state_report(1))
            .with_context(|| {
                "Cannot enable raw GameSir Tarantula Pro input. Connect it through its 2.4 GHz \
                 dongle in XInput/PC mode and install the udev rule for 3537:103e."
            })?;
        self.wait_for_ack()
            .and_then(|()| self.wait_for_stream())
            .with_context(|| {
                "Cannot enable raw GameSir Tarantula Pro input. Connect it through its 2.4 GHz \
                 dongle in XInput/PC mode and install the udev rule for 3537:103e."
            })
    }

    /// Best-effort startup bias capture; failures leave the bias at zero.
    fn calibrate_gyro_bias(&mut self) {
        let deadline = Instant::now() + GYRO_CALIBRATION_TIMEOUT;
        let mut samples = Vec::with_capacity(GYRO_CALIBRATION_SAMPLES);
        let mut buf = [0u8; INPUT_REPORT_LEN];
        while samples.len() < GYRO_CALIBRATION_SAMPLES && Instant::now() < deadline {
            let Ok(n) = self.read_report(20, &mut buf) else {
                break;
            };
            if n == 0 {
                continue;
            }
            if let DecodedReport::State(state) = decode_report(&buf[..n], 0) {
                self.last_state_at = Instant::now();
                samples.push(state.gyro);
            }
        }
        if let Some(bias) = estimate_gyro_bias(&samples) {
            self.gyro_bias = bias;
        }
    }

    fn observe(&mut self, data: &[u8]) -> Option<ControllerState> {
        let timestamp = self.sensor_start.elapsed().as_micros() as u32;
        match decode_report(data, timestamp) {
            DecodedReport::State(mut state) => {
                apply_gyro_bias(&mut state, self.gyro_bias);
                self.last_state_at = Instant::now();
                Some(*state)
            }
            DecodedReport::Info(info) => {
                self.last_battery = Some(battery_from_raw(info.battery));
                self.last_battery_refresh = Some(Instant::now());
                None
            }
            DecodedReport::Ack { .. } | DecodedReport::Ignored | DecodedReport::Malformed => None,
        }
    }

    fn wait_for_stream(&mut self) -> Result<()> {
        let deadline = Instant::now() + STREAM_TIMEOUT;
        let mut buf = [0u8; INPUT_REPORT_LEN];
        while Instant::now() < deadline {
            let n = self.read_report(20, &mut buf)?;
            if n == 0 {
                continue;
            }
            if self.observe(&buf[..n]).is_some() {
                return Ok(());
            }
        }
        bail!("Timed out waiting for the GameSir Tarantula Pro input stream")
    }

    fn request_battery_if_due(&mut self) {
        let due = self
            .last_battery_refresh
            .is_none_or(|last| last.elapsed() >= BATTERY_REFRESH_INTERVAL);
        if due {
            self.last_battery_refresh = Some(Instant::now());
            let _ = self.write_report(&get_info_report());
        }
    }

    pub fn poll_latest_input(&mut self) -> Result<Option<PolledGameSirInput>> {
        self.request_battery_if_due();
        let mut buf = [0u8; INPUT_REPORT_LEN];
        let mut latest_state = None;
        for _ in 0..128 {
            let n = self.read_report(0, &mut buf)?;
            if n == 0 {
                break;
            }
            if let Some(state) = self.observe(&buf[..n]) {
                latest_state = Some(state);
            }
        }
        if self.last_state_at.elapsed() >= STREAM_TIMEOUT {
            bail!("GameSir Tarantula Pro input stream timed out");
        }
        Ok(latest_state.map(|state| PolledGameSirInput {
            state,
            battery: self.last_battery.clone(),
        }))
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        let mut buf = [0u8; INPUT_REPORT_LEN];
        for _ in 0..4 {
            let n = self.read_report(50, &mut buf)?;
            if n == 0 {
                continue;
            }
            if let Some(state) = self.observe(&buf[..n]) {
                return Ok(state);
            }
        }
        bail!("Timeout reading GameSir Tarantula Pro state")
    }

    pub fn get_battery(&mut self) -> Result<BatteryInfo> {
        let refresh_due = self
            .last_battery_refresh
            .is_none_or(|last| last.elapsed() >= BATTERY_REFRESH_INTERVAL);
        if !refresh_due && let Some(info) = self.last_battery.clone() {
            return Ok(info);
        }
        match self.read_get_info() {
            Ok(info) => {
                let battery = battery_from_raw(info.battery);
                self.last_battery = Some(battery.clone());
                self.last_battery_refresh = Some(Instant::now());
                Ok(battery)
            }
            Err(error) => match self.last_battery.clone() {
                Some(battery) => Ok(battery),
                None => Err(error),
            },
        }
    }

    pub fn get_firmware_info(&self) -> Result<(u16, String, String)> {
        bail!(
            "Firmware information is not supported for the {}",
            self.kind().label()
        )
    }

    pub fn set_motion(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn force_motion(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn set_restore_on_drop(&mut self, enabled: bool) {
        self.restore_on_drop = enabled;
    }

    pub fn prepare_handoff(&mut self) {
        self.restore_on_drop = false;
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

    pub fn set_vibration(&mut self, rumble: u8, _trigger: u8) -> Result<()> {
        self.set_rumble(rumble, rumble)
    }

    pub fn set_rumble(&mut self, left: u8, right: u8) -> Result<()> {
        self.write_report(&rumble_report(left, right))
    }

    pub fn set_haptics(&mut self, _samples: &[i8]) -> Result<()> {
        Ok(())
    }

    pub fn set_speaker(&mut self, _mode: &str) -> Result<()> {
        Ok(())
    }

    pub fn set_volume(&mut self, _volume: u8) -> Result<()> {
        Ok(())
    }
}

impl Drop for GameSirTarantulaPro {
    fn drop(&mut self) {
        if self.restore_on_drop {
            let _ = self.write_report(&set_raw_state_report(0));
        }
    }
}

fn device_matches(info: &HidDeviceInfo, selector: &str) -> bool {
    info.serial_number().is_some_and(|s| s == selector) || info.path().to_string_lossy() == selector
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_report() -> Vec<u8> {
        let mut data = vec![0u8; GAMESIR_STATE_LEN];
        data[0] = GAMESIR_REPORT_ID;
        data[1] = 0x1a;
        data[2] = GAMESIR_OBJECT;
        data[3] = GAMESIR_OP_STATE;
        data[4] = 128;
        data[5] = 128;
        data[6] = 128;
        data[7] = 128;
        data
    }

    fn set_i16_be(data: &mut [u8], offset: usize, value: i16) {
        data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn state_for(data: Vec<u8>) -> ControllerState {
        match decode_report(&data, 0) {
            DecodedReport::State(state) => *state,
            other => panic!("expected state, got {other:?}"),
        }
    }

    #[test]
    fn builds_exact_frames() {
        let raw = set_raw_state_report(1);
        assert_eq!(raw.len(), 32);
        assert_eq!(&raw[..5], &[0x07, 0x04, 0x0a, 0x02, 0x01]);
        assert!(raw[5..].iter().all(|byte| *byte == 0));

        let off = set_raw_state_report(0);
        assert_eq!(&off[..5], &[0x07, 0x04, 0x0a, 0x02, 0x00]);

        let info = get_info_report();
        assert_eq!(info.len(), 32);
        assert_eq!(&info[..4], &[0x07, 0x03, 0x0a, 0x01]);
        assert!(info[4..].iter().all(|byte| *byte == 0));

        let rumble = rumble_report(0x40, 0x80);
        assert_eq!(rumble.len(), 32);
        assert_eq!(
            &rumble[..8],
            &[0x07, 0x07, 0x0a, 0x04, 0x40, 0x80, 0x00, 0x00]
        );
        assert!(rumble[8..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn parses_info_reply_and_battery() {
        let info = [
            0x06, 0x0e, 0x0a, 0x01, 0x42, 0x01, 0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x37,
            0x01,
        ];
        assert_eq!(
            parse_info_reply(&info),
            Some(DeviceInfo {
                device_id: 0x42,
                firmware: 0x0102,
                mac: [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
                battery: 0x37,
                device_type: 0x01,
            })
        );
        assert_eq!(
            decode_report(&info, 9),
            DecodedReport::Info(DeviceInfo {
                device_id: 0x42,
                firmware: 0x0102,
                mac: [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
                battery: 0x37,
                device_type: 0x01,
            })
        );
        assert_eq!(
            battery_from_raw(255),
            BatteryInfo {
                capacity: 100,
                status: "Charging".to_string(),
            }
        );
        assert_eq!(
            battery_from_raw(55),
            BatteryInfo {
                capacity: 55,
                status: "Discharging".to_string(),
            }
        );
    }

    #[test]
    fn decodes_every_standard_button_bit() {
        for (offset, bit, expected) in [
            (10, 0x01, BTN_CIRCLE),
            (10, 0x02, BTN_CROSS),
            (10, 0x04, BTN_TRIANGLE),
            (10, 0x08, BTN_SQUARE),
            (10, 0x10, BTN_L1),
            (10, 0x20, BTN_L2),
            (10, 0x40, BTN_L3),
            (10, 0x80, BTN_R1),
            (11, 0x01, BTN_R2),
            (11, 0x02, BTN_R3),
            (11, 0x04, BTN_CREATE),
            (11, 0x08, BTN_OPTIONS),
        ] {
            let mut data = state_report();
            data[offset] = bit;
            assert_eq!(
                state_for(data).buttons,
                expected,
                "byte {offset} bit {bit:#x}"
            );
        }
    }

    #[test]
    fn decodes_every_extra_button_bit() {
        for (offset, bit, expected) in [
            (12, 0x01, BTN_PS),
            (12, 0x02, BTN_GAMESIR_SHARE),
            (13, 0x01, BTN_GAMESIR_L4),
            (13, 0x02, BTN_GAMESIR_R4),
            (13, 0x04, BTN_GAMESIR_T1),
            (13, 0x08, BTN_GAMESIR_T2),
            (13, 0x10, BTN_GAMESIR_T3),
            (13, 0x20, BTN_GAMESIR_C1),
            (13, 0x40, BTN_GAMESIR_C2),
            (13, 0x80, BTN_GAMESIR_C3),
            (14, 0x01, BTN_GAMESIR_C4),
        ] {
            let mut data = state_report();
            data[offset] = bit;
            let state = state_for(data);
            assert_eq!(state.buttons, expected, "byte {offset} bit {bit:#x}");
            assert_eq!(state.extra_buttons, 0);
        }
    }

    #[test]
    fn ignores_unassigned_bits_including_m_and_constant_bit_six() {
        for (offset, bits) in [(12, 0xfc), (14, 0xfe)] {
            let mut data = state_report();
            data[offset] = bits;
            let state = state_for(data);
            assert_eq!(state.buttons, 0, "byte {offset}");
            assert_eq!(state.extra_buttons, 0, "byte {offset}");
        }
    }

    #[test]
    fn decodes_eight_way_dpad_and_neutral() {
        for (bits, expected) in [
            (0x00, DPAD_NEUTRAL),
            (0x40, DPAD_N),
            (0x60, DPAD_NE),
            (0x20, DPAD_E),
            (0xa0, DPAD_SE),
            (0x80, DPAD_S),
            (0x90, DPAD_SW),
            (0x10, DPAD_W),
            (0x50, DPAD_NW),
            (0xc0, DPAD_NEUTRAL),
            (0x30, DPAD_NEUTRAL),
        ] {
            let mut data = state_report();
            data[11] = bits;
            assert_eq!(state_for(data).dpad, expected, "secondary bits {bits:#x}");
        }
    }

    #[test]
    fn passes_stick_axes_through_without_inversion() {
        let state = state_for(state_report());
        assert_eq!(
            (state.left_x, state.left_y, state.right_x, state.right_y),
            (128, 128, 128, 128)
        );

        let mut data = state_report();
        data[4] = 0;
        data[5] = 0;
        data[6] = 255;
        data[7] = 255;
        let state = state_for(data);
        assert_eq!(
            (state.left_x, state.left_y, state.right_x, state.right_y),
            (0, 0, 255, 255)
        );

        let mut data = state_report();
        data[5] = 200;
        assert_eq!(state_for(data).left_y, 200);
    }

    #[test]
    fn estimates_gyro_bias_from_stable_samples() {
        assert_eq!(estimate_gyro_bias(&[[9, 3, -7]; 32]), Some([9, 3, -7]));

        let samples: Vec<[i16; 3]> = (0..32)
            .map(|index| [if index % 2 == 0 { 9 } else { 10 }, 3, -7])
            .collect();
        assert_eq!(estimate_gyro_bias(&samples), Some([10, 3, -7]));
    }

    #[test]
    fn rejects_gyro_bias_when_samples_move() {
        let mut samples = [[9, 3, -7]; 32];
        samples[0][1] = 104;
        assert_eq!(estimate_gyro_bias(&samples), None);
    }

    #[test]
    fn rejects_gyro_bias_with_too_few_samples() {
        assert_eq!(estimate_gyro_bias(&[[9, 3, -7]; 15]), None);
    }

    #[test]
    fn rejects_gyro_bias_with_large_mean_magnitude() {
        assert_eq!(estimate_gyro_bias(&[[400, 0, 0]; 32]), Some([400, 0, 0]));
        assert_eq!(estimate_gyro_bias(&[[401, 0, 0]; 32]), None);
    }

    #[test]
    fn subtracts_gyro_bias_with_saturation() {
        let mut state = ControllerState::default();
        state.gyro = [100, -100, 400];
        apply_gyro_bias(&mut state, [40, -40, 50]);
        assert_eq!(state.gyro, [60, -60, 350]);
        assert_eq!(state.accel, [0, 0, 0]);

        state.gyro = [i16::MIN, i16::MAX, 0];
        apply_gyro_bias(&mut state, [1, -1, 0]);
        assert_eq!(state.gyro, [i16::MIN, i16::MAX, 0]);
    }

    #[test]
    fn trigger_digital_bits_or_analog_threshold() {
        let mut data = state_report();
        data[8] = 29;
        data[9] = 29;
        let state = state_for(data);
        assert_eq!(state.l2, 29);
        assert_eq!(state.r2, 29);
        assert_eq!(state.buttons & (BTN_L2 | BTN_R2), 0);

        let mut data = state_report();
        data[8] = 30;
        data[9] = 30;
        assert_eq!(state_for(data).buttons & (BTN_L2 | BTN_R2), BTN_L2 | BTN_R2);

        let mut data = state_report();
        data[10] = 0x20;
        data[11] = 0x01;
        let state = state_for(data);
        assert_eq!(state.l2, 0);
        assert_eq!(state.r2, 0);
        assert_eq!(state.buttons & (BTN_L2 | BTN_R2), BTN_L2 | BTN_R2);
    }

    #[test]
    fn applies_initial_motion_axis_order_and_scales() {
        let mut data = state_report();
        set_i16_be(&mut data, 15, 1);
        set_i16_be(&mut data, 17, 2);
        set_i16_be(&mut data, 19, 3);
        set_i16_be(&mut data, 21, 100);
        set_i16_be(&mut data, 23, 200);
        set_i16_be(&mut data, 25, 300);
        let state = state_for(data);

        assert_eq!(state.accel, [1, 2, 3]);
        assert_eq!(state.gyro, [100, 300, -200]);
        assert_eq!(state.gyro_counts_per_dps, GAMESIR_GYRO_COUNTS_PER_DPS);
        assert_eq!(state.accel_counts_per_g, GAMESIR_ACCEL_COUNTS_PER_G);
        assert_eq!(state.timestamp_unit_us, 1.0);
    }

    #[test]
    fn ack_info_and_state_are_never_confused() {
        let ack = [0x06, 0x04, 0x0a, 0x02, 0x00];
        assert_eq!(
            decode_report(&ack, 0),
            DecodedReport::Ack {
                object: 0x0a,
                op: 0x02,
                status: 0
            }
        );
        assert!(matches!(
            decode_report(&state_report(), 0),
            DecodedReport::State(_)
        ));
        assert!(matches!(
            decode_report(
                &[
                    0x06, 0x0e, 0x0a, 0x01, 0x42, 0x01, 0x02, 0, 0, 0, 0, 0, 0, 0x37, 0x01
                ],
                0
            ),
            DecodedReport::Info(_)
        ));
        assert_eq!(
            decode_report(&[0x06, 0x04, 0x0a, 0x02], 0),
            DecodedReport::Malformed
        );
        assert_eq!(
            decode_report(&[0x06, 0x0e, 0x0a, 0x01], 0),
            DecodedReport::Malformed
        );
        assert_eq!(
            decode_report(&state_report()[..26], 0),
            DecodedReport::Malformed
        );
    }

    #[test]
    fn rejects_non_vendor_report_ids_and_short_frames() {
        assert_eq!(decode_report(&[], 0), DecodedReport::Ignored);
        assert_eq!(decode_report(&[0x01; 16], 0), DecodedReport::Ignored);
        assert_eq!(decode_report(&[0x02; 16], 0), DecodedReport::Ignored);
        assert_eq!(decode_report(&[0x03; 16], 0), DecodedReport::Ignored);
        assert_eq!(decode_report(&[0x06, 0x00], 0), DecodedReport::Malformed);
        assert_eq!(
            decode_report(&[0x06, 0x04, 0x0a, 0x09, 0], 0),
            DecodedReport::Ignored
        );
    }

    #[test]
    fn decode_timestamps_are_wall_clock_microseconds() {
        match decode_report(&state_report(), 1234) {
            DecodedReport::State(state) => assert_eq!(state.sensor_timestamp, 1234),
            other => panic!("expected state, got {other:?}"),
        }
    }

    #[test]
    fn ids_interfaces_and_skip_reasons_are_explicit() {
        assert!(is_gamesir_vid(GAMESIR_VID));
        assert!(!is_gamesir_vid(0x054c));
        assert!(is_supported_pid(TARANTULA_PRO_PID));
        assert!(!is_supported_pid(0x1050));
        assert!(is_controller_interface(TARANTULA_PRO_PID, 1));
        assert!(!is_controller_interface(TARANTULA_PRO_PID, 0));
        assert!(!is_controller_interface(TARANTULA_PRO_PID, 2));
        assert!(!is_controller_interface(0x1050, 1));
        assert!(skip_reason(TARANTULA_PRO_PID).is_none());
        assert!(
            skip_reason(0x1050)
                .expect("wired skip")
                .contains("not supported yet")
        );
        assert!(skip_reason(0x103f).is_some());
    }
}
