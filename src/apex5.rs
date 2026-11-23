//! Wired Flydigi V2 controllers (Apex 5, Vader 5 Pro).
//!
//! Both models expose a vendor HID interface (usage page 0xFFA0). Flydigi V2
//! frames start with `5A A5`, carry a command and a length, and end with a
//! checksum where applicable. Enabling the routing flags starts a 32-byte 0xEF
//! input stream with sticks, buttons, and motion. The report's sample counter
//! is inert on both units, so sample timestamps are synthesized from the host
//! wall clock in microseconds.

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{
    common::BatteryInfo,
    inputs::{
        APEX_LM, APEX_M1, APEX_M2, APEX_M3, APEX_M4, APEX_RM, BTN_CIRCLE, BTN_CREATE, BTN_CROSS,
        BTN_L1, BTN_L2, BTN_L3, BTN_OPTIONS, BTN_PS, BTN_R1, BTN_R2, BTN_R3, BTN_SQUARE,
        BTN_TRIANGLE, ControllerKind, ControllerState, DPAD_E, DPAD_N, DPAD_NE, DPAD_NEUTRAL,
        DPAD_NW, DPAD_S, DPAD_SE, DPAD_SW, DPAD_W, VADER_C, VADER_CIRCLE, VADER_Z,
    },
};

pub const FLYDIGI_VID: u16 = 0x37d7;
pub const APEX5_PID: u16 = 0x2501;
pub const VADER5_PID: u16 = 0x2401;
pub const FLYDIGI_VENDOR_USAGE_PAGE: u16 = 0xffa0;
pub const APEX5_INTERFACE: i32 = 2;
pub const VADER5_INTERFACE: i32 = 1;

pub const APEX5_OUTPUT_REPORT: u8 = 0x03;
pub const VADER5_OUTPUT_REPORT: u8 = 0x00;
pub const APEX5_INPUT_REPORT: u8 = 0x04;
pub const APEX5_CMD_GET_INFO: u8 = 0x01;
pub const APEX5_CMD_ROUTING_READ: u8 = 0x10;
pub const APEX5_CMD_ROUTING_SET: u8 = 0x11;
pub const APEX5_CMD_RUMBLE: u8 = 0x12;
pub const APEX5_CMD_INPUT_STREAM: u8 = 0xef;

pub const APEX5_GYRO_COUNTS_PER_DPS: f32 = 32768.0 / 2000.0;
pub const VADER5_GYRO_COUNTS_PER_DPS: f32 = 32768.0 / 2000.0;
pub const APEX5_ACCEL_COUNTS_PER_G: f32 = 256.0;
pub const VADER5_ACCEL_COUNTS_PER_G: f32 = 4096.0;
pub const APEX5_MODEL_IDS: [u8; 6] = [128, 129, 133, 134, 135, 136];
pub const VADER5_MODEL_IDS: [u8; 1] = [130];
pub const APEX5_TRIGGER_THRESHOLD: u8 = 30;

pub const APEX5_VENDOR_FRAME_LEN: usize = 31;
pub const VADER5_VENDOR_FRAME_LEN: usize = 32;
const INPUT_REPORT_LEN: usize = 32;
const REPLY_TIMEOUT: Duration = Duration::from_millis(300);
const STREAM_TIMEOUT: Duration = Duration::from_millis(1500);
const BATTERY_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlydigiModel {
    Apex5,
    Vader5Pro,
}

impl FlydigiModel {
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            APEX5_PID => Some(Self::Apex5),
            VADER5_PID => Some(Self::Vader5Pro),
            _ => None,
        }
    }

    pub fn interface(self) -> i32 {
        match self {
            Self::Apex5 => APEX5_INTERFACE,
            Self::Vader5Pro => VADER5_INTERFACE,
        }
    }

    pub fn output_lead(self) -> u8 {
        match self {
            Self::Apex5 => APEX5_OUTPUT_REPORT,
            Self::Vader5Pro => VADER5_OUTPUT_REPORT,
        }
    }

    pub fn vendor_frame_len(self) -> usize {
        match self {
            Self::Apex5 => APEX5_VENDOR_FRAME_LEN,
            Self::Vader5Pro => VADER5_VENDOR_FRAME_LEN,
        }
    }

    pub fn kind(self) -> ControllerKind {
        match self {
            Self::Apex5 => ControllerKind::FlydigiApex5,
            Self::Vader5Pro => ControllerKind::FlydigiVader5Pro,
        }
    }

    pub fn label(self) -> &'static str {
        self.kind().label()
    }

    pub fn accepts_model(self, model: u8) -> bool {
        match self {
            Self::Apex5 => is_apex5_model(model),
            Self::Vader5Pro => is_vader5_model(model),
        }
    }

    pub fn gyro_counts_per_dps(self) -> f32 {
        match self {
            Self::Apex5 => APEX5_GYRO_COUNTS_PER_DPS,
            Self::Vader5Pro => VADER5_GYRO_COUNTS_PER_DPS,
        }
    }

    pub fn accel_counts_per_g(self) -> f32 {
        match self {
            Self::Apex5 => APEX5_ACCEL_COUNTS_PER_G,
            Self::Vader5Pro => VADER5_ACCEL_COUNTS_PER_G,
        }
    }
}

pub fn is_flydigi_vid(vid: u16) -> bool {
    vid == FLYDIGI_VID
}

pub fn is_supported_pid(pid: u16) -> bool {
    matches!(pid, APEX5_PID | VADER5_PID)
}

pub fn is_controller_interface(pid: u16, interface_number: i32) -> bool {
    match FlydigiModel::from_pid(pid) {
        Some(model) => interface_number == model.interface(),
        None => false,
    }
}

/// Vendor HID interfaces used by supported Flydigi models.
pub fn is_vendor_interface(interface_number: i32) -> bool {
    matches!(interface_number, APEX5_INTERFACE | VADER5_INTERFACE)
}

pub fn is_apex5_model(model: u8) -> bool {
    APEX5_MODEL_IDS.contains(&model)
}

pub fn is_vader5_model(model: u8) -> bool {
    VADER5_MODEL_IDS.contains(&model)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoutingStatus {
    pub controller_data: u8,
    pub raw_data: u8,
    pub third_party: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InfoReply {
    pub model: u8,
    pub connection: u8,
    pub battery: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedReport {
    State(Box<ControllerState>),
    Ignored,
    Malformed,
}

fn checksum(frame: &[u8]) -> u8 {
    frame.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn vendor_frame(model: FlydigiModel, command: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x5a, 0xa5, command, (payload.len() as u8) + 2];
    bytes.extend_from_slice(payload);
    let sum = checksum(&bytes[2..]);
    bytes.push(sum);
    assert!(bytes.len() <= model.vendor_frame_len());
    bytes.resize(model.vendor_frame_len(), 0);
    bytes.insert(0, model.output_lead());
    bytes
}

/// Rumble frames carry no checksum: the last payload byte is motor data.
fn rumble_frame(model: FlydigiModel, left: u8, right: u8) -> Vec<u8> {
    let mut bytes = vec![0x5a, 0xa5, APEX5_CMD_RUMBLE, 0x06, left, right];
    bytes.resize(model.vendor_frame_len(), 0);
    bytes.insert(0, model.output_lead());
    bytes
}

pub fn get_info_report(model: FlydigiModel) -> Vec<u8> {
    vendor_frame(model, APEX5_CMD_GET_INFO, &[])
}

pub fn routing_read_report(model: FlydigiModel) -> Vec<u8> {
    vendor_frame(model, APEX5_CMD_ROUTING_READ, &[])
}

pub fn routing_set_report(model: FlydigiModel, controller_data: u8, raw_data: u8) -> Vec<u8> {
    vendor_frame(
        model,
        APEX5_CMD_ROUTING_SET,
        &[controller_data, raw_data, 0xff, 0xff, 0xff],
    )
}

pub fn rumble_report(model: FlydigiModel, left: u8, right: u8) -> Vec<u8> {
    rumble_frame(model, left, right)
}

/// Strip an optional report ID before the V2 magic.
pub fn v2_payload(data: &[u8]) -> Option<&[u8]> {
    if data.starts_with(&[0x5a, 0xa5]) {
        Some(data)
    } else if data
        .get(1..)
        .is_some_and(|rest| rest.starts_with(&[0x5a, 0xa5]))
    {
        Some(&data[1..])
    } else {
        None
    }
}

fn has_command(payload: &[u8], command: u8) -> bool {
    payload.starts_with(&[0x5a, 0xa5]) && payload.get(2) == Some(&command)
}

pub fn status_from_reply(data: &[u8]) -> Option<RoutingStatus> {
    let payload = v2_payload(data)?;
    if !has_command(payload, APEX5_CMD_ROUTING_READ) {
        return None;
    }
    Some(RoutingStatus {
        controller_data: *payload.get(5)?,
        raw_data: *payload.get(6)?,
        third_party: *payload.get(9)?,
    })
}

pub fn parse_info_reply(data: &[u8]) -> Option<InfoReply> {
    let payload = v2_payload(data)?;
    if !has_command(payload, APEX5_CMD_GET_INFO) {
        return None;
    }
    Some(InfoReply {
        model: *payload.get(5)?,
        connection: *payload.get(6)?,
        battery: *payload.get(11)?,
    })
}

pub fn battery_from_info(raw: u8) -> BatteryInfo {
    let capacity = (raw & 0x0f).saturating_mul(20).min(100);
    let status = match raw >> 4 {
        1 => "Charging",
        2 => "Full",
        _ => "Discharging",
    };
    BatteryInfo {
        capacity,
        status: status.to_string(),
    }
}

fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

fn neg_i16(value: i16) -> i16 {
    (-(value as i32)).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// This unit's X/Y gyro channels read 16x below the 16-bit Z channel, so
/// left-shift them to a common scale.
fn gyro_xy_scale(value: i16) -> i16 {
    ((value as i32) << 4).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn stick_axis(value: i16) -> u8 {
    ((((value as i32 + 32768) * 255) + 32767) / 65535) as u8
}

fn invert_axis(value: u8) -> u8 {
    255 - value
}

fn dpad_from_bits(primary: u8) -> u8 {
    let horizontal = (primary & 0x02 != 0) as i8 - (primary & 0x08 != 0) as i8;
    let vertical = (primary & 0x04 != 0) as i8 - (primary & 0x01 != 0) as i8;
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

fn parse_state(model: FlydigiModel, data: &[u8], timestamp: u32) -> Option<ControllerState> {
    let minimum_len = match model {
        // The Apex input report's leading 0x04 is stripped from `data`.
        FlydigiModel::Apex5 => INPUT_REPORT_LEN - 1,
        FlydigiModel::Vader5Pro => INPUT_REPORT_LEN,
    };
    if data.len() < minimum_len {
        return None;
    }

    let primary = *data.get(11)?;
    let secondary = *data.get(12)?;
    let extra = *data.get(13)?;
    let guide = *data.get(14)?;

    let mut buttons = 0;
    if primary & 0x10 != 0 {
        buttons |= BTN_CROSS;
    }
    if primary & 0x20 != 0 {
        buttons |= BTN_CIRCLE;
    }
    if primary & 0x40 != 0 {
        buttons |= BTN_CREATE;
    }
    if primary & 0x80 != 0 {
        buttons |= BTN_SQUARE;
    }
    if secondary & 0x01 != 0 {
        buttons |= BTN_TRIANGLE;
    }
    if secondary & 0x02 != 0 {
        buttons |= BTN_OPTIONS;
    }
    if secondary & 0x04 != 0 {
        buttons |= BTN_L1;
    }
    if secondary & 0x08 != 0 {
        buttons |= BTN_R1;
    }
    if secondary & 0x40 != 0 {
        buttons |= BTN_L3;
    }
    if secondary & 0x80 != 0 {
        buttons |= BTN_R3;
    }
    if guide & 0x08 != 0 {
        buttons |= BTN_PS;
    }

    let mut extra_buttons = 0;
    if extra & 0x04 != 0 {
        extra_buttons |= APEX_M1;
    }
    if extra & 0x08 != 0 {
        extra_buttons |= APEX_M2;
    }
    if extra & 0x10 != 0 {
        extra_buttons |= APEX_M3;
    }
    if extra & 0x20 != 0 {
        extra_buttons |= APEX_M4;
    }
    if extra & 0x40 != 0 {
        extra_buttons |= APEX_LM;
    }
    if extra & 0x80 != 0 {
        extra_buttons |= APEX_RM;
    }
    if model == FlydigiModel::Vader5Pro {
        if extra & 0x01 != 0 {
            extra_buttons |= VADER_C;
        }
        if extra & 0x02 != 0 {
            extra_buttons |= VADER_Z;
        }
        if guide & 0x01 != 0 {
            extra_buttons |= VADER_CIRCLE;
        }
    }

    let l2 = *data.get(15)?;
    let r2 = *data.get(16)?;
    if l2 >= APEX5_TRIGGER_THRESHOLD {
        buttons |= BTN_L2;
    }
    if r2 >= APEX5_TRIGGER_THRESHOLD {
        buttons |= BTN_R2;
    }

    let hw_gyro = [i16_at(data, 17)?, i16_at(data, 19)?, i16_at(data, 21)?];
    let hw_accel = [i16_at(data, 23)?, i16_at(data, 25)?, i16_at(data, 27)?];

    let (gyro, accel) = match model {
        FlydigiModel::Apex5 => (
            // SDL axis order: pitch, yaw, roll. X/Y are 12-bit on this unit.
            [
                gyro_xy_scale(hw_gyro[1]),
                gyro_xy_scale(hw_gyro[0]),
                hw_gyro[2],
            ],
            [hw_accel[0], hw_accel[2], neg_i16(hw_accel[1])],
        ),
        FlydigiModel::Vader5Pro => (
            [hw_gyro[0], hw_gyro[2], neg_i16(hw_gyro[1])],
            [hw_accel[0], hw_accel[2], neg_i16(hw_accel[1])],
        ),
    };

    Some(ControllerState {
        left_x: stick_axis(i16_at(data, 3)?),
        left_y: invert_axis(stick_axis(i16_at(data, 5)?)),
        right_x: stick_axis(i16_at(data, 7)?),
        right_y: invert_axis(stick_axis(i16_at(data, 9)?)),
        l2,
        r2,
        buttons,
        dpad: dpad_from_bits(primary),
        gyro,
        accel,
        sensor_timestamp: timestamp,
        extra_buttons,
        gyro_counts_per_dps: model.gyro_counts_per_dps(),
        accel_counts_per_g: model.accel_counts_per_g(),
        timestamp_unit_us: 1.0,
        ..ControllerState::default()
    })
}

/// Decode one input report, with or without its report-id byte. `timestamp` is
/// in wall-clock microseconds; the unit's own sample counter is inert.
pub fn decode_report(model: FlydigiModel, data: &[u8], timestamp: u32) -> DecodedReport {
    let payload = if model == FlydigiModel::Apex5 && data.first() == Some(&APEX5_INPUT_REPORT) {
        if data.len() < 4 {
            return DecodedReport::Malformed;
        }
        if !data[1..].starts_with(&[0x5a, 0xa5]) {
            return DecodedReport::Ignored;
        }
        &data[1..]
    } else {
        match v2_payload(data) {
            Some(payload) => payload,
            None => return DecodedReport::Ignored,
        }
    };
    if !has_command(payload, APEX5_CMD_INPUT_STREAM) {
        return DecodedReport::Ignored;
    }
    match parse_state(model, payload, timestamp) {
        Some(state) => DecodedReport::State(Box::new(state)),
        None => DecodedReport::Malformed,
    }
}

pub struct PolledApex5Input {
    pub state: ControllerState,
    pub battery: Option<BatteryInfo>,
}

pub struct Apex5 {
    device: HidDevice,
    model: FlydigiModel,
    product_id: u16,
    serial: String,
    identity: String,
    original_routing: Option<RoutingStatus>,
    restore_on_drop: bool,
    last_battery: Option<BatteryInfo>,
    last_battery_refresh: Option<Instant>,
    /// Wall-clock base for sample timestamps; the report counter is inert.
    sensor_start: Instant,
}

impl Apex5 {
    pub fn new(api: &HidApi, selector: Option<&str>) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut candidates: Vec<&DeviceInfo> = api
            .device_list()
            .filter(|info| {
                is_flydigi_vid(info.vendor_id())
                    && is_supported_pid(info.product_id())
                    && is_controller_interface(info.product_id(), info.interface_number())
                    && info.usage_page() == FLYDIGI_VENDOR_USAGE_PAGE
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
                    "Flydigi controller '{}' not found. Connect an Apex 5 or Vader 5 Pro via \
                     USB-C in PC mode.",
                    selector
                );
            }
        } else if candidates.is_empty() {
            bail!(
                "No Flydigi controller found. Connect an Apex 5 or Vader 5 Pro via USB-C in PC \
                 mode."
            );
        }

        let mut last_error = None;
        for info in candidates {
            let Some(model) = FlydigiModel::from_pid(info.product_id()) else {
                continue;
            };
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
                model,
                product_id,
                serial,
                identity,
                original_routing: None,
                restore_on_drop: true,
                last_battery: None,
                last_battery_refresh: None,
                sensor_start: Instant::now(),
            };

            let info = driver
                .read_get_info()
                .with_context(|| format!("Cannot read {} info", model.label()))?;
            if !model.accepts_model(info.model) {
                bail!(
                    "Flydigi model {} is not a supported {} variant",
                    info.model,
                    model.label()
                );
            }
            driver.last_battery = Some(battery_from_info(info.battery));
            driver.last_battery_refresh = Some(Instant::now());
            driver.original_routing = driver.read_routing_status();
            driver.set_routing(1, 1)?;
            if let Err(error) = driver.wait_for_stream() {
                driver.restore_routing();
                return Err(error);
            }
            return Ok(driver);
        }

        if let Some(error) = last_error {
            return Err(anyhow::Error::new(error).context("Cannot open Flydigi controller"));
        }
        bail!("No Flydigi controller found. Connect an Apex 5 or Vader 5 Pro via USB-C in PC mode.")
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn vendor_id(&self) -> u16 {
        FLYDIGI_VID
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn kind(&self) -> ControllerKind {
        self.model.kind()
    }

    pub fn set_update_mode(&self, _active: bool) {}

    fn write_report(&self, report: &[u8]) -> Result<()> {
        let written = self
            .device
            .write(report)
            .with_context(|| format!("Failed to send {} output report", self.model.label()))?;
        if written != report.len() {
            bail!(
                "Short {} output write: {written} of {} bytes",
                self.model.label(),
                report.len()
            );
        }
        Ok(())
    }

    fn read_report(&self, timeout_ms: i32, buf: &mut [u8]) -> Result<usize> {
        self.device
            .read_timeout(buf, timeout_ms)
            .with_context(|| format!("Failed to read {} input report", self.model.label()))
    }

    fn read_get_info(&self) -> Result<InfoReply> {
        self.write_report(&get_info_report(self.model))?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            let n = self.read_report(20, &mut buf)?;
            if n == 0 {
                continue;
            }
            if let Some(info) = parse_info_reply(&buf[..n]) {
                return Ok(info);
            }
        }
        bail!("Timed out waiting for {} GetInfo reply", self.model.label())
    }

    fn read_routing_status(&self) -> Option<RoutingStatus> {
        self.write_report(&routing_read_report(self.model)).ok()?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            let n = self.read_report(20, &mut buf).ok()?;
            if n == 0 {
                continue;
            }
            if let Some(status) = status_from_reply(&buf[..n]) {
                return Some(status);
            }
        }
        None
    }

    fn set_routing(&self, controller_data: u8, raw_data: u8) -> Result<()> {
        self.write_report(&routing_set_report(self.model, controller_data, raw_data))
    }

    fn restore_routing(&mut self) {
        if let Some(status) = self.original_routing.take() {
            let _ = self.set_routing(status.controller_data, status.raw_data);
        }
    }

    fn observe(&mut self, data: &[u8]) -> Option<ControllerState> {
        let timestamp = self.sensor_start.elapsed().as_micros() as u32;
        match decode_report(self.model, data, timestamp) {
            DecodedReport::State(state) => Some(*state),
            DecodedReport::Ignored | DecodedReport::Malformed => None,
        }
    }

    fn wait_for_stream(&mut self) -> Result<()> {
        let deadline = Instant::now() + STREAM_TIMEOUT;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            let n = self.read_report(20, &mut buf)?;
            if n == 0 {
                continue;
            }
            if self.observe(&buf[..n]).is_some() {
                return Ok(());
            }
        }
        bail!(
            "Timed out waiting for the {} input stream. In Flydigi Space Station, enable the \
             third-party controller mapping setting for the wired controller, then reconnect.",
            self.model.label()
        )
    }

    pub fn poll_latest_input(&mut self) -> Result<Option<PolledApex5Input>> {
        let mut buf = [0u8; 64];
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
        Ok(latest_state.map(|state| PolledApex5Input {
            state,
            battery: self.last_battery.clone(),
        }))
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        let mut buf = [0u8; 64];
        for _ in 0..64 {
            let n = self.read_report(8, &mut buf)?;
            if n == 0 {
                continue;
            }
            if let Some(state) = self.observe(&buf[..n]) {
                return Ok(state);
            }
        }
        bail!("Timeout reading {} state", self.model.label())
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
                let battery = battery_from_info(info.battery);
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
            self.model.label()
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
        self.write_report(&rumble_report(self.model, left, right))
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

impl Drop for Apex5 {
    fn drop(&mut self) {
        if self.restore_on_drop {
            self.restore_routing();
        }
    }
}

fn device_matches(info: &DeviceInfo, selector: &str) -> bool {
    info.serial_number().is_some_and(|s| s == selector) || info.path().to_string_lossy() == selector
}

#[cfg(test)]
mod tests {
    use super::*;

    const APEX: FlydigiModel = FlydigiModel::Apex5;
    const VADER: FlydigiModel = FlydigiModel::Vader5Pro;

    fn report(model: FlydigiModel) -> Vec<u8> {
        let mut data = vec![0u8; INPUT_REPORT_LEN];
        let lead = match model {
            FlydigiModel::Apex5 => 0x04,
            FlydigiModel::Vader5Pro => 0x5a,
        };
        data[0] = lead;
        if model == FlydigiModel::Vader5Pro {
            data[1] = 0xa5;
            data[2] = APEX5_CMD_INPUT_STREAM;
        } else {
            data[1] = 0x5a;
            data[2] = 0xa5;
            data[3] = APEX5_CMD_INPUT_STREAM;
        }
        data
    }

    fn set_i16(data: &mut [u8], offset: usize, value: i16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn state_for(model: FlydigiModel, data: Vec<u8>) -> ControllerState {
        match decode_report(model, &data, 0) {
            DecodedReport::State(state) => *state,
            other => panic!("expected state, got {other:?}"),
        }
    }

    #[test]
    fn builds_exact_apex_frames() {
        let info = get_info_report(APEX);
        assert_eq!(info.len(), 32);
        assert_eq!(&info[..6], &[0x03, 0x5a, 0xa5, 0x01, 0x02, 0x03]);
        assert!(info[6..].iter().all(|byte| *byte == 0));

        let read = routing_read_report(APEX);
        assert_eq!(read.len(), 32);
        assert_eq!(&read[..6], &[0x03, 0x5a, 0xa5, 0x10, 0x02, 0x12]);
        assert!(read[6..].iter().all(|byte| *byte == 0));

        let set = routing_set_report(APEX, 1, 1);
        assert_eq!(set.len(), 32);
        assert_eq!(
            &set[..11],
            &[
                0x03, 0x5a, 0xa5, 0x11, 0x07, 0x01, 0x01, 0xff, 0xff, 0xff, 0x17
            ]
        );
        assert!(set[11..].iter().all(|byte| *byte == 0));

        let other = routing_set_report(APEX, 0x12, 0x34);
        assert_eq!(&other[..5], &[0x03, 0x5a, 0xa5, 0x11, 0x07]);
        assert_eq!(other[10], checksum(&other[3..10]));

        let rumble = rumble_report(APEX, 0x40, 0x80);
        assert_eq!(rumble.len(), 32);
        assert_eq!(&rumble[..7], &[0x03, 0x5a, 0xa5, 0x12, 0x06, 0x40, 0x80]);
        assert!(rumble[7..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn builds_exact_vader_frames() {
        let info = get_info_report(VADER);
        assert_eq!(info.len(), 33);
        assert_eq!(&info[..6], &[0x00, 0x5a, 0xa5, 0x01, 0x02, 0x03]);
        assert!(info[6..].iter().all(|byte| *byte == 0));

        let read = routing_read_report(VADER);
        assert_eq!(read.len(), 33);
        assert_eq!(&read[..6], &[0x00, 0x5a, 0xa5, 0x10, 0x02, 0x12]);
        assert!(read[6..].iter().all(|byte| *byte == 0));

        let set = routing_set_report(VADER, 1, 1);
        assert_eq!(set.len(), 33);
        assert_eq!(
            &set[..11],
            &[
                0x00, 0x5a, 0xa5, 0x11, 0x07, 0x01, 0x01, 0xff, 0xff, 0xff, 0x17
            ]
        );
        assert!(set[11..].iter().all(|byte| *byte == 0));

        let rumble = rumble_report(VADER, 0x40, 0x80);
        assert_eq!(rumble.len(), 33);
        assert_eq!(&rumble[..7], &[0x00, 0x5a, 0xa5, 0x12, 0x06, 0x40, 0x80]);
        assert!(rumble[7..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn parses_vader_info_fixture() {
        let info = [
            0x5a, 0xa5, 0x01, 0x01, 0x00, 0x82, 0x01, 0x00, 0x00, 0x00, 0x00, 0x25, 0x45, 0x01,
            0x00, 0x71, 0x40, 0x00, 0x00, 0x35, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
            0x26, 0x1f, 0x00, 0x40,
        ];
        assert_eq!(
            parse_info_reply(&info),
            Some(InfoReply {
                model: 130,
                connection: 1,
                battery: 0x25,
            })
        );
        assert!(is_vader5_model(130));
        assert_eq!(
            battery_from_info(0x25),
            BatteryInfo {
                capacity: 100,
                status: "Full".to_string(),
            }
        );

        let mut prefixed = vec![0x00];
        prefixed.extend_from_slice(&info);
        assert_eq!(parse_info_reply(&prefixed), parse_info_reply(&info));
    }

    #[test]
    fn parses_apex_info_and_routing_reply_fixtures() {
        let info = [
            0x04, 0x5a, 0xa5, 0x01, 0x01, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x15, 0x45,
            0x01, 0x00, 0x70, 0x27, 0x00, 0x00, 0x45, 0x25, 0x00, 0x00, 0x01, 0x28, 0x00, 0x00,
            0x11, 0x31, 0x1f, 0x00,
        ];
        assert_eq!(
            parse_info_reply(&info),
            Some(InfoReply {
                model: 128,
                connection: 1,
                battery: 0x15,
            })
        );
        assert!(is_apex5_model(128));

        let mut status = vec![0u8; 32];
        status[0] = 0x04;
        status[1] = 0x5a;
        status[2] = 0xa5;
        status[3] = 0x10;
        status[6] = 1;
        status[7] = 0;
        status[10] = 1;
        assert_eq!(
            status_from_reply(&status),
            Some(RoutingStatus {
                controller_data: 1,
                raw_data: 0,
                third_party: 1,
            })
        );
    }

    #[test]
    fn parses_vader_routing_flags_before_and_after_set() {
        let mut initial = vec![0u8; 32];
        initial[0] = 0x5a;
        initial[1] = 0xa5;
        initial[2] = 0x10;
        initial[5..10].copy_from_slice(&[1, 0, 0, 0, 0]);
        assert_eq!(
            status_from_reply(&initial),
            Some(RoutingStatus {
                controller_data: 1,
                raw_data: 0,
                third_party: 0,
            })
        );

        let mut after = initial.clone();
        after[5..10].copy_from_slice(&[1, 1, 0, 0, 0]);
        assert_eq!(
            status_from_reply(&after),
            Some(RoutingStatus {
                controller_data: 1,
                raw_data: 1,
                third_party: 0,
            })
        );
    }

    #[test]
    fn apex_decodes_every_assigned_button_bit() {
        for (offset, bit, expected) in [
            (12, 0x10, BTN_CROSS),
            (12, 0x20, BTN_CIRCLE),
            (12, 0x40, BTN_CREATE),
            (12, 0x80, BTN_SQUARE),
            (13, 0x01, BTN_TRIANGLE),
            (13, 0x02, BTN_OPTIONS),
            (13, 0x04, BTN_L1),
            (13, 0x08, BTN_R1),
            (13, 0x40, BTN_L3),
            (13, 0x80, BTN_R3),
            (15, 0x08, BTN_PS),
        ] {
            let mut data = report(APEX);
            data[offset] = bit;
            assert_eq!(
                state_for(APEX, data).buttons,
                expected,
                "byte {offset} bit {bit:#x}"
            );
        }
    }

    #[test]
    fn apex_decodes_every_extra_button_bit() {
        for (bit, expected) in [
            (0x04, APEX_M1),
            (0x08, APEX_M2),
            (0x10, APEX_M3),
            (0x20, APEX_M4),
            (0x40, APEX_LM),
            (0x80, APEX_RM),
        ] {
            let mut data = report(APEX);
            data[14] = bit;
            let s = state_for(APEX, data);
            assert_eq!(s.extra_buttons, expected, "extra bit {bit:#x}");
            assert_eq!(s.buttons, 0);
        }
    }

    #[test]
    fn vader_decodes_every_standard_button_bit() {
        for (offset, bit, expected) in [
            (11, 0x10, BTN_CROSS),
            (11, 0x20, BTN_CIRCLE),
            (11, 0x40, BTN_CREATE),
            (11, 0x80, BTN_SQUARE),
            (12, 0x01, BTN_TRIANGLE),
            (12, 0x02, BTN_OPTIONS),
            (12, 0x04, BTN_L1),
            (12, 0x08, BTN_R1),
            (12, 0x40, BTN_L3),
            (12, 0x80, BTN_R3),
            (14, 0x08, BTN_PS),
        ] {
            let mut data = report(VADER);
            data[offset] = bit;
            assert_eq!(
                state_for(VADER, data).buttons,
                expected,
                "byte {offset} bit {bit:#x}"
            );
        }
    }

    #[test]
    fn vader_decodes_every_extra_and_guide_bit() {
        for (offset, bit, expected) in [
            (13, 0x01, VADER_C),
            (13, 0x02, VADER_Z),
            (13, 0x04, APEX_M1),
            (13, 0x08, APEX_M2),
            (13, 0x10, APEX_M3),
            (13, 0x20, APEX_M4),
            (13, 0x40, APEX_LM),
            (13, 0x80, APEX_RM),
            (14, 0x01, VADER_CIRCLE),
        ] {
            let mut data = report(VADER);
            data[offset] = bit;
            let s = state_for(VADER, data);
            assert_eq!(s.extra_buttons, expected, "byte {offset} bit {bit:#x}");
            assert_eq!(s.buttons, 0);
        }
    }

    #[test]
    fn vader_ignores_unassigned_bits() {
        for (offset, bits) in [(12, 0x30), (13, 0x00), (14, 0xf6)] {
            let mut data = report(VADER);
            data[offset] = bits;
            let s = state_for(VADER, data);
            assert_eq!(s.buttons, 0, "byte {offset}");
            assert_eq!(s.extra_buttons, 0, "byte {offset}");
        }
    }

    #[test]
    fn apex_ignores_c_z_and_circle_bits() {
        for (offset, bits) in [(14, 0x03), (15, 0x01)] {
            let mut data = report(APEX);
            data[offset] = bits;
            let s = state_for(APEX, data);
            assert_eq!(s.buttons, 0, "byte {offset}");
            assert_eq!(s.extra_buttons, 0, "byte {offset}");
        }
        let mut data = report(APEX);
        data[14] = 0x03;
        data[15] = 0x09;
        let s = state_for(APEX, data);
        assert_eq!(s.buttons, BTN_PS);
        assert_eq!(s.extra_buttons, 0);
    }

    #[test]
    fn decodes_eight_way_dpad_and_neutral() {
        for (model, offset) in [(VADER, 11), (APEX, 12)] {
            for (bits, expected) in [
                (0x00, DPAD_NEUTRAL),
                (0x01, DPAD_N),
                (0x03, DPAD_NE),
                (0x02, DPAD_E),
                (0x06, DPAD_SE),
                (0x04, DPAD_S),
                (0x0c, DPAD_SW),
                (0x08, DPAD_W),
                (0x09, DPAD_NW),
                (0x05, DPAD_NEUTRAL),
                (0x0a, DPAD_NEUTRAL),
            ] {
                let mut data = report(model);
                data[offset] = bits;
                assert_eq!(
                    state_for(model, data).dpad,
                    expected,
                    "{model:?} primary bits {bits:#x}"
                );
            }
        }
    }

    #[test]
    fn normalizes_stick_endpoints_center_and_inverts_y() {
        for (model, lx, ly, rx, ry) in [(VADER, 3, 5, 7, 9), (APEX, 4, 6, 8, 10)] {
            let s = state_for(model, report(model));
            assert_eq!(
                (s.left_x, s.left_y, s.right_x, s.right_y),
                (128, 127, 128, 127)
            );

            let mut data = report(model);
            set_i16(&mut data, lx, -32768);
            set_i16(&mut data, ly, -32768);
            set_i16(&mut data, rx, 32767);
            set_i16(&mut data, ry, 32767);
            let s = state_for(model, data);
            assert_eq!((s.left_x, s.left_y, s.right_x, s.right_y), (0, 255, 255, 0));

            let mut data = report(model);
            set_i16(&mut data, ly, 16384);
            let s = state_for(model, data);
            assert!(s.left_y < 128, "positive raw Y must be inverted upward");
        }
    }

    #[test]
    fn trigger_threshold_is_inclusive_at_30() {
        for (model, l2_offset, r2_offset) in [(VADER, 15, 16), (APEX, 16, 17)] {
            let mut data = report(model);
            data[l2_offset] = 29;
            data[r2_offset] = 29;
            let s = state_for(model, data);
            assert_eq!(s.l2, 29);
            assert_eq!(s.r2, 29);
            assert_eq!(s.buttons & (BTN_L2 | BTN_R2), 0);

            let mut data = report(model);
            data[l2_offset] = 30;
            data[r2_offset] = 30;
            let s = state_for(model, data);
            assert_eq!(s.buttons & (BTN_L2 | BTN_R2), BTN_L2 | BTN_R2);
        }
    }

    #[test]
    fn vader_applies_axis_order_and_scales() {
        let mut data = report(VADER);
        set_i16(&mut data, 17, 100);
        set_i16(&mut data, 19, 200);
        set_i16(&mut data, 21, 300);
        set_i16(&mut data, 23, 1);
        set_i16(&mut data, 25, 2);
        set_i16(&mut data, 27, 3);
        let s = state_for(VADER, data);

        assert_eq!(s.gyro, [100, 300, -200]);
        assert_eq!(s.accel, [1, 3, -2]);
        assert_eq!(s.gyro_counts_per_dps, VADER5_GYRO_COUNTS_PER_DPS);
        assert_eq!(s.accel_counts_per_g, VADER5_ACCEL_COUNTS_PER_G);
        assert_eq!(s.timestamp_unit_us, 1.0);
    }

    #[test]
    fn apex_keeps_its_axis_order_and_scales() {
        let mut data = report(APEX);
        set_i16(&mut data, 18, 100);
        set_i16(&mut data, 20, 200);
        set_i16(&mut data, 22, 300);
        set_i16(&mut data, 24, 1);
        set_i16(&mut data, 26, 2);
        set_i16(&mut data, 28, 3);
        let s = state_for(APEX, data);

        // pitch = raw21 * 16, yaw = raw19 * 16, roll = raw23.
        assert_eq!(s.gyro, [3200, 1600, 300]);
        assert_eq!(s.accel, [1, 3, -2]);
        assert_eq!(s.gyro_counts_per_dps, APEX5_GYRO_COUNTS_PER_DPS);
        assert_eq!(s.accel_counts_per_g, APEX5_ACCEL_COUNTS_PER_G);
    }

    #[test]
    fn decode_timestamps_are_wall_clock_microseconds() {
        match decode_report(VADER, &report(VADER), 1234) {
            DecodedReport::State(state) => assert_eq!(state.sensor_timestamp, 1234),
            other => panic!("expected state, got {other:?}"),
        }
    }

    #[test]
    fn gyro_xy_scale_saturates_at_i16_bounds() {
        assert_eq!(gyro_xy_scale(i16::MAX), i16::MAX);
        assert_eq!(gyro_xy_scale(i16::MIN), i16::MIN);
        assert_eq!(gyro_xy_scale(1), 16);
        assert_eq!(gyro_xy_scale(-1), -16);
    }

    #[test]
    fn battery_nibbles_map_status_and_capacity() {
        assert_eq!(
            battery_from_info(0x15),
            BatteryInfo {
                capacity: 100,
                status: "Charging".to_string(),
            }
        );
        assert_eq!(
            battery_from_info(0x25),
            BatteryInfo {
                capacity: 100,
                status: "Full".to_string(),
            }
        );
        assert_eq!(
            battery_from_info(0x00),
            BatteryInfo {
                capacity: 0,
                status: "Discharging".to_string(),
            }
        );
        assert_eq!(
            battery_from_info(0x03),
            BatteryInfo {
                capacity: 60,
                status: "Discharging".to_string(),
            }
        );
        assert_eq!(
            battery_from_info(0x0f),
            BatteryInfo {
                capacity: 100,
                status: "Discharging".to_string(),
            }
        );
    }

    #[test]
    fn prefixed_and_unprefixed_reports_decode_identically() {
        let raw = report(VADER);
        let mut prefixed = vec![0x00];
        prefixed.extend_from_slice(&raw);
        assert_eq!(
            decode_report(VADER, &raw, 7),
            decode_report(VADER, &prefixed, 7)
        );

        let mut apex = vec![0u8; 32];
        apex[0] = 0x5a;
        apex[1] = 0xa5;
        apex[2] = APEX5_CMD_INPUT_STREAM;
        apex[11] = 0x10;
        let apex_state = state_for(APEX, apex.clone());
        let mut prefixed = vec![0x04];
        prefixed.extend_from_slice(&apex);
        assert_eq!(
            decode_report(APEX, &apex, 7),
            decode_report(APEX, &prefixed, 7)
        );
        assert_eq!(state_for(APEX, prefixed), apex_state);
    }

    #[test]
    fn rejects_wrong_id_magic_command_and_short_reports() {
        assert_eq!(decode_report(APEX, &[0x03; 32], 0), DecodedReport::Ignored);
        assert_eq!(decode_report(VADER, &[0x03; 32], 0), DecodedReport::Ignored);

        let mut wrong_magic = report(APEX);
        wrong_magic[1] = 0;
        assert_eq!(decode_report(APEX, &wrong_magic, 0), DecodedReport::Ignored);

        let mut wrong_command = report(APEX);
        wrong_command[3] = 0x01;
        assert_eq!(
            decode_report(APEX, &wrong_command, 0),
            DecodedReport::Ignored
        );

        assert_eq!(decode_report(APEX, &[0x04], 0), DecodedReport::Malformed);
        assert_eq!(
            decode_report(APEX, &report(APEX)[..31], 0),
            DecodedReport::Malformed
        );
        assert_eq!(
            decode_report(VADER, &report(VADER)[..31], 0),
            DecodedReport::Malformed
        );
        assert!(matches!(
            decode_report(APEX, &report(APEX), 0),
            DecodedReport::State(_)
        ));
        assert!(matches!(
            decode_report(VADER, &report(VADER), 0),
            DecodedReport::State(_)
        ));
    }

    #[test]
    fn model_and_interface_allowlists_are_explicit() {
        for model in [128, 129, 133, 134, 135, 136] {
            assert!(is_apex5_model(model));
            assert!(!is_vader5_model(model));
        }
        for model in [0, 1, 127, 130, 131, 132, 137, 255] {
            assert!(!is_apex5_model(model));
        }
        assert!(is_vader5_model(130));
        assert!(!is_vader5_model(128));

        assert!(is_supported_pid(APEX5_PID));
        assert!(is_supported_pid(VADER5_PID));
        assert!(!is_supported_pid(0x2502));
        assert!(is_controller_interface(APEX5_PID, 2));
        assert!(!is_controller_interface(APEX5_PID, 1));
        assert!(is_controller_interface(VADER5_PID, 1));
        assert!(!is_controller_interface(VADER5_PID, 2));
        assert!(!is_controller_interface(0x2502, 1));
        assert_eq!(FlydigiModel::from_pid(APEX5_PID), Some(APEX));
        assert_eq!(FlydigiModel::from_pid(VADER5_PID), Some(VADER));
        assert_eq!(FlydigiModel::from_pid(0x2502), None);
    }
}
