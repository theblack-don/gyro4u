use std::{
    fs::{File, OpenOptions},
    mem::{size_of, zeroed},
    os::unix::io::AsRawFd,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread::{self, JoinHandle, sleep},
    time::Duration,
};

use anyhow::{Context, Result, bail};

use crate::common::{DS_PID, DS_VID, DSE_PID, TouchpadMode};
use crate::uinput::{
    EV_ABS, EV_KEY, EV_SYN, INPUT_PROP_POINTER, InputEvent, SYN_REPORT, UinputDevice, UinputSpec,
};

const TAG: &str = "[gyro4u touchpad]";

const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOUCH: u16 = 0x14a;
const BTN_TOOL_DOUBLETAP: u16 = 0x14d;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;

const INPUT_PROP_BUTTONPAD: u16 = 0x02;

fn iow(ty: u8, nr: u8, size: u32) -> u64 {
    (1u64 << 30) | ((size as u64) << 16) | ((ty as u64) << 8) | (nr as u64)
}

fn eviocgrab() -> u64 {
    iow(b'E', 0x90, size_of::<i32>() as u32)
}

fn errno_name(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(libc::EBUSY) => "EBUSY".into(),
        Some(libc::EACCES) => "EACCES".into(),
        Some(libc::EPERM) => "EPERM".into(),
        Some(libc::ENODEV) => "ENODEV".into(),
        Some(libc::ENOENT) => "ENOENT".into(),
        Some(code) => format!("errno {code}"),
        None => "unknown error".into(),
    }
}

fn create_touchpad(vendor: u16, product: u16) -> Result<UinputDevice> {
    UinputDevice::create(UinputSpec {
        name: "Gyro4U DualSense Touchpad",
        vendor,
        product,
        version: 0x8111,
        keys: &[BTN_TOOL_FINGER, BTN_TOUCH, BTN_TOOL_DOUBLETAP],
        axes: &[
            (ABS_X, 0, 1919, 0),
            (ABS_Y, 0, 1079, 0),
            (ABS_MT_SLOT, 0, 1, 0),
            (ABS_MT_POSITION_X, 0, 1919, 0),
            (ABS_MT_POSITION_Y, 0, 1079, 0),
            (ABS_MT_TRACKING_ID, 0, 65535, 0),
        ],
        rels: &[],
        props: &[INPUT_PROP_POINTER, INPUT_PROP_BUTTONPAD],
    })
}

const GESTURE_START_DISTANCE: i32 = 48;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Slot {
    active: bool,
    positioned: bool,
    id: i32,
    x: i32,
    y: i32,
}

#[derive(Default)]
struct FilterState {
    native: [Slot; 2],
    anchor: [Slot; 2],
    slot: usize,
    emitted: [Slot; 2],
    waiting: bool,
    emitting: bool,
}

impl FilterState {
    fn feed(&mut self, ev: InputEvent, touch: &mut Option<UinputDevice>) -> std::io::Result<()> {
        match ev.kind {
            EV_ABS => match ev.code {
                ABS_MT_SLOT => self.slot = ev.value.clamp(0, 1) as usize,
                ABS_MT_TRACKING_ID => {
                    if ev.value < 0 {
                        self.native[self.slot] = Slot::default();
                    } else {
                        self.native[self.slot] = Slot {
                            active: true,
                            positioned: false,
                            id: ev.value,
                            x: 0,
                            y: 0,
                        };
                    }
                }
                ABS_MT_POSITION_X => {
                    self.native[self.slot].x = ev.value;
                    self.native[self.slot].positioned = true;
                }
                ABS_MT_POSITION_Y => {
                    self.native[self.slot].y = ev.value;
                    self.native[self.slot].positioned = true;
                }
                _ => {}
            },
            EV_SYN if ev.code == SYN_REPORT => self.flush(touch)?,
            _ => {}
        }
        Ok(())
    }

    fn flush(&mut self, touch: &mut Option<UinputDevice>) -> std::io::Result<()> {
        let Some(t) = touch.as_mut() else {
            return Ok(());
        };

        let active = self.native.iter().filter(|s| s.active).count();
        let mut dirty = false;

        if active >= 2 {
            let ready = self
                .native
                .iter()
                .filter(|s| s.active)
                .all(|s| s.positioned);

            if !self.emitting && !self.waiting && ready {
                self.anchor = self.native;
                self.waiting = true;
            }

            if !self.emitting && self.waiting && ready {
                let moved = self
                    .native
                    .iter()
                    .zip(self.anchor.iter())
                    .filter(|(n, _)| n.active)
                    .any(|(n, a)| {
                        (n.x - a.x).abs() >= GESTURE_START_DISTANCE
                            || (n.y - a.y).abs() >= GESTURE_START_DISTANCE
                    });

                if moved {
                    t.push(EV_KEY, BTN_TOUCH, 1);
                    t.push(EV_KEY, BTN_TOOL_DOUBLETAP, 1);
                    self.emitted = [Slot::default(); 2];
                    for i in 0..2 {
                        let a = self.anchor[i];
                        if a.active {
                            t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                            t.push(EV_ABS, ABS_MT_TRACKING_ID, a.id);
                            t.push(EV_ABS, ABS_MT_POSITION_X, a.x);
                            t.push(EV_ABS, ABS_MT_POSITION_Y, a.y);
                            self.emitted[i] = a;
                        }
                    }
                    t.push(EV_SYN, SYN_REPORT, 0);
                    t.flush()?;

                    for i in 0..2 {
                        let n = self.native[i];
                        if n.active {
                            t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                            t.push(EV_ABS, ABS_MT_POSITION_X, n.x);
                            t.push(EV_ABS, ABS_MT_POSITION_Y, n.y);
                        }
                    }
                    self.emitted = self.native;
                    self.waiting = false;
                    self.emitting = true;
                    dirty = true;
                }
            } else if self.emitting {
                for i in 0..2 {
                    let n = self.native[i];
                    let e = self.emitted[i];
                    if n.active && !e.active {
                        t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                        t.push(EV_ABS, ABS_MT_TRACKING_ID, n.id);
                        t.push(EV_ABS, ABS_MT_POSITION_X, n.x);
                        t.push(EV_ABS, ABS_MT_POSITION_Y, n.y);
                        self.emitted[i] = n;
                        dirty = true;
                    } else if n.active && e.active {
                        if n.id != e.id {
                            t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                            t.push(EV_ABS, ABS_MT_TRACKING_ID, -1);
                            t.push(EV_ABS, ABS_MT_TRACKING_ID, n.id);
                            t.push(EV_ABS, ABS_MT_POSITION_X, n.x);
                            t.push(EV_ABS, ABS_MT_POSITION_Y, n.y);
                            self.emitted[i] = n;
                            dirty = true;
                        } else if n.x != e.x || n.y != e.y {
                            t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                            t.push(EV_ABS, ABS_MT_POSITION_X, n.x);
                            t.push(EV_ABS, ABS_MT_POSITION_Y, n.y);
                            self.emitted[i] = n;
                            dirty = true;
                        }
                    } else if !n.active && e.active {
                        t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                        t.push(EV_ABS, ABS_MT_TRACKING_ID, -1);
                        self.emitted[i] = Slot::default();
                        dirty = true;
                    }
                }
            }
        } else {
            self.waiting = false;
            if self.emitting {
                for i in 0..2 {
                    if self.emitted[i].active {
                        t.push(EV_ABS, ABS_MT_SLOT, i as i32);
                        t.push(EV_ABS, ABS_MT_TRACKING_ID, -1);
                    }
                }
                t.push(EV_KEY, BTN_TOOL_DOUBLETAP, 0);
                t.push(EV_KEY, BTN_TOUCH, 0);
                self.emitted = [Slot::default(); 2];
                self.emitting = false;
                dirty = true;
            }
        }

        if dirty {
            t.push(EV_SYN, SYN_REPORT, 0);
            t.flush()?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct TouchpadModeHandle(Arc<AtomicU8>);

impl TouchpadModeHandle {
    pub(crate) fn new(mode: TouchpadMode) -> Self {
        Self(Arc::new(AtomicU8::new(Self::encode(mode))))
    }

    pub(crate) fn set(&self, mode: TouchpadMode) {
        self.0.store(Self::encode(mode), Ordering::Relaxed);
    }

    pub(crate) fn get(&self) -> TouchpadMode {
        Self::decode(self.0.load(Ordering::Relaxed))
    }

    // Legacy Mouse/PassThrough modes are normalized so the filter always owns the node.
    fn encode(mode: TouchpadMode) -> u8 {
        match mode {
            TouchpadMode::GesturesOnly => 2,
            TouchpadMode::Disabled | TouchpadMode::Mouse | TouchpadMode::PassThrough => 0,
        }
    }

    fn decode(value: u8) -> TouchpadMode {
        match value {
            2 => TouchpadMode::GesturesOnly,
            _ => TouchpadMode::Disabled,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TouchpadOwnedHandle(Arc<AtomicBool>);

impl TouchpadOwnedHandle {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub(crate) fn set(&self, owned: bool) {
        self.0.store(owned, Ordering::Relaxed);
    }

    pub(crate) fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub(crate) struct TouchpadFilter {
    mode: TouchpadModeHandle,
    owned: TouchpadOwnedHandle,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TouchpadFilter {
    pub(crate) fn start(mode: TouchpadMode) -> Self {
        Self::start_with_handle(TouchpadModeHandle::new(mode), TouchpadOwnedHandle::new())
    }

    pub(crate) fn start_with_handle(mode: TouchpadModeHandle, owned: TouchpadOwnedHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_mode = mode.clone();
        let thread_owned = owned.clone();
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("ds4u-touchpad".into())
            .spawn(move || run(thread_mode, thread_owned, thread_stop))
            .ok();

        Self {
            mode,
            owned,
            stop,
            thread,
        }
    }

    pub(crate) fn set_mode(&self, mode: TouchpadMode) {
        self.mode.set(mode);
    }
}

impl Drop for TouchpadFilter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        self.owned.set(false);
    }
}

struct Active {
    file: File,
    mode: TouchpadMode,
    filter: FilterState,
    touch: Option<UinputDevice>,
}

impl Active {
    fn open(mode: TouchpadMode) -> Result<Self> {
        let (path, vendor, product) =
            find_touchpad().context("DualSense touchpad device not found")?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;

        let fd = file.as_raw_fd();
        if unsafe { libc::ioctl(fd, eviocgrab() as _, 1i32) } != 0 {
            let err = std::io::Error::last_os_error();
            bail!(
                "could not grab {}: {} ({})",
                path.display(),
                errno_name(&err),
                err
            );
        }
        println!("{TAG} grabbed {}", path.display());

        let touch = if mode == TouchpadMode::GesturesOnly {
            Some(create_touchpad(vendor, product)?)
        } else {
            None
        };

        Ok(Self {
            file,
            mode,
            filter: FilterState::default(),
            touch,
        })
    }

    fn run(&mut self, mode: &TouchpadModeHandle, stop: &AtomicBool) -> std::io::Result<()> {
        let fd = self.file.as_raw_fd();
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let mut events: [InputEvent; 64] = unsafe { zeroed() };

        loop {
            if stop.load(Ordering::Relaxed) || mode.get() != self.mode {
                return Ok(());
            }

            let ready = unsafe { libc::poll(&mut pfd, 1, 100) };
            if ready < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }
            if ready == 0 {
                continue;
            }
            if pfd.revents & libc::POLLIN == 0 {
                if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    return Ok(());
                }
                continue;
            }

            let n = unsafe {
                libc::read(
                    fd,
                    events.as_mut_ptr() as *mut libc::c_void,
                    size_of::<[InputEvent; 64]>(),
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) {
                    continue;
                }
                return Err(err);
            }
            if n == 0 {
                return Ok(());
            }

            let count = n as usize / size_of::<InputEvent>();
            for ev in &events[..count] {
                self.filter.feed(*ev, &mut self.touch)?;
            }
        }
    }
}

fn find_touchpad() -> Option<(PathBuf, u16, u16)> {
    let mut enumerator = udev::Enumerator::new().ok()?;
    enumerator.match_subsystem("input").ok()?;
    let devices = enumerator.scan_devices().ok()?;

    for dev in devices {
        let Some(devnode) = dev.devnode() else {
            continue;
        };
        let Some(name) = devnode.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }
        if dev.syspath().to_string_lossy().contains("/virtual/") {
            continue;
        }
        if dev
            .property_value("ID_INPUT_TOUCHPAD")
            .and_then(|v| v.to_str())
            != Some("1")
        {
            continue;
        }

        let Some(vendor) = dev
            .property_value("ID_VENDOR_ID")
            .and_then(|v| v.to_str())
            .and_then(|v| u16::from_str_radix(v.trim(), 16).ok())
        else {
            continue;
        };
        let Some(product) = dev
            .property_value("ID_MODEL_ID")
            .and_then(|v| v.to_str())
            .and_then(|v| u16::from_str_radix(v.trim(), 16).ok())
        else {
            continue;
        };

        if vendor == DS_VID && matches!(product, DS_PID | DSE_PID) {
            return Some((devnode.to_path_buf(), vendor, product));
        }
    }
    None
}

fn run(mode: TouchpadModeHandle, owned: TouchpadOwnedHandle, stop: Arc<AtomicBool>) {
    struct ClearOwned<'a>(&'a TouchpadOwnedHandle);

    impl Drop for ClearOwned<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }

    let _clear_owned = ClearOwned(&owned);
    let mut active: Option<Active> = None;
    let mut reported_unavailable = false;

    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }

        let desired = mode.get();

        if active.as_ref().is_some_and(|a| a.mode != desired) {
            active = None;
            owned.set(false);
        }

        if active.is_none() {
            match Active::open(desired) {
                Ok(a) => {
                    println!("{TAG} mode {:?} active", desired);
                    reported_unavailable = false;
                    owned.set(true);
                    active = Some(a);
                }
                Err(e) => {
                    owned.set(false);
                    if !reported_unavailable {
                        eprintln!("{TAG} {e}");
                        reported_unavailable = true;
                    }
                    sleep(Duration::from_millis(500));
                    continue;
                }
            }
        }

        if let Some(mut a) = active.take() {
            let result = a.run(&mode, &stop);
            owned.set(false);
            if let Err(e) = result {
                eprintln!("{TAG} stopped: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_modes_normalize_to_disabled() {
        for mode in [TouchpadMode::Mouse, TouchpadMode::PassThrough] {
            let handle = TouchpadModeHandle::new(mode);
            assert_eq!(handle.get(), TouchpadMode::Disabled);

            handle.set(TouchpadMode::GesturesOnly);
            handle.set(mode);
            assert_eq!(handle.get(), TouchpadMode::Disabled);
        }
    }

    #[test]
    fn owned_modes_round_trip() {
        for mode in [TouchpadMode::Disabled, TouchpadMode::GesturesOnly] {
            let handle = TouchpadModeHandle::new(mode);
            assert_eq!(handle.get(), mode);
        }
    }

    #[test]
    fn owned_handle_tracks_state() {
        let owned = TouchpadOwnedHandle::new();
        assert!(!owned.get());
        owned.set(true);
        assert!(owned.get());
        owned.set(false);
        assert!(!owned.get());
    }
}
