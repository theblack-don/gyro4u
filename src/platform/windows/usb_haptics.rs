use crate::common::HapticPattern;

pub const HAPTIC_CARRIER_HZ: f32 = 160.0;

pub fn find_dualsense_card() -> Option<u32> {
    None
}

pub struct UsbHapticStream;

impl UsbHapticStream {
    pub fn new() -> Self {
        Self
    }

    pub fn is_active(&self) -> bool {
        false
    }

    pub fn set_params(&self, _pattern: HapticPattern, _strength: u8, _speed: f32) {}

    pub fn start(&mut self, _card: u32, _pattern: HapticPattern, _strength: u8, _speed: f32) {}

    pub fn stop(&mut self) {}
}

impl Default for UsbHapticStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for UsbHapticStream {
    fn drop(&mut self) {
        self.stop();
    }
}
