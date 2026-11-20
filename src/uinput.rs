// Shared uinput plumbing. Not all of this is used yet; the EV_REL/mouse path
// is here so the next chunks don't have to duplicate it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::mem::{size_of, zeroed};
use std::os::unix::io::AsRawFd;

use anyhow::{Context, Result, bail};

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;

pub const SYN_REPORT: u16 = 0x00;

pub const INPUT_PROP_POINTER: u16 = 0x00;

pub const BUS_USB: u16 = 0x03;

pub const UINPUT_MAX_NAME_SIZE: usize = 80;
pub const ABS_CNT: usize = 64;

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
fn ui_set_relbit() -> u64 {
    ioc(IOC_WRITE, 102, size_of::<i32>() as u64)
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
pub struct InputEvent {
    pub(crate) tv_sec: i64,
    pub(crate) tv_usec: i64,
    pub(crate) kind: u16,
    pub(crate) code: u16,
    pub(crate) value: i32,
}

pub struct UinputSpec<'a> {
    pub name: &'a str,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub keys: &'a [u16],
    /// `(code, min, max, flat)` for each ABS axis.
    pub axes: &'a [(u16, i32, i32, i32)],
    pub rels: &'a [u16],
    pub props: &'a [u16],
}

pub struct UinputDevice {
    file: File,
    last: HashMap<(u16, u16), i32>,
    out: Vec<u8>,
}

impl UinputDevice {
    pub fn create(spec: UinputSpec) -> Result<Self> {
        let file = OpenOptions::new().write(true).open("/dev/uinput").context(
            "opening /dev/uinput (is the uinput kernel module loaded, \
             and do you have write permission on it?)",
        )?;
        let fd = file.as_raw_fd();

        unsafe {
            if !spec.keys.is_empty() && libc::ioctl(fd, ui_set_evbit() as _, EV_KEY as i32) < 0 {
                bail!("UI_SET_EVBIT(EV_KEY) failed");
            }
            if !spec.axes.is_empty() && libc::ioctl(fd, ui_set_evbit() as _, EV_ABS as i32) < 0 {
                bail!("UI_SET_EVBIT(EV_ABS) failed");
            }
            if !spec.rels.is_empty() && libc::ioctl(fd, ui_set_evbit() as _, EV_REL as i32) < 0 {
                bail!("UI_SET_EVBIT(EV_REL) failed");
            }
            for key in spec.keys {
                if libc::ioctl(fd, ui_set_keybit() as _, *key as i32) < 0 {
                    bail!("UI_SET_KEYBIT failed");
                }
            }
            for (axis, _, _, _) in spec.axes {
                if libc::ioctl(fd, ui_set_absbit() as _, *axis as i32) < 0 {
                    bail!("UI_SET_ABSBIT failed");
                }
            }
            for rel in spec.rels {
                if libc::ioctl(fd, ui_set_relbit() as _, *rel as i32) < 0 {
                    bail!("UI_SET_RELBIT failed");
                }
            }
            for prop in spec.props {
                if libc::ioctl(fd, ui_set_propbit() as _, *prop as i32) < 0 {
                    bail!("UI_SET_PROPBIT failed");
                }
            }
        }

        let mut dev: UinputUserDev = unsafe { zeroed() };
        let name = spec.name.as_bytes();
        let len = name.len().min(UINPUT_MAX_NAME_SIZE - 1);
        dev.name[..len].copy_from_slice(&name[..len]);
        dev.id = InputId {
            bustype: BUS_USB,
            vendor: spec.vendor,
            product: spec.product,
            version: spec.version,
        };
        for (axis, min, max, flat) in spec.axes {
            dev.absmin[*axis as usize] = *min;
            dev.absmax[*axis as usize] = *max;
            dev.absflat[*axis as usize] = *flat;
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
            last: HashMap::new(),
            out: Vec::with_capacity(24 * 20),
        })
    }

    pub fn push(&mut self, kind: u16, code: u16, value: i32) {
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

    pub fn emit(&mut self, kind: u16, code: u16, value: i32) -> bool {
        if self.last.insert((kind, code), value) != Some(value) {
            self.push(kind, code, value);
            true
        } else {
            false
        }
    }

    pub fn sync(&mut self) -> io::Result<()> {
        self.push(EV_SYN, SYN_REPORT, 0);
        self.flush()
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if !self.out.is_empty() {
            (&self.file).write_all(&self.out)?;
            self.out.clear();
        }
        Ok(())
    }

    pub fn has_pending(&self) -> bool {
        !self.out.is_empty()
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
