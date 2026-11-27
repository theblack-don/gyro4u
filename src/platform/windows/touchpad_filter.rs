use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use crate::common::TouchpadMode;

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
}

impl TouchpadFilter {
    pub(crate) fn start(mode: TouchpadMode) -> Self {
        Self::start_with_handle(TouchpadModeHandle::new(mode), TouchpadOwnedHandle::new())
    }

    pub(crate) fn start_with_handle(mode: TouchpadModeHandle, owned: TouchpadOwnedHandle) -> Self {
        Self { mode, owned }
    }

    pub(crate) fn set_mode(&self, mode: TouchpadMode) {
        self.mode.set(mode);
    }
}

impl Drop for TouchpadFilter {
    fn drop(&mut self) {
        self.owned.set(false);
    }
}
