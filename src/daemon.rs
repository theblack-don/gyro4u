use std::{
    io::{BufRead, BufReader, Write},
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, sleep},
    time::{Duration, Instant},
};

use hidapi::HidApi;

use crate::{
    common::{BatteryInfo, HapticPattern, LightbarEffect, TouchpadMode},
    controller::{self, PhysicalController, PolledInput},
    dualsense::{HAPTICS_PACKET_FRAMES, HAPTICS_SAMPLE_RATE},
    gyro::GyroRuntime,
    haptics_stream::generate_packet,
    input_grab::InputGrab,
    inputs::{ControllerKind, ControllerState},
    ipc::{
        APP_VERSION, DaemonCommand, DaemonResponse, DaemonStream, IPC_PROTOCOL_VERSION, IpcClient,
        addr_display, bind_daemon, cleanup_endpoint, daemon_endpoint,
    },
    profiles::ProfileManager,
    remap::{AuxOutput, Binding, ProfileAction, RemapEngine},
    settings::SettingsManager,
    steam_controller::SteamPadSide,
    stick_mouse::StickMouseRuntime,
    touchpad_filter::{TouchpadFilter, TouchpadModeHandle, TouchpadOwnedHandle},
    trackpad_mouse::{SteamPadMouseMode, SteamTrackpadMouseRuntime},
    transform::{GyroProcessor, InputTransform},
    util::{RateLimiter, mlock, rlock, wait_cv, wlock},
    virtual_gamepad::VirtualGamepad,
    virtual_keyboard::VirtualKeyboard,
    virtual_mouse::VirtualMouse,
};

const TAG: &str = "[gyro4u daemon]";

const MOTION_STALL_TIMEOUT: Duration = Duration::from_millis(250);
const MOTION_FORCE_COOLDOWN: Duration = Duration::from_secs(1);
const MOTION_RETRY_INTERVAL: Duration = Duration::from_millis(500);

static LAST_MOTION_ERROR_LOG: AtomicU64 = AtomicU64::new(0);

pub struct DaemonManager {
    client: Option<Arc<Mutex<IpcClient>>>,
    detached: bool,
    last_attempt: Option<Instant>,
}

impl DaemonManager {
    pub fn new() -> Self {
        let mut manager = Self {
            client: None,
            detached: false,
            last_attempt: None,
        };
        manager.refresh();
        manager
    }

    pub fn is_detached(&self) -> bool {
        self.detached
    }

    pub fn refresh(&mut self) {
        if self.client.is_some() || self.detached {
            return;
        }
        if self
            .last_attempt
            .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.last_attempt = Some(Instant::now());
        let addr = daemon_endpoint();
        self.client = IpcClient::try_connect(&addr).map(|c| Arc::new(Mutex::new(c)));
    }

    pub fn connect_new_client(&self) -> Option<Arc<Mutex<IpcClient>>> {
        if self.detached {
            return None;
        }
        let addr = daemon_endpoint();
        IpcClient::try_connect(&addr).map(|c| Arc::new(Mutex::new(c)))
    }

    pub fn attach(&mut self) {
        self.detached = false;
        self.last_attempt = None;
        self.refresh();
    }

    pub fn detach(&mut self) {
        self.client = None;
        self.detached = true;
    }

    pub fn forget_client(&mut self) {
        self.client = None;
        self.last_attempt = None;
    }

    pub fn set_update_in_progress(&mut self, active: bool) {
        if let Some(ref arc) = self.client {
            let _ = mlock(arc).set_update_mode(active);
        }
    }
}

struct DaemonInner {
    active_transform: InputTransform,
    active_effect: LightbarEffect,
    lightbar_color: (u8, u8, u8, u8),
    player_leds: u8,
    mic_enabled: bool,
    active_profile_name: String,
    trigger_left: Option<(u8, [u8; 10])>,
    trigger_right: Option<(u8, [u8; 10])>,
    haptic: (HapticPattern, u8, f32),
    raw_haptics: bool,
    gyro: GyroProcessor,
}

#[derive(Clone)]
struct LatestInput {
    state: ControllerState,
    battery: Option<BatteryInfo>,
    at: Instant,
}

struct DaemonState {
    device: Mutex<Option<PhysicalController>>,
    device_kind: RwLock<ControllerKind>,
    grab: Mutex<Option<InputGrab>>,
    update_in_progress: AtomicBool,
    output_reset: AtomicBool,
    pending_reset: AtomicBool,
    motion_request: AtomicBool,
    device_generation: AtomicU64,
    inner: RwLock<DaemonInner>,
    hotplug: (Mutex<bool>, Condvar),
    latest_input: Mutex<Option<LatestInput>>,
    steam_liveness: Mutex<controller::SteamLiveness>,
    touchpad_mode: TouchpadModeHandle,
    touchpad_owned: TouchpadOwnedHandle,
}

impl DaemonState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            device: Mutex::new(None),
            device_kind: RwLock::new(ControllerKind::DualSense),
            grab: Mutex::new(None),
            update_in_progress: AtomicBool::new(false),
            output_reset: AtomicBool::new(false),
            pending_reset: AtomicBool::new(false),
            motion_request: AtomicBool::new(false),
            device_generation: AtomicU64::new(0),
            inner: RwLock::new(DaemonInner {
                active_transform: InputTransform::default(),
                active_effect: LightbarEffect::None,
                lightbar_color: (0, 128, 255, 255),
                player_leds: 1,
                mic_enabled: false,
                active_profile_name: String::new(),
                trigger_left: None,
                trigger_right: None,
                haptic: (HapticPattern::None, 0, 1.0),
                raw_haptics: false,
                gyro: GyroProcessor::default(),
            }),
            hotplug: (Mutex::new(false), Condvar::new()),
            latest_input: Mutex::new(None),
            steam_liveness: Mutex::new(controller::SteamLiveness::default()),
            touchpad_mode: TouchpadModeHandle::new(TouchpadMode::Mouse),
            touchpad_owned: TouchpadOwnedHandle::new(),
        })
    }
}

fn fresh_input(state: &DaemonState) -> Option<LatestInput> {
    (*mlock(&state.latest_input))
        .clone()
        .filter(|l| l.at.elapsed() < Duration::from_millis(500))
}

fn fresh_battery(state: &DaemonState) -> Option<BatteryInfo> {
    if let Some(info) = fresh_input(state).and_then(|l| l.battery) {
        return Some(info);
    }
    mlock(&state.device)
        .as_mut()
        .and_then(|controller| controller.get_battery().ok())
}

fn signal_hotplug(state: &DaemonState) {
    let (l, c) = &state.hotplug;
    *mlock(l) = true;
    c.notify_all();
}

fn clear_steam_live(state: &DaemonState) {
    mlock(&state.steam_liveness).live = None;
}

fn apply_steam_probe(cache: &mut controller::SteamLiveness, probe: &controller::SteamProbeResult) {
    cache.receiver_present = probe.receiver_present;
    match &probe.state {
        controller::SteamSlotState::Live(slot) => cache.live = Some(slot.clone()),
        controller::SteamSlotState::NotLive => cache.live = None,
        controller::SteamSlotState::Unknown => {
            if !probe.receiver_present {
                cache.live = None;
            }
        }
    }
}

fn bump_device_generation(state: &DaemonState) {
    state.device_generation.fetch_add(1, Ordering::SeqCst);
}

fn motion_is_wanted(state: &DaemonState) -> bool {
    let inner = rlock(&state.inner);
    inner.active_transform.gyro_mouse.enabled
        || inner.active_transform.stick_mouse.enabled
        || state.motion_request.load(Ordering::Relaxed)
}

fn log_motion_error(context: &str, error: impl std::fmt::Display) {
    if crate::util::rate_limited_gate(&LAST_MOTION_ERROR_LOG, Duration::from_secs(5)) {
        eprintln!("{} {context}: {error}", TAG);
    }
}

fn reassert_motion(state: &Arc<DaemonState>) {
    if !motion_is_wanted(state) {
        return;
    }
    let mut dev = mlock(&state.device);
    if let Some(controller) = dev.as_mut()
        && let Err(e) = controller.force_motion(true)
    {
        log_motion_error("Steam IMU re-enable failed", e);
    }
}

fn profile_matches_device(profile_device: ControllerKind, connected: ControllerKind) -> bool {
    profile_device.same_profile_group(connected)
}

/// Keep the client-provided device when no controller is connected; reject an
/// explicit profile-group mismatch rather than silently rewriting the profile.
fn prepare_profile_for_save(
    profile: crate::profiles::Profile,
    active_kind: Option<ControllerKind>,
) -> Result<crate::profiles::Profile, String> {
    if let Some(active_kind) = active_kind
        && !profile_matches_device(profile.device, active_kind)
    {
        return Err(format!(
            "Profile '{}' targets {}, but {} is connected",
            profile.name,
            profile.device.label(),
            active_kind.label()
        ));
    }
    Ok(profile)
}

fn load_profile_for_kind(
    manager: &ProfileManager,
    kind: ControllerKind,
    preferred_name: &str,
) -> Option<crate::profiles::Profile> {
    let matches = |name: &str| {
        manager
            .load_profile(name)
            .ok()
            .filter(|profile| profile_matches_device(profile.device, kind))
    };
    if manager.profile_exists(preferred_name)
        && let Some(profile) = matches(preferred_name)
    {
        return Some(profile);
    }
    let remembered = SettingsManager::new()
        .load()
        .remembered_profile(kind.profile_group())
        .to_string();
    if remembered != preferred_name && manager.profile_exists(&remembered) {
        return matches(&remembered);
    }
    None
}

fn apply_profile_to_state(state: &Arc<DaemonState>, name: &str) -> String {
    let kind = *rlock(&state.device_kind);
    let pm = ProfileManager::new();
    let profile = load_profile_for_kind(&pm, kind, name);
    let p = profile.unwrap_or_else(|| pm.ensure_default_for(kind));

    let mut inner = wlock(&state.inner);
    let mut transform = p.to_input_transform();
    transform.normalize_bindings();
    inner.active_transform = transform;
    state
        .touchpad_mode
        .set(inner.active_transform.touchpad_mode);
    state.output_reset.store(true, Ordering::SeqCst);
    state.pending_reset.store(true, Ordering::SeqCst);
    let r = (p.lightbar_r * 255.0) as u8;
    let g = (p.lightbar_g * 255.0) as u8;
    let b = (p.lightbar_b * 255.0) as u8;
    let br = p.lightbar_brightness as u8;
    inner.lightbar_color = (r, g, b, br);
    inner.player_leds = p.player_leds;
    inner.mic_enabled = p.mic_enabled;

    use crate::common::TriggerMode;
    inner.trigger_left = match p.trigger_left_config.mode {
        TriggerMode::Off => None,
        _ => Some(p.trigger_left_config.to_effect()),
    };
    inner.trigger_right = match p.trigger_right_config.mode {
        TriggerMode::Off => None,
        _ => Some(p.trigger_right_config.to_effect()),
    };

    inner.haptic = (p.haptic_pattern, p.haptic_strength, p.haptic_speed);
    inner.gyro = p.to_gyro_processor();

    p.name.clone()
}

fn push_triggers_to_device(state: &Arc<DaemonState>) {
    let (left, right) = {
        let inner = rlock(&state.inner);
        (inner.trigger_left, inner.trigger_right)
    };
    if let Some(ds) = mlock(&state.device).as_mut() {
        let l = left.or(Some((0x05, [0u8; 10])));
        let r = right.or(Some((0x05, [0u8; 10])));
        let _ = ds.set_trigger_effects(l, r);
    }
}

fn switch_profile(state: &Arc<DaemonState>, name: &str) -> String {
    let loaded = apply_profile_to_state(state, name);
    let kind = *rlock(&state.device_kind);
    wlock(&state.inner).active_profile_name = loaded.clone();
    let sm = SettingsManager::new();
    let mut settings = sm.load();
    settings.remember_profile(kind.profile_group(), &loaded);
    sm.save(&settings);
    {
        let (color, leds, mic) = {
            let i = rlock(&state.inner);
            (i.lightbar_color, i.player_leds, i.mic_enabled)
        };
        let (r, g, b, br) = color;
        if let Some(ds) = mlock(&state.device).as_mut() {
            let _ = ds.set_lightbar(r, g, b, br);
            let _ = ds.set_player_leds(leds);
            let _ = ds.set_mic(mic);
        }
    }
    push_triggers_to_device(state);
    reassert_motion(state);
    println!("{} switched to profile '{}'", TAG, loaded);
    loaded
}

fn cycle_profile(state: &Arc<DaemonState>) -> String {
    let active = rlock(&state.inner).active_profile_name.clone();
    let kind = *rlock(&state.device_kind);
    let profiles = ProfileManager::new().list_profiles_for(kind);
    let next = if profiles.is_empty() {
        ProfileManager::default_profile_name(kind).to_string()
    } else if let Some(i) = profiles.iter().position(|p| p.name == active) {
        profiles[(i + 1) % profiles.len()].name.clone()
    } else {
        profiles[0].name.clone()
    };
    switch_profile(state, &next)
}

pub fn run_daemon() {
    use std::os::unix::io::AsRawFd;

    let addr = daemon_endpoint();

    let lock_path = dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("gyro4u.lock");
    let instance_lock = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{} cannot open {}: {}", TAG, lock_path.display(), e);
            return;
        }
    };
    if unsafe { libc::flock(instance_lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EWOULDBLOCK) => println!("{} daemon already running", TAG),
            _ => eprintln!("{} cannot lock {}: {}", TAG, lock_path.display(), err),
        }
        return;
    }
    let _instance_lock = instance_lock;

    if std::os::unix::net::UnixStream::connect(&addr).is_ok() {
        println!("{} daemon already running", TAG);
        return;
    }

    cleanup_endpoint(&addr);

    let listener = bind_daemon(&addr)
        .unwrap_or_else(|e| panic!("{} cannot bind {}: {}", TAG, addr_display(&addr), e));

    println!(
        "{} listening on {} (IPC protocol {}, app {})",
        TAG,
        addr_display(&addr),
        IPC_PROTOCOL_VERSION,
        APP_VERSION
    );

    let state = DaemonState::new();

    {
        let sm = SettingsManager::new();
        let settings = sm.load();
        let name = settings
            .remembered_profile(crate::inputs::ProfileGroup::DualSense)
            .to_string();
        let loaded = apply_profile_to_state(&state, &name);
        wlock(&state.inner).active_profile_name = loaded.clone();
        println!("{} profile '{}' loaded", TAG, loaded);
    }

    let _touchpad_filter = TouchpadFilter::start_with_handle(
        state.touchpad_mode.clone(),
        state.touchpad_owned.clone(),
    );

    {
        let s = Arc::clone(&state);
        thread::spawn(move || device_connection_loop(s));
    }

    {
        let s = Arc::clone(&state);
        thread::spawn(move || hotplug_thread(s));
    }

    {
        let s = Arc::clone(&state);
        thread::spawn(move || steam_liveness_loop(s));
    }

    {
        let s = Arc::clone(&state);
        thread::spawn(move || effect_loop(s));
    }

    {
        let s = Arc::clone(&state);
        thread::spawn(move || haptic_loop(s));
    }

    {
        let s = Arc::clone(&state);
        thread::spawn(move || raw_haptic_loop(s));
    }

    {
        let s = Arc::clone(&state);
        thread::spawn(move || gamepad_loop(s));
    }

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let state = Arc::clone(&state);
                thread::spawn(move || handle_client(s, state));
            }
            Err(e) => eprintln!("{} accept error: {}", TAG, e),
        }
    }
}

enum Poll {
    NoDevice,
    Failed,
    Data {
        generation: u64,
        ids: (u16, u16, ControllerKind),
        polled: Option<PolledInput>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MotionCommand {
    Enable { force: bool },
    Disable,
}

/// Tracks the daemon's view of the device IMU mode and detects a frozen sensor
/// timestamp despite fresh input reports.
struct MotionSync {
    generation: Option<u64>,
    applied: bool,
    last_failure: Option<Instant>,
    last_timestamp: Option<u32>,
    timestamp_changed_at: Option<Instant>,
    last_force: Option<Instant>,
}

impl MotionSync {
    fn new() -> Self {
        Self {
            generation: None,
            applied: false,
            last_failure: None,
            last_timestamp: None,
            timestamp_changed_at: None,
            last_force: None,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn generation(&self) -> Option<u64> {
        self.generation
    }

    fn poll(
        &mut self,
        now: Instant,
        generation: u64,
        want: bool,
        timestamp: u32,
    ) -> Option<MotionCommand> {
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.applied = false;
            self.last_failure = None;
            self.last_force = None;
            self.last_timestamp = Some(timestamp);
            self.timestamp_changed_at = Some(now);
        } else if self.last_timestamp != Some(timestamp) {
            self.last_timestamp = Some(timestamp);
            self.timestamp_changed_at = Some(now);
        }

        let retry_ready = self
            .last_failure
            .is_none_or(|last| now.duration_since(last) >= MOTION_RETRY_INTERVAL);

        if !want {
            return (self.applied && retry_ready).then_some(MotionCommand::Disable);
        }

        if !self.applied {
            return retry_ready.then_some(MotionCommand::Enable { force: false });
        }

        let stalled = self
            .timestamp_changed_at
            .is_some_and(|changed| now.duration_since(changed) >= MOTION_STALL_TIMEOUT);
        let force_ready = self
            .last_force
            .is_none_or(|last| now.duration_since(last) >= MOTION_FORCE_COOLDOWN);
        (stalled && force_ready && retry_ready).then_some(MotionCommand::Enable { force: true })
    }

    fn confirm(&mut self, now: Instant, command: MotionCommand) {
        self.last_failure = None;
        match command {
            MotionCommand::Enable { force } => {
                self.applied = true;
                self.timestamp_changed_at = Some(now);
                if force {
                    self.last_force = Some(now);
                }
            }
            MotionCommand::Disable => self.applied = false,
        }
    }

    fn failed(&mut self, now: Instant, command: MotionCommand) {
        self.last_failure = Some(now);
        if matches!(command, MotionCommand::Enable { force: true }) {
            self.last_force = Some(now);
        }
    }
}

fn transform_has_binding(transform: &InputTransform, pred: impl Fn(&Binding) -> bool) -> bool {
    transform
        .button_bindings
        .values()
        .flatten()
        .chain(
            transform
                .layers
                .iter()
                .flat_map(|layer| layer.bindings.values().flatten()),
        )
        .map(|slot| &slot.binding)
        .any(pred)
}

fn release_aux(keyboard: &mut Option<VirtualKeyboard>, mouse: &mut Option<VirtualMouse>) {
    if let Some(keyboard) = keyboard.as_mut() {
        let _ = keyboard.release_all();
    }
    if let Some(mouse) = mouse.as_mut() {
        let _ = mouse.release_all();
    }
}

fn gamepad_loop(state: Arc<DaemonState>) {
    let mut pad: Option<VirtualGamepad> = None;
    let mut pad_ids: Option<(u16, u16, ControllerKind)> = None;
    let mut pad_disabled = false;
    let mut motion_sync = MotionSync::new();
    let mut remap = RemapEngine::new();
    let mut aux = AuxOutput::default();
    let mut gyro_runtime = GyroRuntime::new();
    let mut stick_mouse = StickMouseRuntime::new();
    let mut trackpad_mouse = SteamTrackpadMouseRuntime::new();
    let mut haptic_backoff_until: Option<Instant> = None;
    let mut haptic_error_log = RateLimiter::new(Duration::from_secs(5));
    let mut keyboard: Option<VirtualKeyboard> = None;
    let mut mouse: Option<VirtualMouse> = None;
    let mut keyboard_disabled = false;
    let mut mouse_disabled = false;

    loop {
        sleep(Duration::from_millis(1));

        if state.output_reset.swap(false, Ordering::SeqCst) {
            release_aux(&mut keyboard, &mut mouse);
            gyro_runtime.reset();
            stick_mouse.reset();
            trackpad_mouse.reset();
            haptic_backoff_until = None;
            if let Some(pad) = pad.as_mut() {
                pad.reset();
            }
        }

        if state.pending_reset.swap(false, Ordering::SeqCst) {
            remap.reset_pending();
        }

        if state.update_in_progress.load(Ordering::Relaxed) {
            release_aux(&mut keyboard, &mut mouse);
            pad = None;
            pad_ids = None;
            motion_sync.reset();
            remap.reset();
            gyro_runtime.reset();
            stick_mouse.reset();
            trackpad_mouse.reset();
            haptic_backoff_until = None;
            sleep(Duration::from_millis(20));
            continue;
        }

        let poll = {
            let mut dev = mlock(&state.device);
            match dev.as_mut() {
                None => Poll::NoDevice,
                Some(controller) => {
                    let ids = (
                        controller.vendor_id(),
                        controller.product_id(),
                        controller.kind(),
                    );
                    match controller.poll_latest_input() {
                        Ok(p) => Poll::Data {
                            generation: state.device_generation.load(Ordering::SeqCst),
                            ids,
                            polled: p,
                        },
                        Err(_) => Poll::Failed,
                    }
                }
            }
        };

        let (generation, ids, polled) = match poll {
            Poll::NoDevice => {
                release_aux(&mut keyboard, &mut mouse);
                pad = None;
                pad_ids = None;
                motion_sync.reset();
                remap.reset();
                gyro_runtime.reset();
                stick_mouse.reset();
                trackpad_mouse.reset();
                haptic_backoff_until = None;
                sleep(Duration::from_millis(20));
                continue;
            }
            Poll::Failed => {
                println!("{} input read failed - dropping handle", TAG);
                if rlock(&state.device_kind).is_steam() {
                    clear_steam_live(&state);
                }
                release_aux(&mut keyboard, &mut mouse);
                pad = None;
                pad_ids = None;
                motion_sync.reset();
                remap.reset();
                gyro_runtime.reset();
                stick_mouse.reset();
                trackpad_mouse.reset();
                haptic_backoff_until = None;
                *mlock(&state.device) = None;
                bump_device_generation(&state);
                signal_hotplug(&state);
                continue;
            }
            Poll::Data {
                generation,
                ids,
                polled,
            } => (generation, ids, polled),
        };

        if motion_sync.generation() != Some(generation) {
            gyro_runtime.reset();
            stick_mouse.reset();
            trackpad_mouse.reset();
        }

        if !pad_disabled && pad_ids != Some(ids) {
            pad = None;
            gyro_runtime.reset();
            stick_mouse.reset();
            trackpad_mouse.reset();
            haptic_backoff_until = None;
            match VirtualGamepad::new(ids.2, ids.0, ids.1) {
                Ok(p) => {
                    println!(
                        "{} virtual gamepad {:04x}:{:04x} ({})",
                        TAG,
                        ids.0,
                        ids.1,
                        ids.2.label()
                    );
                    pad = Some(p);
                    pad_ids = Some(ids);
                }
                Err(e) => {
                    eprintln!("{} virtual gamepad disabled: {}", TAG, e);
                    pad_disabled = true;
                }
            }
        }

        let Some(p) = polled else { continue };

        if p.disconnected {
            println!("{} controller reported a wireless disconnect", TAG);
            if rlock(&state.device_kind).is_steam() {
                clear_steam_live(&state);
            }
            *mlock(&state.device) = None;
            *mlock(&state.grab) = None;
            motion_sync.reset();
            bump_device_generation(&state);
            signal_hotplug(&state);
            continue;
        }

        *mlock(&state.latest_input) = Some(LatestInput {
            state: p.state,
            battery: p.battery.clone(),
            at: Instant::now(),
        });

        let Some(pad) = pad.as_mut() else { continue };

        let (transform, touchpad_allowed) = {
            let inner = rlock(&state.inner);
            let kind = *rlock(&state.device_kind);
            let touchpad_allowed = !matches!(
                kind.capabilities().touchpad,
                crate::inputs::TouchpadKind::None
            ) && (kind.is_steam()
                || (state.touchpad_owned.get()
                    && matches!(
                        state.touchpad_mode.get(),
                        TouchpadMode::Disabled | TouchpadMode::GesturesOnly
                    )));
            (inner.active_transform.clone(), touchpad_allowed)
        };

        let wants_keyboard = transform_has_binding(&transform, |b| matches!(b, Binding::Key(_)));
        if wants_keyboard && !keyboard_disabled && keyboard.is_none() {
            match VirtualKeyboard::new() {
                Ok(device) => {
                    println!("{} virtual keyboard ready", TAG);
                    keyboard = Some(device);
                }
                Err(e) => {
                    eprintln!("{} keyboard disabled: {}", TAG, e);
                    keyboard_disabled = true;
                }
            }
        }

        let wants_mouse = transform.stick_mouse.enabled
            || transform.gyro_mouse.enabled
            || matches!(
                transform.steam_left_pad_mouse.mode,
                SteamPadMouseMode::Mouse | SteamPadMouseMode::Scroll
            )
            || matches!(
                transform.steam_right_pad_mouse.mode,
                SteamPadMouseMode::Mouse | SteamPadMouseMode::Scroll
            )
            || transform_has_binding(&transform, |b| {
                matches!(b, Binding::Mouse(_) | Binding::Wheel(_))
            });
        if wants_mouse && !mouse_disabled && mouse.is_none() {
            match VirtualMouse::new() {
                Ok(device) => {
                    println!("{} virtual mouse ready", TAG);
                    mouse = Some(device);
                }
                Err(e) => {
                    eprintln!("{} mouse disabled: {}", TAG, e);
                    mouse_disabled = true;
                }
            }
        }

        let want_motion = transform.gyro_mouse.enabled
            || transform.stick_mouse.enabled
            || state.motion_request.load(Ordering::Relaxed);
        let now = Instant::now();
        if let Some(command) =
            motion_sync.poll(now, generation, want_motion, p.state.sensor_timestamp)
        {
            let outcome = mlock(&state.device)
                .as_mut()
                .map(|controller| match command {
                    MotionCommand::Enable { force: true } => controller.force_motion(true),
                    MotionCommand::Enable { force: false } => controller.set_motion(true),
                    MotionCommand::Disable => controller.set_motion(false),
                });
            match outcome {
                Some(Ok(())) => {
                    motion_sync.confirm(now, command);
                    if matches!(command, MotionCommand::Enable { force: true }) {
                        gyro_runtime.reset();
                        stick_mouse.reset();
                    }
                }
                Some(Err(e)) => {
                    motion_sync.failed(now, command);
                    log_motion_error("controller motion write failed", e);
                }
                None => {}
            }
        }

        let gyro_rel =
            gyro_runtime.process(&p.state, &transform.gyro_mouse, state.touchpad_owned.get());
        let stick_rel = stick_mouse.process(
            (p.state.right_x, p.state.right_y),
            &transform.stick_mouse,
            p.state.sensor_timestamp,
            p.state.timestamp_unit_us,
        );
        let pad_output = trackpad_mouse.process(
            &p.state.steam_left_pad,
            &transform.steam_left_pad_mouse,
            &p.state.steam_right_pad,
            &transform.steam_right_pad_mouse,
            Instant::now(),
        );
        let raw_left = (p.state.left_x, p.state.left_y);
        let raw_right = (p.state.right_x, p.state.right_y);
        let mut st = p.state;
        transform.apply_axes(&mut st);
        aux.clear();
        remap.apply(
            &mut st,
            &transform.button_bindings,
            &transform.layers,
            &transform.press_timing,
            Instant::now(),
            touchpad_allowed,
            &transform.stick_digital,
            transform.stick_mouse.enabled,
            raw_left,
            raw_right,
            &mut aux,
        );
        aux.rel = (
            gyro_rel.0 + stick_rel.0 + pad_output.rel.0,
            gyro_rel.1 + stick_rel.1 + pad_output.rel.1,
        );
        aux.wheel.0 += pad_output.wheel.0;
        aux.wheel.1 += pad_output.wheel.1;
        let _ = pad.update(&st);
        if let Some(keyboard) = keyboard.as_mut() {
            let _ = keyboard.apply(&aux.keys);
        }
        if let Some(mouse) = mouse.as_mut() {
            let _ = mouse.apply(&aux.mouse_buttons, aux.rel, aux.wheel);
        }

        let [left_haptic, right_haptic] = pad_output.haptics;
        if (left_haptic.is_some() || right_haptic.is_some())
            && haptic_backoff_until.is_none_or(|until| Instant::now() >= until)
        {
            let mut dev = mlock(&state.device);
            if let Some(device) = dev.as_mut()
                && device.capabilities().trackpad_haptics
            {
                let events = [
                    (
                        SteamPadSide::Left,
                        left_haptic,
                        transform.steam_left_pad_mouse.haptic_strength.gain_db(),
                    ),
                    (
                        SteamPadSide::Right,
                        right_haptic,
                        transform.steam_right_pad_mouse.haptic_strength.gain_db(),
                    ),
                ];
                for (side, haptic, gain_db) in events {
                    let (Some(haptic), Some(gain_db)) = (haptic, gain_db) else {
                        continue;
                    };
                    if let Err(e) = device.steam_pad_haptic(side, haptic, gain_db) {
                        haptic_backoff_until = Some(Instant::now() + Duration::from_millis(500));
                        if haptic_error_log.ready() {
                            eprintln!("{} pad haptic write failed: {e}", TAG);
                        }
                        break;
                    }
                }
            }
        }

        // Apply after this tick's outputs; the next tick picks up the new profile.
        if let Some(action) = aux.profile.take() {
            match action {
                ProfileAction::Switch(name) => {
                    switch_profile(&state, &name);
                }
                ProfileAction::Cycle => {
                    cycle_profile(&state);
                }
            }
        }
    }
}

fn activate_controller(
    state: &Arc<DaemonState>,
    dev: &mut Option<PhysicalController>,
    mut controller: PhysicalController,
) {
    let info = controller.info().clone();
    println!(
        "{} controller connected: {} ({}, {})",
        TAG,
        info.kind.label(),
        info.serial,
        info.transport
    );

    *wlock(&state.device_kind) = info.kind;
    let remembered = SettingsManager::new()
        .load()
        .remembered_profile(info.kind.profile_group())
        .to_string();
    let loaded = apply_profile_to_state(state, &remembered);
    wlock(&state.inner).active_profile_name = loaded;

    let (want_motion, snap) = {
        let i = rlock(&state.inner);
        (
            i.active_transform.gyro_mouse.enabled
                || i.active_transform.stick_mouse.enabled
                || state.motion_request.load(Ordering::Relaxed),
            (
                i.lightbar_color,
                i.player_leds,
                i.mic_enabled,
                i.trigger_left,
                i.trigger_right,
            ),
        )
    };
    let ((r, g, b, br), leds, mic, tl, tr) = snap;
    let _ = controller.set_lightbar(r, g, b, br);
    let _ = controller.set_player_leds(leds);
    let _ = controller.set_mic(mic);
    let l = tl.or(Some((0x05, [0u8; 10])));
    let r2 = tr.or(Some((0x05, [0u8; 10])));
    let _ = controller.set_trigger_effects(l, r2);

    // A replaced handle for the same controller must not restore device-global
    // state when it is dropped; the new handle owns the device now.
    let same_restorable_device =
        (info.kind.is_steam() || info.kind.is_flydigi() || info.kind.is_gamesir())
            && dev.as_ref().is_some_and(|old| {
                old.kind().family() == info.kind.family() && old.info().identity == info.identity
            });
    let was_steam = dev.as_ref().is_some_and(|old| old.kind().is_steam());

    // Swap the handles while still holding the device lock so IPC clients
    // never observe a gap. The old controller is dropped afterwards.
    let mut old = dev.replace(controller);
    if same_restorable_device && let Some(old) = old.as_mut() {
        old.set_restore_on_drop(false);
    }
    drop(old);

    if was_steam && !info.kind.is_steam() {
        // Let the idle probe re-establish receiver liveness for the new
        // DualSense-active state instead of trusting a pre-switch snapshot.
        clear_steam_live(state);
    }

    // Reassert after the old handle is gone so its drop cannot clobber it.
    if let Some(controller) = dev.as_mut() {
        let result = if want_motion {
            controller.force_motion(true)
        } else {
            controller.set_motion(false)
        };
        if let Err(e) = result {
            log_motion_error("Steam IMU setup failed after activation", e);
        }
    }
    bump_device_generation(state);
}

fn device_connection_loop(state: Arc<DaemonState>) {
    let mut hid_error_log = crate::util::RateLimiter::new(Duration::from_secs(30));
    let mut discovery_error_log = crate::util::RateLimiter::new(Duration::from_secs(30));
    let mut switch_error_log = crate::util::RateLimiter::new(Duration::from_secs(30));
    let mut no_device_log = crate::util::RateLimiter::new(Duration::from_secs(300));

    loop {
        let mut connected_now = false;
        if !state.update_in_progress.load(Ordering::Relaxed) {
            let settings = SettingsManager::new().load();
            let preference = settings.controller_preference;
            let mut dev = mlock(&state.device);

            match HidApi::new() {
                Ok(api) => {
                    let presence = controller::present_families(&api, None);
                    let current_family = dev.as_ref().map(|device| device.kind().family());
                    let target = controller::requested_family(preference).or_else(|| {
                        controller::switch_target(preference, current_family, presence)
                    });

                    if let Some(target) = target
                        && current_family.is_some()
                        && current_family != Some(target)
                    {
                        // Open the requested target before dropping the
                        // current controller. A failed switch keeps it.
                        let target_preference =
                            if controller::requested_family(preference).is_some() {
                                target.preference()
                            } else {
                                preference
                            };
                        match PhysicalController::discover(&api, None, target_preference) {
                            Ok(controller) => {
                                println!("{} switching controller to {}", TAG, target.label());
                                activate_controller(&state, &mut dev, controller);
                                connected_now = true;
                            }
                            Err(e) => {
                                if switch_error_log.ready() {
                                    eprintln!(
                                        "{} cannot switch to {} yet: {}",
                                        TAG,
                                        target.label(),
                                        e
                                    );
                                }
                            }
                        }
                    } else if dev.is_none() {
                        match PhysicalController::discover(&api, None, preference) {
                            Ok(controller) => {
                                activate_controller(&state, &mut dev, controller);
                                connected_now = true;
                            }
                            Err(e) => {
                                let message = e.to_string();
                                let no_controller = message.starts_with("No supported controller");
                                let limiter = if no_controller {
                                    &mut no_device_log
                                } else {
                                    &mut discovery_error_log
                                };
                                if limiter.ready() {
                                    if no_controller {
                                        println!("{} waiting for a controller", TAG);
                                    } else {
                                        eprintln!(
                                            "{} controller discovery failed: {}",
                                            TAG, message
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if hid_error_log.ready() {
                        eprintln!("{} cannot initialize HID API: {}", TAG, e);
                    }
                }
            }

            drop(dev);

            if connected_now {
                *mlock(&state.grab) = None;
                sleep(Duration::from_millis(200));
                *mlock(&state.grab) = Some(InputGrab::acquire());
            }
        }

        let (lock, cvar) = &state.hotplug;
        let mut signaled = mlock(lock);
        if !*signaled {
            let (g, _timed_out) = wait_cv(cvar, signaled, Duration::from_secs(5));
            signaled = g;
        }
        *signaled = false;
    }
}

fn hotplug_thread(state: Arc<DaemonState>) {
    use std::os::fd::AsRawFd;

    let socket = match udev::MonitorBuilder::new()
        .and_then(|b| b.match_subsystem("hidraw"))
        .and_then(|b| b.listen())
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{} udev monitor unavailable: {} (falling back to polling)",
                TAG, e
            );
            return;
        }
    };
    let fd = socket.as_raw_fd();

    println!("{} udev hotplug watcher active", TAG);
    let (lock, cvar) = &state.hotplug;

    loop {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let r = unsafe { libc::poll(&mut pfd, 1, -1) };
        if r < 0 {
            sleep(Duration::from_secs(1));
            continue;
        }

        let mut interesting = false;
        for ev in socket.iter() {
            use udev::EventType;
            if matches!(
                ev.event_type(),
                EventType::Add | EventType::Remove | EventType::Bind | EventType::Unbind
            ) {
                interesting = true;
            }
        }

        if interesting {
            *mlock(lock) = true;
            cvar.notify_all();
        }
    }
}

const STEAM_IDLE_PROBE: Duration = Duration::from_secs(1);
const STEAM_LIVE_REVALIDATE: Duration = Duration::from_secs(5);

/// Watches idle Steam puck/receiver slots while another family is active. The
/// probe is read-only; a no-live -> live transition wakes the device loop so an
/// explicit Steam target can open immediately.
fn steam_liveness_loop(state: Arc<DaemonState>) {
    loop {
        if state.update_in_progress.load(Ordering::Relaxed) {
            sleep(Duration::from_millis(500));
            continue;
        }

        let active_steam = match state.device.try_lock() {
            Ok(device) => device
                .as_ref()
                .is_some_and(|controller| controller.kind().is_steam()),
            Err(_) => {
                sleep(Duration::from_millis(250));
                continue;
            }
        };
        if active_steam {
            sleep(Duration::from_millis(500));
            continue;
        }

        let before = (*mlock(&state.steam_liveness)).clone();
        let probe = HidApi::new()
            .ok()
            .map(|api| controller::probe_steam_slots(&api));

        if let Some(probe) = probe {
            let mut after = before.clone();
            apply_steam_probe(&mut after, &probe);
            *mlock(&state.steam_liveness) = after.clone();

            match (&before.live, &after.live) {
                (None, Some(slot)) => {
                    println!(
                        "{} Steam Controller live on {} ({})",
                        TAG, slot.transport, slot.identity
                    );
                    signal_hotplug(&state);
                }
                (Some(_), None) => {
                    println!("{} Steam Controller no longer live", TAG);
                }
                (Some(old), Some(new)) if old != new => {
                    println!(
                        "{} Steam Controller moved to {} ({})",
                        TAG, new.transport, new.identity
                    );
                }
                _ => {}
            }

            if before.receiver_present != after.receiver_present {
                println!(
                    "{} Steam receiver {}",
                    TAG,
                    if after.receiver_present {
                        "detected"
                    } else {
                        "removed"
                    }
                );
            }
        }

        let interval = if mlock(&state.steam_liveness).live.is_some() {
            STEAM_LIVE_REVALIDATE
        } else {
            STEAM_IDLE_PROBE
        };
        sleep(interval);
    }
}

fn no_input_resp(state: &DaemonState) -> DaemonResponse {
    if mlock(&state.device).is_some() {
        DaemonResponse::Error {
            message: "no fresh input report".into(),
        }
    } else {
        DaemonResponse::NoDevice
    }
}

fn handle_client(stream: DaemonStream, state: Arc<DaemonState>) {
    let write_half = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut reader = BufReader::new(stream);
    let mut writer = write_half;

    let send = |w: &mut DaemonStream, resp: DaemonResponse| {
        if let Ok(mut line) = serde_json::to_string(&resp) {
            line.push('\n');
            let _ = w.write_all(line.as_bytes());
        }
    };

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let cmd: DaemonCommand = match serde_json::from_str(line.trim()) {
            Ok(c) => c,
            Err(e) => {
                send(
                    &mut writer,
                    DaemonResponse::Error {
                        message: e.to_string(),
                    },
                );
                continue;
            }
        };

        match cmd {
            DaemonCommand::Ping {
                protocol,
                app_version,
            } => {
                if protocol == IPC_PROTOCOL_VERSION {
                    send(
                        &mut writer,
                        DaemonResponse::Pong {
                            protocol: IPC_PROTOCOL_VERSION,
                            app_version: APP_VERSION.to_string(),
                        },
                    );
                } else {
                    println!(
                        "{} rejected incompatible client (protocol {}, app {})",
                        TAG, protocol, app_version
                    );
                    send(
                        &mut writer,
                        DaemonResponse::IncompatibleProtocol {
                            daemon_protocol: IPC_PROTOCOL_VERSION,
                            daemon_version: APP_VERSION.to_string(),
                        },
                    );
                }
            }

            DaemonCommand::SetControllerPreference { preference } => {
                let manager = SettingsManager::new();
                let mut settings = manager.load();
                settings.controller_preference = preference;
                manager.save(&settings);
                println!(
                    "{} controller preference set to {}",
                    TAG,
                    preference.label()
                );
                signal_hotplug(&state);
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::Shutdown => {
                send(&mut writer, DaemonResponse::Ok);
                println!("{} shutdown requested via IPC", TAG);
                *mlock(&state.grab) = None;
                // Drop the HID handle first so a Steam Controller restores
                // lizard mode before the process exits.
                *mlock(&state.device) = None;
                std::process::exit(0);
            }

            DaemonCommand::SetUpdateMode { active } => {
                if active {
                    state.update_in_progress.store(true, Ordering::SeqCst);
                    *mlock(&state.device) = None;
                    *mlock(&state.grab) = None;
                    bump_device_generation(&state);
                    println!("{} device released for firmware update", TAG);
                } else {
                    state.update_in_progress.store(false, Ordering::SeqCst);
                    println!("{} firmware update done, device will reconnect", TAG);
                }
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SetInputTransform { mut transform } => {
                transform.normalize_bindings();
                state.touchpad_mode.set(transform.touchpad_mode);
                let changed = {
                    let mut inner = wlock(&state.inner);
                    let changed = inner.active_transform.button_bindings
                        != transform.button_bindings
                        || inner.active_transform.layers != transform.layers
                        || inner.active_transform.press_timing != transform.press_timing
                        || inner.active_transform.stick_digital != transform.stick_digital
                        || inner.active_transform.gyro_mouse != transform.gyro_mouse
                        || inner.active_transform.stick_mouse != transform.stick_mouse
                        || inner.active_transform.steam_left_pad_mouse
                            != transform.steam_left_pad_mouse
                        || inner.active_transform.steam_right_pad_mouse
                            != transform.steam_right_pad_mouse;
                    inner.active_transform = transform;
                    changed
                };
                if changed {
                    state.output_reset.store(true, Ordering::SeqCst);
                }
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::ClearInputTransform => {
                let transform = InputTransform::default();
                state.touchpad_mode.set(transform.touchpad_mode);
                wlock(&state.inner).active_transform = transform;
                state.output_reset.store(true, Ordering::SeqCst);
                state.pending_reset.store(true, Ordering::SeqCst);
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SetLightbarEffect { effect } => {
                let restoring = matches!(effect, LightbarEffect::None);
                wlock(&state.inner).active_effect = effect;

                if restoring {
                    let (r, g, b, br) = rlock(&state.inner).lightbar_color;
                    if let Some(ds) = mlock(&state.device).as_mut() {
                        let _ = ds.set_lightbar(r, g, b, br);
                    }
                }
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SetHapticPattern {
                pattern,
                strength,
                speed,
            } => {
                let restoring = matches!(pattern, HapticPattern::None);
                wlock(&state.inner).haptic = (pattern, strength.min(7), speed.max(0.05));
                if restoring && let Some(ds) = mlock(&state.device).as_mut() {
                    let _ = ds.set_vibration(0, 0);
                }
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SetRawHaptics { active } => {
                wlock(&state.inner).raw_haptics = active;
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SetGyro {
                enabled,
                smoothing,
                sensitivity,
            } => {
                let mut inner = wlock(&state.inner);
                let g = &mut inner.gyro;
                g.enabled = enabled;
                g.smoothing = smoothing.clamp(0.0, 0.95);
                g.sensitivity = sensitivity.max(0.0);
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SetTriggerEffects { left, right } => {
                {
                    let mut inner = wlock(&state.inner);
                    if let Some(l) = left {
                        inner.trigger_left = Some(l);
                    }
                    if let Some(r) = right {
                        inner.trigger_right = Some(r);
                    }
                }
                if state.update_in_progress.load(Ordering::Relaxed) {
                    send(
                        &mut writer,
                        DaemonResponse::Error {
                            message: "Firmware update in progress".to_string(),
                        },
                    );
                    continue;
                }
                let (l, r) = {
                    let i = rlock(&state.inner);
                    (i.trigger_left, i.trigger_right)
                };
                if let Some(ds) = mlock(&state.device).as_mut() {
                    let lf = l.or(Some((0x05, [0u8; 10])));
                    let rt = r.or(Some((0x05, [0u8; 10])));
                    let _ = ds.set_trigger_effects(lf, rt);
                }
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::SwitchProfile { name } => {
                switch_profile(&state, &name);
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::ReloadProfile => {
                let name = rlock(&state.inner).active_profile_name.clone();
                let name = if name.is_empty() {
                    "Default".to_string()
                } else {
                    name
                };
                apply_profile_to_state(&state, &name);
                push_triggers_to_device(&state);
                reassert_motion(&state);
                println!("{} reloaded profile '{}'", TAG, name);
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::ListProfiles => {
                let pm = ProfileManager::new();
                let kind = *rlock(&state.device_kind);
                let profiles = pm
                    .list_profiles_for(kind)
                    .into_iter()
                    .map(|p| p.name)
                    .collect();
                send(&mut writer, DaemonResponse::ProfileList { profiles });
            }

            DaemonCommand::SaveProfile { profile } => {
                let pm = ProfileManager::new();
                let active_kind = mlock(&state.device).as_ref().map(PhysicalController::kind);
                match prepare_profile_for_save(profile, active_kind) {
                    Ok(profile) => match pm.save_profile(&profile) {
                        Ok(_) => send(&mut writer, DaemonResponse::Ok),
                        Err(e) => send(
                            &mut writer,
                            DaemonResponse::Error {
                                message: e.to_string(),
                            },
                        ),
                    },
                    Err(message) => send(&mut writer, DaemonResponse::Error { message }),
                }
            }

            DaemonCommand::DeleteProfile { name } => {
                let pm = ProfileManager::new();
                match pm.delete_profile(&name) {
                    Ok(_) => send(&mut writer, DaemonResponse::Ok),
                    Err(e) => send(
                        &mut writer,
                        DaemonResponse::Error {
                            message: e.to_string(),
                        },
                    ),
                }
            }

            DaemonCommand::RenameProfile { old, new } => {
                let pm = ProfileManager::new();
                match pm.load_profile(&old) {
                    Ok(profile) => {
                        let group = profile.device.profile_group();
                        match pm.rename_profile(&old, &new) {
                            Ok(()) => {
                                {
                                    let mut inner = wlock(&state.inner);
                                    if inner.active_profile_name == old {
                                        inner.active_profile_name = new.clone();
                                    }
                                }
                                let sm = SettingsManager::new();
                                let mut settings = sm.load();
                                if settings.remembered_profile(group) == old.as_str() {
                                    settings.remember_profile(group, &new);
                                    sm.save(&settings);
                                }
                                println!("{} renamed profile '{}' to '{}'", TAG, old, new);
                                send(&mut writer, DaemonResponse::Ok);
                            }
                            Err(e) => send(
                                &mut writer,
                                DaemonResponse::Error {
                                    message: e.to_string(),
                                },
                            ),
                        }
                    }
                    Err(e) => send(
                        &mut writer,
                        DaemonResponse::Error {
                            message: e.to_string(),
                        },
                    ),
                }
            }

            DaemonCommand::GetActiveProfile => {
                let name = rlock(&state.inner).active_profile_name.clone();
                send(&mut writer, DaemonResponse::ActiveProfile { name });
            }

            DaemonCommand::GetInputState => match fresh_input(&state) {
                Some(l) => {
                    let mut s = l.state;
                    rlock(&state.inner).active_transform.apply_axes(&mut s);
                    send(&mut writer, DaemonResponse::InputState(s));
                }
                None => send(&mut writer, no_input_resp(&state)),
            },

            DaemonCommand::GetBattery => match fresh_battery(&state) {
                Some(info) => send(&mut writer, DaemonResponse::Battery(info)),
                None => send(&mut writer, no_input_resp(&state)),
            },

            DaemonCommand::GetSteamLiveness => {
                let liveness = (*mlock(&state.steam_liveness)).clone();
                send(&mut writer, DaemonResponse::SteamLiveness(liveness));
            }

            DaemonCommand::SetMotion { enabled } => {
                state.motion_request.store(enabled, Ordering::Relaxed);
                if let Some(controller) = mlock(&state.device).as_mut() {
                    let result = if enabled {
                        controller.force_motion(true)
                    } else {
                        controller.set_motion(false)
                    };
                    if let Err(e) = result {
                        log_motion_error("IPC motion request failed", e);
                    }
                }
                send(&mut writer, DaemonResponse::Ok);
            }

            cmd @ (DaemonCommand::GetFirmwareInfo | DaemonCommand::GetControllerInfo) => {
                let transform = rlock(&state.inner).active_transform.clone();
                let mut dev = mlock(&state.device);
                match dev.as_mut() {
                    None => send(&mut writer, DaemonResponse::NoDevice),
                    Some(ds) => {
                        let resp = dispatch(ds, cmd, &transform);
                        send(&mut writer, resp);
                    }
                }
            }

            cmd => {
                if state.update_in_progress.load(Ordering::Relaxed) {
                    send(
                        &mut writer,
                        DaemonResponse::Error {
                            message: "Firmware update in progress".to_string(),
                        },
                    );
                    continue;
                }

                if let DaemonCommand::SetLightbar {
                    r,
                    g,
                    b,
                    brightness,
                } = &cmd
                {
                    let mut inner = wlock(&state.inner);
                    inner.lightbar_color = (*r, *g, *b, *brightness);
                    if !matches!(inner.active_effect, LightbarEffect::None) {
                        send(&mut writer, DaemonResponse::Ok);
                        continue;
                    }
                }

                if let DaemonCommand::SetTriggerEffect {
                    left,
                    right,
                    effect_type,
                    params,
                } = &cmd
                {
                    let mut inner = wlock(&state.inner);
                    let new_val = if *effect_type == 0x05 {
                        None
                    } else {
                        Some((*effect_type, *params))
                    };
                    if *left {
                        inner.trigger_left = new_val;
                    }
                    if *right {
                        inner.trigger_right = new_val;
                    }
                } else if let DaemonCommand::SetTriggerOff = &cmd {
                    let mut inner = wlock(&state.inner);
                    inner.trigger_left = None;
                    inner.trigger_right = None;
                }

                let transform = rlock(&state.inner).active_transform.clone();
                let mut dev = mlock(&state.device);
                match dev.as_mut() {
                    None => {
                        let (l, c) = &state.hotplug;
                        *mlock(l) = true;
                        c.notify_all();
                        send(&mut writer, DaemonResponse::NoDevice);
                    }
                    Some(ds) => {
                        let resp = dispatch(ds, cmd, &transform);
                        let failed = matches!(&resp, DaemonResponse::Error { .. });
                        send(&mut writer, resp);
                        if failed {
                            println!("{} device error - dropping handle", TAG);
                            *dev = None;
                            bump_device_generation(&state);
                            let (l, c) = &state.hotplug;
                            *mlock(l) = true;
                            c.notify_all();
                        }
                    }
                }
            }
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

fn effect_loop(state: Arc<DaemonState>) {
    let start = Instant::now();
    loop {
        sleep(Duration::from_millis(33));

        let effect = rlock(&state.inner).active_effect.clone();
        if matches!(effect, LightbarEffect::None) {
            continue;
        }

        if state.update_in_progress.load(Ordering::Relaxed) {
            continue;
        }

        let t = start.elapsed().as_secs_f32();
        let (base_r, base_g, base_b, base_br) = rlock(&state.inner).lightbar_color;

        let (r, g, b) = match effect {
            LightbarEffect::Breath { speed } => {
                let factor = ((t * speed * std::f32::consts::TAU).sin() * 0.5 + 0.5).max(0.0);
                (
                    (base_r as f32 * factor) as u8,
                    (base_g as f32 * factor) as u8,
                    (base_b as f32 * factor) as u8,
                )
            }
            LightbarEffect::Rainbow { speed } => {
                let hue = (t * speed * 360.0) % 360.0;
                hsv_to_rgb(hue, 1.0, 1.0)
            }
            LightbarEffect::Strobe { speed } => {
                if ((t * speed * 2.0) as u32).is_multiple_of(2) {
                    (base_r, base_g, base_b)
                } else {
                    (0, 0, 0)
                }
            }
            LightbarEffect::None => unreachable!(),
        };

        if let Ok(mut dev) = state.device.try_lock()
            && let Some(ds) = dev.as_mut()
        {
            let _ = ds.set_lightbar(r, g, b, base_br);
        }
    }
}

fn haptic_loop(state: Arc<DaemonState>) {
    let start = Instant::now();
    let mut last_amp: u8 = 255;
    loop {
        sleep(Duration::from_millis(40));
        if state.update_in_progress.load(Ordering::Relaxed) {
            continue;
        }

        let (raw, (pattern, strength, speed)) = {
            let g = rlock(&state.inner);
            (g.raw_haptics, g.haptic)
        };
        if raw {
            if last_amp != 0 {
                let mut dev = mlock(&state.device);
                if let Some(ds) = dev.as_mut() {
                    let _ = ds.set_rumble(0, 0);
                }
                last_amp = 0;
            }
            continue;
        }

        if matches!(pattern, HapticPattern::None) {
            if last_amp != 0 {
                let mut dev = mlock(&state.device);
                if let Some(ds) = dev.as_mut() {
                    let _ = ds.set_rumble(0, 0);
                }
                last_amp = 0;
            }
            continue;
        }

        let t = start.elapsed().as_secs_f32();
        let s = (strength.min(7) as f32 / 7.0).clamp(0.0, 1.0);
        let intensity = match pattern {
            HapticPattern::None => 0.0,
            HapticPattern::Constant => s,
            HapticPattern::Pulse => {
                let phase = (t * speed) % 1.0;
                if phase < 0.5 { s } else { 0.0 }
            }
            HapticPattern::Ramp => {
                let phase = (t * speed) % 1.0;
                s * phase
            }
            HapticPattern::Wave => {
                let v = (t * speed * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                s * v
            }
        };

        let amp = (intensity.clamp(0.0, 1.0) * 255.0).round() as u8;
        if amp != last_amp {
            let mut dev = mlock(&state.device);
            if let Some(ds) = dev.as_mut() {
                let _ = ds.set_rumble(amp, amp);
            }
            last_amp = amp;
        }
    }
}

fn raw_haptic_loop(state: Arc<DaemonState>) {
    let start = Instant::now();
    let period = Duration::from_secs_f32(HAPTICS_PACKET_FRAMES as f32 / HAPTICS_SAMPLE_RATE as f32);
    loop {
        sleep(period);
        if state.update_in_progress.load(Ordering::Relaxed) {
            continue;
        }
        let (raw, (pattern, strength, speed)) = {
            let g = rlock(&state.inner);
            (g.raw_haptics, g.haptic)
        };
        if !raw || matches!(pattern, HapticPattern::None) {
            continue;
        }
        let t0 = start.elapsed().as_secs_f32();
        let packet = generate_packet(pattern, strength, speed, t0);
        let mut dev = mlock(&state.device);
        if let Some(ds) = dev.as_mut().filter(|d| d.is_bluetooth()) {
            let _ = ds.set_haptics(&packet);
        }
    }
}

fn dispatch(
    device: &mut PhysicalController,
    cmd: DaemonCommand,
    transform: &InputTransform,
) -> DaemonResponse {
    macro_rules! ok_or_err {
        ($e:expr) => {
            match $e {
                Ok(_) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Error {
                    message: e.to_string(),
                },
            }
        };
    }

    match cmd {
        DaemonCommand::Ping { .. } => DaemonResponse::Pong {
            protocol: IPC_PROTOCOL_VERSION,
            app_version: APP_VERSION.to_string(),
        },

        DaemonCommand::GetBattery => match device.get_battery() {
            Ok(b) => DaemonResponse::Battery(b),
            Err(e) => DaemonResponse::Error {
                message: e.to_string(),
            },
        },

        DaemonCommand::GetInputState => match device.get_input_state() {
            Ok(mut s) => {
                transform.apply_axes(&mut s);
                DaemonResponse::InputState(s)
            }
            Err(e) => DaemonResponse::Error {
                message: e.to_string(),
            },
        },

        DaemonCommand::GetFirmwareInfo => match device.get_firmware_info() {
            Ok((v, d, t)) => DaemonResponse::FirmwareInfo {
                version: v,
                build_date: d,
                build_time: t,
            },
            Err(e) => DaemonResponse::Error {
                message: e.to_string(),
            },
        },

        DaemonCommand::GetControllerInfo => DaemonResponse::ControllerInfo {
            serial: device.serial().to_string(),
            product_id: device.product_id(),
            is_bt: device.is_bluetooth(),
            kind: device.kind(),
            transport: device.info().transport.clone(),
        },

        DaemonCommand::SetLightbar {
            r,
            g,
            b,
            brightness,
        } => ok_or_err!(device.set_lightbar(r, g, b, brightness)),

        DaemonCommand::SetLightbarEnabled { enabled } => {
            ok_or_err!(device.set_lightbar_enabled(enabled))
        }

        DaemonCommand::SetPlayerLeds { leds } => ok_or_err!(device.set_player_leds(leds)),

        DaemonCommand::SetMic { enabled } => ok_or_err!(device.set_mic(enabled)),

        DaemonCommand::SetMicLed { state } => ok_or_err!(device.set_mic_led(state)),

        DaemonCommand::SetTriggerOff => ok_or_err!(device.set_trigger_off()),

        DaemonCommand::SetTriggerEffect {
            right,
            left,
            effect_type,
            params,
        } => ok_or_err!(device.set_trigger_effect(left, right, effect_type, &params)),

        DaemonCommand::SetVibration { rumble, trigger } => {
            ok_or_err!(device.set_vibration(rumble, trigger))
        }

        DaemonCommand::SetRumble { left, right } => {
            ok_or_err!(device.set_rumble(left, right))
        }

        DaemonCommand::SetSpeaker { mode } => ok_or_err!(device.set_speaker(&mode)),

        DaemonCommand::SetVolume { volume } => ok_or_err!(device.set_volume(volume)),

        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Profile;

    fn profile(device: ControllerKind) -> Profile {
        Profile {
            name: "TheFinals".into(),
            device,
            ..Default::default()
        }
    }

    #[test]
    fn stick_mouse_alone_requests_imu() {
        let state = DaemonState::new();
        assert!(!motion_is_wanted(&state));
        wlock(&state.inner).active_transform.stick_mouse.enabled = true;
        assert!(motion_is_wanted(&state));
    }

    #[test]
    fn motion_sync_requires_one_enable_then_disables_once() {
        let start = Instant::now();
        let mut sync = MotionSync::new();

        let enable = sync.poll(start, 1, true, 10);
        assert_eq!(enable, Some(MotionCommand::Enable { force: false }));
        sync.confirm(start, enable.unwrap());
        assert_eq!(sync.poll(start, 1, true, 10), None);

        let disable = sync.poll(start + Duration::from_millis(10), 1, false, 10);
        assert_eq!(disable, Some(MotionCommand::Disable));
        sync.confirm(start + Duration::from_millis(10), disable.unwrap());
        assert_eq!(
            sync.poll(start + Duration::from_millis(20), 1, false, 10),
            None
        );
    }

    #[test]
    fn motion_sync_stays_quiet_while_timestamp_advances() {
        let start = Instant::now();
        let mut sync = MotionSync::new();
        let enable = sync.poll(start, 1, true, 0).expect("initial IMU enable");
        sync.confirm(start, enable);

        for tick in 1..=10 {
            let now = start + Duration::from_millis(tick * 40);
            assert_eq!(
                sync.poll(now, 1, true, tick as u32),
                None,
                "advancing timestamp tick {tick}"
            );
        }
    }

    #[test]
    fn motion_sync_forces_once_per_cooldown_when_timestamp_stalls() {
        let start = Instant::now();
        let mut sync = MotionSync::new();
        let enable = sync.poll(start, 1, true, 42).expect("initial IMU enable");
        sync.confirm(start, enable);

        assert_eq!(
            sync.poll(start + Duration::from_millis(249), 1, true, 42),
            None
        );

        let forced_at = start + Duration::from_millis(250);
        let force = sync.poll(forced_at, 1, true, 42);
        assert_eq!(force, Some(MotionCommand::Enable { force: true }));
        sync.confirm(forced_at, force.unwrap());

        assert_eq!(
            sync.poll(forced_at + Duration::from_millis(400), 1, true, 42),
            None,
            "cooldown suppresses repeated forced writes"
        );
        assert_eq!(
            sync.poll(forced_at + Duration::from_millis(1001), 1, true, 42),
            Some(MotionCommand::Enable { force: true })
        );
    }

    #[test]
    fn motion_sync_renogotiates_on_device_generation_change() {
        let start = Instant::now();
        let mut sync = MotionSync::new();
        let enable = sync
            .poll(start, 1, true, 100)
            .expect("first generation enable");
        sync.confirm(start, enable);
        assert_eq!(
            sync.poll(start + Duration::from_millis(50), 1, true, 101),
            None
        );

        let renegotiate = sync.poll(start + Duration::from_millis(60), 2, true, 5);
        assert_eq!(renegotiate, Some(MotionCommand::Enable { force: false }));
        assert_eq!(sync.generation(), Some(2));
    }

    #[test]
    fn save_without_device_preserves_client_family() {
        let saved = prepare_profile_for_save(profile(ControllerKind::SteamController), None)
            .expect("save allowed");
        assert_eq!(saved.device, ControllerKind::SteamController);
    }

    #[test]
    fn save_rejects_explicit_family_mismatch() {
        let error = match prepare_profile_for_save(
            profile(ControllerKind::SteamController),
            Some(ControllerKind::DualSense),
        ) {
            Ok(_) => panic!("mismatch must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("Steam Controller"));
    }

    #[test]
    fn save_accepts_matching_family() {
        let saved = prepare_profile_for_save(
            profile(ControllerKind::SteamController),
            Some(ControllerKind::SteamController),
        )
        .expect("matching save allowed");
        assert_eq!(saved.device, ControllerKind::SteamController);

        let saved = prepare_profile_for_save(
            profile(ControllerKind::DualSenseEdge),
            Some(ControllerKind::DualSense),
        )
        .expect("same family save allowed");
        assert_eq!(saved.device, ControllerKind::DualSenseEdge);
    }

    #[test]
    fn save_rejects_apex_and_vader_profiles_across_groups() {
        let error = match prepare_profile_for_save(
            profile(ControllerKind::FlydigiApex5),
            Some(ControllerKind::FlydigiVader5Pro),
        ) {
            Ok(_) => panic!("Apex profile on Vader must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("Flydigi Apex 5"));
        assert!(error.contains("Flydigi Vader 5 Pro"));

        let error = match prepare_profile_for_save(
            profile(ControllerKind::FlydigiVader5Pro),
            Some(ControllerKind::FlydigiApex5),
        ) {
            Ok(_) => panic!("Vader profile on Apex must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("Flydigi Vader 5 Pro"));
        assert!(error.contains("Flydigi Apex 5"));
    }

    #[test]
    fn save_rejects_gamesir_profile_while_another_family_is_active() {
        let error = match prepare_profile_for_save(
            profile(ControllerKind::GameSirTarantulaPro),
            Some(ControllerKind::DualSense),
        ) {
            Ok(_) => panic!("GameSir profile on DualSense must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("GameSir Tarantula Pro"));
        assert!(error.contains("DualSense"));

        let saved = prepare_profile_for_save(
            profile(ControllerKind::GameSirTarantulaPro),
            Some(ControllerKind::GameSirTarantulaPro),
        )
        .expect("matching GameSir save allowed");
        assert_eq!(saved.device, ControllerKind::GameSirTarantulaPro);
    }

    #[test]
    fn save_accepts_same_group_profiles_for_flydigi_and_sony() {
        for (profile_kind, connected) in [
            (ControllerKind::FlydigiApex5, ControllerKind::FlydigiApex5),
            (
                ControllerKind::FlydigiVader5Pro,
                ControllerKind::FlydigiVader5Pro,
            ),
            (ControllerKind::DualSense, ControllerKind::DualSenseEdge),
            (ControllerKind::DualSenseEdge, ControllerKind::DualSense),
        ] {
            let saved = prepare_profile_for_save(profile(profile_kind), Some(connected))
                .expect("same-group save allowed");
            assert_eq!(saved.device, profile_kind);
        }
    }

    #[test]
    fn save_rejects_eightbitdo_profile_on_sony_controller() {
        let error = match prepare_profile_for_save(
            profile(ControllerKind::EightBitDoPro3),
            Some(ControllerKind::DualSense),
        ) {
            Ok(_) => panic!("mismatch must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("8BitDo Pro 3"));
        assert!(error.contains("DualSense"));

        let saved = prepare_profile_for_save(
            profile(ControllerKind::EightBitDoPro3),
            Some(ControllerKind::EightBitDoPro3),
        )
        .expect("matching 8BitDo save allowed");
        assert_eq!(saved.device, ControllerKind::EightBitDoPro3);
    }

    fn steam_slot(slot: i32) -> controller::SteamLiveSlot {
        controller::SteamLiveSlot {
            vendor_id: crate::steam_controller::STEAM_VID,
            product_id: crate::steam_controller::STEAM_PID_PUCK,
            interface_number: slot,
            path: format!("/dev/hidraw1{slot}"),
            serial: Some("FXB9961001F79".to_string()),
            identity: "FXB9961001F79".to_string(),
            transport: format!("Wireless puck slot {slot}"),
        }
    }

    fn probe(slot: Option<i32>) -> controller::SteamProbeResult {
        controller::SteamProbeResult {
            receiver_present: true,
            state: slot.map_or(controller::SteamSlotState::Unknown, |slot| {
                controller::SteamSlotState::Live(steam_slot(slot))
            }),
        }
    }

    #[test]
    fn steam_probe_cache_tracks_live_and_receiver_presence() {
        let mut cache = controller::SteamLiveness::default();
        apply_steam_probe(&mut cache, &probe(Some(3)));
        assert!(cache.receiver_present);
        assert_eq!(cache.live.as_ref().map(|s| s.interface_number), Some(3));

        apply_steam_probe(
            &mut cache,
            &controller::SteamProbeResult {
                receiver_present: false,
                state: controller::SteamSlotState::Unknown,
            },
        );
        assert!(!cache.receiver_present);
        assert!(cache.live.is_none());
    }

    #[test]
    fn steam_probe_unknown_keeps_cached_live_while_receiver_is_present() {
        let mut cache = controller::SteamLiveness::default();
        apply_steam_probe(&mut cache, &probe(Some(4)));

        apply_steam_probe(&mut cache, &probe(None));
        assert!(cache.receiver_present);
        assert_eq!(cache.live.as_ref().map(|s| s.interface_number), Some(4));

        apply_steam_probe(
            &mut cache,
            &controller::SteamProbeResult {
                receiver_present: true,
                state: controller::SteamSlotState::NotLive,
            },
        );
        assert!(cache.live.is_none());
    }
}
