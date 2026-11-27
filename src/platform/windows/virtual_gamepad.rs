use anyhow::{Result, bail};

use crate::inputs::{ControllerKind, ControllerState};

pub struct VirtualGamepad;

impl VirtualGamepad {
    pub fn new(_kind: ControllerKind, _vendor: u16, _product: u16) -> Result<Self> {
        bail!("virtual gamepad output is not supported on Windows yet")
    }

    pub fn update(&mut self, _state: &ControllerState) -> Result<()> {
        Ok(())
    }

    pub fn reset(&mut self) {}
}
