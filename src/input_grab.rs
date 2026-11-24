use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;

use crate::apex5::{FLYDIGI_VID, is_supported_pid};
use crate::common::{DS_PID, DS_VID, DSE_PID};
use crate::eightbitdo::{EIGHTBITDO_PRO3_PID, EIGHTBITDO_VID};
use crate::gamesir::{GAMESIR_VID, TARANTULA_PRO_PID};
use crate::steam_controller::{
    STEAM_PID_BLE, STEAM_PID_PUCK, STEAM_PID_RECEIVER, STEAM_PID_WIRED, STEAM_VID,
};

const TAG: &str = "[gyro4u daemon]";

fn iow(ty: u8, nr: u8, size: u32) -> u64 {
    (1u64 << 30) | ((size as u64) << 16) | ((ty as u64) << 8) | (nr as u64)
}

fn eviocgrab() -> u64 {
    iow(b'E', 0x90, size_of::<i32>() as u32)
}

pub struct InputGrab {
    handles: Vec<File>,
}

impl InputGrab {
    pub fn acquire() -> Self {
        let mut handles = Vec::new();

        let mut en = match udev::Enumerator::new() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("{TAG} input grab: udev enumerator failed: {e}");
                return Self { handles };
            }
        };
        if en.match_subsystem("input").is_err() {
            return Self { handles };
        }

        let devices = match en.scan_devices() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{TAG} input grab: scan failed: {e}");
                return Self { handles };
            }
        };

        for dev in devices {
            let Some(devnode) = dev.devnode() else {
                continue;
            };
            let Some(name) = devnode.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if !name.starts_with("event") {
                continue;
            }

            if dev.syspath().to_string_lossy().contains("/virtual/") {
                continue;
            }

            // The touchpad is grabbed separately by touchpad_filter so its
            // mode can be applied without hiding the whole controller.
            if dev
                .property_value("ID_INPUT_TOUCHPAD")
                .and_then(|v| v.to_str())
                == Some("1")
            {
                continue;
            }

            let vendor = dev
                .property_value("ID_VENDOR_ID")
                .and_then(|v| v.to_str())
                .and_then(|v| u16::from_str_radix(v.trim(), 16).ok());
            let product = dev
                .property_value("ID_MODEL_ID")
                .and_then(|v| v.to_str())
                .and_then(|v| u16::from_str_radix(v.trim(), 16).ok());

            let is_gamesir = vendor == Some(GAMESIR_VID) && product == Some(TARANTULA_PRO_PID);

            // Only the gamepad node for other controllers; motion sensors, the
            // touchpad and the headset jack keep their normal userspace access.
            // GameSir's dongle also exposes keyboard, mouse and auxiliary event
            // nodes, so grab all of them and stop its onboard mappings
            // (Esc/Shift/middle click) from leaking to the OS.
            if !is_gamesir
                && dev
                    .property_value("ID_INPUT_JOYSTICK")
                    .and_then(|v| v.to_str())
                    != Some("1")
            {
                continue;
            }

            let is_dualsense =
                vendor == Some(DS_VID) && matches!(product, Some(DS_PID) | Some(DSE_PID));
            let is_steam = vendor == Some(STEAM_VID)
                && matches!(
                    product,
                    Some(STEAM_PID_WIRED)
                        | Some(STEAM_PID_BLE)
                        | Some(STEAM_PID_PUCK)
                        | Some(STEAM_PID_RECEIVER)
                );
            let is_eightbitdo =
                vendor == Some(EIGHTBITDO_VID) && matches!(product, Some(EIGHTBITDO_PRO3_PID));
            let is_flydigi = vendor == Some(FLYDIGI_VID) && product.is_some_and(is_supported_pid);
            if !is_dualsense && !is_steam && !is_eightbitdo && !is_flydigi && !is_gamesir {
                continue;
            }

            match OpenOptions::new().read(true).write(true).open(devnode) {
                Ok(f) => {
                    let fd = f.as_raw_fd();
                    if unsafe { libc::ioctl(fd, eviocgrab() as _, 1i32) } == 0 {
                        println!("{TAG} grabbed {} (native input hidden)", devnode.display());
                        handles.push(f);
                    } else {
                        eprintln!(
                            "{TAG} could not grab {} (already grabbed by something else?)",
                            devnode.display()
                        );
                    }
                }
                Err(e) => eprintln!("{TAG} could not open {}: {}", devnode.display(), e),
            }
        }

        Self { handles }
    }
}

impl Drop for InputGrab {
    fn drop(&mut self) {
        for f in &self.handles {
            let fd = f.as_raw_fd();
            unsafe {
                libc::ioctl(fd, eviocgrab() as _, 0i32);
            }
        }
    }
}
