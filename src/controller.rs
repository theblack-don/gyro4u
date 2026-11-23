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
    apex5::{self, Apex5},
    common::{BatteryInfo, DS_PID, DS_VID, DSE_PID, MicLedState},
    dualsense::{DualSense, HAPTICS_PACKET_SAMPLES},
    eightbitdo::{self, EightBitDoPro3},
    gamesir::{self, GameSirTarantulaPro},
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
    EightBitDo(EightBitDoPro3),
    Flydigi(Apex5),
    GameSir(GameSirTarantulaPro),
}

pub struct PhysicalController {
    inner: ControllerInner,
    info: ControllerInfo,
}

pub use crate::inputs::ControllerFamily;

/// Presence snapshot for all supported families. Auto selection prefers
/// DualSense, then Steam, then 8BitDo, then Flydigi, then GameSir.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FamilyPresence {
    pub dualsense: bool,
    pub steam: bool,
    pub eightbitdo: bool,
    pub flydigi: bool,
    pub gamesir: bool,
}

impl FamilyPresence {
    pub fn present(self, family: ControllerFamily) -> bool {
        match family {
            ControllerFamily::DualSense => self.dualsense,
            ControllerFamily::Steam => self.steam,
            ControllerFamily::EightBitDo => self.eightbitdo,
            ControllerFamily::Flydigi => self.flydigi,
            ControllerFamily::GameSir => self.gamesir,
        }
    }

    pub fn ordered(self) -> Vec<ControllerFamily> {
        [
            ControllerFamily::DualSense,
            ControllerFamily::Steam,
            ControllerFamily::EightBitDo,
            ControllerFamily::Flydigi,
            ControllerFamily::GameSir,
        ]
        .into_iter()
        .filter(|family| self.present(*family))
        .collect()
    }
}

pub fn family_of_kind(kind: ControllerKind) -> ControllerFamily {
    kind.family()
}

/// The family explicitly requested by a non-Auto preference.
pub fn requested_family(preference: ControllerPreference) -> Option<ControllerFamily> {
    match preference {
        ControllerPreference::Auto => None,
        ControllerPreference::DualSense => Some(ControllerFamily::DualSense),
        ControllerPreference::Steam => Some(ControllerFamily::Steam),
        ControllerPreference::EightBitDo => Some(ControllerFamily::EightBitDo),
        ControllerPreference::Flydigi => Some(ControllerFamily::Flydigi),
        ControllerPreference::GameSir => Some(ControllerFamily::GameSir),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceClass {
    Controller(ControllerFamily),
    SkippedSteamInterface,
    SkippedEightBitDoReceiver,
    SkippedEightBitDoModel,
    SkippedFlydigiModel,
    SkippedGameSirModel,
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
    if eightbitdo::is_eightbitdo_vid(vendor_id) {
        if eightbitdo::is_supported_pid(product_id) {
            return DeviceClass::Controller(ControllerFamily::EightBitDo);
        }
        return if eightbitdo::is_receiver_pid(product_id) {
            DeviceClass::SkippedEightBitDoReceiver
        } else {
            DeviceClass::SkippedEightBitDoModel
        };
    }
    if apex5::is_flydigi_vid(vendor_id) {
        if apex5::is_supported_pid(product_id) {
            return if apex5::is_controller_interface(product_id, interface_number) {
                DeviceClass::Controller(ControllerFamily::Flydigi)
            } else {
                DeviceClass::Unrelated
            };
        }
        if apex5::is_vendor_interface(interface_number) {
            return DeviceClass::SkippedFlydigiModel;
        }
        return DeviceClass::Unrelated;
    }
    if gamesir::is_gamesir_vid(vendor_id) {
        if gamesir::is_supported_pid(product_id) {
            return if gamesir::is_controller_interface(product_id, interface_number) {
                DeviceClass::Controller(ControllerFamily::GameSir)
            } else {
                DeviceClass::Unrelated
            };
        }
        if interface_number == gamesir::GAMESIR_INTERFACE {
            return DeviceClass::SkippedGameSirModel;
        }
        return DeviceClass::Unrelated;
    }
    DeviceClass::Unrelated
}

/// Resolve a target family from what is present. Auto uses the family
/// priority; an explicit preference never substitutes another family.
pub fn select_family(
    preference: ControllerPreference,
    presence: FamilyPresence,
) -> Option<ControllerFamily> {
    match preference {
        ControllerPreference::Auto => presence.ordered().into_iter().next(),
        ControllerPreference::DualSense => {
            presence.dualsense.then_some(ControllerFamily::DualSense)
        }
        ControllerPreference::Steam => presence.steam.then_some(ControllerFamily::Steam),
        ControllerPreference::EightBitDo => {
            presence.eightbitdo.then_some(ControllerFamily::EightBitDo)
        }
        ControllerPreference::Flydigi => presence.flydigi.then_some(ControllerFamily::Flydigi),
        ControllerPreference::GameSir => presence.gamesir.then_some(ControllerFamily::GameSir),
    }
}

/// Target to switch to, or `None` when the current family already satisfies
/// the preference or the requested family is not present.
pub fn switch_target(
    preference: ControllerPreference,
    current: Option<ControllerFamily>,
    presence: FamilyPresence,
) -> Option<ControllerFamily> {
    let target = select_family(preference, presence)?;
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

impl DetectedDevice {
    /// Model name for UI text; family labels stay coarse for settings.
    pub fn label(&self) -> String {
        device_label_for(self.vendor_id, self.product_id)
    }
}

/// Human-readable model label for a detected controller.
pub fn device_label_for(vendor_id: u16, product_id: u16) -> String {
    if vendor_id == DS_VID {
        return match product_id {
            DSE_PID => "DualSense Edge",
            _ => "DualSense",
        }
        .to_string();
    }
    if steam_controller::is_steam_vid(vendor_id) {
        return ControllerFamily::Steam.label().to_string();
    }
    if eightbitdo::is_eightbitdo_vid(vendor_id) {
        return if eightbitdo::is_supported_pid(product_id) {
            "8BitDo Pro 3"
        } else {
            ControllerFamily::EightBitDo.label()
        }
        .to_string();
    }
    if apex5::is_flydigi_vid(vendor_id) {
        return match product_id {
            apex5::APEX5_PID => "Flydigi Apex 5",
            apex5::VADER5_PID => "Flydigi Vader 5 Pro",
            _ => ControllerFamily::Flydigi.label(),
        }
        .to_string();
    }
    if gamesir::is_gamesir_vid(vendor_id) {
        return match product_id {
            gamesir::TARANTULA_PRO_PID => "GameSir Tarantula Pro",
            _ => ControllerFamily::GameSir.label(),
        }
        .to_string();
    }
    "Controller".to_string()
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
                    ControllerFamily::EightBitDo => {
                        if entry.interface_number == -1 {
                            "Bluetooth (experimental)".to_string()
                        } else {
                            "USB".to_string()
                        }
                    }
                    ControllerFamily::Flydigi => "USB".to_string(),
                    ControllerFamily::GameSir => "2.4 GHz".to_string(),
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
            DeviceClass::SkippedEightBitDoReceiver => {
                skipped.push(SkippedDevice {
                    vendor_id: entry.vendor_id,
                    product_id: entry.product_id,
                    interface_number: entry.interface_number,
                    serial: entry.serial,
                    path: entry.path,
                    reason: eightbitdo::skip_reason(entry.product_id)
                        .unwrap_or("unsupported 8BitDo device")
                        .to_string(),
                });
            }
            DeviceClass::SkippedEightBitDoModel => {
                skipped.push(SkippedDevice {
                    vendor_id: entry.vendor_id,
                    product_id: entry.product_id,
                    interface_number: entry.interface_number,
                    serial: entry.serial,
                    path: entry.path,
                    reason: eightbitdo::skip_reason(entry.product_id)
                        .unwrap_or("unsupported 8BitDo device")
                        .to_string(),
                });
            }
            DeviceClass::SkippedFlydigiModel => {
                skipped.push(SkippedDevice {
                    vendor_id: entry.vendor_id,
                    product_id: entry.product_id,
                    interface_number: entry.interface_number,
                    serial: entry.serial,
                    path: entry.path,
                    reason: "unsupported Flydigi model; only the wired Apex 5 and Vader 5 Pro are \
                             supported"
                        .to_string(),
                });
            }
            DeviceClass::SkippedGameSirModel => {
                skipped.push(SkippedDevice {
                    vendor_id: entry.vendor_id,
                    product_id: entry.product_id,
                    interface_number: entry.interface_number,
                    serial: entry.serial,
                    path: entry.path,
                    reason: gamesir::skip_reason(entry.product_id)
                        .unwrap_or("unsupported GameSir model")
                        .to_string(),
                });
            }
            DeviceClass::Unrelated => {}
        }
    }

    devices.sort_by_key(|d| {
        (
            d.family.priority(),
            d.product_id,
            d.interface_number,
            d.path.clone(),
        )
    });
    skipped.sort_by_key(|d| (d.product_id, d.interface_number, d.path.clone()));
    (devices, skipped)
}

static LAST_SKIPPED_DEVICE_LOG: AtomicU64 = AtomicU64::new(0);

pub fn present_families(api: &HidApi, selector: Option<&str>) -> FamilyPresence {
    let (devices, skipped) = scan_devices(api);

    if !skipped.is_empty()
        && crate::util::rate_limited_gate(&LAST_SKIPPED_DEVICE_LOG, Duration::from_secs(60))
    {
        for device in &skipped {
            eprintln!(
                "[gyro4u] skipped device {:04x}:{:04x} interface {} ({}): {}",
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
            ControllerFamily::Steam
            | ControllerFamily::EightBitDo
            | ControllerFamily::Flydigi
            | ControllerFamily::GameSir => {
                device.serial.as_deref() == Some(selector) || device.path == selector
            }
        },
    };

    FamilyPresence {
        dualsense: devices
            .iter()
            .any(|d| d.family == ControllerFamily::DualSense && matches(d)),
        steam: devices
            .iter()
            .any(|d| d.family == ControllerFamily::Steam && matches(d)),
        eightbitdo: devices
            .iter()
            .any(|d| d.family == ControllerFamily::EightBitDo && matches(d)),
        flydigi: devices
            .iter()
            .any(|d| d.family == ControllerFamily::Flydigi && matches(d)),
        gamesir: devices
            .iter()
            .any(|d| d.family == ControllerFamily::GameSir && matches(d)),
    }
}

/// One row per detected family for the top-bar switcher. `label` names the
/// chosen model; Steam puck/receiver slots collapse into one row. A controller
/// is live when wired/BLE is present, the family is active, or a safe probe
/// observed a live receiver slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyRow {
    pub family: ControllerFamily,
    pub label: String,
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
            label: chosen.label(),
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
            label: ControllerFamily::Steam.label().to_string(),
            active,
            live,
            receiver_only: !live,
            transport,
            identity,
        });
    } else if let Some(slot) = live_slot {
        rows.push(FamilyRow {
            family: ControllerFamily::Steam,
            label: ControllerFamily::Steam.label().to_string(),
            active: active_family == Some(ControllerFamily::Steam),
            live: true,
            receiver_only: false,
            transport: Some(slot.transport.clone()),
            identity: Some(slot.identity.clone()),
        });
    }

    let eightbitdo: Vec<&DetectedDevice> = devices
        .iter()
        .filter(|device| device.family == ControllerFamily::EightBitDo)
        .collect();
    if let Some(chosen) = eightbitdo
        .iter()
        .find(|device| identity_matches(device, active_identity))
        .or_else(|| eightbitdo.first())
    {
        rows.push(FamilyRow {
            family: ControllerFamily::EightBitDo,
            label: chosen.label(),
            active: active_family == Some(ControllerFamily::EightBitDo),
            live: true,
            receiver_only: false,
            transport: Some(chosen.transport.clone()),
            identity: Some(chosen.identity.clone()),
        });
    }

    let flydigi: Vec<&DetectedDevice> = devices
        .iter()
        .filter(|device| device.family == ControllerFamily::Flydigi)
        .collect();
    if let Some(chosen) = flydigi
        .iter()
        .find(|device| identity_matches(device, active_identity))
        .or_else(|| flydigi.first())
    {
        rows.push(FamilyRow {
            family: ControllerFamily::Flydigi,
            label: chosen.label(),
            active: active_family == Some(ControllerFamily::Flydigi),
            live: true,
            receiver_only: false,
            transport: Some(chosen.transport.clone()),
            identity: Some(chosen.identity.clone()),
        });
    }

    let gamesir: Vec<&DetectedDevice> = devices
        .iter()
        .filter(|device| device.family == ControllerFamily::GameSir)
        .collect();
    if let Some(chosen) = gamesir
        .iter()
        .find(|device| identity_matches(device, active_identity))
        .or_else(|| gamesir.first())
    {
        rows.push(FamilyRow {
            family: ControllerFamily::GameSir,
            label: chosen.label(),
            active: active_family == Some(ControllerFamily::GameSir),
            live: true,
            receiver_only: false,
            transport: Some(chosen.transport.clone()),
            identity: Some(chosen.identity.clone()),
        });
    }

    rows
}

fn no_controller_error(selector: Option<&str>) -> anyhow::Error {
    match selector {
        Some(selector) => anyhow!(
            "No supported controller '{}' found. Connect a DualSense, DualSense Edge, Steam \
             Controller (2026), 8BitDo Pro 3 in D mode, a Flydigi Apex 5 / Vader 5 Pro in \
             wired PC mode, or a GameSir Tarantula Pro through its 2.4 GHz dongle in XInput/PC \
             mode.",
            selector
        ),
        None => anyhow!(
            "No supported controller found. Connect a DualSense, DualSense Edge, Steam \
             Controller (2026), 8BitDo Pro 3 in D mode, a Flydigi Apex 5 / Vader 5 Pro in \
             wired PC mode, or a GameSir Tarantula Pro through its 2.4 GHz dongle in XInput/PC \
             mode."
        ),
    }
}

impl PhysicalController {
    pub fn discover(
        api: &HidApi,
        selector: Option<&str>,
        preference: ControllerPreference,
    ) -> Result<Self> {
        let presence = present_families(api, selector);
        let Some(preferred) = select_family(preference, presence) else {
            if let Some(family) = requested_family(preference) {
                return Err(anyhow!(
                    "No supported controller of the requested family ({}) found. Connect a {} \
                     or choose Automatic.",
                    family.label(),
                    family.label()
                ));
            }
            return Err(no_controller_error(selector));
        };

        // Auto may fall back to the other families if opening the preferred
        // one fails. An explicit preference must never silently substitute.
        let families: Vec<ControllerFamily> = match requested_family(preference) {
            Some(_) => vec![preferred],
            None => {
                let mut families = vec![preferred];
                families.extend(
                    presence
                        .ordered()
                        .into_iter()
                        .filter(|family| *family != preferred),
                );
                families
            }
        };

        let mut last_error = None;
        for family in families {
            if !presence.present(family) {
                continue;
            }
            let opened = match family {
                ControllerFamily::DualSense => {
                    DualSense::new(api, selector).map(Self::from_dualsense)
                }
                ControllerFamily::Steam => {
                    SteamController::new(api, selector).map(Self::from_steam)
                }
                ControllerFamily::EightBitDo => {
                    EightBitDoPro3::new(api, selector).map(Self::from_eightbitdo)
                }
                ControllerFamily::Flydigi => Apex5::new(api, selector).map(Self::from_flydigi),
                ControllerFamily::GameSir => {
                    GameSirTarantulaPro::new(api, selector).map(Self::from_gamesir)
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

    fn from_eightbitdo(device: EightBitDoPro3) -> Self {
        let info = ControllerInfo {
            serial: device.serial().to_string(),
            identity: device.identity().to_string(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            kind: ControllerKind::EightBitDoPro3,
            is_bluetooth: device.is_bluetooth(),
            transport: if device.is_bluetooth() {
                "Bluetooth (experimental)".to_string()
            } else {
                "USB".to_string()
            },
        };
        Self {
            inner: ControllerInner::EightBitDo(device),
            info,
        }
    }

    fn from_flydigi(device: Apex5) -> Self {
        let info = ControllerInfo {
            serial: device.serial().to_string(),
            identity: device.identity().to_string(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            kind: device.kind(),
            is_bluetooth: false,
            transport: "USB".to_string(),
        };
        Self {
            inner: ControllerInner::Flydigi(device),
            info,
        }
    }

    fn from_gamesir(device: GameSirTarantulaPro) -> Self {
        let info = ControllerInfo {
            serial: device.serial().to_string(),
            identity: device.identity().to_string(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            kind: device.kind(),
            is_bluetooth: false,
            transport: "2.4 GHz".to_string(),
        };
        Self {
            inner: ControllerInner::GameSir(device),
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
            ControllerInner::EightBitDo(device) => {
                let polled = device.poll_latest_input()?;
                Ok(polled.map(|polled| PolledInput {
                    state: polled.state,
                    battery: polled.battery,
                    disconnected: false,
                }))
            }
            ControllerInner::Flydigi(device) => {
                let polled = device.poll_latest_input()?;
                Ok(polled.map(|polled| PolledInput {
                    state: polled.state,
                    battery: polled.battery,
                    disconnected: false,
                }))
            }
            ControllerInner::GameSir(device) => {
                let polled = device.poll_latest_input()?;
                Ok(polled.map(|polled| PolledInput {
                    state: polled.state,
                    battery: polled.battery,
                    disconnected: false,
                }))
            }
        }
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.get_input_state(),
            ControllerInner::Steam(device) => device.get_input_state(),
            ControllerInner::EightBitDo(device) => device.get_input_state(),
            ControllerInner::Flydigi(device) => device.get_input_state(),
            ControllerInner::GameSir(device) => device.get_input_state(),
        }
    }

    pub fn get_battery(&mut self) -> Result<BatteryInfo> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.get_battery(),
            ControllerInner::Steam(device) => device.get_battery(),
            ControllerInner::EightBitDo(device) => device.get_battery(),
            ControllerInner::Flydigi(device) => device.get_battery(),
            ControllerInner::GameSir(device) => device.get_battery(),
        }
    }

    pub fn get_firmware_info(&self) -> Result<(u16, String, String)> {
        match &self.inner {
            ControllerInner::DualSense(device) => device.get_firmware_info(),
            ControllerInner::Steam(device) => device.get_firmware_info(),
            ControllerInner::EightBitDo(device) => device.get_firmware_info(),
            ControllerInner::Flydigi(device) => device.get_firmware_info(),
            ControllerInner::GameSir(device) => device.get_firmware_info(),
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
            ControllerInner::EightBitDo(_) => {
                bail!("Firmware updates are not supported for the 8BitDo Pro 3 in gyro4u")
            }
            ControllerInner::Flydigi(_) => {
                bail!(
                    "Firmware updates are not supported for the {} in gyro4u",
                    self.info.kind.label()
                )
            }
            ControllerInner::GameSir(_) => {
                bail!(
                    "Firmware updates are not supported for the {} in gyro4u",
                    self.info.kind.label()
                )
            }
        }
    }

    pub fn set_update_mode(&self, active: bool) {
        match &self.inner {
            ControllerInner::DualSense(device) => device.set_update_mode(active),
            ControllerInner::Steam(device) => device.set_update_mode(active),
            ControllerInner::EightBitDo(device) => device.set_update_mode(active),
            ControllerInner::Flydigi(device) => device.set_update_mode(active),
            ControllerInner::GameSir(device) => device.set_update_mode(active),
        }
    }

    pub fn set_motion(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(_) => Ok(()),
            ControllerInner::Steam(device) => device.set_motion(enabled),
            ControllerInner::EightBitDo(device) => device.set_motion(enabled),
            ControllerInner::Flydigi(device) => device.set_motion(enabled),
            ControllerInner::GameSir(device) => device.set_motion(enabled),
        }
    }

    pub fn force_motion(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(_) => Ok(()),
            ControllerInner::Steam(device) => device.force_motion(enabled),
            ControllerInner::EightBitDo(device) => device.force_motion(enabled),
            ControllerInner::Flydigi(device) => device.force_motion(enabled),
            ControllerInner::GameSir(device) => device.force_motion(enabled),
        }
    }

    pub fn set_restore_on_drop(&mut self, enabled: bool) {
        match &mut self.inner {
            ControllerInner::Steam(device) => device.set_restore_on_drop(enabled),
            ControllerInner::Flydigi(device) => device.set_restore_on_drop(enabled),
            ControllerInner::GameSir(device) => device.set_restore_on_drop(enabled),
            ControllerInner::DualSense(_) | ControllerInner::EightBitDo(_) => {}
        }
    }

    pub fn prepare_handoff(&mut self) {
        match &mut self.inner {
            ControllerInner::Steam(device) => device.prepare_handoff(),
            ControllerInner::Flydigi(device) => device.prepare_handoff(),
            ControllerInner::GameSir(device) => device.prepare_handoff(),
            ControllerInner::DualSense(_) | ControllerInner::EightBitDo(_) => {}
        }
    }

    pub fn set_lightbar(&mut self, r: u8, g: u8, b: u8, brightness: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_lightbar(r, g, b, brightness),
            ControllerInner::Steam(device) => device.set_lightbar(r, g, b, brightness),
            ControllerInner::EightBitDo(device) => device.set_lightbar(r, g, b, brightness),
            ControllerInner::Flydigi(device) => device.set_lightbar(r, g, b, brightness),
            ControllerInner::GameSir(device) => device.set_lightbar(r, g, b, brightness),
        }
    }

    pub fn set_lightbar_enabled(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_lightbar_enabled(enabled),
            ControllerInner::Steam(device) => device.set_lightbar_enabled(enabled),
            ControllerInner::EightBitDo(device) => device.set_lightbar_enabled(enabled),
            ControllerInner::Flydigi(device) => device.set_lightbar_enabled(enabled),
            ControllerInner::GameSir(device) => device.set_lightbar_enabled(enabled),
        }
    }

    pub fn set_player_leds(&mut self, leds: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_player_leds(leds),
            ControllerInner::Steam(device) => device.set_player_leds(leds),
            ControllerInner::EightBitDo(device) => device.set_player_leds(leds),
            ControllerInner::Flydigi(device) => device.set_player_leds(leds),
            ControllerInner::GameSir(device) => device.set_player_leds(leds),
        }
    }

    pub fn set_mic(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_mic(enabled),
            ControllerInner::Steam(device) => device.set_mic(enabled),
            ControllerInner::EightBitDo(device) => device.set_mic(enabled),
            ControllerInner::Flydigi(device) => device.set_mic(enabled),
            ControllerInner::GameSir(device) => device.set_mic(enabled),
        }
    }

    pub fn set_mic_led(&mut self, state: MicLedState) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_mic_led(state),
            ControllerInner::Steam(device) => device.set_mic_led(state),
            ControllerInner::EightBitDo(device) => device.set_mic_led(state),
            ControllerInner::Flydigi(device) => device.set_mic_led(state),
            ControllerInner::GameSir(device) => device.set_mic_led(state),
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
            ControllerInner::EightBitDo(device) => device.set_trigger_effects(left, right),
            ControllerInner::Flydigi(device) => device.set_trigger_effects(left, right),
            ControllerInner::GameSir(device) => device.set_trigger_effects(left, right),
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
            ControllerInner::EightBitDo(device) => {
                device.set_trigger_effect(left, right, mode, params)
            }
            ControllerInner::Flydigi(device) => {
                device.set_trigger_effect(left, right, mode, params)
            }
            ControllerInner::GameSir(device) => {
                device.set_trigger_effect(left, right, mode, params)
            }
        }
    }

    pub fn set_trigger_off(&mut self) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_trigger_off(),
            ControllerInner::Steam(device) => device.set_trigger_off(),
            ControllerInner::EightBitDo(device) => device.set_trigger_off(),
            ControllerInner::Flydigi(device) => device.set_trigger_off(),
            ControllerInner::GameSir(device) => device.set_trigger_off(),
        }
    }

    pub fn set_vibration(&mut self, rumble: u8, trigger: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_vibration(rumble, trigger),
            ControllerInner::Steam(device) => device.set_vibration(rumble, trigger),
            ControllerInner::EightBitDo(device) => device.set_vibration(rumble, trigger),
            ControllerInner::Flydigi(device) => device.set_vibration(rumble, trigger),
            ControllerInner::GameSir(device) => device.set_vibration(rumble, trigger),
        }
    }

    pub fn set_rumble(&mut self, left: u8, right: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_rumble(left, right),
            ControllerInner::Steam(device) => device.set_rumble(left, right),
            ControllerInner::EightBitDo(device) => device.set_rumble(left, right),
            ControllerInner::Flydigi(device) => device.set_rumble(left, right),
            ControllerInner::GameSir(device) => device.set_rumble(left, right),
        }
    }

    pub fn set_haptics(&mut self, samples: &[i8; HAPTICS_PACKET_SAMPLES]) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_haptics(samples),
            ControllerInner::Steam(device) => device.set_haptics(samples),
            ControllerInner::EightBitDo(device) => device.set_haptics(samples),
            ControllerInner::Flydigi(device) => device.set_haptics(samples),
            ControllerInner::GameSir(device) => device.set_haptics(samples),
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
            ControllerInner::EightBitDo(_) => {
                bail!("Steam Controller pad haptics are not supported on the 8BitDo Pro 3")
            }
            ControllerInner::Flydigi(_) => {
                bail!(
                    "Steam Controller pad haptics are not supported on the {}",
                    self.info.kind.label()
                )
            }
            ControllerInner::GameSir(_) => {
                bail!(
                    "Steam Controller pad haptics are not supported on the {}",
                    self.info.kind.label()
                )
            }
        }
    }

    pub fn set_speaker(&mut self, mode: &str) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_speaker(mode),
            ControllerInner::Steam(device) => device.set_speaker(mode),
            ControllerInner::EightBitDo(device) => device.set_speaker(mode),
            ControllerInner::Flydigi(device) => device.set_speaker(mode),
            ControllerInner::GameSir(device) => device.set_speaker(mode),
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<()> {
        match &mut self.inner {
            ControllerInner::DualSense(device) => device.set_volume(volume),
            ControllerInner::Steam(device) => device.set_volume(volume),
            ControllerInner::EightBitDo(device) => device.set_volume(volume),
            ControllerInner::Flydigi(device) => device.set_volume(volume),
            ControllerInner::GameSir(device) => device.set_volume(volume),
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
        let is_eightbitdo = eightbitdo::is_eightbitdo_vid(info.vendor_id())
            && eightbitdo::is_supported_pid(info.product_id());
        let is_flydigi = apex5::is_flydigi_vid(info.vendor_id())
            && apex5::is_supported_pid(info.product_id())
            && apex5::is_controller_interface(info.product_id(), info.interface_number());
        let is_gamesir = gamesir::is_gamesir_vid(info.vendor_id())
            && gamesir::is_supported_pid(info.product_id())
            && gamesir::is_controller_interface(info.product_id(), info.interface_number());
        if !is_dualsense && !is_steam && !is_eightbitdo && !is_flydigi && !is_gamesir {
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
                device.label(),
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
    let eightbitdo = devices
        .iter()
        .any(|device| device.family == ControllerFamily::EightBitDo);
    let flydigi = devices
        .iter()
        .any(|device| device.family == ControllerFamily::Flydigi);
    let gamesir = devices
        .iter()
        .any(|device| device.family == ControllerFamily::GameSir);
    let presence = FamilyPresence {
        dualsense,
        steam,
        eightbitdo,
        flydigi,
        gamesir,
    };

    out.push_str(&format!("Preference: {}\n", preference.label()));
    match select_family(preference, presence) {
        Some(family) => out.push_str(&format!("Would connect: {}\n", family.label())),
        None => out.push_str("Would connect: nothing\n"),
    }
    if steam {
        out.push_str(
            "Note: puck/receiver slots are probed for a live controller at connect time.\n",
        );
    }
    if eightbitdo {
        out.push_str(
            "Note: wired 8BitDo is supported; Bluetooth DInput is experimental, and the 2.4G \
             receiver is not supported.\n",
        );
    }
    if flydigi {
        out.push_str(
            "Note: wired Flydigi Apex 5 and Vader 5 Pro in PC mode are supported; gyro and basic \
             rumble are available, but adaptive triggers, RGB, and firmware updates are not.\n",
        );
    }
    if gamesir {
        out.push_str(
            "Note: the GameSir Tarantula Pro is supported through its 2.4 GHz dongle in \
             XInput/PC mode; gyro, basic rumble, and the L4/R4/T1-T3/C1-C4/Share extras are \
             available, but RGB, firmware updates, and adaptive triggers are not. Wired \
             3537:1050 is not supported yet.\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presence(dualsense: bool, steam: bool, eightbitdo: bool) -> FamilyPresence {
        FamilyPresence {
            dualsense,
            steam,
            eightbitdo,
            flydigi: false,
            gamesir: false,
        }
    }

    fn presence_with_flydigi(
        dualsense: bool,
        steam: bool,
        eightbitdo: bool,
        flydigi: bool,
    ) -> FamilyPresence {
        FamilyPresence {
            dualsense,
            steam,
            eightbitdo,
            flydigi,
            gamesir: false,
        }
    }

    #[test]
    fn auto_selects_steam_when_only_steam_present() {
        assert_eq!(
            select_family(ControllerPreference::Auto, presence(false, true, false)),
            Some(ControllerFamily::Steam)
        );
    }

    #[test]
    fn auto_selects_dualsense_when_only_dualsense_present() {
        assert_eq!(
            select_family(ControllerPreference::Auto, presence(true, false, false)),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn auto_selects_eightbitdo_when_only_eightbitdo_present() {
        assert_eq!(
            select_family(ControllerPreference::Auto, presence(false, false, true)),
            Some(ControllerFamily::EightBitDo)
        );
    }

    fn presence_with_gamesir(
        dualsense: bool,
        steam: bool,
        eightbitdo: bool,
        flydigi: bool,
        gamesir: bool,
    ) -> FamilyPresence {
        FamilyPresence {
            dualsense,
            steam,
            eightbitdo,
            flydigi,
            gamesir,
        }
    }

    #[test]
    fn auto_selects_gamesir_when_only_gamesir_present() {
        assert_eq!(
            select_family(
                ControllerPreference::Auto,
                presence_with_gamesir(false, false, false, false, true)
            ),
            Some(ControllerFamily::GameSir)
        );
    }

    #[test]
    fn gamesir_preference_wins_when_present() {
        assert_eq!(
            select_family(
                ControllerPreference::GameSir,
                presence_with_gamesir(true, true, true, true, true)
            ),
            Some(ControllerFamily::GameSir)
        );
        assert_eq!(
            select_family(ControllerPreference::GameSir, presence(false, false, false)),
            None
        );
    }

    #[test]
    fn auto_keeps_gamesir_last() {
        assert_eq!(
            select_family(
                ControllerPreference::Auto,
                presence_with_gamesir(false, false, false, true, true)
            ),
            Some(ControllerFamily::Flydigi)
        );
        assert_eq!(
            requested_family(ControllerPreference::GameSir),
            Some(ControllerFamily::GameSir)
        );
    }

    #[test]
    fn auto_selects_flydigi_when_only_flydigi_present() {
        assert_eq!(
            select_family(
                ControllerPreference::Auto,
                presence_with_flydigi(false, false, false, true)
            ),
            Some(ControllerFamily::Flydigi)
        );
    }

    #[test]
    fn auto_prefers_eightbitdo_over_flydigi() {
        assert_eq!(
            select_family(
                ControllerPreference::Auto,
                presence_with_flydigi(false, false, true, true)
            ),
            Some(ControllerFamily::EightBitDo)
        );
        assert_eq!(
            select_family(
                ControllerPreference::Auto,
                presence_with_flydigi(false, true, true, true)
            ),
            Some(ControllerFamily::Steam)
        );
        assert_eq!(
            select_family(
                ControllerPreference::Auto,
                presence_with_flydigi(true, true, true, true)
            ),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn flydigi_preference_wins_when_present() {
        assert_eq!(
            select_family(
                ControllerPreference::Flydigi,
                presence_with_flydigi(true, true, true, true)
            ),
            Some(ControllerFamily::Flydigi)
        );
    }

    #[test]
    fn auto_prefers_dualsense_then_steam() {
        assert_eq!(
            select_family(ControllerPreference::Auto, presence(true, true, true)),
            Some(ControllerFamily::DualSense)
        );
        assert_eq!(
            select_family(ControllerPreference::Auto, presence(false, true, true)),
            Some(ControllerFamily::Steam)
        );
    }

    #[test]
    fn steam_preference_wins_when_present() {
        assert_eq!(
            select_family(ControllerPreference::Steam, presence(true, true, true)),
            Some(ControllerFamily::Steam)
        );
    }

    #[test]
    fn dualsense_preference_wins_when_present() {
        assert_eq!(
            select_family(ControllerPreference::DualSense, presence(true, true, true)),
            Some(ControllerFamily::DualSense)
        );
    }

    #[test]
    fn eightbitdo_preference_wins_when_present() {
        assert_eq!(
            select_family(ControllerPreference::EightBitDo, presence(true, true, true)),
            Some(ControllerFamily::EightBitDo)
        );
    }

    #[test]
    fn explicit_preference_does_not_substitute_another_family() {
        assert_eq!(
            select_family(ControllerPreference::Steam, presence(true, false, false)),
            None
        );
        assert_eq!(
            select_family(
                ControllerPreference::DualSense,
                presence(false, true, false)
            ),
            None
        );
        assert_eq!(
            select_family(
                ControllerPreference::EightBitDo,
                presence(true, true, false)
            ),
            None
        );
        assert_eq!(
            select_family(ControllerPreference::Flydigi, presence(true, true, true)),
            None
        );
        assert_eq!(
            select_family(ControllerPreference::GameSir, presence(true, true, true)),
            None
        );
    }

    #[test]
    fn switch_target_absent_target_keeps_current() {
        assert_eq!(
            switch_target(
                ControllerPreference::Steam,
                Some(ControllerFamily::DualSense),
                presence(true, false, false)
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
                presence(true, true, true)
            ),
            Some(ControllerFamily::Steam)
        );
        assert_eq!(
            switch_target(
                ControllerPreference::EightBitDo,
                Some(ControllerFamily::DualSense),
                presence(true, true, true)
            ),
            Some(ControllerFamily::EightBitDo)
        );
        assert_eq!(
            switch_target(
                ControllerPreference::Flydigi,
                Some(ControllerFamily::DualSense),
                presence_with_flydigi(true, false, false, true)
            ),
            Some(ControllerFamily::Flydigi)
        );
    }

    #[test]
    fn switch_target_same_family_is_a_no_op() {
        assert_eq!(
            switch_target(
                ControllerPreference::Steam,
                Some(ControllerFamily::Steam),
                presence(true, true, false)
            ),
            None
        );
        assert_eq!(
            switch_target(
                ControllerPreference::Auto,
                Some(ControllerFamily::DualSense),
                presence(true, true, true)
            ),
            None
        );
        assert_eq!(
            switch_target(
                ControllerPreference::EightBitDo,
                Some(ControllerFamily::EightBitDo),
                presence(false, false, true)
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
                presence(true, true, true)
            ),
            Some(ControllerFamily::DualSense)
        );
        assert_eq!(
            switch_target(
                ControllerPreference::Auto,
                Some(ControllerFamily::DualSense),
                presence(true, true, true)
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
        assert_eq!(
            requested_family(ControllerPreference::EightBitDo),
            Some(ControllerFamily::EightBitDo)
        );
        assert_eq!(
            requested_family(ControllerPreference::Flydigi),
            Some(ControllerFamily::Flydigi)
        );
        assert_eq!(
            requested_family(ControllerPreference::GameSir),
            Some(ControllerFamily::GameSir)
        );
    }

    #[test]
    fn family_of_kind_maps_edge_pro3_and_flydigi_models() {
        assert_eq!(
            family_of_kind(ControllerKind::DualSenseEdge),
            ControllerFamily::DualSense
        );
        assert_eq!(
            family_of_kind(ControllerKind::SteamController),
            ControllerFamily::Steam
        );
        assert_eq!(
            family_of_kind(ControllerKind::EightBitDoPro3),
            ControllerFamily::EightBitDo
        );
        assert_eq!(
            family_of_kind(ControllerKind::FlydigiApex5),
            ControllerFamily::Flydigi
        );
        assert_eq!(
            family_of_kind(ControllerKind::FlydigiVader5Pro),
            ControllerFamily::Flydigi
        );
        assert_eq!(
            family_of_kind(ControllerKind::GameSirTarantulaPro),
            ControllerFamily::GameSir
        );
    }

    #[test]
    fn no_family_selected_when_nothing_is_present() {
        for preference in [
            ControllerPreference::Auto,
            ControllerPreference::DualSense,
            ControllerPreference::Steam,
            ControllerPreference::EightBitDo,
            ControllerPreference::Flydigi,
            ControllerPreference::GameSir,
        ] {
            assert_eq!(
                select_family(preference, presence(false, false, false)),
                None
            );
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

    #[test]
    fn eightbitdo_pro3_is_supported_but_receivers_and_other_models_are_skipped() {
        use crate::eightbitdo::{
            EIGHTBITDO_PRO3_PID, EIGHTBITDO_RECEIVER_DOCKED_PID, EIGHTBITDO_RECEIVER_XINPUT_PID,
            EIGHTBITDO_VID,
        };

        for interface in [-1, 0] {
            assert_eq!(
                classify_device(EIGHTBITDO_VID, EIGHTBITDO_PRO3_PID, interface),
                DeviceClass::Controller(ControllerFamily::EightBitDo)
            );
        }
        for pid in [
            EIGHTBITDO_RECEIVER_DOCKED_PID,
            EIGHTBITDO_RECEIVER_XINPUT_PID,
        ] {
            assert_eq!(
                classify_device(EIGHTBITDO_VID, pid, 0),
                DeviceClass::SkippedEightBitDoReceiver
            );
        }
        assert_eq!(
            classify_device(EIGHTBITDO_VID, 0x1234, 0),
            DeviceClass::SkippedEightBitDoModel
        );
    }

    #[test]
    fn flydigi_models_are_supported_on_their_vendor_interfaces() {
        use crate::apex5::{APEX5_PID, FLYDIGI_VID, VADER5_PID};

        assert_eq!(
            classify_device(FLYDIGI_VID, APEX5_PID, 2),
            DeviceClass::Controller(ControllerFamily::Flydigi)
        );
        assert_eq!(
            classify_device(FLYDIGI_VID, VADER5_PID, 1),
            DeviceClass::Controller(ControllerFamily::Flydigi)
        );
        assert_eq!(
            classify_device(FLYDIGI_VID, 0x2502, 2),
            DeviceClass::SkippedFlydigiModel
        );
        assert_eq!(
            classify_device(FLYDIGI_VID, 0x2502, 1),
            DeviceClass::SkippedFlydigiModel
        );
        for (pid, interface) in [
            (APEX5_PID, -1),
            (APEX5_PID, 0),
            (APEX5_PID, 1),
            (APEX5_PID, 3),
            (VADER5_PID, -1),
            (VADER5_PID, 0),
            (VADER5_PID, 2),
            (VADER5_PID, 3),
            (0x2502, -1),
            (0x2502, 0),
            (0x2502, 3),
        ] {
            assert_eq!(
                classify_device(FLYDIGI_VID, pid, interface),
                DeviceClass::Unrelated,
                "pid {pid:#06x} interface {interface}"
            );
        }
    }

    #[test]
    fn gamesir_tarantula_is_supported_on_interface_one() {
        use crate::gamesir::{GAMESIR_VID, TARANTULA_PRO_PID};

        assert_eq!(
            classify_device(GAMESIR_VID, TARANTULA_PRO_PID, 1),
            DeviceClass::Controller(ControllerFamily::GameSir)
        );
        for interface in [-1, 0, 2, 3] {
            assert_eq!(
                classify_device(GAMESIR_VID, TARANTULA_PRO_PID, interface),
                DeviceClass::Unrelated,
                "interface {interface}"
            );
        }
        assert_eq!(
            classify_device(GAMESIR_VID, 0x1050, 1),
            DeviceClass::SkippedGameSirModel
        );
        assert_eq!(
            classify_device(GAMESIR_VID, 0x1050, 0),
            DeviceClass::Unrelated
        );
        assert_eq!(classify_device(0x1234, 0x5678, 1), DeviceClass::Unrelated);
    }

    #[test]
    fn device_labels_name_the_detected_model() {
        use crate::apex5::{APEX5_PID, FLYDIGI_VID, VADER5_PID};
        use crate::eightbitdo::{EIGHTBITDO_PRO3_PID, EIGHTBITDO_VID};
        use crate::steam_controller::{STEAM_PID_BLE, STEAM_PID_WIRED, STEAM_VID};

        assert_eq!(
            device_label_for(EIGHTBITDO_VID, EIGHTBITDO_PRO3_PID),
            "8BitDo Pro 3"
        );
        assert_eq!(device_label_for(EIGHTBITDO_VID, 0x1234), "8BitDo");
        assert_eq!(device_label_for(FLYDIGI_VID, APEX5_PID), "Flydigi Apex 5");
        assert_eq!(
            device_label_for(FLYDIGI_VID, VADER5_PID),
            "Flydigi Vader 5 Pro"
        );
        assert_eq!(device_label_for(FLYDIGI_VID, 0x2502), "Flydigi");
        assert_eq!(
            device_label_for(
                crate::gamesir::GAMESIR_VID,
                crate::gamesir::TARANTULA_PRO_PID
            ),
            "GameSir Tarantula Pro"
        );
        assert_eq!(
            device_label_for(crate::gamesir::GAMESIR_VID, 0x1050),
            "GameSir"
        );
        assert_eq!(device_label_for(DS_VID, DS_PID), "DualSense");
        assert_eq!(device_label_for(DS_VID, DSE_PID), "DualSense Edge");
        assert_eq!(
            device_label_for(STEAM_VID, STEAM_PID_WIRED),
            "Steam Controller"
        );
        assert_eq!(
            device_label_for(STEAM_VID, STEAM_PID_BLE),
            "Steam Controller"
        );
        assert_eq!(device_label_for(0x1234, 0x5678), "Controller");
        assert_eq!(ControllerFamily::EightBitDo.label(), "8BitDo");
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
                ControllerFamily::EightBitDo => crate::eightbitdo::EIGHTBITDO_VID,
                ControllerFamily::Flydigi => crate::apex5::FLYDIGI_VID,
                ControllerFamily::GameSir => crate::gamesir::GAMESIR_VID,
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
            } else if family == ControllerFamily::GameSir {
                "2.4 GHz".to_string()
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
        assert_eq!(rows[0].label, "Steam Controller");
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
    fn inventory_labels_use_the_detected_model() {
        let dualsense = detected(
            ControllerFamily::DualSense,
            DS_PID,
            3,
            Some("DS-SERIAL"),
            "/dev/hidraw0",
        );
        let rows = inventory_rows(&[dualsense], None, None, None);
        assert_eq!(rows[0].label, "DualSense");

        let edge = detected(
            ControllerFamily::DualSense,
            DSE_PID,
            3,
            Some("DSE-SERIAL"),
            "/dev/hidraw1",
        );
        let rows = inventory_rows(&[edge], None, None, None);
        assert_eq!(rows[0].label, "DualSense Edge");

        let pad = detected(
            ControllerFamily::EightBitDo,
            crate::eightbitdo::EIGHTBITDO_PRO3_PID,
            0,
            Some("E8BDO-SERIAL"),
            "/dev/hidraw7",
        );
        let rows = inventory_rows(&[pad], None, None, None);
        assert_eq!(rows[0].label, "8BitDo Pro 3");
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
    fn eightbitdo_pro3_adds_a_live_family_row() {
        let pad = detected(
            ControllerFamily::EightBitDo,
            crate::eightbitdo::EIGHTBITDO_PRO3_PID,
            0,
            Some("E8BDO-SERIAL"),
            "/dev/hidraw7",
        );

        let rows = inventory_rows(
            &[pad],
            Some(ControllerFamily::EightBitDo),
            Some("E8BDO-SERIAL"),
            None,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].family, ControllerFamily::EightBitDo);
        assert!(rows[0].active && rows[0].live);
        assert_eq!(rows[0].transport.as_deref(), Some("USB"));
        assert_eq!(rows[0].identity.as_deref(), Some("E8BDO-SERIAL"));
    }

    #[test]
    fn flydigi_models_add_a_live_family_row_with_model_labels() {
        for (pid, label) in [
            (crate::apex5::APEX5_PID, "Flydigi Apex 5"),
            (crate::apex5::VADER5_PID, "Flydigi Vader 5 Pro"),
        ] {
            let interface = if pid == crate::apex5::VADER5_PID {
                1
            } else {
                2
            };
            let pad = detected(
                ControllerFamily::Flydigi,
                pid,
                interface,
                Some("FLYDIGI-SERIAL"),
                "/dev/hidraw9",
            );

            let rows = inventory_rows(
                &[pad],
                Some(ControllerFamily::Flydigi),
                Some("FLYDIGI-SERIAL"),
                None,
            );

            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].family, ControllerFamily::Flydigi);
            assert_eq!(rows[0].label, label);
            assert!(rows[0].active && rows[0].live);
            assert_eq!(rows[0].transport.as_deref(), Some("USB"));
            assert_eq!(rows[0].identity.as_deref(), Some("FLYDIGI-SERIAL"));
        }
    }

    #[test]
    fn gamesir_adds_a_live_family_row_with_the_model_label() {
        let pad = detected(
            ControllerFamily::GameSir,
            crate::gamesir::TARANTULA_PRO_PID,
            1,
            Some("GAMESIR-SERIAL"),
            "/dev/hidraw9",
        );

        let rows = inventory_rows(
            &[pad],
            Some(ControllerFamily::GameSir),
            Some("GAMESIR-SERIAL"),
            None,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].family, ControllerFamily::GameSir);
        assert_eq!(rows[0].label, "GameSir Tarantula Pro");
        assert!(rows[0].active && rows[0].live);
        assert_eq!(rows[0].transport.as_deref(), Some("2.4 GHz"));
        assert_eq!(rows[0].identity.as_deref(), Some("GAMESIR-SERIAL"));
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
