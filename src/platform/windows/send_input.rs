use anyhow::{Result, anyhow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput};

pub(super) fn send(inputs: &[INPUT]) -> Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }

    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        return Err(anyhow!(
            "SendInput inserted {sent} of {} events: {}",
            inputs.len(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
