//! 8BitDo Pro 3 DInput support.
//!
//! The wired controller streams 32-byte report 0x04 at roughly 200 Hz. The
//! Bluetooth DInput variant uses report 0x01 with the same layout and is
//! accepted as experimental. The 2.4G receiver PIDs are intentionally not
//! supported: they expose XInput/Switch data only.

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{
    common::BatteryInfo,
    inputs::{
        BTN_CIRCLE, BTN_CREATE, BTN_CROSS, BTN_L1, BTN_L2, BTN_L3, BTN_OPTIONS, BTN_PS, BTN_R1,
        BTN_R2, BTN_R3, BTN_SQUARE, BTN_TRIANGLE, ControllerState, DPAD_E, DPAD_N, DPAD_NE,
        DPAD_NEUTRAL, DPAD_NW, DPAD_S, DPAD_SE, DPAD_SW, DPAD_W, EXTRA_BTN_EIGHTBITDO_L4,
        EXTRA_BTN_EIGHTBITDO_PL, EXTRA_BTN_EIGHTBITDO_PR, EXTRA_BTN_EIGHTBITDO_R4,
    },
};

pub const EIGHTBITDO_VID: u16 = 0x2dc8;
pub const EIGHTBITDO_PRO3_PID: u16 = 0x6009;
pub const EIGHTBITDO_RECEIVER_DOCKED_PID: u16 = 0x3109;
pub const EIGHTBITDO_RECEIVER_XINPUT_PID: u16 = 0x310b;

pub const REPORT_INPUT: u8 = 0x04;
pub const REPORT_INPUT_BLUETOOTH: u8 = 0x01;
pub const REPORT_RUMBLE: u8 = 0x05;

pub const EIGHTBITDO_GYRO_COUNTS_PER_DPS: f32 = 32768.0 / 2000.0;
pub const EIGHTBITDO_ACCEL_COUNTS_PER_G: f32 = 4096.0;
pub const EIGHTBITDO_TIMESTAMP_UNIT_US: f32 = 1.0;

const REPORT_LEN: usize = 32;

pub fn is_eightbitdo_vid(vid: u16) -> bool {
    vid == EIGHTBITDO_VID
}

pub fn is_supported_pid(pid: u16) -> bool {
    pid == EIGHTBITDO_PRO3_PID
}

pub fn is_receiver_pid(pid: u16) -> bool {
    matches!(
        pid,
        EIGHTBITDO_RECEIVER_DOCKED_PID | EIGHTBITDO_RECEIVER_XINPUT_PID
    )
}

/// Why a visible 8BitDo device is skipped. Supported Pro 3 handles return None.
pub fn skip_reason(pid: u16) -> Option<&'static str> {
    if is_receiver_pid(pid) {
        Some(
            "2.4G receiver is XInput/Switch only; DInput, gyro, and extra buttons require \
             USB-C or Bluetooth in D mode",
        )
    } else if is_supported_pid(pid) {
        None
    } else {
        Some("unsupported 8BitDo model; only the Pro 3 in DInput mode is supported")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedReport {
    State(Box<ControllerState>),
    Ignored,
    Malformed,
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

fn neg_i16(value: i16) -> i16 {
    (-(value as i32)).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn stick_axis(raw: u8) -> u8 {
    match raw {
        0 => 0,
        0x7f => 128,
        1..=0x7e => ((raw as f32) * 128.0 / 127.0).round() as u8,
        _ => (128.0 + (raw as f32 - 127.0) * 127.0 / 128.0).round() as u8,
    }
}

fn dpad_from_nibble(nibble: u8) -> u8 {
    match nibble & 0x0f {
        0 => DPAD_N,
        1 => DPAD_NE,
        2 => DPAD_E,
        3 => DPAD_SE,
        4 => DPAD_S,
        5 => DPAD_SW,
        6 => DPAD_W,
        7 => DPAD_NW,
        _ => DPAD_NEUTRAL,
    }
}

fn parse_state(data: &[u8]) -> Option<ControllerState> {
    if data.len() < REPORT_LEN {
        return None;
    }
    let buttons_low = *data.get(8)?;
    let buttons_high = *data.get(9)?;
    let buttons_extra = *data.get(10)?;

    let mut buttons = 0;
    if buttons_low & (1 << 0) != 0 {
        buttons |= BTN_CROSS; // A
    }
    if buttons_low & (1 << 1) != 0 {
        buttons |= BTN_CIRCLE; // B
    }
    if buttons_low & (1 << 3) != 0 {
        buttons |= BTN_SQUARE; // X
    }
    if buttons_low & (1 << 4) != 0 {
        buttons |= BTN_TRIANGLE; // Y
    }
    if buttons_low & (1 << 6) != 0 {
        buttons |= BTN_L1;
    }
    if buttons_low & (1 << 7) != 0 {
        buttons |= BTN_R1;
    }
    if buttons_high & (1 << 0) != 0 {
        buttons |= BTN_L2;
    }
    if buttons_high & (1 << 1) != 0 {
        buttons |= BTN_R2;
    }
    if buttons_high & (1 << 2) != 0 {
        buttons |= BTN_CREATE;
    }
    if buttons_high & (1 << 3) != 0 {
        buttons |= BTN_OPTIONS;
    }
    if buttons_high & (1 << 4) != 0 {
        buttons |= BTN_PS;
    }
    if buttons_high & (1 << 5) != 0 {
        buttons |= BTN_L3;
    }
    if buttons_high & (1 << 6) != 0 {
        buttons |= BTN_R3;
    }

    let mut extra_buttons = 0;
    if buttons_low & (1 << 5) != 0 {
        extra_buttons |= EXTRA_BTN_EIGHTBITDO_PL;
    }
    if buttons_low & (1 << 2) != 0 {
        extra_buttons |= EXTRA_BTN_EIGHTBITDO_PR;
    }
    if buttons_extra & (1 << 0) != 0 {
        extra_buttons |= EXTRA_BTN_EIGHTBITDO_L4;
    }
    if buttons_extra & (1 << 1) != 0 {
        extra_buttons |= EXTRA_BTN_EIGHTBITDO_R4;
    }

    let hw_accel = [i16_at(data, 15)?, i16_at(data, 17)?, i16_at(data, 19)?];
    let hw_gyro = [i16_at(data, 21)?, i16_at(data, 23)?, i16_at(data, 25)?];

    // Orientation follows SDL; hardware sign verification is still pending.
    Some(ControllerState {
        left_x: stick_axis(*data.get(2)?),
        left_y: stick_axis(*data.get(3)?),
        right_x: stick_axis(*data.get(4)?),
        right_y: stick_axis(*data.get(5)?),
        l2: *data.get(7)?,
        r2: *data.get(6)?,
        buttons,
        dpad: dpad_from_nibble(*data.get(1)?),
        gyro: [neg_i16(hw_gyro[1]), hw_gyro[2], neg_i16(hw_gyro[0])],
        accel: [neg_i16(hw_accel[1]), hw_accel[2], neg_i16(hw_accel[0])],
        sensor_timestamp: u32_at(data, 27)?,
        extra_buttons,
        gyro_counts_per_dps: EIGHTBITDO_GYRO_COUNTS_PER_DPS,
        accel_counts_per_g: EIGHTBITDO_ACCEL_COUNTS_PER_G,
        timestamp_unit_us: EIGHTBITDO_TIMESTAMP_UNIT_US,
        ..ControllerState::default()
    })
}

/// Decode one HID input report, including its report-id byte.
pub fn decode_report(data: &[u8]) -> DecodedReport {
    match data.first() {
        Some(&REPORT_INPUT) | Some(&REPORT_INPUT_BLUETOOTH) => match parse_state(data) {
            Some(state) => DecodedReport::State(Box::new(state)),
            None => DecodedReport::Malformed,
        },
        _ => DecodedReport::Ignored,
    }
}

pub fn battery_from_report(data: &[u8]) -> Option<BatteryInfo> {
    if !matches!(
        data.first(),
        Some(&REPORT_INPUT) | Some(&REPORT_INPUT_BLUETOOTH)
    ) {
        return None;
    }
    let raw = *data.get(14)?;
    let level = (raw & 0x7f).min(100);
    let charging = raw & 0x80 != 0;
    // The layout exposes a single charging bit; 100% is treated as Full.
    let status = if level >= 100 {
        "Full"
    } else if charging {
        "Charging"
    } else {
        "Discharging"
    };
    Some(BatteryInfo {
        capacity: level,
        status: status.to_string(),
    })
}

/// Output report 0x05. The two amplitudes drive the low/high frequency
/// motors; trigger rumble is not expected to be supported on the Pro 3.
pub fn rumble_report(left: u8, right: u8) -> [u8; 5] {
    [REPORT_RUMBLE, left, right, 0, 0]
}

pub struct PolledEightBitDoInput {
    pub state: ControllerState,
    pub battery: Option<BatteryInfo>,
}

pub struct EightBitDoPro3 {
    device: HidDevice,
    product_id: u16,
    serial: String,
    identity: String,
    is_bluetooth: bool,
    update_mode: Arc<AtomicBool>,
    last_battery: Option<BatteryInfo>,
}

impl EightBitDoPro3 {
    pub fn new(api: &HidApi, selector: Option<&str>) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut candidates: Vec<&DeviceInfo> = api
            .device_list()
            .filter(|info| {
                is_eightbitdo_vid(info.vendor_id()) && is_supported_pid(info.product_id())
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
                    "8BitDo Pro 3 '{}' not found. Check the USB-C cable and D mode switch.",
                    selector
                );
            }
        } else if candidates.is_empty() {
            bail!("No 8BitDo Pro 3 found. Connect it via USB-C in D mode.");
        }

        candidates.sort_by_key(|info| (info.interface_number() == -1, info.interface_number()));

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
            return Ok(Self {
                device,
                product_id,
                serial,
                identity,
                is_bluetooth: info.interface_number() == -1,
                update_mode: Arc::new(AtomicBool::new(false)),
                last_battery: None,
            });
        }

        if let Some(error) = last_error {
            return Err(anyhow::Error::new(error).context("Cannot open 8BitDo Pro 3"));
        }
        bail!("No 8BitDo Pro 3 found. Connect it via USB-C in D mode.")
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn vendor_id(&self) -> u16 {
        EIGHTBITDO_VID
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn is_bluetooth(&self) -> bool {
        self.is_bluetooth
    }

    pub fn set_update_mode(&self, active: bool) {
        self.update_mode.store(active, Ordering::SeqCst);
    }

    fn read_report(&self, timeout_ms: i32, buf: &mut [u8]) -> Result<usize> {
        self.device
            .read_timeout(buf, timeout_ms)
            .context("Failed to read 8BitDo Pro 3 input report")
    }

    fn observe(&mut self, data: &[u8]) -> Option<ControllerState> {
        if let Some(battery) = battery_from_report(data) {
            self.last_battery = Some(battery);
        }
        match decode_report(data) {
            DecodedReport::State(state) => Some(*state),
            DecodedReport::Ignored | DecodedReport::Malformed => None,
        }
    }

    pub fn poll_latest_input(&mut self) -> Result<Option<PolledEightBitDoInput>> {
        if self.update_mode.load(Ordering::Relaxed) {
            return Ok(None);
        }
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
        Ok(latest_state.map(|state| PolledEightBitDoInput {
            state,
            battery: self.last_battery.clone(),
        }))
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        if self.update_mode.load(Ordering::Relaxed) {
            bail!("Controller is in update mode");
        }
        let mut buf = [0u8; 64];
        for _ in 0..4 {
            let n = self.read_report(16, &mut buf)?;
            if n == 0 {
                continue;
            }
            if let Some(state) = self.observe(&buf[..n]) {
                return Ok(state);
            }
        }
        bail!("Timeout reading 8BitDo Pro 3 state")
    }

    pub fn get_battery(&mut self) -> Result<BatteryInfo> {
        if let Some(info) = self.last_battery.clone() {
            return Ok(info);
        }
        let mut buf = [0u8; 64];
        for _ in 0..16 {
            let n = self.read_report(0, &mut buf)?;
            if n == 0 {
                break;
            }
            if let Some(battery) = battery_from_report(&buf[..n]) {
                self.last_battery = Some(battery.clone());
                return Ok(battery);
            }
        }
        bail!("No 8BitDo Pro 3 battery report received yet")
    }

    pub fn get_firmware_info(&self) -> Result<(u16, String, String)> {
        bail!("Firmware information is not supported for the 8BitDo Pro 3")
    }

    fn write_report(&mut self, report: &[u8]) -> Result<()> {
        let written = self
            .device
            .write(report)
            .context("Failed to send 8BitDo Pro 3 output report")?;
        if written != report.len() {
            bail!(
                "Short 8BitDo Pro 3 output write: {written} of {} bytes",
                report.len()
            );
        }
        Ok(())
    }

    pub fn set_motion(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn force_motion(&mut self, _enabled: bool) -> Result<()> {
        Ok(())
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

fn device_matches(info: &DeviceInfo, selector: &str) -> bool {
    info.serial_number().is_some_and(|s| s == selector) || info.path().to_string_lossy() == selector
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: u8) -> Vec<u8> {
        let mut data = vec![0u8; REPORT_LEN];
        data[0] = id;
        data[1] = 0x0f;
        data[2..6].fill(0x7f);
        data
    }

    fn set_i16(data: &mut [u8], offset: usize, value: i16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn state(data: Vec<u8>) -> ControllerState {
        match decode_report(&data) {
            DecodedReport::State(state) => *state,
            other => panic!("expected state, got {other:?}"),
        }
    }

    #[test]
    fn decodes_all_face_shoulder_and_system_bits() {
        for (byte, bit, expected) in [
            (8, 0, BTN_CROSS),
            (8, 1, BTN_CIRCLE),
            (8, 3, BTN_SQUARE),
            (8, 4, BTN_TRIANGLE),
            (8, 6, BTN_L1),
            (8, 7, BTN_R1),
            (9, 0, BTN_L2),
            (9, 1, BTN_R2),
            (9, 2, BTN_CREATE),
            (9, 3, BTN_OPTIONS),
            (9, 4, BTN_PS),
            (9, 5, BTN_L3),
            (9, 6, BTN_R3),
        ] {
            let mut data = report(REPORT_INPUT);
            data[byte] = 1 << bit;
            assert_eq!(state(data).buttons, expected, "byte {byte} bit {bit}");
        }
    }

    #[test]
    fn decodes_all_extra_buttons_without_reusing_sony_or_steam_masks() {
        for (byte, bit, expected) in [
            (8, 5, EXTRA_BTN_EIGHTBITDO_PL),
            (8, 2, EXTRA_BTN_EIGHTBITDO_PR),
            (10, 0, EXTRA_BTN_EIGHTBITDO_L4),
            (10, 1, EXTRA_BTN_EIGHTBITDO_R4),
        ] {
            let mut data = report(REPORT_INPUT);
            data[byte] = 1 << bit;
            let s = state(data);
            assert_eq!(s.extra_buttons, expected, "byte {byte} bit {bit}");
            assert_eq!(s.buttons, 0);
        }
    }

    #[test]
    fn decodes_dpad_eight_way_and_neutral() {
        for (nibble, expected) in [
            (0, DPAD_N),
            (1, DPAD_NE),
            (2, DPAD_E),
            (3, DPAD_SE),
            (4, DPAD_S),
            (5, DPAD_SW),
            (6, DPAD_W),
            (7, DPAD_NW),
            (8, DPAD_NEUTRAL),
            (0x0f, DPAD_NEUTRAL),
        ] {
            let mut data = report(REPORT_INPUT);
            data[1] = nibble;
            assert_eq!(state(data).dpad, expected, "nibble {nibble:#x}");
        }
    }

    #[test]
    fn sticks_center_on_128_and_preserve_endpoints() {
        let mut data = report(REPORT_INPUT);
        data[2..6].fill(0x00);
        let s = state(data);
        assert_eq!((s.left_x, s.left_y, s.right_x, s.right_y), (0, 0, 0, 0));

        let mut data = report(REPORT_INPUT);
        data[2..6].fill(0xff);
        let s = state(data);
        assert_eq!(
            (s.left_x, s.left_y, s.right_x, s.right_y),
            (255, 255, 255, 255)
        );

        let s = state(report(REPORT_INPUT));
        assert_eq!(
            (s.left_x, s.left_y, s.right_x, s.right_y),
            (128, 128, 128, 128)
        );

        let mut data = report(REPORT_INPUT);
        data[2] = 0x3f;
        data[3] = 0xbf;
        let s = state(data);
        assert!(s.left_x < 128);
        assert!(s.left_y > 128);
    }

    #[test]
    fn triggers_follow_sdl_byte_order() {
        let mut data = report(REPORT_INPUT);
        data[6] = 200; // R2
        data[7] = 42; // L2
        let s = state(data);
        assert_eq!(s.r2, 200);
        assert_eq!(s.l2, 42);
    }

    #[test]
    fn battery_reports_charging_discharging_and_full() {
        let mut data = report(REPORT_INPUT);
        data[14] = 55;
        assert_eq!(
            battery_from_report(&data),
            Some(BatteryInfo {
                capacity: 55,
                status: "Discharging".to_string(),
            })
        );

        data[14] = 0x80 | 55;
        assert_eq!(
            battery_from_report(&data),
            Some(BatteryInfo {
                capacity: 55,
                status: "Charging".to_string(),
            })
        );

        data[14] = 100;
        assert_eq!(
            battery_from_report(&data),
            Some(BatteryInfo {
                capacity: 100,
                status: "Full".to_string(),
            })
        );

        data[14] = 0x80 | 100;
        assert_eq!(
            battery_from_report(&data),
            Some(BatteryInfo {
                capacity: 100,
                status: "Full".to_string(),
            })
        );
    }

    #[test]
    fn accepts_wired_and_bluetooth_report_ids() {
        for id in [REPORT_INPUT, REPORT_INPUT_BLUETOOTH] {
            let mut data = report(id);
            data[8] = 1;
            assert_eq!(state(data).buttons, BTN_CROSS);
        }
    }

    #[test]
    fn rejects_short_and_unknown_reports() {
        assert_eq!(decode_report(&[]), DecodedReport::Ignored);
        assert_eq!(decode_report(&[0x99; REPORT_LEN]), DecodedReport::Ignored);
        assert_eq!(
            decode_report(&report(REPORT_INPUT)[..20]),
            DecodedReport::Malformed
        );
        assert_eq!(
            decode_report(&report(REPORT_INPUT)[..REPORT_LEN - 1]),
            DecodedReport::Malformed
        );
        assert!(matches!(
            decode_report(&report(REPORT_INPUT)),
            DecodedReport::State(_)
        ));
    }

    #[test]
    fn rumble_packet_uses_low_and_high_frequency_bytes() {
        assert_eq!(rumble_report(255, 0x40), [0x05, 255, 0x40, 0, 0]);
    }

    #[test]
    fn applies_sdl_sensor_orientation_and_scaling() {
        let mut data = report(REPORT_INPUT);
        set_i16(&mut data, 15, 100); // accel X
        set_i16(&mut data, 17, 200); // accel Y
        set_i16(&mut data, 19, 300); // accel Z
        set_i16(&mut data, 21, 4000); // gyro X
        set_i16(&mut data, 23, 5000); // gyro Y
        set_i16(&mut data, 25, 6000); // gyro Z
        set_u32(&mut data, 27, 1_234_567);
        let s = state(data);

        assert_eq!(s.accel, [-200, 300, -100]);
        assert_eq!(s.gyro, [-5000, 6000, -4000]);
        assert_eq!(s.sensor_timestamp, 1_234_567);
        assert_eq!(s.gyro_counts_per_dps, 32768.0 / 2000.0);
        assert_eq!(s.accel_counts_per_g, 4096.0);
        assert_eq!(s.timestamp_unit_us, 1.0);
    }

    #[test]
    fn share_button_is_not_a_remappable_input() {
        let mut data = report(REPORT_INPUT);
        data[9] = 1 << 7;
        let s = state(data);
        assert_eq!(s.buttons, 0);
        assert_eq!(s.extra_buttons, 0);
    }

    #[test]
    fn classifies_receiver_and_unknown_models_as_skipped() {
        assert!(is_supported_pid(EIGHTBITDO_PRO3_PID));
        assert!(skip_reason(EIGHTBITDO_PRO3_PID).is_none());
        for pid in [
            EIGHTBITDO_RECEIVER_DOCKED_PID,
            EIGHTBITDO_RECEIVER_XINPUT_PID,
        ] {
            let reason = skip_reason(pid).expect("receiver skipped");
            assert!(reason.contains("2.4G"));
            assert!(reason.contains("D mode"));
        }
        assert!(skip_reason(0x1234).is_some());
        assert!(!is_eightbitdo_vid(0x054c));
    }
}
