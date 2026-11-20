use hidapi::HidApi;
use std::{
    sync::{self, Arc, Mutex, mpsc},
    thread::{self, sleep},
    time::{Duration, Instant},
};

use crate::{
    backend::{ControllerBackend, DirectBackend, IpcBackend, TRIGGER_OFF},
    common::*,
    controller::{self, ControllerFamily, PhysicalController},
    daemon::DaemonManager,
    firmware_controller::FirmwareController,
    gyro::GyroConfig,
    input_poller::InputPoller,
    inputs::{ControllerKind, ControllerState},
    ipc::{DaemonCompatibility, IpcClient, probe_daemon, socket_path},
    keycodes,
    profiles::{
        Profile, ProfileManager, ProfileNameConflict, TriggerConfig, profile_name_conflict,
    },
    remap::{Binding, BindingSlot, Key, StickConfig},
    settings::{ControllerPreference, Settings, SettingsManager},
    state::*,
    stick_mouse::StickMouseConfig,
    theme::{Theme, ThemeManager},
    touchpad_filter::TouchpadFilter,
    transform::InputTransform,
    util::{RateLimiter, mlock},
};

fn input_signature(s: &ControllerState) -> (u32, u32, u8, bool, bool, u8) {
    (
        s.buttons,
        s.extra_buttons,
        s.dpad,
        s.steam_left_pad.touch,
        s.steam_right_pad.touch,
        s.right_y,
    )
}

pub(crate) const STEAM_WAKE_RETRY: &str = "Steam Controller not detected — turn it on; retrying";

fn pending_status_text(
    pending: PendingControllerTarget,
    current: Option<ControllerFamily>,
    present: bool,
    live: bool,
) -> String {
    let steam_wake = pending.family == ControllerFamily::Steam && !live;
    if steam_wake {
        STEAM_WAKE_RETRY.to_string()
    } else if pending.since.elapsed() < Duration::from_secs(5) {
        format!("Switching to {}...", pending.family.label())
    } else if present {
        format!("Could not switch to {} - retrying", pending.family.label())
    } else if let Some(current) = current {
        format!(
            "{} not detected - retrying (still using {})",
            pending.family.label(),
            current.label()
        )
    } else {
        format!("{} not detected - retrying", pending.family.label())
    }
}

pub(crate) struct DS4UApp {
    pub(crate) settings: Settings,
    pub(crate) settings_manager: SettingsManager,
    pub(crate) theme: Theme,
    pub(crate) theme_manager: ThemeManager,

    api: HidApi,
    pub(crate) controller: Option<Arc<Mutex<PhysicalController>>>,
    pub(crate) controller_kind: Option<ControllerKind>,
    pub(crate) controller_inventory: Vec<controller::FamilyRow>,
    pub(crate) pending_controller_target: Option<PendingControllerTarget>,
    pub(crate) controller_switch_error: String,
    steam_live_slot: Option<controller::SteamLiveSlot>,
    last_steam_liveness_check: Instant,

    pub(crate) ipc: Option<Arc<Mutex<IpcClient>>>,

    pub(crate) last_connection_check: Instant,

    pub(crate) active_section: Section,

    pub(crate) controller_is_bt: Option<bool>,
    pub(crate) controller_product_id: Option<u16>,

    pub(crate) profile_manager: ProfileManager,
    pub(crate) current_profile: Option<Profile>,
    pub(crate) profile_edit_name: String,
    pub(crate) profile_create_error: String,
    pub(crate) profile_convert_offer: Option<String>,

    daemon_manager: DaemonManager,

    pub(crate) battery_info: Option<BatteryInfo>,
    pub(crate) last_battery_update: Instant,

    pub(crate) lightbar: LightbarState,
    pub(crate) player_leds: u8,
    pub(crate) microphone: MicrophoneState,
    pub(crate) triggers: TriggersState,
    pub(crate) sticks: StickSettings,
    pub(crate) audio: AudioSettings,
    pub(crate) vibration: VibrationSettings,
    pub(crate) gyro: GyroState,
    pub(crate) touchpad: TouchpadState,
    pub(crate) haptic_state: HapticState,
    pub(crate) remap: RemapState,

    pub(crate) firmware: FirmwareController,
    pub(crate) input: InputPoller,
    pub(crate) nav: crate::ui::navigation::Navigator,
    pub(crate) egui_ctx: Option<egui::Context>,

    pub(crate) controller_serial: Option<String>,

    pub(crate) status_message: String,
    pub(crate) error_message: String,

    pending_connect_since: Option<Instant>,
    pub(crate) input_transform: InputTransform,

    pub(crate) lightbar_effect: LightbarEffect,

    pub(crate) daemon_alive_cached: bool,
    pub(crate) daemon_incompatible: Option<String>,
    last_daemon_probe: Instant,
    discovery_error_limiter: RateLimiter,
    haptic_stream: crate::haptics_stream::HapticStream,
    usb_haptic_stream: crate::usb_haptics::UsbHapticStream,
    daemon_raw_active: bool,
    touchpad_filter: Option<TouchpadFilter>,
}

impl DS4UApp {
    pub(crate) fn new() -> Self {
        let api = HidApi::new().unwrap();

        let settings_manager = SettingsManager::new();
        let settings = settings_manager.load();
        let theme_manager = ThemeManager::new();
        let theme = theme_manager.load_by_id(&settings.theme_id);

        let mut app = Self {
            settings,
            settings_manager,
            theme,
            theme_manager,

            api,
            controller: None,
            controller_kind: None,
            controller_inventory: Vec::new(),
            pending_controller_target: None,
            controller_switch_error: String::new(),
            steam_live_slot: None,
            last_steam_liveness_check: Instant::now() - Duration::from_secs(10),
            ipc: None,
            last_connection_check: Instant::now(),

            active_section: Section::Inputs,

            controller_is_bt: None,
            controller_product_id: None,

            profile_manager: ProfileManager::new(),
            current_profile: None,
            profile_edit_name: String::new(),
            profile_create_error: String::new(),
            profile_convert_offer: None,

            daemon_manager: DaemonManager::new(),

            battery_info: None,
            last_battery_update: Instant::now() - Duration::from_secs(10),

            lightbar: LightbarState {
                r: 0.0,
                g: 0.5,
                b: 1.0,
                brightness: 255.0,
                enabled: true,
            },

            player_leds: 1,

            microphone: MicrophoneState {
                enabled: false,
                led_state: MicLedState::Off,
            },

            triggers: TriggersState {
                left: TriggerConfig::default(),
                right: TriggerConfig::default(),
            },

            sticks: StickSettings {
                left_curve: SensitivityCurve::Default,
                right_curve: SensitivityCurve::Default,
                left_deadzone: 0.1,
                right_deadzone: 0.1,
                left_outer_deadzone: 1.0,
                right_outer_deadzone: 1.0,
                left_invert_x: false,
                left_invert_y: false,
                right_invert_x: false,
                right_invert_y: false,
                swap: false,
                digital_left_deadzone: 0.4,
                digital_right_deadzone: 0.4,
                digital_left_analog: false,
                digital_right_analog: false,
                stick_mouse: StickMouseConfig::default(),
            },

            audio: AudioSettings {
                volume: 0,
                speaker_mode: SpeakerMode::Internal,
            },

            vibration: VibrationSettings {
                rumble: 0,
                trigger: 0,
            },

            gyro: GyroState {
                mouse: GyroConfig::default(),
                calibration: None,
            },

            touchpad: TouchpadState {
                show_overlay: true,
                mode: TouchpadMode::Disabled,
                tap_to_click: true,
                natural_scrolling: false,
                sensitivity: 1.0,
                left_mouse: crate::trackpad_mouse::SteamPadMouseConfig::default(),
                right_mouse: crate::trackpad_mouse::SteamPadMouseConfig::default(),
            },

            haptic_state: HapticState {
                pattern: HapticPattern::None,
                strength: 0,
                speed: 1.0,
            },

            remap: RemapState::default(),

            firmware: FirmwareController::new(),
            input: InputPoller::new(),
            nav: crate::ui::navigation::Navigator::new(),
            egui_ctx: None,

            controller_serial: None,

            status_message: String::new(),
            error_message: String::new(),

            pending_connect_since: None,
            input_transform: InputTransform::default(),

            lightbar_effect: LightbarEffect::None,

            daemon_alive_cached: false,
            daemon_incompatible: None,
            last_daemon_probe: Instant::now() - Duration::from_secs(10),
            discovery_error_limiter: RateLimiter::new(Duration::from_secs(5)),
            haptic_stream: crate::haptics_stream::HapticStream::new(),
            usb_haptic_stream: crate::usb_haptics::UsbHapticStream::new(),
            daemon_raw_active: false,
            touchpad_filter: None,
        };

        if app.settings.auto_start_daemon && !app.daemon_alive() {
            app.start_daemon_process();
        }

        app.check_for_controller();

        app.load_profile_for_device();

        app
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.controller.is_some() || self.ipc.is_some()
    }

    fn backend(&self) -> Option<Box<dyn ControllerBackend>> {
        if let Some(ipc) = &self.ipc {
            return Some(Box::new(IpcBackend(ipc.clone())));
        }
        if let Some(ctrl) = &self.controller {
            return Some(Box::new(DirectBackend(ctrl.clone())));
        }
        None
    }

    pub(crate) fn start_input_polling(&mut self) {
        let (tx, rx) = mpsc::channel();
        let stop_flag = Arc::new(sync::atomic::AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);

        self.input.state_rx = Some(rx);
        self.input.stop = Some(stop_flag);
        self.input.polling = true;
        self.input.last_state_at = Instant::now();

        let waker = self.egui_ctx.clone();

        let handle = if self.ipc.is_some() {
            if let Some(ipc) = &self.ipc
                && let Ok(mut client) = ipc.try_lock()
            {
                let _ = client.set_motion(true);
            }
            let path = socket_path();
            thread::spawn(move || {
                let mut client = match IpcClient::connect(&path) {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let _ = client.set_motion(true);
                let mut last_signature = None;
                while !stop_clone.load(sync::atomic::Ordering::Relaxed) {
                    match client.get_input_state() {
                        Ok(state) => {
                            let signature = input_signature(&state);
                            let _ = tx.send(state);
                            if let Some(ctx) = &waker
                                && last_signature != Some(signature)
                            {
                                ctx.request_repaint();
                            }
                            last_signature = Some(signature);
                            sleep(Duration::from_millis(8));
                        }
                        Err(_) => {
                            sleep(Duration::from_millis(8));
                        }
                    }
                }
                let _ = client.set_motion(false);
            })
        } else {
            let Some(controller) = self.controller.clone() else {
                return;
            };
            thread::spawn(move || {
                {
                    let mut reader = mlock(&controller);
                    let _ = reader.set_motion(true);
                }
                let mut last_signature = None;
                while !stop_clone.load(sync::atomic::Ordering::Relaxed) {
                    let state = {
                        let mut reader = mlock(&controller);
                        reader.get_input_state()
                    };
                    match state {
                        Ok(state) => {
                            let signature = input_signature(&state);
                            let _ = tx.send(state);
                            if let Some(ctx) = &waker
                                && last_signature != Some(signature)
                            {
                                ctx.request_repaint();
                            }
                            last_signature = Some(signature);
                            sleep(Duration::from_millis(2));
                        }
                        Err(_) => {
                            sleep(Duration::from_millis(8));
                        }
                    }
                }
            })
        };
        self.input.thread = Some(handle);
    }

    pub(crate) fn stop_input_polling(&mut self) {
        self.input.stop();
        if let Some(ipc) = &self.ipc {
            if let Ok(mut client) = ipc.try_lock() {
                let _ = client.set_motion(false);
            }
        } else if let Some(controller) = &self.controller {
            let _ = mlock(controller).set_motion(false);
        }
    }

    fn stop_input_polling_for_handoff(&mut self) {
        if let Some(controller) = &self.controller {
            mlock(controller).prepare_handoff();
        }
        self.input.stop();
    }

    fn handoff_direct_controller(&mut self) {
        if self.controller.is_none() {
            return;
        }
        self.stop_input_polling_for_handoff();
        self.controller = None;
        self.battery_info = None;
    }

    fn attach_daemon_waiting(&mut self, client: Arc<Mutex<IpcClient>>) {
        self.handoff_direct_controller();
        self.input.stop();
        self.stop_touchpad_filter();
        self.battery_info = None;
        self.controller_serial = None;
        self.controller_is_bt = None;
        self.controller_product_id = None;
        self.controller_kind = None;
        self.steam_live_slot = None;
        self.last_steam_liveness_check = Instant::now() - Duration::from_secs(10);
        self.ipc = Some(client);
        self.status_message = "Attached to daemon (waiting for controller)".to_string();
        self.error_message.clear();
    }

    pub(crate) fn current_family(&self) -> Option<ControllerFamily> {
        self.controller_kind.map(controller::family_of_kind)
    }
    fn inventory_has(&self, family: ControllerFamily) -> bool {
        self.controller_inventory
            .iter()
            .any(|row| row.family == family)
    }

    fn family_live(&self, family: ControllerFamily) -> bool {
        self.controller_inventory
            .iter()
            .any(|row| row.family == family && row.live)
    }

    fn refresh_controller_inventory(&mut self) {
        self.refresh_steam_liveness();
        let (devices, _) = controller::scan_devices(&self.api);
        self.controller_inventory = controller::inventory_rows(
            &devices,
            self.current_family(),
            self.controller_serial.as_deref(),
            self.steam_live_slot.as_ref(),
        );
    }

    fn refresh_steam_liveness(&mut self) {
        if self.ipc.is_none() {
            self.steam_live_slot = None;
            return;
        }
        if self.last_steam_liveness_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_steam_liveness_check = Instant::now();
        let Some(ipc) = self.ipc.clone() else {
            return;
        };
        let Ok(mut client) = ipc.try_lock() else {
            return;
        };
        if let Ok(liveness) = client.get_steam_liveness() {
            self.steam_live_slot = liveness.live;
        }
    }

    fn install_controller(&mut self, controller: PhysicalController) {
        let info = controller.info().clone();
        let firmware = controller
            .capabilities()
            .firmware
            .then(|| controller.get_firmware_info().ok())
            .flatten();

        // Stop the old reader only after the replacement handle opened.
        self.stop_input_polling();
        self.stop_touchpad_filter();

        if let Some((version, build_date, build_time)) = firmware {
            self.firmware.current_version = Some(version);
            self.firmware.build_date = Some(build_date);
            self.firmware.build_time = Some(build_time);
        } else {
            self.firmware.current_version = None;
            self.firmware.build_date = None;
            self.firmware.build_time = None;
        }

        self.controller_serial = Some(info.identity.clone());
        self.controller_is_bt = Some(info.is_bluetooth);
        self.controller_product_id = Some(info.product_id);
        self.controller_kind = Some(info.kind);
        self.controller = Some(Arc::new(Mutex::new(controller)));
        self.firmware.latest_version = None;
        self.clear_pending_target();
        self.status_message = format!("{} connected", info.kind.label());
        self.error_message.clear();
        self.lightbar.enabled = true;
        self.load_profile_for_device();
        if self.is_steam() {
            self.battery_info = None;
        } else {
            self.update_battery();
        }
        self.apply_lightbar();
        self.apply_player_leds();
        self.apply_microphone();
        self.apply_input_transform();
        self.apply_triggers();
        self.start_touchpad_filter();
        self.refresh_controller_inventory();
        if let Some(ctx) = &self.egui_ctx {
            ctx.request_repaint();
        }
    }

    fn connect_controller(&mut self) {
        match PhysicalController::discover(&self.api, None, self.settings.controller_preference) {
            Ok(controller) => self.install_controller(controller),
            Err(e) => {
                if self.controller.is_none() {
                    self.controller_kind = None;
                }
                if self.discovery_error_limiter.ready() {
                    let message = e.to_string();
                    eprintln!("[gyro4u] controller discovery failed: {}", message);
                    self.error_message = message;
                }
                self.stop_touchpad_filter();
            }
        }
    }

    fn load_profile_for_device(&mut self) {
        let kind = self.controller_kind.unwrap_or(ControllerKind::DualSense);
        let current = self.current_profile.as_ref();
        if current.is_some_and(|profile| profile.matches_device(kind)) {
            return;
        }
        let remembered = self
            .settings
            .remembered_profile(kind.is_steam())
            .to_string();
        let loaded = if self.profile_manager.profile_exists(&remembered) {
            self.profile_manager
                .load_profile(&remembered)
                .ok()
                .filter(|profile| profile.matches_device(kind))
        } else {
            None
        };
        let profile = loaded.unwrap_or_else(|| self.profile_manager.ensure_default_for(kind));
        self.load_profile(&profile);
    }

    fn begin_pending_target(&mut self, family: ControllerFamily) {
        match self.pending_controller_target {
            Some(pending) if pending.family == family => {}
            _ => {
                self.pending_controller_target = Some(PendingControllerTarget {
                    family,
                    since: Instant::now(),
                });
                self.controller_switch_error.clear();
            }
        }
    }

    fn clear_pending_target(&mut self) {
        self.pending_controller_target = None;
        self.controller_switch_error.clear();
    }

    fn update_pending_message(&mut self) {
        let Some(pending) = self.pending_controller_target else {
            return;
        };
        if self.current_family() == Some(pending.family) {
            self.clear_pending_target();
            return;
        }
        let present = self.inventory_has(pending.family);
        let live = self.family_live(pending.family);
        let steam_wake = pending.family == ControllerFamily::Steam && !live;
        let message = pending_status_text(pending, self.current_family(), present, live);
        self.status_message = message.clone();
        if steam_wake {
            self.controller_switch_error.clear();
        } else if pending.since.elapsed() >= Duration::from_secs(5) {
            self.controller_switch_error = message;
        }
    }

    fn attempt_pending_target(&mut self) -> bool {
        let Some(pending) = self.pending_controller_target else {
            return false;
        };
        let current = self.current_family();
        if current == Some(pending.family) {
            self.clear_pending_target();
            return true;
        }
        let present = self.inventory_has(pending.family);
        let live = self.family_live(pending.family);
        if !present {
            if pending.family == ControllerFamily::Steam {
                self.controller_switch_error.clear();
                self.status_message = pending_status_text(pending, current, present, live);
            } else {
                self.controller_switch_error = match current {
                    Some(current) => format!(
                        "{} not detected - still using {}",
                        pending.family.label(),
                        current.label()
                    ),
                    None => format!("{} not detected", pending.family.label()),
                };
                self.status_message = self.controller_switch_error.clone();
            }
            return false;
        }

        let target_preference =
            if controller::requested_family(self.settings.controller_preference).is_some() {
                pending.family.preference()
            } else {
                self.settings.controller_preference
            };
        match PhysicalController::discover(&self.api, None, target_preference) {
            Ok(controller) => {
                self.install_controller(controller);
                true
            }
            Err(e) => {
                if pending.family == ControllerFamily::Steam && !live {
                    self.controller_switch_error.clear();
                    self.status_message = pending_status_text(pending, current, present, live);
                } else {
                    self.controller_switch_error =
                        format!("Could not switch to {}: {e}", pending.family.label());
                    self.status_message = self.controller_switch_error.clone();
                    if self.discovery_error_limiter.ready() {
                        eprintln!("[gyro4u] {}", self.controller_switch_error);
                    }
                }
                false
            }
        }
    }

    fn evaluate_direct_target(&mut self) {
        let Some(current) = self.current_family() else {
            return;
        };
        let preference = self.settings.controller_preference;
        let target = controller::requested_family(preference).or_else(|| {
            controller::switch_target(
                preference,
                Some(current),
                self.inventory_has(ControllerFamily::DualSense),
                self.inventory_has(ControllerFamily::Steam),
            )
        });
        let Some(target) = target else {
            return;
        };
        if target == current {
            if self
                .pending_controller_target
                .is_some_and(|p| p.family == target)
            {
                self.clear_pending_target();
            }
            return;
        }
        self.begin_pending_target(target);
        let _ = self.attempt_pending_target();
    }

    pub(crate) fn connect_via_daemon(&mut self, client: Arc<Mutex<IpcClient>>) {
        self.handoff_direct_controller();
        self.stop_touchpad_filter();
        let mut c = mlock(&client);

        if let Ok(Some((serial, pid, is_bt, kind, _transport))) = c.get_controller_info() {
            self.controller_serial = Some(serial);
            self.controller_is_bt = Some(is_bt);
            self.controller_product_id = Some(pid);
            self.controller_kind = Some(kind);
        }

        if self
            .controller_kind
            .is_some_and(|kind| kind.capabilities().firmware)
            && let Ok((ver, date, time)) = c.get_firmware_info()
        {
            self.firmware.current_version = Some(ver);
            self.firmware.build_date = Some(date);
            self.firmware.build_time = Some(time);
        }

        drop(c);

        self.firmware.latest_version = None;
        self.ipc = Some(client);
        self.status_message = self
            .controller_kind
            .map(|kind| format!("{} connected (via daemon)", kind.label()))
            .unwrap_or_else(|| "Controller connected (via daemon)".to_string());
        self.error_message.clear();
        self.lightbar.enabled = true;
        self.update_battery();
        self.apply_lightbar();
        self.apply_player_leds();
        self.apply_microphone();
        self.apply_input_transform();
        self.apply_triggers();
        self.apply_haptic_pattern();
        self.load_profile_for_device();
        self.sync_active_profile();
        if self
            .pending_controller_target
            .is_some_and(|pending| Some(pending.family) == self.current_family())
        {
            self.clear_pending_target();
        }
        let _ = self.refresh_hid_devices();
        self.refresh_controller_inventory();
    }

    fn disconnect_controller(&mut self) {
        self.stop_touchpad_filter();
        self.controller = None;
        self.battery_info = None;
        self.ipc = None;
        self.controller_is_bt = None;
        self.controller_product_id = None;
        self.controller_serial = None;
        self.controller_kind = None;
        self.steam_live_slot = None;
        self.status_message = "Controller disconnected".to_string();
    }

    pub(crate) fn update_battery(&mut self) {
        self.last_battery_update = Instant::now();

        if self.firmware.updating {
            return;
        }

        if let Some(ref ipc) = self.ipc.clone() {
            match mlock(ipc).get_battery() {
                Ok(info) => self.battery_info = Some(info),
                Err(_) if self.controller_kind.is_some() && !self.is_steam() => {
                    self.stop_input_polling();
                    self.disconnect_controller();
                }
                Err(_) => {}
            }
            return;
        }

        let Some(controller) = &self.controller else {
            return;
        };
        let Ok(mut ctrl) = controller.try_lock() else {
            return;
        };

        match ctrl.get_battery() {
            Ok(info) => self.battery_info = Some(info),
            Err(_) if !self.is_steam() => {
                drop(ctrl);
                self.disconnect_controller();
            }
            Err(_) => {}
        }
    }

    pub(crate) fn check_firmware_progress(&mut self) {
        let updates: Vec<ProgressUpdate> = self
            .firmware
            .progress_rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();

        for update in updates {
            match update {
                ProgressUpdate::Progress(p) => {
                    self.firmware.progress = p;
                }
                ProgressUpdate::Status(s) => {
                    self.firmware.status = s;
                }
                ProgressUpdate::Complete => {
                    self.firmware.updating = false;
                    self.firmware.last_flash_result = Some(Ok(()));
                    self.status_message = "Firmware update completed".to_string();
                    self.firmware.progress = 100;
                    self.firmware.update_mode_flag = None;
                    self.daemon_manager.set_update_in_progress(false);
                    if self.firmware.used_daemon {
                        self.firmware.used_daemon = false;
                        self.controller = None;
                    }
                    self.firmware.reap_thread();
                }
                ProgressUpdate::Error(e) => {
                    let was_flashing = self.firmware.updating;
                    self.firmware.updating = false;
                    self.firmware.checking_latest = false;
                    if was_flashing {
                        self.firmware.last_flash_result = Some(Err(e.clone()));
                    }
                    self.error_message = e;
                    self.firmware.progress = 0;
                    self.firmware.update_mode_flag = None;
                    self.daemon_manager.set_update_in_progress(false);
                    if self.firmware.used_daemon {
                        self.firmware.used_daemon = false;
                        self.controller = None;
                    }
                    self.firmware.reap_thread();
                }
                ProgressUpdate::LatestVersion(v) => {
                    self.firmware.latest_version = Some(v);
                    self.firmware.checking_latest = false;
                    self.firmware.reap_thread();
                }
            }
        }
    }

    pub(crate) fn haptic_stream_active(&self) -> bool {
        self.haptic_stream.is_active()
            || self.usb_haptic_stream.is_active()
            || self.daemon_raw_active
    }

    pub(crate) fn start_raw_haptics(&mut self) {
        if self.is_steam() {
            self.error_message =
                "Raw haptics are not implemented for the Steam Controller yet".to_string();
            return;
        }
        let pattern = self.haptic_state.pattern;
        let strength = self.haptic_state.strength;
        let speed = self.haptic_state.speed;
        self.error_message.clear();
        match self.controller_is_bt {
            Some(true) => {
                if let Some(ctrl) = self.controller.clone() {
                    self.stop_input_polling();
                    self.haptic_stream.start(ctrl, pattern, strength, speed);
                    self.status_message = "Streaming raw haptics (Bluetooth)".to_string();
                } else if let Some(ipc) = self.ipc.clone() {
                    match mlock(&ipc).set_raw_haptics(true) {
                        Ok(()) => {
                            self.daemon_raw_active = true;
                            self.status_message =
                                "Streaming raw haptics (Bluetooth via daemon)".to_string();
                        }
                        Err(e) => self.error_message = format!("Daemon haptics failed: {e}"),
                    }
                } else {
                    self.error_message = "No controller connected".to_string();
                }
            }
            Some(false) => {
                let Some(card) = crate::usb_haptics::find_dualsense_card() else {
                    self.error_message =
                        "Couldn't find the controller's USB audio device".to_string();
                    return;
                };
                let _ = self.usb_haptic_stream.start(card, pattern, strength, speed);
                self.status_message = "Streaming raw haptics (USB)".to_string();
            }
            None => {
                self.error_message = "No controller connected".to_string();
            }
        }
    }

    pub(crate) fn stop_raw_haptics(&mut self) {
        self.haptic_stream.stop();
        self.usb_haptic_stream.stop();
        if self.daemon_raw_active {
            if let Some(ipc) = self.ipc.clone() {
                let _ = mlock(&ipc).set_raw_haptics(false);
            }
            self.daemon_raw_active = false;
        }
        self.status_message = "Raw haptics stopped".to_string();
    }

    pub(crate) fn update_raw_haptics(&mut self) {
        let p = self.haptic_state.pattern;
        let s = self.haptic_state.strength;
        let sp = self.haptic_state.speed;
        self.haptic_stream.set_params(p, s, sp);
        self.usb_haptic_stream.set_params(p, s, sp);
    }

    fn acquire_direct_fw(&mut self) -> bool {
        if self.is_steam() {
            self.error_message =
                "Firmware updates are not supported for the Steam Controller".to_string();
            return false;
        }
        if self.controller.is_some() {
            return true;
        }

        self.daemon_manager.set_update_in_progress(true);
        sleep(Duration::from_millis(1500));

        if self.api.refresh_devices().is_err() {
            self.daemon_manager.set_update_in_progress(false);
            return false;
        }

        match PhysicalController::discover(&self.api, None, self.settings.controller_preference) {
            Ok(controller) => {
                self.controller_kind = Some(controller.kind());
                self.controller = Some(Arc::new(Mutex::new(controller)));
                self.firmware.used_daemon = true;
                true
            }
            Err(e) => {
                self.error_message = format!("Cannot open device for flash update: {}", e);
                self.daemon_manager.set_update_in_progress(false);
                false
            }
        }
    }

    pub(crate) fn capabilities(&self) -> Option<crate::inputs::ControllerCapabilities> {
        self.controller_kind.map(|kind| kind.capabilities())
    }

    pub(crate) fn is_steam(&self) -> bool {
        self.capabilities().is_some_and(|caps| {
            caps.kind.is_steam() && caps.touchpad == crate::inputs::TouchpadKind::DualCircular
        })
    }

    pub(crate) fn apply_lightbar(&mut self) {
        if !self.capabilities().is_some_and(|caps| caps.lightbar) {
            return;
        }
        let r = (self.lightbar.r * 255.0) as u8;
        let g = (self.lightbar.g * 255.0) as u8;
        let b = (self.lightbar.b * 255.0) as u8;
        let br = self.lightbar.brightness as u8;
        if let Some(be) = self.backend() {
            be.set_lightbar(r, g, b, br);
        }
    }

    pub(crate) fn apply_lightbar_effect(&mut self) {
        if !self.capabilities().is_some_and(|caps| caps.lightbar) {
            return;
        }
        if let Some(be) = self.backend() {
            be.set_lightbar_effect(self.lightbar_effect.clone());
        }
    }

    pub(crate) fn apply_haptic_pattern(&mut self) {
        if !self.capabilities().is_some_and(|caps| caps.haptics) {
            return;
        }
        if let Some(be) = self.backend() {
            be.set_haptic_pattern(
                self.haptic_state.pattern,
                self.haptic_state.strength,
                self.haptic_state.speed,
            );
        }
    }

    pub(crate) fn apply_player_leds(&mut self) {
        if !self.capabilities().is_some_and(|caps| caps.player_leds) {
            return;
        }
        if let Some(be) = self.backend() {
            be.set_player_leds(self.player_leds);
        }
    }

    pub(crate) fn apply_microphone(&mut self) {
        if !self.capabilities().is_some_and(|caps| caps.microphone) {
            return;
        }
        if let Some(be) = self.backend() {
            be.set_mic(self.microphone.enabled);
            be.set_mic_led(self.microphone.led_state);
        }
    }

    pub(crate) fn apply_vibration(&mut self) {
        if !self.capabilities().is_some_and(|caps| caps.haptics) {
            return;
        }
        if let Some(be) = self.backend() {
            be.set_vibration(self.vibration.rumble, self.vibration.trigger);
        }
    }

    pub(crate) fn test_rumble(&self, amp_l: u8, amp_r: u8, duration_ms: u64) {
        if !self.capabilities().is_some_and(|caps| caps.haptics) {
            return;
        }
        let ctrl = self.controller.clone();
        let ipc = self.ipc.clone();
        std::thread::spawn(move || {
            if let Some(ctrl) = &ctrl
                && let Ok(mut c) = ctrl.lock()
            {
                let _ = c.set_rumble(amp_l, amp_r);
            } else if let Some(ipc) = &ipc
                && let Ok(mut c) = ipc.lock()
            {
                let _ = c.set_rumble(amp_l, amp_r);
            }
            std::thread::sleep(std::time::Duration::from_millis(duration_ms));
            if let Some(ctrl) = &ctrl
                && let Ok(mut c) = ctrl.lock()
            {
                let _ = c.set_rumble(0, 0);
            } else if let Some(ipc) = &ipc
                && let Ok(mut c) = ipc.lock()
            {
                let _ = c.set_rumble(0, 0);
            }
        });
    }

    pub(crate) fn apply_triggers(&mut self) {
        if !self
            .capabilities()
            .is_some_and(|caps| caps.adaptive_triggers)
        {
            return;
        }
        let to_eff = |cfg: &TriggerConfig| {
            if matches!(cfg.mode, TriggerMode::Off) {
                None
            } else {
                Some(cfg.to_effect())
            }
        };
        let l = to_eff(&self.triggers.left).or(Some(TRIGGER_OFF));
        let r = to_eff(&self.triggers.right).or(Some(TRIGGER_OFF));
        if let Some(be) = self.backend() {
            be.set_trigger_effects(l, r);
        }
    }

    pub(crate) fn load_profile(&mut self, profile: &Profile) {
        self.load_profile_inner(profile, true);
    }

    fn load_profile_inner(&mut self, profile: &Profile, notify_daemon: bool) {
        self.lightbar.r = profile.lightbar_r;
        self.lightbar.g = profile.lightbar_g;
        self.lightbar.b = profile.lightbar_b;
        self.lightbar.brightness = profile.lightbar_brightness;
        self.player_leds = profile.player_leds;
        self.microphone.enabled = profile.mic_enabled;

        self.sticks.left_curve = profile.stick_left_curve.clone();
        self.sticks.right_curve = profile.stick_right_curve.clone();
        self.sticks.left_deadzone = profile.stick_left_deadzone;
        self.sticks.right_deadzone = profile.stick_right_deadzone;
        self.sticks.left_outer_deadzone = profile.stick_left_outer_deadzone;
        self.sticks.right_outer_deadzone = profile.stick_right_outer_deadzone;
        self.sticks.left_invert_x = profile.stick_left_invert_x;
        self.sticks.left_invert_y = profile.stick_left_invert_y;
        self.sticks.right_invert_x = profile.stick_right_invert_x;
        self.sticks.right_invert_y = profile.stick_right_invert_y;
        self.sticks.swap = profile.stick_swap;
        self.sticks.digital_left_deadzone = profile.stick_digital.left_deadzone;
        self.sticks.digital_right_deadzone = profile.stick_digital.right_deadzone;
        self.sticks.digital_left_analog = profile.stick_digital.left_analog;
        self.sticks.digital_right_analog = profile.stick_digital.right_analog;
        self.sticks.stick_mouse = profile.stick_mouse.clone();

        self.triggers.left = profile.trigger_left_config.clone();
        self.triggers.right = profile.trigger_right_config.clone();

        self.gyro.mouse = profile.gyro_mouse.clone();
        self.gyro.calibration = None;
        self.touchpad.mode = if !profile.touchpad_enabled {
            TouchpadMode::Disabled
        } else {
            profile.touchpad_mode
        };
        self.touchpad.show_overlay = profile.touchpad_show_overlay;
        self.touchpad.tap_to_click = profile.touchpad_tap_to_click;
        self.touchpad.natural_scrolling = profile.touchpad_natural_scrolling;
        self.touchpad.sensitivity = profile.touchpad_sensitivity;
        self.touchpad.left_mouse = profile.steam_left_pad_mouse.clone();
        self.touchpad.right_mouse = profile.steam_right_pad_mouse.clone();

        self.haptic_state.pattern = profile.haptic_pattern;
        self.haptic_state.strength = profile.haptic_strength;
        self.haptic_state.speed = profile.haptic_speed;

        self.remap.bindings = profile.button_bindings.clone();
        self.remap.layers = profile.layers.clone();
        self.remap.press_timing = profile.press_timing.clone();
        self.remap.selected = None;
        self.remap.selected_layer = None;
        self.remap.capture = None;

        self.current_profile = Some(profile.clone());

        self.settings
            .remember_profile(profile.device.is_steam(), &profile.name);
        self.settings_manager.save(&self.settings);

        self.apply_lightbar();
        self.apply_player_leds();
        self.apply_microphone();
        self.apply_input_transform();
        self.apply_triggers();
        self.apply_haptic_pattern();

        if notify_daemon && let Some(ref ipc) = self.ipc.clone() {
            let _ = mlock(ipc).switch_profile(&profile.name);
        }
    }

    fn sync_active_profile(&mut self) {
        let Some(ipc) = self.ipc.clone() else {
            return;
        };
        let Ok(name) = mlock(&ipc).get_active_profile() else {
            return;
        };
        if name.is_empty()
            || self
                .current_profile
                .as_ref()
                .is_some_and(|p| p.name == name)
        {
            return;
        }
        if let Ok(profile) = self.profile_manager.load_profile(&name) {
            self.load_profile_inner(&profile, false);
        }
    }

    pub(crate) fn sync_profile(&mut self) {
        if let Err(e) = self.sync_profile_checked() {
            self.error_message = e;
        }
    }

    pub(crate) fn sync_profile_checked(&mut self) -> Result<(), String> {
        let stick_digital = self.stick_config();
        let Some(profile) = self.current_profile.as_mut() else {
            return Err("No active profile to save".to_string());
        };

        profile.lightbar_r = self.lightbar.r;
        profile.lightbar_g = self.lightbar.g;
        profile.lightbar_b = self.lightbar.b;
        profile.lightbar_brightness = self.lightbar.brightness;
        profile.player_leds = self.player_leds;
        profile.mic_enabled = self.microphone.enabled;

        profile.stick_left_curve = self.sticks.left_curve.clone();
        profile.stick_right_curve = self.sticks.right_curve.clone();
        profile.stick_left_deadzone = self.sticks.left_deadzone;
        profile.stick_right_deadzone = self.sticks.right_deadzone;
        profile.stick_left_outer_deadzone = self.sticks.left_outer_deadzone;
        profile.stick_right_outer_deadzone = self.sticks.right_outer_deadzone;
        profile.stick_left_invert_x = self.sticks.left_invert_x;
        profile.stick_left_invert_y = self.sticks.left_invert_y;
        profile.stick_right_invert_x = self.sticks.right_invert_x;
        profile.stick_right_invert_y = self.sticks.right_invert_y;
        profile.stick_swap = self.sticks.swap;
        profile.stick_digital = stick_digital;
        profile.stick_mouse = self.sticks.stick_mouse.clone();

        profile.trigger_left_config = self.triggers.left.clone();
        profile.trigger_right_config = self.triggers.right.clone();

        profile.gyro_mouse = self.gyro.mouse.clone();
        profile.touchpad_enabled = !matches!(self.touchpad.mode, TouchpadMode::Disabled);
        profile.touchpad_mode = self.touchpad.mode;
        profile.touchpad_show_overlay = self.touchpad.show_overlay;
        profile.touchpad_tap_to_click = self.touchpad.tap_to_click;
        profile.touchpad_natural_scrolling = self.touchpad.natural_scrolling;
        profile.touchpad_sensitivity = self.touchpad.sensitivity;
        profile.steam_left_pad_mouse = self.touchpad.left_mouse.clone();
        profile.steam_right_pad_mouse = self.touchpad.right_mouse.clone();
        if let Some(kind) = self.controller_kind {
            profile.device = kind;
        }

        profile.haptic_pattern = self.haptic_state.pattern;
        profile.haptic_strength = self.haptic_state.strength;
        profile.haptic_speed = self.haptic_state.speed;

        profile.button_bindings = self.remap.bindings.clone();
        profile.layers = self.remap.layers.clone();
        profile.press_timing = self.remap.press_timing.clone();
        let profile = profile.clone();

        if let Some(ipc) = self.ipc.clone() {
            match mlock(&ipc).save_profile(profile.clone()) {
                Ok(()) => Ok(()),
                Err(e) => match self.profile_manager.save_profile(&profile) {
                    Ok(()) => Err(format!("Daemon save failed ({e}); saved locally")),
                    Err(local) => Err(format!(
                        "Daemon save failed ({e}); local save also failed: {local}"
                    )),
                },
            }
        } else {
            self.profile_manager
                .save_profile(&profile)
                .map_err(|e| format!("Failed to save profile '{}': {e}", profile.name))
        }
    }

    pub(crate) fn check_controller_connection(&mut self) {
        if self.using_daemon() {
            self.sync_active_profile();
        }

        if self.firmware.updating {
            return;
        }

        if self.refresh_hid_devices() {
            self.refresh_controller_inventory();
        }

        if self.is_steam()
            && !self.using_daemon()
            && self.input.polling
            && self.input.last_state_at.elapsed() > Duration::from_secs(2)
        {
            self.stop_input_polling();
            self.disconnect_controller();
            return;
        }

        if let Some(ref ipc) = self.ipc.clone() {
            let info = mlock(ipc).get_controller_info();
            match info {
                Ok(Some((serial, pid, is_bt, kind, _transport))) => {
                    let kind_changed = self.controller_kind != Some(kind);
                    self.controller_serial = Some(serial);
                    self.controller_is_bt = Some(is_bt);
                    self.controller_product_id = Some(pid);
                    self.controller_kind = Some(kind);
                    if kind_changed {
                        self.load_profile_for_device();
                        self.refresh_controller_inventory();
                    }
                    if self
                        .pending_controller_target
                        .is_some_and(|pending| pending.family == controller::family_of_kind(kind))
                    {
                        self.clear_pending_target();
                        self.status_message = format!("{} connected", kind.label());
                    }
                }
                Ok(None) => {
                    // A graceful family swap keeps the old controller visible
                    // for a short grace period instead of flashing Searching.
                    let grace = self
                        .pending_controller_target
                        .is_some_and(|pending| pending.since.elapsed() < Duration::from_secs(3));
                    if !grace {
                        self.controller = None;
                        self.battery_info = None;
                        self.controller_serial = None;
                        self.controller_is_bt = None;
                        self.controller_product_id = None;
                        self.controller_kind = None;
                        self.status_message = "Waiting for controller (via daemon)".to_string();
                    }
                }
                Err(_) => {
                    self.stop_input_polling();
                    self.disconnect_controller();
                }
            }
            self.update_pending_message();
            return;
        }

        // Hand off a direct connection once a daemon owns the controller.
        if self.daemon_alive()
            && !self.daemon_manager.is_detached()
            && let Some(client) = self.daemon_manager.connect_new_client()
        {
            let info = mlock(&client).get_controller_info();
            match info {
                Ok(Some(_)) => {
                    self.connect_via_daemon(client);
                    self.update_pending_message();
                    return;
                }
                Ok(None) => {
                    // The compatible daemon owns discovery now. Release the
                    // direct handle before it can race the daemon.
                    self.attach_daemon_waiting(client);
                    self.update_pending_message();
                    return;
                }
                Err(_) => {}
            }
        }

        let Some(serial) = self.controller_serial.clone() else {
            return;
        };

        let still_present = controller::is_present(&self.api, &serial);

        if !still_present {
            if self.pending_controller_target.is_some() && self.attempt_pending_target() {
                return;
            }
            self.stop_input_polling();
            self.disconnect_controller();
            self.update_pending_message();
            return;
        }

        self.evaluate_direct_target();
    }

    fn refresh_hid_devices(&mut self) -> bool {
        match self.api.refresh_devices() {
            Ok(()) => true,
            Err(e) => {
                if self.discovery_error_limiter.ready() {
                    let message = format!("Cannot enumerate HID devices: {}", e);
                    eprintln!("[gyro4u] {}", message);
                    self.error_message = message;
                }
                false
            }
        }
    }

    pub(crate) fn check_for_controller(&mut self) {
        self.last_connection_check = Instant::now();
        self.daemon_manager.refresh();
        let daemon_compatible = self.daemon_alive();

        if self.ipc.is_none()
            && !self.daemon_manager.is_detached()
            && self.daemon_incompatible.is_none()
        {
            if let Some(client) = self.daemon_manager.connect_new_client() {
                let info = mlock(&client).get_controller_info();
                match info {
                    Ok(Some(_)) => {
                        self.connect_via_daemon(client);
                        return;
                    }
                    Ok(None) => {
                        // A compatible daemon owns discovery; wait for it instead
                        // of racing it for the HID handle.
                        self.attach_daemon_waiting(client);
                        return;
                    }
                    Err(e) => {
                        if self.discovery_error_limiter.ready() {
                            self.error_message = format!("Daemon connection lost: {}", e);
                        }
                    }
                }
            } else if daemon_compatible {
                self.status_message = "Waiting for daemon...".to_string();
                return;
            }
        }

        if !self.refresh_hid_devices() {
            return;
        }
        self.refresh_controller_inventory();

        if self.controller.is_none()
            && let Some(target) = controller::requested_family(self.settings.controller_preference)
            && self.current_family() != Some(target)
        {
            self.begin_pending_target(target);
            self.update_pending_message();
        }

        if self.controller.is_none() && !controller::list_devices(&self.api).is_empty() {
            match self.pending_connect_since {
                None => {
                    self.pending_connect_since = Some(Instant::now());
                }
                Some(since) if since.elapsed() >= Duration::from_millis(400) => {
                    self.pending_connect_since = None;
                    self.connect_controller();
                }
                _ => {}
            }
        } else if self.controller.is_none() {
            self.pending_connect_since = None;
        }
    }

    pub(crate) fn fetch_latest_verision_async(&mut self) {
        if self.firmware.checking_latest {
            return;
        }

        if self.is_steam() {
            return;
        }
        let Some(pid) = self.controller_product_id else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let downloader = self.firmware.downloader.clone();

        self.firmware.checking_latest = true;
        self.firmware.progress_rx = Some(rx);
        thread::spawn(move || match downloader.get_latest_version() {
            Ok((ds_ver, dse_ver)) => {
                let ver = if pid == DS_PID { ds_ver } else { dse_ver };
                let _ = tx.send(ProgressUpdate::LatestVersion(ver));
            }
            Err(e) => {
                let _ = tx.send(ProgressUpdate::Error(format!(
                    "Version check failed: {}",
                    e
                )));
            }
        });
    }

    pub(crate) fn refresh_firmware_info(&mut self) {
        let result = if self.is_steam() {
            Err(anyhow::anyhow!(
                "Firmware updates are not supported for the Steam Controller"
            ))
        } else if let Some(ref ipc) = self.ipc.clone() {
            mlock(ipc).get_firmware_info()
        } else if let Some(ref ctrl) = self.controller {
            mlock(ctrl).get_firmware_info()
        } else {
            self.error_message = "No controller available".into();
            return;
        };

        match result {
            Ok((ver, date, time)) => {
                self.firmware.current_version = Some(ver);
                self.status_message = format!("Firmware: 0x{:04X}  ({} {})", ver, date, time);
                self.firmware.build_date = Some(date);
                self.firmware.build_time = Some(time);
                self.error_message.clear();
            }
            Err(e) => {
                self.error_message = format!("Failed to read firmware: {}", e);
            }
        }
    }

    pub(crate) fn flash_latest(&mut self) {
        self.stop_input_polling();

        if !self.acquire_direct_fw() {
            return;
        }

        let pid = self.controller_product_id.unwrap_or(DS_PID);
        let ctrl = Arc::clone(self.controller.as_ref().unwrap());
        let (tx, rx) = mpsc::channel();
        let downloader = self.firmware.downloader.clone();

        self.firmware.progress_rx = Some(rx);
        self.firmware.updating = true;
        self.firmware.progress = 0;
        self.firmware.last_flash_result = None;
        self.firmware.status = "Downloading latest firmware...".to_string();

        let handle = thread::spawn(move || {
            {
                let c = mlock(&ctrl);
                c.set_update_mode(true);
            }

            let mut ctrl = mlock(&ctrl);

            let fw_data = match downloader.download_latest_firmware(pid, {
                let tx = tx.clone();
                move |p| {
                    let _ = tx.send(ProgressUpdate::Progress(p / 2));
                }
            }) {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx.send(ProgressUpdate::Error(e.to_string()));
                    return;
                }
            };

            ctrl.set_update_mode(false);

            let _ = tx.send(ProgressUpdate::Status("Flashing...".to_string()));
            let tx_flash = tx.clone();
            let result = ctrl.update_firmware(&fw_data, move |p| {
                let _ = tx_flash.send(ProgressUpdate::Progress(50 + p / 2));
            });

            ctrl.set_update_mode(false);

            match result {
                Ok(_) => {
                    let _ = tx.send(ProgressUpdate::Complete);
                }
                Err(e) => {
                    let _ = tx.send(ProgressUpdate::Error(e.to_string()));
                }
            }
        });
        self.firmware.thread = Some(handle);
    }

    pub(crate) fn flash_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select firmware file")
            .add_filter("Firmware binary", &["bin"])
            .pick_file()
        else {
            return;
        };

        self.firmware.is_last_flash_file = true;
        let fw_data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.error_message = format!("Failed to read file: {}", e);
                return;
            }
        };

        self.stop_input_polling();

        if !self.acquire_direct_fw() {
            return;
        }

        let ctrl = Arc::clone(self.controller.as_ref().unwrap());
        let (tx, rx) = mpsc::channel();

        self.firmware.progress_rx = Some(rx);
        self.firmware.updating = true;
        self.firmware.progress = 0;
        self.firmware.last_flash_result = None;
        self.firmware.status = "Flasing from file...".to_string();

        let handle = thread::spawn(move || {
            {
                let c = mlock(&ctrl);
                c.set_update_mode(true);
            }

            let mut ctrl = mlock(&ctrl);

            ctrl.set_update_mode(false);

            let tx_flash = tx.clone();
            let result = ctrl.update_firmware(&fw_data, move |p| {
                let _ = tx_flash.send(ProgressUpdate::Progress(50 + p / 2));
            });

            ctrl.set_update_mode(false);

            match result {
                Ok(_) => {
                    let _ = tx.send(ProgressUpdate::Complete);
                }
                Err(e) => {
                    let _ = tx.send(ProgressUpdate::Error(e.to_string()));
                }
            }
        });
        self.firmware.thread = Some(handle);
    }

    pub(crate) fn stick_config(&self) -> StickConfig {
        StickConfig {
            left_deadzone: self.sticks.digital_left_deadzone,
            right_deadzone: self.sticks.digital_right_deadzone,
            left_analog: self.sticks.digital_left_analog,
            right_analog: self.sticks.digital_right_analog,
        }
    }

    pub(crate) fn apply_input_transform(&mut self) {
        let mut t = self
            .current_profile
            .as_ref()
            .map(|p| p.to_input_transform())
            .unwrap_or_default();
        t.left_curve = self.sticks.left_curve.clone();
        t.right_curve = self.sticks.right_curve.clone();
        t.left_deadzone = self.sticks.left_deadzone;
        t.right_deadzone = self.sticks.right_deadzone;
        t.left_outer_deadzone = self.sticks.left_outer_deadzone;
        t.right_outer_deadzone = self.sticks.right_outer_deadzone;
        t.left_invert_x = self.sticks.left_invert_x;
        t.left_invert_y = self.sticks.left_invert_y;
        t.right_invert_x = self.sticks.right_invert_x;
        t.right_invert_y = self.sticks.right_invert_y;
        t.stick_swap = self.sticks.swap;
        t.touchpad_mode = self.touchpad.mode;
        t.trigger_left = self.triggers.left.deadband.clone();
        t.trigger_right = self.triggers.right.deadband.clone();
        t.button_bindings = self.remap.bindings.clone();
        t.layers = self.remap.layers.clone();
        t.press_timing = self.remap.press_timing.clone();
        t.stick_digital = self.stick_config();
        t.gyro_mouse = self.gyro.mouse.clone();
        if !self.capabilities().is_some_and(|caps| caps.motion) {
            t.gyro_mouse.enabled = false;
        }
        t.stick_mouse = self.sticks.stick_mouse.clone();
        t.steam_left_pad_mouse = self.touchpad.left_mouse.clone();
        t.steam_right_pad_mouse = self.touchpad.right_mouse.clone();
        t.button_remap.clear();
        t.disabled_buttons.clear();
        t.normalize_bindings();

        self.input_transform = t.clone();

        if self.ipc.is_some() {
            self.touchpad_filter = None;
        } else if let Some(filter) = &self.touchpad_filter {
            filter.set_mode(self.touchpad.mode);
        }
        if let Some(be) = self.backend() {
            be.set_input_transform(t);
        }
    }

    pub(crate) fn handle_binding_capture(&mut self, ctx: &egui::Context) {
        let Some((control, layer)) = self.remap.capture.as_ref().map(|c| (c.control, c.layer))
        else {
            return;
        };

        for event in ctx.input(|i| i.events.clone()) {
            let egui::Event::Key {
                pressed: true,
                key,
                physical_key,
                repeat,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if repeat {
                continue;
            }
            if key == egui::Key::Escape {
                self.remap.capture = None;
                ctx.request_repaint();
                return;
            }
            let Some(name) = keycodes::from_egui(physical_key.unwrap_or(key)) else {
                return;
            };

            // egui has no modifier key events; take held modifiers from the chord.
            let mut names = Vec::new();
            if modifiers.ctrl {
                names.push("LeftCtrl");
            }
            if modifiers.shift {
                names.push("LeftShift");
            }
            if modifiers.alt {
                names.push("LeftAlt");
            }
            if modifiers.mac_cmd {
                names.push("LeftMeta");
            }
            names.push(name);

            let list = match layer {
                Some(index) => match self.remap.layers.get_mut(index) {
                    Some(layer) => layer.bindings.entry(control).or_default(),
                    None => return,
                },
                None => self.remap.bindings.entry(control).or_default(),
            };
            for name in names {
                if let Some(key) = Key::new(name) {
                    list.push(BindingSlot::from(Binding::Key(key)));
                }
            }
            self.remap.capture = None;
            self.apply_input_transform();
            self.sync_profile();
            ctx.request_repaint();
            return;
        }
    }

    fn start_touchpad_filter(&mut self) {
        if self.is_steam() || self.using_daemon() || self.touchpad_filter.is_some() {
            return;
        }
        self.touchpad_filter = Some(TouchpadFilter::start(self.touchpad.mode));
    }

    fn stop_touchpad_filter(&mut self) {
        self.touchpad_filter = None;
    }

    pub(crate) fn create_profile(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        self.profile_convert_offer = None;
        if let Err(e) = ProfileManager::validate_profile_name(name) {
            let message = e.to_string();
            self.profile_create_error = message.clone();
            return Err(message);
        }
        if name.is_empty() {
            let message = "Profile name cannot be empty".to_string();
            self.profile_create_error = message.clone();
            return Err(message);
        }

        let kind = self.controller_kind.unwrap_or(ControllerKind::DualSense);
        if self.profile_manager.profile_exists(name) {
            let existing = self.profile_manager.load_profile(name).ok();
            let message = match profile_name_conflict(existing.as_ref(), kind) {
                ProfileNameConflict::SameFamily => format!(
                    "A {} profile named '{}' already exists; choose another name",
                    controller::family_of_kind(kind).label(),
                    name
                ),
                ProfileNameConflict::OtherFamily => {
                    let other = existing
                        .as_ref()
                        .map(|profile| controller::family_of_kind(profile.device).label())
                        .unwrap_or("other family");
                    self.profile_convert_offer = Some(name.to_string());
                    format!(
                        "'{}' already exists for {}; convert it to {} in All Profiles",
                        name,
                        other,
                        controller::family_of_kind(kind).label()
                    )
                }
                ProfileNameConflict::None => format!(
                    "Profile '{}' already exists but could not be read; choose another name",
                    name
                ),
            };
            self.profile_create_error = message.clone();
            return Err(message);
        }
        self.profile_convert_offer = None;

        let mut p = Profile::default_for_device(name.to_string(), kind);
        if let Some(cur) = &self.current_profile {
            let mut clone = cur.clone();
            clone.name = name.to_string();
            clone.device = kind;
            p = clone;
        }
        self.profile_manager.save_profile(&p).map_err(|e| {
            let message = format!("Could not save profile '{}': {e}", name);
            self.profile_create_error = message.clone();
            message
        })?;
        if let Some(ipc) = self.ipc.clone()
            && let Err(e) = mlock(&ipc).save_profile(p.clone())
        {
            let message = format!(
                "Profile '{}' saved locally but daemon sync failed: {e}",
                name
            );
            self.profile_create_error = message.clone();
            return Err(message);
        }
        self.load_profile(&p);
        self.profile_create_error.clear();
        self.profile_convert_offer = None;
        Ok(())
    }

    pub(crate) fn convert_profile(&mut self, name: &str) -> Result<(), String> {
        let Some(kind) = self.controller_kind else {
            return Err("No active controller to convert the profile for".to_string());
        };
        let mut profile = self
            .profile_manager
            .load_profile(name)
            .map_err(|e| format!("Could not load profile '{}': {e}", name))?;
        if profile.matches_device(kind) {
            return Ok(());
        }
        profile.convert_to_device(kind);
        self.profile_manager
            .save_profile(&profile)
            .map_err(|e| format!("Could not save profile '{}': {e}", name))?;

        let daemon_error = if let Some(ipc) = self.ipc.clone() {
            mlock(&ipc).save_profile(profile.clone()).err()
        } else {
            None
        };

        if self
            .current_profile
            .as_ref()
            .is_some_and(|current| current.name == name)
        {
            self.load_profile(&profile);
        }

        if let Some(e) = daemon_error {
            return Err(format!(
                "Profile '{}' converted locally but daemon sync failed: {e}",
                name
            ));
        }
        Ok(())
    }

    pub(crate) fn delete_profile(&mut self, name: &str) -> Result<(), String> {
        if name == "Default" || name == "SteamDefault" {
            return Err("Cannot delete the default profile".to_string());
        }
        if let Some(ipc) = self.ipc.clone() {
            mlock(&ipc)
                .delete_profile(name)
                .map_err(|e| format!("Failed to delete profile '{}': {e}", name))?;
        } else {
            self.profile_manager
                .delete_profile(name)
                .map_err(|e| format!("Failed to delete profile '{}': {e}", name))?;
        }
        if let Some(cur) = self.current_profile.as_ref()
            && cur.name == name
        {
            let default = self
                .profile_manager
                .ensure_default_for(self.controller_kind.unwrap_or(ControllerKind::DualSense));
            self.load_profile(&default);
        }
        Ok(())
    }

    pub(crate) fn using_daemon(&self) -> bool {
        self.ipc.is_some()
    }

    pub(crate) fn daemon_alive(&mut self) -> bool {
        if self.last_daemon_probe.elapsed() >= Duration::from_millis(1500) {
            self.last_daemon_probe = Instant::now();
            match probe_daemon(&crate::ipc::daemon_endpoint()) {
                DaemonCompatibility::Compatible { .. } => {
                    self.daemon_alive_cached = true;
                    self.daemon_incompatible = None;
                }
                DaemonCompatibility::Incompatible { detail } => {
                    if self.daemon_incompatible.as_deref() != Some(detail.as_str()) {
                        self.error_message = format!(
                            "Running daemon is incompatible ({detail}). Stop it and start this \
                             build's daemon."
                        );
                    }
                    self.daemon_alive_cached = false;
                    self.daemon_incompatible = Some(detail);
                }
                DaemonCompatibility::Unavailable { .. } => {
                    self.daemon_alive_cached = false;
                    self.daemon_incompatible = None;
                }
            }
        }
        self.daemon_alive_cached
    }

    pub(crate) fn refresh_daemon_state(&mut self) {
        self.last_daemon_probe = Instant::now() - Duration::from_secs(10);
        let _ = self.daemon_alive();
    }

    fn replace_incompatible_daemon(&mut self, detail: &str) -> bool {
        let addr = crate::ipc::daemon_endpoint();
        eprintln!("[gyro4u] replacing incompatible daemon: {}", detail);

        if let Err(e) = crate::ipc::shutdown_daemon(&addr) {
            self.daemon_incompatible = Some(detail.to_string());
            self.error_message = format!(
                "Running daemon is incompatible ({detail}) and could not be stopped: {e}. \
                 Stop it manually and restart Gyro4U."
            );
            return false;
        }

        if !crate::ipc::wait_for_daemon_release(&addr, Duration::from_secs(3)) {
            self.daemon_incompatible = Some(detail.to_string());
            self.error_message = format!(
                "Running daemon is incompatible ({detail}) and did not exit within 3 s. \
                 Stop it manually and restart Gyro4U."
            );
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));

        if self.ipc.is_some() {
            self.stop_input_polling();
            self.disconnect_controller();
        }
        self.daemon_manager.forget_client();
        self.daemon_alive_cached = false;
        self.daemon_incompatible = None;
        true
    }

    pub(crate) fn start_daemon_process(&mut self) {
        let addr = crate::ipc::daemon_endpoint();
        match probe_daemon(&addr) {
            DaemonCompatibility::Compatible { .. } => {
                self.daemon_alive_cached = true;
                self.daemon_incompatible = None;
                self.status_message = "Daemon already running".into();
                return;
            }
            DaemonCompatibility::Incompatible { detail } => {
                if !self.replace_incompatible_daemon(&detail) {
                    return;
                }
            }
            DaemonCompatibility::Unavailable { .. } => {
                self.daemon_alive_cached = false;
            }
        }

        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                self.error_message = format!("Cannot locate gyro4u binary: {}", e);
                return;
            }
        };
        match std::process::Command::new(&exe)
            .arg("--daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(_) => {
                self.status_message = "Daemon starting...".into();
                self.error_message.clear();
                match crate::ipc::wait_for_daemon_ready(&addr, Duration::from_secs(5)) {
                    DaemonCompatibility::Compatible { .. } => {
                        self.status_message = "Daemon started".into();
                        self.daemon_alive_cached = true;
                        self.daemon_incompatible = None;
                        self.daemon_manager.attach();
                        self.last_daemon_probe = Instant::now();
                    }
                    DaemonCompatibility::Incompatible { detail } => {
                        self.daemon_incompatible = Some(detail.clone());
                        self.error_message =
                            format!("Daemon started with an incompatible protocol: {detail}");
                    }
                    DaemonCompatibility::Unavailable { detail } => {
                        self.error_message = format!("Daemon did not start within 5 s: {detail}");
                    }
                }
            }
            Err(e) => {
                self.error_message = format!("Failed to start daemon: {}", e);
            }
        }
    }

    pub(crate) fn stop_daemon_process(&mut self) {
        let addr = crate::ipc::daemon_endpoint();
        if matches!(probe_daemon(&addr), DaemonCompatibility::Unavailable { .. }) {
            self.error_message = "Daemon is not running".into();
            self.daemon_alive_cached = false;
            self.daemon_incompatible = None;
            return;
        }

        if self.ipc.is_some() {
            self.stop_input_polling();
            self.disconnect_controller();
        }

        match crate::ipc::shutdown_daemon(&addr) {
            Ok(()) => {
                if crate::ipc::wait_for_daemon_release(&addr, Duration::from_secs(3)) {
                    self.status_message = "Daemon stopped".into();
                    self.error_message.clear();
                } else {
                    self.error_message = "Daemon did not stop within 3 s; stop it manually".into();
                }
            }
            Err(e) => {
                self.error_message = format!("Shutdown failed: {}", e);
            }
        }

        self.daemon_manager.forget_client();
        self.daemon_alive_cached = false;
        self.daemon_incompatible = None;
        self.last_daemon_probe = Instant::now();
    }

    pub(crate) fn attach_daemon(&mut self) {
        let addr = crate::ipc::daemon_endpoint();
        let client = match IpcClient::try_connect(&addr) {
            Some(c) => Arc::new(Mutex::new(c)),
            None => {
                self.error_message = match probe_daemon(&addr) {
                    DaemonCompatibility::Incompatible { detail } => format!(
                        "Running daemon is incompatible: {detail}. Stop it from Settings, then \
                         start this build's daemon."
                    ),
                    _ => "Daemon is not running".into(),
                };
                return;
            }
        };
        let has_dev = matches!(mlock(&client).get_controller_info(), Ok(Some(_)));
        self.daemon_manager.attach();
        if has_dev {
            self.connect_via_daemon(client);
        } else {
            self.attach_daemon_waiting(client);
        }
    }

    pub(crate) fn detach_daemon(&mut self) {
        if self.ipc.is_none() {
            return;
        }
        self.daemon_manager.detach();
        self.stop_input_polling();
        self.disconnect_controller();
        self.status_message = "Detached from daemon (direct mode)".into();
    }

    pub(crate) fn set_controller_preference(&mut self, preference: ControllerPreference) {
        // Persist even when the enum is unchanged: the previous switch may
        // have failed, and re-selecting must retry the target family.
        self.settings.controller_preference = preference;
        self.settings_manager.save(&self.settings);

        if self.refresh_hid_devices() {
            self.refresh_controller_inventory();
        }

        let current = self.current_family();
        let target = controller::requested_family(preference).or_else(|| {
            controller::switch_target(
                preference,
                current,
                self.inventory_has(ControllerFamily::DualSense),
                self.inventory_has(ControllerFamily::Steam),
            )
        });
        let pending = target.filter(|target| current != Some(*target));
        if let Some(family) = pending {
            self.begin_pending_target(family);
        } else {
            self.clear_pending_target();
        }
        if let Some(ctx) = &self.egui_ctx {
            ctx.request_repaint();
        }

        if let Some(ipc) = self.ipc.clone() {
            match mlock(&ipc).set_controller_preference(preference) {
                Ok(()) => {
                    if pending.is_none() {
                        self.status_message =
                            format!("Controller preference: {}", preference.label());
                    }
                }
                Err(e) => {
                    self.error_message = format!("Failed to update daemon preference: {e}");
                }
            }
            self.check_controller_connection();
            return;
        }

        if self.controller.is_none() {
            if pending.is_none() {
                self.status_message = format!("Controller preference: {}", preference.label());
            }
            self.check_for_controller();
            return;
        }

        if pending.is_some() {
            self.update_pending_message();
            let _ = self.attempt_pending_target();
        } else {
            self.status_message = format!("Controller preference: {}", preference.label());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(family: ControllerFamily, age: Duration) -> PendingControllerTarget {
        PendingControllerTarget {
            family,
            since: Instant::now() - age,
        }
    }

    #[test]
    fn steam_pending_uses_wake_copy_while_receiver_only() {
        let message = pending_status_text(
            pending(ControllerFamily::Steam, Duration::from_secs(1)),
            Some(ControllerFamily::DualSense),
            true,
            false,
        );

        assert_eq!(message, STEAM_WAKE_RETRY);
        assert!(message.contains("turn it on"));
    }

    #[test]
    fn steam_pending_switches_copy_when_live() {
        let message = pending_status_text(
            pending(ControllerFamily::Steam, Duration::from_secs(1)),
            Some(ControllerFamily::DualSense),
            true,
            true,
        );
        assert_eq!(message, "Switching to Steam Controller...");

        let message = pending_status_text(
            pending(ControllerFamily::Steam, Duration::from_secs(6)),
            Some(ControllerFamily::DualSense),
            true,
            true,
        );
        assert_eq!(message, "Could not switch to Steam Controller - retrying");
    }

    #[test]
    fn missing_non_steam_target_keeps_current_family_in_the_copy() {
        let message = pending_status_text(
            pending(ControllerFamily::DualSense, Duration::from_secs(6)),
            Some(ControllerFamily::Steam),
            false,
            false,
        );

        assert!(message.contains("still using Steam Controller"));
    }
}
