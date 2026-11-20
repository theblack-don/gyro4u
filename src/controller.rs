use std::{
    fs::File,
    io::Read,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::atomic::AtomicU64,
    time::Duration,
};

use anyhow::{Result, anyhow, bail};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};

use crate::{
    common::{BatteryInfo, DS_PID, DS_VID, DSE_PID, MicLedState},
    dualsense::{DualSense, HAPTICS_PACKET_SAMPLES},
    inputs::{ControllerCapabilities, ControllerKind, ControllerState},
    settings::ControllerPreference,
    steam_controller::{self, SteamController, SteamPadHaptic, SteamPadSide},
};

#[derive(Clone, Debug)]
pub struct ControllerInfo {
    pub serial: String,
    pub identity: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub kind: ControllerKind,
    pub is_bluetooth: bool,
    pub transport: String,
}

pub struct PolledInput {
    pub state: ControllerState,
    pub battery: Option<BatteryInfo>,
    pub disconnected: bool,
}

pub enum ControllerInner {
    DualSense(DualSense),
    Steam(SteamController),
}

pub struct PhysicalController {
    inner: ControllerInner,
    info: ControllerInfo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerFamily {
    DualSense,
    Steam,
}

impl ControllerFamily {
    pub fn is_steam(self) -> bool {
        matches!(self, Self::Steam)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DualSense => "DualSense",
            Self::Steam => "Steam Controller",
        }
    }

    pub fn preference(self) -> ControllerPreference {
        match self {
            Self::DualSense => ControllerPreference::DualSense,
            Self::Steam => ControllerPreference::Steam,
        }
    }
}

pub fn family_of_kind(kind: ControllerKind) -> ControllerFamily {
    if kind.is_steam() {
        ControllerFamily::Steam
    } else {
        ControllerFamily::DualSense
    }
}

/// The family explicitly requested by a non-Auto preference.
pub fn requested_family(preference: ControllerPreference) -> Option<ControllerFamily> {
    match preference {
        ControllerPreference::Auto => None,
        ControllerPreference::DualSense => Some(ControllerFamily::DualSense),
        ControllerPreference::Steam => Some(ControllerFamily::Steam),
    }
}

pub fn family_present(
    family: ControllerFamily,
    dualsense_present: bool,
    steam_present: bool,
) -> bool {
    match family {
        ControllerFamily::DualSense => dualsense_present,
        ControllerFamily::Steam => steam_present,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceClass {
    Controller(ControllerFamily),
    SkippedSteamInterface,
    Unrelated,
}

/// Pure HID classification: only controller interfaces become candidates.
pub fn classify_device(vendor_id: u16, product_id: u16, interface_number: i32) -> DeviceClass {
    if vendor_id == DS_VID && matches!(product_id, DS_PID | DSE_PID) {
        return DeviceClass::Controller(ControllerFamily::DualSense);
    }
    if steam_controller::is_steam_vid(vendor_id) && steam_controller::is_steam_pid(product_id) {
        return if steam_controller::is_controller_interface(product_id, interface_number) {
            DeviceClass::Controller(ControllerFamily::Steam)
        } else {
            DeviceClass::SkippedSteamInterface
        };
    }
    DeviceClass::Unrelated
}

/// Resolve a target family from what is present. Auto uses DualSense first
/// when both families are present; an explicit preference never substitutes
/// the other family when its target is absent.
pub fn select_family(
    preference: ControllerPreference,
    dualsense_present: bool,
    steam_present: bool,
) -> Option<ControllerFamily> {
    match preference {
        ControllerPreference::Auto => match (dualsense_present, steam_present) {
            (false, false) => None,
            (true, false) => Some(ControllerFamily::DualSense),
            (false, true) => Some(ControllerFamily::Steam),
            (true, true) => Some(ControllerFamily::DualSense),
        },
        ControllerPreference::DualSense => dualsense_present.then_some(ControllerFamily::DualSense),
        ControllerPreference::Steam => steam_present.then_some(ControllerFamily::Steam),
    }
}

/// Target to switch to, or `None` when the current family already satisfies
/// the preference or the requested family is not present.
pub fn switch_target(
    preference: ControllerPreference,
    current: Option<ControllerFamily>,
    dualsense_present: bool,
    steam_present: bool,
) -> Option<ControllerFamily> {
    let target = select_family(preference, dualsense_present, steam_present)?;
    (current != Some(target)).then_some(target)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedDevice {
    pub family: ControllerFamily,
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface_number: i32,
    pub serial: Option<String>,
    pub identity: String,
    pub path: String,
    pub transport: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface_number: i32,
    pub serial: Option<String>,
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HidEntry {
    vendor_id: u16,
    product_id: u16,
    interface_number: i32,
    serial: Option<String>,
    path: String,
}

/// hidapi reports one entry per HID usage, so the same physical interface can
/// appear several times. Keep one entry per interface, preserving any serial.
fn dedup_entries(entries: impl IntoIterator<Item = HidEntry>) -> Vec<HidEntry> {
    let mut unique: Vec<HidEntry> = Vec::new();
    for entry in entries {
        match unique.iter_mut().find(|existing| {
            existing.path == entry.path && existing.interface_number == entry.interface_number
        }) {
            Some(existing) => {
                if existing.serial.is_none() {
                    existing.serial = entry.serial;
                }
            }
            None => unique.push(entry),
        }
    }
    unique
}

pub fn scan_devices(api: &HidApi) -> (Vec<DetectedDevice>, Vec<SkippedDevice>) {
    let mut devices = Vec::new();
    let mut skipped = Vec::new();

    let entries = dedup_entries(api.device_list().map(|info| {
        HidEntry {
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            interface_number: info.interface_number(),
            serial: info
                .serial_number()
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            path: info.path().to_string_lossy().to_string(),
        }
    }));

    for entry in entries {
        match classify_device(entry.vendor_id, entry.product_id, entry.interface_number) {
            DeviceClass::Controller(family) => {
                let identity = entry.serial.clone().unwrap_or_else(|| entry.path.clone());
                let transport = match family {
                    ControllerFamily::DualSense => {
                        if entry.interface_number == -1 {
                            "Bluetooth".to_string()
                        } else {
                            "USB".to_string()
                        }
                    }
                    ControllerFamily::Steam => {
                        steam_controller::transport_for(entry.product_id, entry.interface_number)
                            .label()
                    }
                };
                devices.push(DetectedDevice {
                    family,
                    vendor_id: entry.vendor_id,
                    product_id: entry.product_id,
                    interface_number: entry.interface_number,
                    serial: entry.serial,
                    identity,
                    path: entry.path,
                    transport,
                });
            }
            DeviceClass::SkippedSteamInterface => {
                let reason = format!(
                    "interface {} is not a controller slot",
                    entry.interface_number
                );
                skipped.push(SkippedDevice {
                    vendor_id: entry.vendor_id,
                    product_id: entry.product_id,
                    interface_number: entry.interface_number,
                    serial: entry.serial,
                    path: entry.path,
                    reason,
                });
            }
            DeviceClass::Unrelated => {}
        }
    }

    devices.sort_by_key(|d| {
        (
            d.family.is_steam(),
            d.product_id,
            d.interface_number,
            d.path.clone(),
        )
    });
    skipped.sort_by_key(|d| (d.product_id, d.interface_number, d.path.clone()));
    (devices, skipped)
}

static LAST_SKIPPED_STEAM_LOG: AtomicU64 = AtomicU64::new(0);

pub fn present_families(api: &HidApi, selector: Option<&str>) -> (bool, bool) {
    let (devices, skipped) = scan_devices(api);

    if !skipped.is_empty()
        && crate::util::rate_limited_gate(&LAST_SKIPPED_STEAM_LOG, Duration::from_secs(60))
    {
        for device in &skipped {
            eprintln!(
                "[gyro4u] skipped Steam device {:04x}:{:04x} interface {} ({}): {}",
                device.vendor_id,
                device.product_id,
                device.interface_number,
                device.path,
                device.reason
            );
        }
    }

    let matches = |device: &DetectedDevice| match selector {
        None => true,
        Some(selector) => match device.family {
            ControllerFamily::DualSense => device.serial.as_deref() == Some(selector),
            ControllerFamily::Steam => {
                device.serial.as_deref() == Some(selector) || device.path == selector
            }
        },
    };

    let dualsense = devices
        .iter()
        .any(|d| d.family == ControllerFamily::DualSense && matches(d));
    let steam = devices
        .iter()
        .any(|d| d.family == ControllerFamily::Steam && matches(d));
    (dualsense, steam)
}

/// A family-level view for the top-bar switcher. Steam puck/receiver slots
/// collapse into one row; a controller is live when wired/BLE is present, the
/// family is active, or a safe probe observed a live receiver slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyRow {
    pub family: ControllerFamily,
    pub active: bool,
    pub live: bool,
    pub receiver_only: bool,
    pub transport: Option<String>,
    pub identity: Option<String>,
}

/// Identity of a Steam puck/receiver slot observed streaming input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamLiveSlot {
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface_number: i32,
    pub path: String,
    pub serial: Option<String>,
    pub identity: String,
    pub transport: String,
}

/// Cached result of the last safe receiver probe.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamLiveness {
    pub receiver_present: bool,
    pub live: Option<SteamLiveSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SteamSlotState {
    Live(SteamLiveSlot),
    NotLive,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamProbeResult {
    pub receiver_present: bool,
    pub state: SteamSlotState,
}

const STEAM_PROBE_TIMEOUT_MS: i32 = 35;

fn is_receiver_slot(device: &DetectedDevice) -> bool {
    device.family == ControllerFamily::Steam
        && matches!(
            device.product_id,
            steam_controller::STEAM_PID_PUCK | steam_controller::STEAM_PID_RECEIVER
        )
        && (2..=5).contains(&device.interface_number)
}

impl SteamLiveSlot {
    fn from_device(device: &DetectedDevice) -> Self {
        Self {
            vendor_id: device.vendor_id,
            product_id: device.product_id,
            interface_number: device.interface_number,
            path: device.path.clone(),
            serial: device.serial.clone(),
            identity: device.identity.clone(),
            transport: device.transport.clone(),
        }
    }
}

/// Read-only liveness probe for idle Steam puck/receiver slots. It opens every
/// candidate slot non-blocking and polls them together, so one 35 ms window
/// covers all slots. It never writes feature/output reports.
pub fn probe_steam_slots(api: &HidApi) -> SteamProbeResult {
    let (devices, _) = scan_devices(api);
    let candidates: Vec<&DetectedDevice> = devices.iter().filter(|d| is_receiver_slot(d)).collect();
    if candidates.is_empty() {
        return SteamProbeResult {
            receiver_present: false,
            state: SteamSlotState::Unknown,
        };
    }

    let mut opened: Vec<(File, &DetectedDevice)> = Vec::new();
    for device in candidates {
        let result = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&device.path);
        if let Ok(file) = result {
            opened.push((file, device));
        }
    }
    if opened.is_empty() {
        return SteamProbeResult {
            receiver_present: true,
            state: SteamSlotState::Unknown,
        };
    }

    let mut pollfds: Vec<libc::pollfd> = opened
        .iter()
        .map(|(file, _)| libc::pollfd {
            fd: file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    let ready = loop {
        // SAFETY: pollfds is a valid, exclusively borrowed array for the
        // duration of the call and contains only open file descriptors.
        let ready = unsafe {
            libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                STEAM_PROBE_TIMEOUT_MS,
            )
        };
        if ready >= 0 {
            break ready;
        }
        if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return SteamProbeResult {
            receiver_present: true,
            state: SteamSlotState::Unknown,
        };
    };
    if ready == 0 {
        return SteamProbeResult {
            receiver_present: true,
            state: SteamSlotState::Unknown,
        };
    }

    let mut state = SteamSlotState::Unknown;
    for (index, pfd) in pollfds.iter().enumerate() {
        let (file, device) = &mut opened[index];
        if pfd.revents & libc::POLLIN == 0 {
            continue;
        }
        let mut buf = [0u8; 128];
        let read = match file.read(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        if read == 0 {
            continue;
        }
        match steam_controller::report_liveness(&buf[..read]) {
            steam_controller::ReportLiveness::Live => {
                return SteamProbeResult {
                    receiver_present: true,
                    state: SteamSlotState::Live(SteamLiveSlot::from_device(device)),
                };
            }
            steam_controller::ReportLiveness::NotLive => state = SteamSlotState::NotLive,
            steam_controller::ReportLiveness::Unknown => {}
        }
    }

    SteamProbeResult {
        receiver_present: true,
        state,
    }
}

fn identity_matches(device: &DetectedDevice, identity: Option<&str>) -> bool {
    identity.is_some_and(|identity| {
        device.identity.as_str() == identity
            || device.serial.as_deref() == Some(identity)
            || device.path == identity
    })
}

fn steam_device_is_direct(device: &DetectedDevice) -> bool {
    matches!(
        device.product_id,
        steam_controller::STEAM_PID_WIRED | steam_controller::STEAM_PID_BLE
    )
}

pub fn inventory_rows(
    devices: &[DetectedDevice],
    active_family: Option<ControllerFamily>,
    active_identity: Option<&str>,
    steam_live: Option<&SteamLiveSlot>,
) -> Vec<FamilyRow> {
    let mut rows = Vec::new();

    let dualsense: Vec<&DetectedDevice> = devices
        .iter()
        .filter(|device| device.family == ControllerFamily::DualSense)
        .collect();
    if let Some(chosen) = dualsense
        .iter()
        .find(|device| identity_matches(device, active_identity))
        .or_else(|| dualsense.first())
    {
        rows.push(FamilyRow {
            family: ControllerFamily::DualSense,
            active: active_family == Some(ControllerFamily::DualSense),
            live: true,
            receiver_only: false,
            transport: Some(chosen.transport.clone()),
            identity: Some(chosen.identity.clone()),
        });
    }

    let steam: Vec<&DetectedDevice> = devices
        .iter()
        .filter(|device| device.family == ControllerFamily::Steam)
        .collect();
    let live_slot = steam_live.filter(|slot| {
        matches!(
            slot.product_id,
            steam_controller::STEAM_PID_PUCK | steam_controller::STEAM_PID_RECEIVER
        ) && (2..=5).contains(&slot.interface_number)
    });
    if !steam.is_empty() {
        let active = active_family == Some(ControllerFamily::Steam);
        let direct = steam.iter().find(|device| steam_device_is_direct(device));
        let active_direct = if active {
            steam.iter().find(|device| {
                steam_device_is_direct(device) && identity_matches(device, active_identity)
            })
        } else {
            None
        };
        let chosen = active_direct
            .or(direct)
            .or_else(|| {
                live_slot.and_then(|slot| steam.iter().find(|device| device.path == slot.path))
            })
            .or_else(|| {
                steam
                    .iter()
                    .find(|device| identity_matches(device, active_identity))
            })
            .or_else(|| steam.first())
            .expect("steam is not empty");
        let live = direct.is_some() || active || live_slot.is_some();
        let (transport, identity) = match live_slot.filter(|_| active_direct.is_none()) {
            Some(slot) if direct.is_none() => {
                (Some(slot.transport.clone()), Some(slot.identity.clone()))
            }
            _ => (
                Some(chosen.transport.clone()),
                Some(chosen.identity.clone()),
            ),
        };
        rows.push(FamilyRow {
            family: ControllerFamily::Steam,
            active,
            live,
            receiver_only: !live,
            transport,
            identity,
        });
    } else if let Some(slot) = live_slot {
        rows.push(FamilyRow {
            family: ControllerFamily::Steam,
            active: active_family == Some(ControllerFamily::Steam),
            live: true,
            receiver_only: false,
            transport: Some(slot.transport.clone()),
            identity: Some(slot.identity.clone()),
        });
    }

    rows
}

fn no_controller_error(selector: Option<&str>) -> anyhow::Error {
    match selector {
        Some(selector) => anyhow!(
            "No supported controller '{}' found. Connect a DualSense, DualSense Edge, or Steam \
             Controller (2026) via USB, Bluetooth, or a puck/receiver.",
            selector
        ),
        None => anyhow!(
            "No supported controller found. Connect a DualSense, DualSense Edge, or Steam \
             Controller (2026) via USB, Bluetooth, or a puck/receiver."
        ),
    }
}

impl PhysicalController {
    pub fn discover(
        api: &HidApi,
        selector: Option<&str>,
        preference: ControllerPreference,
    ) -> Result<Self> {
        let (dualsense_present, steam_present) = present_families(api, selector);
        let Some(preferred) = select_family(preference, dualsense_present, steam_present) else {
            if let Some(family) = requested_family(preference) {
                return Err(anyhow!(
                    "No supported controller of the requested family ({}) found. Connect a {} \
                     or choose Automatic (DualSense first).",
                    family.label(),
                    family.label()
                ));
            }
            return Err(no_controller_error(selector));
        };

        // Auto may fall back to the other family if opening the preferred one
        // fails. An explicit preference must never silently substitute.
        let families: Vec<ControllerFamily> = match requested_family(preference) {
            Some(_) => vec![preferred],
            None => vec![
                preferred,
                match preferred {
                    ControllerFamily::DualSense => ControllerFamily::Steam,
                    ControllerFamily::Steam => ControllerFamily::DualSense,
                },
            ],
        };

        let mut last_error = None;
        for family in families {
            let present = family_present(family, dualsense_present, steam_present);
            if !present {
                continue;
            }
            let opened = match family {
                ControllerFamily::DualSense => {
                    DualSense::new(api, selector).map(Self::from_dualsense)
                }
                ControllerFamily::Steam => {
                    SteamController::new(api, selector).map(Self::from_steam)
                }
            };
            match opened {
                Ok(controller) => return Ok(controller),
                Err(error) => last_error = Some(error),
            }
        }

        last_error.map_or_else(|| Err(no_controller_error(selector)), Err)
    }

    fn from_dualsense(device: DualSense) -> Self {
        let kind = if device.product_id() == DSE_PID {
            ControllerKind::DualSenseEdge
        } else {
            ControllerKind::DualSense
        };
        let info = ControllerInfo {
            serial: device.serial().to_string(),
            identity: device.serial().to_string(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            kind,
            is_bluetooth: device.is_bluetooth(),
            transport: if device.is_bluetooth() {
                "Bluetooth".to_string()
            } else {
                "USB".to_string()
            },
        };
        Self {
            inner: ControllerInner::DualSense(device),
            info,
        }
    }

    fn from_steam(device: SteamController) -> Self {
        let info = ControllerInfo {
            serial: device.serial().to_string(),
            identity: device.identity().to_string(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            kind: ControllerKind::SteamController,
            is_bluetooth: device.is_bluetooth(),
            transport: device.transport().label(),
        };
        Self {
            inner: ControllerInner::Steam(device),
            info,
        }
    }

    pub fn info(&self) -> &ControllerInfo {
        &self.info
    }

    pub fn kind(&self) -> ControllerKind {
        self.info.kind
    }

    pub fn capabilities(&self) -> ControllerCapabilities {
        self.info.kind.capabilities()
    }

    pub fn vendor_id(&self) -> u16 {
        self.info.vendor_id
    }

    pub fn product_id(&self) -> u16 {
        self.info.product_id
    }

    pub fn serial(&self) -> &str {
        &self.info.serial
    }

    pub fn is_bluetooth(&self) -> bool {
        self.info.is_bluetooth
    }

    pub fn poll_latest_input(&mut self) -> Result<Option<PolledInput>> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => {
                let polled = device.poll_latest_input()?;
                Ok(polled.map(|polled| PolledInput {
                    battery: Some(DualSense::battery_from_status(polled.status_byte)),
                    state: polled.state,
                    disconnected: false,
                }))
            }
            ControllerInner::Steam(device) => {
                let polled = device.poll_latest_input()?;
                Ok(polled.map(|polled| PolledInput {
                    state: polled.state,
                    battery: polled.battery,
                    disconnected: polled.disconnected,
                }))
            }
        }
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.get_input_state(),
            ControllerInner::Steam(device) => device.get_input_state(),
        }
    }

    pub fn get_battery(&mut self) -> Result<BatteryInfo> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.get_battery(),
            ControllerInner::Steam(device) => device.get_battery(),
        }
    }

    pub fn get_firmware_info(&self) -> Result<(u16, String, String)> {
        match &self.inner {
            ControllerInner::DualSense(device) => device.get_firmware_info(),
            ControllerInner::Steam(device) => device.get_firmware_info(),
        }
    }

    pub fn update_firmware(
        &mut self,
        firmware_data: &[u8],
        progress_callback: impl Fn(u32) + Send + 'static,
    ) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => {
                device.update_firmware(firmware_data, progress_callback)
            }
            ControllerInner::Steam(_) => {
                bail!("Firmware updates are not supported for the Steam Controller in gyro4u")
            }
        }
    }

    pub fn set_update_mode(&self, active: bool) {
        match &self.inner {
            ControllerInner::DualSense(device) => device.set_update_mode(active),
            ControllerInner::Steam(device) => device.set_update_mode(active),
        }
    }

    pub fn set_motion(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(_) => Ok(()),
            ControllerInner::Steam(device) => device.set_motion(enabled),
        }
    }

    pub fn force_motion(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(_) => Ok(()),
            ControllerInner::Steam(device) => device.force_motion(enabled),
        }
    }

    pub fn set_restore_on_drop(&mut self, enabled: bool) {
        if let ControllerInner::Steam(device) = &mut self.inner {
            device.set_restore_on_drop(enabled);
        }
    }

    pub fn prepare_handoff(&mut self) {
        if let ControllerInner::Steam(device) = &mut self.inner {
            device.prepare_handoff();
        }
    }

    pub fn set_lightbar(&mut self, r: u8, g: u8, b: u8, brightness: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_lightbar(r, g, b, brightness),
            ControllerInner::Steam(device) => device.set_lightbar(r, g, b, brightness),
        }
    }

    pub fn set_lightbar_enabled(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_lightbar_enabled(enabled),
            ControllerInner::Steam(device) => device.set_lightbar_enabled(enabled),
        }
    }

    pub fn set_player_leds(&mut self, leds: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_player_leds(leds),
            ControllerInner::Steam(device) => device.set_player_leds(leds),
        }
    }

    pub fn set_mic(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_mic(enabled),
            ControllerInner::Steam(device) => device.set_mic(enabled),
        }
    }

    pub fn set_mic_led(&mut self, state: MicLedState) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_mic_led(state),
            ControllerInner::Steam(device) => device.set_mic_led(state),
        }
    }

    pub fn set_trigger_effects(
        &mut self,
        left: Option<(u8, [u8; 10])>,
        right: Option<(u8, [u8; 10])>,
    ) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_trigger_effects(left, right),
            ControllerInner::Steam(device) => device.set_trigger_effects(left, right),
        }
    }

    pub fn set_trigger_effect(
        &mut self,
        left: bool,
        right: bool,
        mode: u8,
        params: &[u8],
    ) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => {
                device.set_trigger_effect(left, right, mode, params)
            }
            ControllerInner::Steam(device) => device.set_trigger_effect(left, right, mode, params),
        }
    }

    pub fn set_trigger_off(&mut self) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_trigger_off(),
            ControllerInner::Steam(device) => device.set_trigger_off(),
        }
    }

    pub fn set_vibration(&mut self, rumble: u8, trigger: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_vibration(rumble, trigger),
            ControllerInner::Steam(device) => device.set_vibration(rumble, trigger),
        }
    }

    pub fn set_rumble(&mut self, left: u8, right: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_rumble(left, right),
            ControllerInner::Steam(device) => device.set_rumble(left, right),
        }
    }

    pub fn set_haptics(&mut self, samples: &[i8; HAPTICS_PACKET_SAMPLES]) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_haptics(samples),
            ControllerInner::Steam(device) => device.set_haptics(samples),
        }
    }

    pub fn steam_pad_haptic(
        &mut self,
        side: SteamPadSide,
        haptic: SteamPadHaptic,
        gain_db: i8,
    ) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(_) => {
                bail!("Steam Controller pad haptics are not supported on DualSense")
            }
            ControllerInner::Steam(device) => device.haptic_command(side, haptic, gain_db),
        }
    }

    pub fn set_speaker(&mut self, mode: &str) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_speaker(mode),
            ControllerInner::Steam(device) => device.set_speaker(mode),
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_volume(volume),
            ControllerInner::Steam(device) => device.set_volume(volume),
        }
    }
}

pub fn is_present(api: &HidApi, identity: &str) -> bool {
    api.device_list().any(|info| {
        let is_dualsense =
            info.vendor_id() == DS_VID && matches!(info.product_id(), DS_PID | DSE_PID);
        let is_steam = steam_controller::is_steam_vid(info.vendor_id())
            && steam_controller::is_steam_pid(info.product_id())
            && steam_controller::is_controller_interface(
                info.product_id(),
                info.interface_number(),
            );
        if !is_dualsense && !is_steam {
            return false;
        }
        info.serial_number()
            .is_some_and(|serial| serial == identity)
            || info.path().to_string_lossy() == identity
    })
}

pub fn list_devices(api: &HidApi) -> Vec<String> {
    let (devices, _) = scan_devices(api);
    devices
        .into_iter()
        .map(|device| format!("{} ({})", device.identity, device.transport))
        .collect()
}

pub fn format_device_report(api: &HidApi, preference: ControllerPreference) -> String {
    let (devices, skipped) = scan_devices(api);
    let mut out = String::new();

    if devices.is_empty() {
        out.push_str("No supported controllers detected.\n");
    } else {
        out.push_str("Detected controllers:\n");
        for device in &devices {
            out.push_str(&format!(
                "  {:<27} {:04x}:{:04x}  {:<30} {}\n",
                device.family.label(),
                device.vendor_id,
                device.product_id,
                device.transport,
                device.identity
            ));
            out.push_str(&format!("    path: {}\n", device.path));
        }
    }

    if !skipped.is_empty() {
        out.push_str("Skipped:\n");
        for device in &skipped {
            let identity = device.serial.as_deref().unwrap_or(device.path.as_str());
            out.push_str(&format!(
                "  {:04x}:{:04x} interface {}  {}: {}\n",
                device.vendor_id,
                device.product_id,
                device.interface_number,
                identity,
                device.reason
            ));
        }
    }

    let dualsense = devices
        .iter()
        .any(|device| device.family == ControllerFamily::DualSense);
    let steam = devices
        .iter()
        .any(|device| device.family == ControllerFamily::Steam);

    out.push_str(&format!("Preference: {}\n", preference.label()));
    match select_family(preference, dualsense, steam) {
        Some(family) => out.push_str(&format!("Would connect: {}\n", family.label())),
        None => out.push_str("Would connect: nothing\n"),
    }
    if steam {
        out.push_str(
            "Note: puck/receiver slots are probed for a live controller at connect time.\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_selects_steam_when_only_steam_present() {
        assert_eq!(
            select_family(ControllerPreference::Auto, false, true),
            Some(ControllerFamily::Steam)
        );
    }

    #[test]
    fn auto_selects_dualsense_when_only_dualsense_present() {
        assert_eq!(
            select_family(ControllerPreference::Auto, true, false),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn auto_prefers_dualsense_when_both_present() {
        assert_eq!(
            select_family(ControllerPreference::Auto, true, true),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn steam_preference_wins_when_both_present() {
        assert_eq!(
            select_family(ControllerPreference::Steam, true, true),
            Some(ControllerFamily::Steam)
        );
    }

    #[test]
    fn dualsense_preference_wins_when_both_present() {
        assert_eq!(
            select_family(ControllerPreference::DualSense, true, true),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn explicit_preference_does_not_substitute_the_other_family() {
        assert_eq!(
            select_family(ControllerPreference::Steam, true, false),
            None
        );
        assert_eq!(
            select_family(ControllerPreference::DualSense, false, true),
            None
        );
    }

    #[test]
    fn switch_target_absent_target_keeps_current() {
        assert_eq!(
            switch_target(
                ControllerPreference::Steam,
                Some(ControllerFamily::DualSense),
                true,
                false
            ),
            None
        );
    }

    #[test]
    fn switch_target_present_and_different_requests_switch() {
        assert_eq!(
            switch_target(
                ControllerPreference::Steam,
                Some(ControllerFamily::DualSense),
                true,
                true
            ),
            Some(ControllerFamily::Steam)
        );
        assert_eq!(
            switch_target(
                ControllerPreference::DualSense,
                Some(ControllerFamily::Steam),
                true,
                true
            ),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn switch_target_same_family_is_a_no_op() {
        assert_eq!(
            switch_target(
                ControllerPreference::Steam,
                Some(ControllerFamily::Steam),
                true,
                true
            ),
            None
        );
        assert_eq!(
            switch_target(
                ControllerPreference::Auto,
                Some(ControllerFamily::DualSense),
                true,
                true
            ),
            None
        );
    }

    #[test]
    fn auto_switch_prefers_dualsense_first() {
        assert_eq!(
            switch_target(
                ControllerPreference::Auto,
                Some(ControllerFamily::Steam),
                true,
                true
            ),
            Some(ControllerFamily::DualSense)
        );
        assert_eq!(
            switch_target(
                ControllerPreference::Auto,
                Some(ControllerFamily::DualSense),
                true,
                true
            ),
            None
        );
    }

    #[test]
    fn requested_family_is_none_only_for_auto() {
        assert_eq!(requested_family(ControllerPreference::Auto), None);
        assert_eq!(
            requested_family(ControllerPreference::DualSense),
            Some(ControllerFamily::DualSense)
        );
        assert_eq!(
            requested_family(ControllerPreference::Steam),
            Some(ControllerFamily::Steam)
        );
    }

    #[test]
    fn family_of_kind_maps_edge_to_dualsense_family() {
        assert_eq!(
            family_of_kind(ControllerKind::DualSenseEdge),
            ControllerFamily::DualSense
        );
        assert_eq!(
            family_of_kind(ControllerKind::SteamController),
            ControllerFamily::Steam
        );
    }

    #[test]
    fn no_family_selected_when_nothing_is_present() {
        for preference in [
            ControllerPreference::Auto,
            ControllerPreference::DualSense,
            ControllerPreference::Steam,
        ] {
            assert_eq!(select_family(preference, false, false), None);
        }
    }

    #[test]
    fn puck_and_receiver_only_accept_controller_slots() {
        use crate::steam_controller::{
            STEAM_PID_BLE, STEAM_PID_PUCK, STEAM_PID_RECEIVER, STEAM_PID_WIRED, STEAM_VID,
        };

        for product_id in [STEAM_PID_PUCK, STEAM_PID_RECEIVER] {
            for slot in 2..=5 {
                assert_eq!(
                    classify_device(STEAM_VID, product_id, slot),
                    DeviceClass::Controller(ControllerFamily::Steam)
                );
            }
            for slot in [-1, 0, 1, 6] {
                assert_eq!(
                    classify_device(STEAM_VID, product_id, slot),
                    DeviceClass::SkippedSteamInterface
                );
            }
        }
        for product_id in [STEAM_PID_WIRED, STEAM_PID_BLE] {
            for interface in [-1, 0, 3] {
                assert_eq!(
                    classify_device(STEAM_VID, product_id, interface),
                    DeviceClass::Controller(ControllerFamily::Steam)
                );
            }
        }
        assert_eq!(classify_device(0x1234, 0x5678, 0), DeviceClass::Unrelated);
    }

    fn hid_entry(interface_number: i32, path: &str, serial: Option<&str>) -> HidEntry {
        HidEntry {
            vendor_id: crate::steam_controller::STEAM_VID,
            product_id: crate::steam_controller::STEAM_PID_PUCK,
            interface_number,
            serial: serial.map(str::to_string),
            path: path.to_string(),
        }
    }

    #[test]
    fn duplicate_usage_entries_collapse_to_one_per_interface() {
        let entries = dedup_entries([
            hid_entry(2, "/dev/hidraw11", None),
            hid_entry(2, "/dev/hidraw11", Some("FXB9961001F79")),
            hid_entry(2, "/dev/hidraw11", None),
            hid_entry(3, "/dev/hidraw13", None),
            hid_entry(3, "/dev/hidraw13", None),
            hid_entry(6, "/dev/hidraw17", Some("FXB9961001F79")),
            hid_entry(6, "/dev/hidraw17", Some("FXB9961001F79")),
        ]);

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.path.as_str(), entry.interface_number))
                .collect::<Vec<_>>(),
            [
                ("/dev/hidraw11", 2),
                ("/dev/hidraw13", 3),
                ("/dev/hidraw17", 6),
            ]
        );
        assert_eq!(entries[0].serial.as_deref(), Some("FXB9961001F79"));
    }

    fn detected(
        family: ControllerFamily,
        product_id: u16,
        interface_number: i32,
        serial: Option<&str>,
        path: &str,
    ) -> DetectedDevice {
        DetectedDevice {
            family,
            vendor_id: match family {
                ControllerFamily::DualSense => DS_VID,
                ControllerFamily::Steam => crate::steam_controller::STEAM_VID,
            },
            product_id,
            interface_number,
            serial: serial.map(str::to_string),
            identity: serial
                .map(str::to_string)
                .unwrap_or_else(|| path.to_string()),
            path: path.to_string(),
            transport: if family == ControllerFamily::Steam {
                steam_controller::transport_for(product_id, interface_number).label()
            } else if interface_number == -1 {
                "Bluetooth".to_string()
            } else {
                "USB".to_string()
            },
        }
    }

    fn steam_puck(slot: i32) -> DetectedDevice {
        detected(
            ControllerFamily::Steam,
            crate::steam_controller::STEAM_PID_PUCK,
            slot,
            Some("FXB9961001F79"),
            &format!("/dev/hidraw1{slot}"),
        )
    }

    fn live_slot(slot: i32) -> SteamLiveSlot {
        let device = steam_puck(slot);
        SteamLiveSlot::from_device(&device)
    }

    #[test]
    fn receiver_live_slot_marks_steam_row_live() {
        let devices: Vec<_> = (2..=5).map(steam_puck).collect();
        let slot = live_slot(3);

        let rows = inventory_rows(&devices, None, None, Some(&slot));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].family, ControllerFamily::Steam);
        assert!(!rows[0].active);
        assert!(rows[0].live);
        assert!(!rows[0].receiver_only);
        assert_eq!(rows[0].transport.as_deref(), Some("Wireless puck slot 3"));
        assert_eq!(rows[0].identity.as_deref(), Some("FXB9961001F79"));
    }

    #[test]
    fn live_slot_without_scanned_receiver_still_adds_steam_row() {
        let slot = live_slot(4);

        let rows = inventory_rows(&[], None, None, Some(&slot));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].family, ControllerFamily::Steam);
        assert!(rows[0].live);
        assert_eq!(rows[0].transport.as_deref(), Some("Wireless puck slot 4"));
    }

    #[test]
    fn active_receiver_uses_the_live_slot_transport() {
        let devices: Vec<_> = (2..=5).map(steam_puck).collect();
        let slot = live_slot(4);

        let rows = inventory_rows(
            &devices,
            Some(ControllerFamily::Steam),
            Some("FXB9961001F79"),
            Some(&slot),
        );

        assert_eq!(rows.len(), 1);
        assert!(rows[0].active && rows[0].live);
        assert_eq!(rows[0].transport.as_deref(), Some("Wireless puck slot 4"));
    }

    #[test]
    fn direct_steam_presence_wins_over_receiver_evidence() {
        let wired = detected(
            ControllerFamily::Steam,
            crate::steam_controller::STEAM_PID_WIRED,
            0,
            Some("STEAM-SERIAL"),
            "/dev/hidraw9",
        );
        let slot = live_slot(3);

        let rows = inventory_rows(&[wired, steam_puck(3)], None, None, Some(&slot));

        assert_eq!(rows.len(), 1);
        assert!(rows[0].live);
        assert_eq!(rows[0].transport.as_deref(), Some("USB"));
        assert_eq!(rows[0].identity.as_deref(), Some("STEAM-SERIAL"));
    }

    #[test]
    fn live_slot_with_unrelated_product_id_is_ignored() {
        let devices: Vec<_> = (2..=5).map(steam_puck).collect();
        let mut slot = live_slot(3);
        slot.product_id = crate::steam_controller::STEAM_PID_WIRED;

        let rows = inventory_rows(&devices, None, None, Some(&slot));

        assert_eq!(rows.len(), 1);
        assert!(!rows[0].live);
        assert!(rows[0].receiver_only);
    }

    #[test]
    fn receiver_slot_candidates_are_puck_and_integrated_interfaces() {
        let mut devices: Vec<_> = (2..=5).map(steam_puck).collect();
        devices.push(detected(
            ControllerFamily::Steam,
            crate::steam_controller::STEAM_PID_RECEIVER,
            5,
            None,
            "/dev/hidraw25",
        ));
        devices.push(detected(
            ControllerFamily::Steam,
            crate::steam_controller::STEAM_PID_WIRED,
            0,
            None,
            "/dev/hidraw26",
        ));

        let candidates: Vec<_> = devices.iter().filter(|d| is_receiver_slot(d)).collect();

        assert_eq!(candidates.len(), 5);
        assert!(
            candidates
                .iter()
                .all(|d| (2..=5).contains(&d.interface_number))
        );
    }

    #[test]
    fn steam_puck_slots_collapse_to_one_family_row() {
        let devices: Vec<_> = (2..=5).map(steam_puck).collect();

        let rows = inventory_rows(&devices, None, None, None);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].family, ControllerFamily::Steam);
        assert!(!rows[0].active);
        assert!(!rows[0].live);
        assert!(rows[0].receiver_only);
    }

    #[test]
    fn active_steam_identity_is_flagged_live() {
        let devices: Vec<_> = (2..=5).map(steam_puck).collect();

        let rows = inventory_rows(
            &devices,
            Some(ControllerFamily::Steam),
            Some("FXB9961001F79"),
            None,
        );

        assert_eq!(rows.len(), 1);
        assert!(rows[0].active);
        assert!(rows[0].live);
        assert!(!rows[0].receiver_only);
        assert_eq!(rows[0].identity.as_deref(), Some("FXB9961001F79"));
    }

    #[test]
    fn healthy_dualsense_and_steam_puck_produce_two_rows() {
        let dualsense = detected(
            ControllerFamily::DualSense,
            DS_PID,
            3,
            Some("DS-SERIAL"),
            "/dev/hidraw0",
        );
        let devices = vec![dualsense, steam_puck(2)];

        let rows = inventory_rows(
            &devices,
            Some(ControllerFamily::DualSense),
            Some("DS-SERIAL"),
            None,
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].family, ControllerFamily::DualSense);
        assert!(rows[0].active && rows[0].live && !rows[0].receiver_only);
        assert_eq!(rows[1].family, ControllerFamily::Steam);
        assert!(!rows[1].active && !rows[1].live && rows[1].receiver_only);
    }

    #[test]
    fn wired_steam_controller_is_live() {
        let wired = detected(
            ControllerFamily::Steam,
            crate::steam_controller::STEAM_PID_WIRED,
            0,
            Some("STEAM-SERIAL"),
            "/dev/hidraw9",
        );

        let rows = inventory_rows(&[wired], None, None, None);

        assert_eq!(rows.len(), 1);
        assert!(rows[0].live);
        assert!(!rows[0].receiver_only);
        assert_eq!(rows[0].transport.as_deref(), Some("USB"));
    }

    #[test]
    fn deduplication_preserves_distinct_interfaces_and_devices() {
        let entries = dedup_entries([
            hid_entry(2, "/dev/hidraw11", None),
            hid_entry(3, "/dev/hidraw11", None),
            hid_entry(4, "/dev/hidraw11", None),
            hid_entry(5, "/dev/hidraw11", None),
            hid_entry(2, "/dev/hidraw12", None),
        ]);

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.path.as_str(), entry.interface_number))
                .collect::<Vec<_>>(),
            [
                ("/dev/hidraw11", 2),
                ("/dev/hidraw11", 3),
                ("/dev/hidraw11", 4),
                ("/dev/hidraw11", 5),
                ("/dev/hidraw12", 2),
            ]
        );
    }
}
