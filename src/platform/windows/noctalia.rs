use std::time::SystemTime;

use crate::theme::Theme;

pub const ID: &str = "noctalia";

pub fn theme() -> Option<Theme> {
    None
}

pub fn source_mtime() -> Option<SystemTime> {
    None
}
