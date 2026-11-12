use std::{
    fs::{File, OpenOptions},
    io::Write,
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

const TAG: &str = "[ds4u touchpad]";

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;

const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOUCH: u16 = 0x14a;
const BTN_TOOL_DOUBLETAP: u16 = 0x14d;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;

const INPUT_PROP_POINTER: u16 = 0x00;
const INPUT_PROP_BUTTONPAD: u16 = 0x02;

const BUS_USB: u16 = 0x03;
const ABS_CNT: usize = 64;
const UINPUT_MAX_NAME_SIZE: usize = 80;

const UINPUT_IOCTL_BASE: u64 = b'U' as u64;
const IOC_NONE: u64 = 0;
const IOC_WRITE: u64 = 1;

fn ioc(dir: u64, nr: u64, size: u64) -> u64 {
    (dir << 30) | (UINPUT_IOCTL_BASE << 8) | nr | (size << 16)
}

fn ui_set_evbit() -> u64 {
    ioc(IOC_WRITE, 100, size_of::<i32>() as u64)
}
fn ui_set_keybit() -> u64 {
    ioc(IOC_WRITE, 101, size_of::<i32>() as u64)
}
fn ui_set_absbit() -> u64 {
    ioc(IOC_WRITE, 103, size_of::<i32>() as u64)
}
fn ui_set_propbit() -> u64 {
    ioc(IOC_WRITE, 110, size_of::<i32>() as u64)
}
fn ui_dev_create() -> u64 {
    ioc(IOC_NONE, 1, 0)
}
fn ui_dev_destroy() -> u64 {
    ioc(IOC_NONE, 2, 0)
}

fn iow(ty: u8, nr: u8, size: u32) -> u64 {
    (1u64 << 30) | ((size as u64) << 16) | ((ty as u64) << 8) | (nr as u64)
}

fn eviocgrab() -> u64 {
    iow(b'E', 0x90, size_of::<i32>() as u32)
}

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputUserDev {
    name: [u8; UINPUT_MAX_NAME_SIZE],
    id: InputId,
    ff_effects_max: u32,
    absmax: [i32; ABS_CNT],
    absmin: [i32; ABS_CNT],
    absfuzz: [i32; ABS_CNT],
    absflat: [i32; ABS_CNT],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

struct UinputDevice {
    file: File,
    out: Vec<u8>,
}

impl UinputDevice {
    fn create(
        name: &[u8],
        vendor: u16,
        product: u16,
        keys: &[u16],
        axes: &[(u16, i32, i32)],
        props: &[u16],
    ) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .context("opening /dev/uinput")?;
        let fd = file.as_raw_fd();

        unsafe {
            if libc::ioctl(fd, ui_set_evbit() as _, EV_KEY as i32) < 0 {
                bail!("UI_SET_EVBIT(EV_KEY) failed");
            }
            if !axes.is_empty() && libc::ioctl(fd, ui_set_evbit() as _, EV_ABS as i32) < 0 {
                bail!("UI_SET_EVBIT(EV_ABS) failed");
            }
            for key in keys {
                if libc::ioctl(fd, ui_set_keybit() as _, *key as i32) < 0 {
                    bail!("UI_SET_KEYBIT failed");
                }
            }
            for (axis, _, _) in axes {
                if libc::ioctl(fd, ui_set_absbit() as _, *axis as i32) < 0 {
                    bail!("UI_SET_ABSBIT failed");
                }
            }
            for prop in props {
                if libc::ioctl(fd, ui_set_propbit() as _, *prop as i32) < 0 {
                    bail!("UI_SET_PROPBIT failed");
                }
            }
        }

        let mut dev: UinputUserDev = unsafe { zeroed() };
        dev.name[..name.len()].copy_from_slice(name);
        dev.id = InputId {
            bustype: BUS_USB,
            vendor,
            product,
            version: 0x8111,
        };
        for (axis, min, max) in axes {
            dev.absmin[*axis as usize] = *min;
            dev.absmax[*axis as usize] = *max;
        }

        let bytes = unsafe {
            std::slice::from_raw_parts(&dev as *const _ as *const u8, size_of::<UinputUserDev>())
        };
        (&file)
            .write_all(bytes)
            .context("writing uinput_user_dev")?;

        if unsafe { libc::ioctl(fd, ui_dev_create() as _) } < 0 {
            bail!("UI_DEV_CREATE failed");
        }

        Ok(Self {
            file,
            out: Vec::with_capacity(24 * 16),
        })
    }

    fn touchpad(vendor: u16, product: u16) -> Result<Self> {
        Self::create(
            b"DS4U DualSense Touchpad\0",
            vendor,
            product,
            &[BTN_TOOL_FINGER, BTN_TOUCH, BTN_TOOL_DOUBLETAP],
            &[
                (ABS_X, 0, 1919),
                (ABS_Y, 0, 1079),
                (ABS_MT_SLOT, 0, 1),
                (ABS_MT_POSITION_X, 0, 1919),
                (ABS_MT_POSITION_Y, 0, 1079),
                (ABS_MT_TRACKING_ID, 0, 65535),
            ],
            &[INPUT_PROP_POINTER, INPUT_PROP_BUTTONPAD],
        )
    }

    fn push(&mut self, kind: u16, code: u16, value: i32) {
        let ev = InputEvent {
            tv_sec: 0,
            tv_usec: 0,
            kind,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(&ev as *const _ as *const u8, size_of::<InputEvent>())
        };
        self.out.extend_from_slice(bytes);
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.out.is_empty() {
            (&self.file).write_all(&self.out)?;
            self.out.clear();
        }
        Ok(())
    }
}

impl Drop for UinputDevice {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        unsafe {
            libc::ioctl(fd, ui_dev_destroy() as _);
        }
    }
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

    fn get(&self) -> TouchpadMode {
        Self::decode(self.0.load(Ordering::Relaxed))
    }

    fn encode(mode: TouchpadMode) -> u8 {
        match mode {
            TouchpadMode::Disabled => 0,
            TouchpadMode::Mouse => 1,
            TouchpadMode::GesturesOnly => 2,
            TouchpadMode::PassThrough => 3,
        }
    }

    fn decode(value: u8) -> TouchpadMode {
        match value {
            0 => TouchpadMode::Disabled,
            2 => TouchpadMode::GesturesOnly,
            3 => TouchpadMode::PassThrough,
            _ => TouchpadMode::Mouse,
        }
    }
}

pub(crate) struct TouchpadFilter {
    mode: TouchpadModeHandle,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TouchpadFilter {
    pub(crate) fn start(mode: TouchpadMode) -> Self {
        Self::start_with_handle(TouchpadModeHandle::new(mode))
    }

    pub(crate) fn start_with_handle(mode: TouchpadModeHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_mode = mode.clone();
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("ds4u-touchpad".into())
            .spawn(move || run(thread_mode, thread_stop))
            .ok();

        Self { mode, stop, thread }
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

        let touch = if mode == TouchpadMode::GesturesOnly {
            Some(UinputDevice::touchpad(vendor, product)?)
        } else {
            None
        };

        let fd = file.as_raw_fd();
        if unsafe { libc::ioctl(fd, eviocgrab() as _, 1i32) } != 0 {
            bail!("could not grab {}", path.display());
        }

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

fn run(mode: TouchpadModeHandle, stop: Arc<AtomicBool>) {
    let mut active: Option<Active> = None;
    let mut reported_unavailable = false;

    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }

        let desired = mode.get();
        if matches!(desired, TouchpadMode::Mouse | TouchpadMode::PassThrough) {
            active = None;
            reported_unavailable = false;
            sleep(Duration::from_millis(200));
            continue;
        }

        if active.as_ref().is_some_and(|a| a.mode != desired) {
            active = None;
        }

        if active.is_none() {
            match Active::open(desired) {
                Ok(a) => {
                    println!("{TAG} mode {:?} active", desired);
                    reported_unavailable = false;
                    active = Some(a);
                }
                Err(e) => {
                    if !reported_unavailable {
                        eprintln!("{TAG} {e}");
                        reported_unavailable = true;
                    }
                    sleep(Duration::from_millis(500));
                    continue;
                }
            }
        }

        if let Some(mut a) = active.take()
            && let Err(e) = a.run(&mode, &stop)
        {
            eprintln!("{TAG} stopped: {e}");
        }
    }
}
