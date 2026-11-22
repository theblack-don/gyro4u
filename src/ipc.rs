use std::{
    env,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    common::{BatteryInfo, HapticPattern, LightbarEffect, MicLedState},
    controller::SteamLiveness,
    inputs::{ControllerKind, ControllerState},
    profiles::Profile,
    settings::ControllerPreference,
    transform::InputTransform,
};

mod transport {
    use std::{
        io,
        os::unix::net::{UnixListener, UnixStream},
        path::PathBuf,
        time::Duration,
    };
    pub type Addr = PathBuf;
    pub type Stream = UnixStream;
    pub type Listener = UnixListener;
    pub fn connect(addr: &Addr) -> io::Result<Stream> {
        UnixStream::connect(addr)
    }
    pub fn bind(addr: &Addr) -> io::Result<Listener> {
        UnixListener::bind(addr)
    }
    pub fn set_timeout(s: &Stream, d: Duration) -> io::Result<()> {
        s.set_read_timeout(Some(d))
    }
    pub fn addr_to_string(addr: &Addr) -> String {
        addr.display().to_string()
    }
}

pub type DaemonAddr = transport::Addr;
pub type DaemonStream = transport::Stream;
pub type DaemonListener = transport::Listener;
pub type ControllerInfoTuple = (String, u16, bool, ControllerKind, String);

pub const IPC_PROTOCOL_VERSION: u32 = 8;
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaemonCompatibility {
    Compatible { protocol: u32, app_version: String },
    Incompatible { detail: String },
    Unavailable { detail: String },
}

pub fn classify_handshake(response: Result<DaemonResponse>) -> DaemonCompatibility {
    match response {
        Ok(DaemonResponse::Pong {
            protocol,
            app_version,
        }) if protocol == IPC_PROTOCOL_VERSION => DaemonCompatibility::Compatible {
            protocol,
            app_version,
        },
        Ok(DaemonResponse::Pong {
            protocol,
            app_version,
        }) => DaemonCompatibility::Incompatible {
            detail: format!(
                "daemon speaks IPC protocol {protocol} ({app_version}), this build requires \
                 protocol {IPC_PROTOCOL_VERSION}"
            ),
        },
        Ok(DaemonResponse::IncompatibleProtocol {
            daemon_protocol,
            daemon_version,
        }) => DaemonCompatibility::Incompatible {
            detail: format!(
                "daemon speaks IPC protocol {daemon_protocol} ({daemon_version}), this build \
                 requires protocol {IPC_PROTOCOL_VERSION}"
            ),
        },
        Ok(DaemonResponse::Error { message }) => DaemonCompatibility::Incompatible {
            detail: format!("daemon rejected the IPC handshake: {message}"),
        },
        Ok(_) => DaemonCompatibility::Incompatible {
            detail: "daemon returned an unexpected IPC handshake response".to_string(),
        },
        Err(e) => DaemonCompatibility::Incompatible {
            detail: format!("daemon IPC handshake failed: {e}"),
        },
    }
}

pub fn daemon_endpoint() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(env::temp_dir)
        .join("gyro4u.socket")
}

#[inline]
pub fn socket_path() -> DaemonAddr {
    daemon_endpoint()
}

pub fn bind_daemon(addr: &DaemonAddr) -> io::Result<DaemonListener> {
    transport::bind(addr)
}

pub fn cleanup_endpoint(addr: &DaemonAddr) {
    if addr.exists() {
        use std::fs;
        let _ = fs::remove_file(addr);
    }
}

pub fn addr_display(addr: &DaemonAddr) -> String {
    transport::addr_to_string(addr)
}

pub fn probe_daemon(addr: &DaemonAddr) -> DaemonCompatibility {
    match IpcClient::connect_with_timeout(addr, Duration::from_secs(1)) {
        Ok(mut client) => client.handshake(),
        Err(e) => DaemonCompatibility::Unavailable {
            detail: e.to_string(),
        },
    }
}

pub fn shutdown_daemon(addr: &DaemonAddr) -> Result<()> {
    let mut client = IpcClient::connect_unchecked(addr)?;
    client.send(DaemonCommand::Shutdown)?;
    Ok(())
}

pub fn wait_for_daemon_release(addr: &DaemonAddr, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if transport::connect(addr).is_err() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn wait_for_daemon_ready(addr: &DaemonAddr, timeout: Duration) -> DaemonCompatibility {
    let deadline = Instant::now() + timeout;
    loop {
        let compatibility = probe_daemon(addr);
        if !matches!(compatibility, DaemonCompatibility::Unavailable { .. })
            || Instant::now() >= deadline
        {
            return compatibility;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "cmd", content = "args")]
pub enum DaemonCommand {
    Ping {
        protocol: u32,
        app_version: String,
    },
    GetBattery,
    GetInputState,
    GetFirmwareInfo,
    GetControllerInfo,
    GetSteamLiveness,
    SetLightbar {
        r: u8,
        g: u8,
        b: u8,
        brightness: u8,
    },
    SetLightbarEnabled {
        enabled: bool,
    },
    SetPlayerLeds {
        leds: u8,
    },
    SetMic {
        enabled: bool,
    },
    SetMicLed {
        state: MicLedState,
    },
    SetTriggerOff,
    SetTriggerEffect {
        right: bool,
        left: bool,
        effect_type: u8,
        params: [u8; 10],
    },
    SetTriggerEffects {
        left: Option<(u8, [u8; 10])>,
        right: Option<(u8, [u8; 10])>,
    },
    SetVibration {
        rumble: u8,
        trigger: u8,
    },
    SetRumble {
        left: u8,
        right: u8,
    },

    SetSpeaker {
        mode: String,
    },
    SetVolume {
        volume: u8,
    },
    SetUpdateMode {
        active: bool,
    },
    SetInputTransform {
        transform: InputTransform,
    },
    ClearInputTransform,
    SetLightbarEffect {
        effect: LightbarEffect,
    },
    SetHapticPattern {
        pattern: HapticPattern,
        strength: u8,
        speed: f32,
    },
    SetRawHaptics {
        active: bool,
    },
    SetGyro {
        enabled: bool,
        smoothing: f32,
        sensitivity: f32,
    },
    SetMotion {
        enabled: bool,
    },
    SwitchProfile {
        name: String,
    },
    ListProfiles,
    ReloadProfile,
    SaveProfile {
        profile: Profile,
    },
    DeleteProfile {
        name: String,
    },
    RenameProfile {
        old: String,
        new: String,
    },
    GetActiveProfile,
    SetControllerPreference {
        preference: ControllerPreference,
    },
    Shutdown,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DaemonResponse {
    Pong {
        protocol: u32,
        app_version: String,
    },
    IncompatibleProtocol {
        daemon_protocol: u32,
        daemon_version: String,
    },
    Ok,
    Error {
        message: String,
    },
    Battery(BatteryInfo),
    InputState(ControllerState),
    FirmwareInfo {
        version: u16,
        build_date: String,
        build_time: String,
    },
    ControllerInfo {
        serial: String,
        product_id: u16,
        is_bt: bool,
        kind: ControllerKind,
        transport: String,
    },
    SteamLiveness(SteamLiveness),
    NoDevice,
    ProfileList {
        profiles: Vec<String>,
    },
    ActiveProfile {
        name: String,
    },
}

pub struct IpcClient {
    reader: BufReader<DaemonStream>,
    writer: DaemonStream,
}

impl IpcClient {
    pub fn connect(addr: &DaemonAddr) -> Result<Self> {
        let mut client = Self::connect_unchecked(addr)?;
        match client.handshake() {
            DaemonCompatibility::Compatible { .. } => Ok(client),
            DaemonCompatibility::Incompatible { detail }
            | DaemonCompatibility::Unavailable { detail } => bail!("{detail}"),
        }
    }

    pub fn connect_unchecked(addr: &DaemonAddr) -> Result<Self> {
        Self::connect_with_timeout(addr, Duration::from_secs(5))
    }

    pub(crate) fn connect_with_timeout(addr: &DaemonAddr, timeout: Duration) -> Result<Self> {
        let stream = transport::connect(addr)?;
        transport::set_timeout(&stream, timeout)?;
        let writer = stream.try_clone()?;

        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }

    pub fn handshake(&mut self) -> DaemonCompatibility {
        classify_handshake(self.request(DaemonCommand::Ping {
            protocol: IPC_PROTOCOL_VERSION,
            app_version: APP_VERSION.to_string(),
        }))
    }

    pub fn try_connect(addr: &DaemonAddr) -> Option<Self> {
        Self::connect(addr).ok()
    }

    pub fn send(&mut self, cmd: DaemonCommand) -> Result<()> {
        let mut line = serde_json::to_string(&cmd)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        Ok(())
    }

    pub fn recv(&mut self) -> Result<DaemonResponse> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        if line.is_empty() {
            bail!("Daemon closed the connection");
        }
        Ok(serde_json::from_str(line.trim())?)
    }

    pub fn request(&mut self, cmd: DaemonCommand) -> Result<DaemonResponse> {
        self.send(cmd)?;
        self.recv()
    }

    pub fn get_battery(&mut self) -> Result<BatteryInfo> {
        match self.request(DaemonCommand::GetBattery)? {
            DaemonResponse::Battery(b) => Ok(b),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn get_input_state(&mut self) -> Result<ControllerState> {
        match self.request(DaemonCommand::GetInputState)? {
            DaemonResponse::InputState(s) => Ok(s),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn get_firmware_info(&mut self) -> Result<(u16, String, String)> {
        match self.request(DaemonCommand::GetFirmwareInfo)? {
            DaemonResponse::FirmwareInfo {
                version,
                build_date,
                build_time,
            } => Ok((version, build_date, build_time)),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn get_controller_info(&mut self) -> Result<Option<ControllerInfoTuple>> {
        match self.request(DaemonCommand::GetControllerInfo)? {
            DaemonResponse::ControllerInfo {
                serial,
                product_id,
                is_bt,
                kind,
                transport,
            } => Ok(Some((serial, product_id, is_bt, kind, transport))),
            DaemonResponse::NoDevice => Ok(None),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn get_steam_liveness(&mut self) -> Result<SteamLiveness> {
        match self.request(DaemonCommand::GetSteamLiveness)? {
            DaemonResponse::SteamLiveness(liveness) => Ok(liveness),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn set_lightbar(&mut self, r: u8, g: u8, b: u8, brightness: u8) -> Result<()> {
        self.request(DaemonCommand::SetLightbar {
            r,
            g,
            b,
            brightness,
        })
        .map(|_| ())
    }

    pub fn set_player_leds(&mut self, leds: u8) -> Result<()> {
        match self.request(DaemonCommand::SetPlayerLeds { leds })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_mic(&mut self, enabled: bool) -> Result<()> {
        match self.request(DaemonCommand::SetMic { enabled })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_mic_led(&mut self, state: MicLedState) -> Result<()> {
        match self.request(DaemonCommand::SetMicLed { state })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_trigger_off(&mut self) -> Result<()> {
        match self.request(DaemonCommand::SetTriggerOff)? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_trigger_effect(
        &mut self,
        right: bool,
        left: bool,
        effect_type: u8,
        params: [u8; 10],
    ) -> Result<()> {
        match self.request(DaemonCommand::SetTriggerEffect {
            right,
            left,
            effect_type,
            params,
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_trigger_effects(
        &mut self,
        left: Option<(u8, [u8; 10])>,
        right: Option<(u8, [u8; 10])>,
    ) -> Result<()> {
        match self.request(DaemonCommand::SetTriggerEffects { left, right })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_vibration(&mut self, rumble: u8, trigger: u8) -> Result<()> {
        match self.request(DaemonCommand::SetVibration { rumble, trigger })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_rumble(&mut self, left: u8, right: u8) -> Result<()> {
        match self.request(DaemonCommand::SetRumble { left, right })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_speaker(&mut self, mode: &str) -> Result<()> {
        match self.request(DaemonCommand::SetSpeaker {
            mode: mode.to_string(),
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<()> {
        match self.request(DaemonCommand::SetVolume { volume })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }
    pub fn set_update_mode(&mut self, active: bool) -> Result<()> {
        self.request(DaemonCommand::SetUpdateMode { active })
            .map(|_| ())
    }

    pub fn set_input_transform(&mut self, transform: InputTransform) -> Result<()> {
        match self.request(DaemonCommand::SetInputTransform { transform })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn clear_input_transform(&mut self) -> Result<()> {
        match self.request(DaemonCommand::ClearInputTransform)? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_lightbar_effect(&mut self, effect: LightbarEffect) -> Result<()> {
        match self.request(DaemonCommand::SetLightbarEffect { effect })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_haptic_pattern(
        &mut self,
        pattern: HapticPattern,
        strength: u8,
        speed: f32,
    ) -> Result<()> {
        match self.request(DaemonCommand::SetHapticPattern {
            pattern,
            strength,
            speed,
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_raw_haptics(&mut self, active: bool) -> Result<()> {
        self.request(DaemonCommand::SetRawHaptics { active })
            .map(|_| ())
    }

    pub fn set_gyro(&mut self, enabled: bool, smoothing: f32, sensitivity: f32) -> Result<()> {
        match self.request(DaemonCommand::SetGyro {
            enabled,
            smoothing,
            sensitivity,
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn set_motion(&mut self, enabled: bool) -> Result<()> {
        match self.request(DaemonCommand::SetMotion { enabled })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn switch_profile(&mut self, name: &str) -> Result<()> {
        match self.request(DaemonCommand::SwitchProfile {
            name: name.to_string(),
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn reload_profile(&mut self) -> Result<()> {
        match self.request(DaemonCommand::ReloadProfile)? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn list_profiles(&mut self) -> Result<Vec<String>> {
        match self.request(DaemonCommand::ListProfiles)? {
            DaemonResponse::ProfileList { profiles } => Ok(profiles),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn save_profile(&mut self, profile: Profile) -> Result<()> {
        match self.request(DaemonCommand::SaveProfile { profile })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<()> {
        match self.request(DaemonCommand::DeleteProfile {
            name: name.to_string(),
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn rename_profile(&mut self, old: &str, new: &str) -> Result<()> {
        match self.request(DaemonCommand::RenameProfile {
            old: old.to_string(),
            new: new.to_string(),
        })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }

    pub fn get_active_profile(&mut self) -> Result<String> {
        match self.request(DaemonCommand::GetActiveProfile)? {
            DaemonResponse::ActiveProfile { name } => Ok(name),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => bail!("Unexpected response"),
        }
    }

    pub fn set_controller_preference(&mut self, preference: ControllerPreference) -> Result<()> {
        match self.request(DaemonCommand::SetControllerPreference { preference })? {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => bail!("{}", message),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incompatible(response: Result<DaemonResponse>) -> bool {
        matches!(
            classify_handshake(response),
            DaemonCompatibility::Incompatible { .. }
        )
    }

    #[test]
    fn accepts_current_protocol_handshake() {
        let response = Ok(DaemonResponse::Pong {
            protocol: IPC_PROTOCOL_VERSION,
            app_version: "0.2.0".to_string(),
        });
        assert_eq!(
            classify_handshake(response),
            DaemonCompatibility::Compatible {
                protocol: IPC_PROTOCOL_VERSION,
                app_version: "0.2.0".to_string(),
            }
        );
    }

    #[test]
    fn rejects_old_protocol_pong() {
        assert!(incompatible(Ok(DaemonResponse::Pong {
            protocol: IPC_PROTOCOL_VERSION - 1,
            app_version: "0.1.0".to_string(),
        })));
    }

    #[test]
    fn protocol_eight_rejects_a_protocol_seven_daemon() {
        assert_eq!(IPC_PROTOCOL_VERSION, 8);
        assert!(incompatible(Ok(DaemonResponse::Pong {
            protocol: 7,
            app_version: "0.2.0".to_string(),
        })));
    }

    #[test]
    fn steam_liveness_round_trips_over_ipc() {
        let liveness = SteamLiveness {
            receiver_present: true,
            live: Some(crate::controller::SteamLiveSlot {
                vendor_id: crate::steam_controller::STEAM_VID,
                product_id: crate::steam_controller::STEAM_PID_PUCK,
                interface_number: 3,
                path: "/dev/hidraw13".to_string(),
                serial: Some("FXB9961001F79".to_string()),
                identity: "FXB9961001F79".to_string(),
                transport: "Wireless puck slot 3".to_string(),
            }),
        };
        let response = DaemonResponse::SteamLiveness(liveness.clone());
        let json = serde_json::to_string(&response).expect("serialize liveness");
        match serde_json::from_str::<DaemonResponse>(&json).expect("deserialize liveness") {
            DaemonResponse::SteamLiveness(decoded) => assert_eq!(decoded, liveness),
            _ => panic!("unexpected response"),
        }

        let command = serde_json::to_string(&DaemonCommand::GetSteamLiveness)
            .expect("serialize liveness command");
        assert!(command.contains("GetSteamLiveness"));
        assert!(serde_json::from_str::<DaemonCommand>(&command).is_ok());
    }

    #[test]
    fn rejects_pong_without_protocol_field() {
        let legacy: Result<DaemonResponse> =
            serde_json::from_str(r#"{"type":"Pong"}"#).map_err(Into::into);
        assert!(incompatible(legacy));
    }

    #[test]
    fn rejects_daemon_error_during_handshake() {
        assert!(incompatible(Ok(DaemonResponse::Error {
            message: "unknown variant `Ping`".to_string(),
        })));
    }

    #[test]
    fn rejects_daemon_notification_of_protocol_mismatch() {
        assert!(incompatible(Ok(DaemonResponse::IncompatibleProtocol {
            daemon_protocol: IPC_PROTOCOL_VERSION + 1,
            daemon_version: "0.3.0".to_string(),
        })));
    }

    #[test]
    fn ping_serializes_protocol_and_version() {
        let json = serde_json::to_string(&DaemonCommand::Ping {
            protocol: IPC_PROTOCOL_VERSION,
            app_version: APP_VERSION.to_string(),
        })
        .expect("serialize");
        assert!(json.contains("\"protocol\""));
        assert!(json.contains("\"app_version\""));
    }

    #[test]
    fn legacy_controller_info_without_kind_is_not_accepted() {
        let response =
            r#"{"type":"ControllerInfo","data":{"serial":"x","product_id":3302,"is_bt":false}}"#;
        assert!(serde_json::from_str::<DaemonResponse>(response).is_err());
    }
}
