use std::{
    io::{BufRead, BufReader, Write},
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, sleep},
    time::{Duration, Instant},
};

use hidapi::HidApi;

use crate::{
    common::{HapticPattern, LightbarEffect, TouchpadMode},
    dualsense::{DualSense, HAPTICS_PACKET_FRAMES, HAPTICS_SAMPLE_RATE},
    gyro::GyroRuntime,
    haptics_stream::generate_packet,
    input_grab::InputGrab,
    inputs::ControllerState,
    ipc::{
        DaemonCommand, DaemonResponse, DaemonStream, IpcClient, addr_display, bind_daemon,
        cleanup_endpoint, daemon_endpoint,
    },
    profiles::ProfileManager,
    remap::{AuxOutput, Binding, ProfileAction, RemapEngine},
    settings::SettingsManager,
    stick_mouse::StickMouseRuntime,
    touchpad_filter::{TouchpadFilter, TouchpadModeHandle, TouchpadOwnedHandle},
    transform::{GyroProcessor, InputTransform},
    util::{mlock, rlock, wait_cv, wlock},
    virtual_gamepad::VirtualGamepad,
    virtual_keyboard::VirtualKeyboard,
    virtual_mouse::VirtualMouse,
};

const TAG: &str = "[gyro4u daemon]";

pub struct DaemonManager {
    client: Option<Arc<Mutex<IpcClient>>>,
}

impl DaemonManager {
    pub fn new() -> Self {
        let addr = daemon_endpoint();
        let client = IpcClient::try_connect(&addr).map(|c| Arc::new(Mutex::new(c)));
        Self { client }
    }

    pub fn is_active(&self) -> bool {
        self.client.is_some()
    }

    pub fn refresh(&mut self) {
        if self.client.is_none() {
            let addr = daemon_endpoint();
            self.client = IpcClient::try_connect(&addr).map(|c| Arc::new(Mutex::new(c)));
        }
    }

    pub fn connect_new_client(&self) -> Option<Arc<Mutex<IpcClient>>> {
        let addr = daemon_endpoint();
        IpcClient::try_connect(&addr).map(|c| Arc::new(Mutex::new(c)))
    }

    pub fn client(&self) -> Option<Arc<Mutex<IpcClient>>> {
        self.client.clone()
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

#[derive(Clone, Copy)]
struct LatestInput {
    state: ControllerState,
    status_byte: u8,
    at: Instant,
}

struct DaemonState {
    device: Mutex<Option<DualSense>>,
    grab: Mutex<Option<InputGrab>>,
    update_in_progress: AtomicBool,
    output_reset: AtomicBool,
    inner: RwLock<DaemonInner>,
    hotplug: (Mutex<bool>, Condvar),
    latest_input: Mutex<Option<LatestInput>>,
    touchpad_mode: TouchpadModeHandle,
    touchpad_owned: TouchpadOwnedHandle,
}

impl DaemonState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            device: Mutex::new(None),
            grab: Mutex::new(None),
            update_in_progress: AtomicBool::new(false),
            output_reset: AtomicBool::new(false),
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
            touchpad_mode: TouchpadModeHandle::new(TouchpadMode::Mouse),
            touchpad_owned: TouchpadOwnedHandle::new(),
        })
    }
}

fn fresh_input(state: &DaemonState) -> Option<LatestInput> {
    (*mlock(&state.latest_input)).filter(|l| l.at.elapsed() < Duration::from_millis(500))
}

fn signal_hotplug(state: &DaemonState) {
    let (l, c) = &state.hotplug;
    *mlock(l) = true;
    c.notify_all();
}

fn apply_profile_to_state(state: &Arc<DaemonState>, name: &str) -> String {
    let pm = ProfileManager::new();
    let profile = if pm.profile_exists(name) {
        pm.load_profile(name).ok()
    } else if pm.profile_exists("Default") {
        pm.load_profile("Default").ok()
    } else {
        Some(pm.ensure_default_exists())
    };

    let Some(p) = profile else {
        return String::new();
    };

    let mut inner = wlock(&state.inner);
    let mut transform = p.to_input_transform();
    transform.normalize_bindings();
    inner.active_transform = transform;
    state
        .touchpad_mode
        .set(inner.active_transform.touchpad_mode);
    state.output_reset.store(true, Ordering::SeqCst);
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
    wlock(&state.inner).active_profile_name = loaded.clone();
    let sm = SettingsManager::new();
    let mut settings = sm.load();
    settings.profile = loaded.clone();
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
    println!("{} switched to profile '{}'", TAG, loaded);
    loaded
}

fn cycle_profile(state: &Arc<DaemonState>) -> String {
    let active = rlock(&state.inner).active_profile_name.clone();
    let profiles = ProfileManager::new().list_profiles();
    let next = if profiles.is_empty() {
        "Default".to_string()
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

    println!("{} listening on {}", TAG, addr_display(&addr));

    let state = DaemonState::new();

    {
        let sm = SettingsManager::new();
        let settings = sm.load();
        let name = if settings.profile.is_empty() {
            "Default"
        } else {
            &settings.profile
        };
        let loaded = apply_profile_to_state(&state, name);
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
    Data((u16, u16), Option<crate::dualsense::PolledInput>),
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
    let mut pad_ids: Option<(u16, u16)> = None;
    let mut pad_disabled = false;
    let mut remap = RemapEngine::new();
    let mut aux = AuxOutput::default();
    let mut gyro_runtime = GyroRuntime::new();
    let mut stick_mouse = StickMouseRuntime::new();
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
            if let Some(pad) = pad.as_mut() {
                pad.reset();
            }
        }

        if state.update_in_progress.load(Ordering::Relaxed) {
            release_aux(&mut keyboard, &mut mouse);
            pad = None;
            pad_ids = None;
            remap.reset();
            gyro_runtime.reset();
            stick_mouse.reset();
            sleep(Duration::from_millis(20));
            continue;
        }

        let poll = {
            let mut dev = mlock(&state.device);
            match dev.as_mut() {
                None => Poll::NoDevice,
                Some(ds) => {
                    let ids = (ds.vendor_id(), ds.product_id());
                    match ds.poll_latest_input() {
                        Ok(p) => Poll::Data(ids, p),
                        Err(_) => Poll::Failed,
                    }
                }
            }
        };

        let (ids, polled) = match poll {
            Poll::NoDevice => {
                release_aux(&mut keyboard, &mut mouse);
                pad = None;
                pad_ids = None;
                remap.reset();
                gyro_runtime.reset();
                stick_mouse.reset();
                sleep(Duration::from_millis(20));
                continue;
            }
            Poll::Failed => {
                println!("{} input read failed - dropping handle", TAG);
                release_aux(&mut keyboard, &mut mouse);
                pad = None;
                pad_ids = None;
                remap.reset();
                gyro_runtime.reset();
                stick_mouse.reset();
                *mlock(&state.device) = None;
                signal_hotplug(&state);
                continue;
            }
            Poll::Data(ids, p) => (ids, p),
        };

        if !pad_disabled && pad_ids != Some(ids) {
            pad = None;
            gyro_runtime.reset();
            stick_mouse.reset();
            match VirtualGamepad::new(ids.0, ids.1) {
                Ok(p) => {
                    println!("{} virtual gamepad {:04x}:{:04x}", TAG, ids.0, ids.1);
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

        *mlock(&state.latest_input) = Some(LatestInput {
            state: p.state,
            status_byte: p.status_byte,
            at: Instant::now(),
        });

        let Some(pad) = pad.as_mut() else { continue };

        let (transform, touchpad_allowed) = {
            let inner = rlock(&state.inner);
            let touchpad_allowed = state.touchpad_owned.get()
                && matches!(
                    state.touchpad_mode.get(),
                    TouchpadMode::Disabled | TouchpadMode::GesturesOnly
                );
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

        let gyro_rel =
            gyro_runtime.process(&p.state, &transform.gyro_mouse, state.touchpad_owned.get());
        let stick_rel = stick_mouse.process(
            (p.state.right_x, p.state.right_y),
            &transform.stick_mouse,
            p.state.sensor_timestamp,
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
        aux.rel = (gyro_rel.0 + stick_rel.0, gyro_rel.1 + stick_rel.1);
        let _ = pad.update(&st);
        if let Some(keyboard) = keyboard.as_mut() {
            let _ = keyboard.apply(&aux.keys);
        }
        if let Some(mouse) = mouse.as_mut() {
            let _ = mouse.apply(&aux.mouse_buttons, aux.rel, aux.wheel);
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

fn device_connection_loop(state: Arc<DaemonState>) {
    loop {
        if !state.update_in_progress.load(Ordering::Relaxed) {
            let mut connected_now = false;
            let mut dev = mlock(&state.device);
            if dev.is_none()
                && let Ok(api) = HidApi::new()
                && let Ok(mut ds) = DualSense::new(&api, None)
            {
                println!("{} controller connected: {}", TAG, ds.serial());

                let snap = {
                    let i = rlock(&state.inner);
                    (
                        i.lightbar_color,
                        i.player_leds,
                        i.mic_enabled,
                        i.trigger_left,
                        i.trigger_right,
                    )
                };
                let ((r, g, b, br), leds, mic, tl, tr) = snap;
                let _ = ds.set_lightbar(r, g, b, br);
                let _ = ds.set_player_leds(leds);
                let _ = ds.set_mic(mic);
                let l = tl.or(Some((0x05, [0u8; 10])));
                let r2 = tr.or(Some((0x05, [0u8; 10])));
                let _ = ds.set_trigger_effects(l, r2);

                *dev = Some(ds);

                connected_now = true;
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
            DaemonCommand::Ping => {
                send(&mut writer, DaemonResponse::Pong);
            }

            DaemonCommand::Shutdown => {
                send(&mut writer, DaemonResponse::Ok);
                println!("{} shutdown requested via IPC", TAG);
                std::process::exit(0);
            }

            DaemonCommand::SetUpdateMode { active } => {
                if active {
                    state.update_in_progress.store(true, Ordering::SeqCst);
                    *mlock(&state.device) = None;
                    *mlock(&state.grab) = None;
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
                        || inner.active_transform.stick_mouse != transform.stick_mouse;
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
                println!("{} reloaded profile '{}'", TAG, name);
                send(&mut writer, DaemonResponse::Ok);
            }

            DaemonCommand::ListProfiles => {
                let pm = ProfileManager::new();
                let profiles = pm.list_profiles().into_iter().map(|p| p.name).collect();
                send(&mut writer, DaemonResponse::ProfileList { profiles });
            }

            DaemonCommand::SaveProfile { profile } => {
                let pm = ProfileManager::new();
                match pm.save_profile(&profile) {
                    Ok(_) => send(&mut writer, DaemonResponse::Ok),
                    Err(e) => send(
                        &mut writer,
                        DaemonResponse::Error {
                            message: e.to_string(),
                        },
                    ),
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

            DaemonCommand::GetBattery => match fresh_input(&state) {
                Some(l) => send(
                    &mut writer,
                    DaemonResponse::Battery(DualSense::battery_from_status(l.status_byte)),
                ),
                None => send(&mut writer, no_input_resp(&state)),
            },

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

fn dispatch(ds: &mut DualSense, cmd: DaemonCommand, transform: &InputTransform) -> DaemonResponse {
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
        DaemonCommand::Ping => DaemonResponse::Pong,

        DaemonCommand::GetBattery => match ds.get_battery() {
            Ok(b) => DaemonResponse::Battery(b),
            Err(e) => DaemonResponse::Error {
                message: e.to_string(),
            },
        },

        DaemonCommand::GetInputState => match ds.get_input_state() {
            Ok(mut s) => {
                transform.apply_axes(&mut s);
                DaemonResponse::InputState(s)
            }
            Err(e) => DaemonResponse::Error {
                message: e.to_string(),
            },
        },

        DaemonCommand::GetFirmwareInfo => match ds.get_firmware_info() {
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
            serial: ds.serial().to_string(),
            product_id: ds.product_id(),
            is_bt: ds.is_bluetooth(),
        },

        DaemonCommand::SetLightbar {
            r,
            g,
            b,
            brightness,
        } => ok_or_err!(ds.set_lightbar(r, g, b, brightness)),

        DaemonCommand::SetLightbarEnabled { enabled } => {
            ok_or_err!(ds.set_lightbar_enabled(enabled))
        }

        DaemonCommand::SetPlayerLeds { leds } => ok_or_err!(ds.set_player_leds(leds)),

        DaemonCommand::SetMic { enabled } => ok_or_err!(ds.set_mic(enabled)),

        DaemonCommand::SetMicLed { state } => ok_or_err!(ds.set_mic_led(state)),

        DaemonCommand::SetTriggerOff => ok_or_err!(ds.set_trigger_off()),

        DaemonCommand::SetTriggerEffect {
            right,
            left,
            effect_type,
            params,
        } => ok_or_err!(ds.set_trigger_effect(left, right, effect_type, &params)),

        DaemonCommand::SetVibration { rumble, trigger } => {
            ok_or_err!(ds.set_vibration(rumble, trigger))
        }

        DaemonCommand::SetRumble { left, right } => {
            ok_or_err!(ds.set_rumble(left, right))
        }

        DaemonCommand::SetSpeaker { mode } => ok_or_err!(ds.set_speaker(&mode)),

        DaemonCommand::SetVolume { volume } => ok_or_err!(ds.set_volume(volume)),

        _ => unreachable!(),
    }
}
