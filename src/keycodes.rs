//! Canonical key names and their Linux evdev key codes.

use std::sync::LazyLock;

/// Canonical keyboard keys exposed by the mapping editor.
///
/// The table intentionally covers codes 1..=32 (Esc..D): Linux tags a uinput
/// device as `ID_INPUT_KEYBOARD` only when that block is registered.
pub const KEY_TABLE: &[(&str, u16)] = &[
    ("Esc", 1),
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    ("Minus", 12),
    ("Equal", 13),
    ("Backspace", 14),
    ("Tab", 15),
    ("Q", 16),
    ("W", 17),
    ("E", 18),
    ("R", 19),
    ("T", 20),
    ("Y", 21),
    ("U", 22),
    ("I", 23),
    ("O", 24),
    ("P", 25),
    ("LeftBracket", 26),
    ("RightBracket", 27),
    ("Enter", 28),
    ("LeftCtrl", 29),
    ("A", 30),
    ("S", 31),
    ("D", 32),
    ("F", 33),
    ("G", 34),
    ("H", 35),
    ("J", 36),
    ("K", 37),
    ("L", 38),
    ("Semicolon", 39),
    ("Apostrophe", 40),
    ("Grave", 41),
    ("LeftShift", 42),
    ("Backslash", 43),
    ("Z", 44),
    ("X", 45),
    ("C", 46),
    ("V", 47),
    ("B", 48),
    ("N", 49),
    ("M", 50),
    ("Comma", 51),
    ("Period", 52),
    ("Slash", 53),
    ("RightShift", 54),
    ("KeypadAsterisk", 55),
    ("LeftAlt", 56),
    ("Space", 57),
    ("CapsLock", 58),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("NumLock", 69),
    ("ScrollLock", 70),
    ("Keypad7", 71),
    ("Keypad8", 72),
    ("Keypad9", 73),
    ("KeypadMinus", 74),
    ("Keypad4", 75),
    ("Keypad5", 76),
    ("Keypad6", 77),
    ("KeypadPlus", 78),
    ("Keypad1", 79),
    ("Keypad2", 80),
    ("Keypad3", 81),
    ("Keypad0", 82),
    ("KeypadPeriod", 83),
    ("F11", 87),
    ("F12", 88),
    ("RightCtrl", 97),
    ("KeypadSlash", 98),
    ("PrintScreen", 99),
    ("RightAlt", 100),
    ("Home", 102),
    ("Up", 103),
    ("PageUp", 104),
    ("Left", 105),
    ("Right", 106),
    ("End", 107),
    ("Down", 108),
    ("PageDown", 109),
    ("Insert", 110),
    ("Delete", 111),
    ("Mute", 113),
    ("VolumeDown", 114),
    ("VolumeUp", 115),
    ("Pause", 119),
    ("LeftMeta", 125),
    ("RightMeta", 126),
    ("NextSong", 163),
    ("PlayPause", 164),
    ("PreviousSong", 165),
    ("Stop", 166),
];

static ALL_CODES: LazyLock<Vec<u16>> = LazyLock::new(|| {
    let mut codes: Vec<u16> = KEY_TABLE.iter().map(|(_, code)| *code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
});

pub fn keycode(name: &str) -> Option<u16> {
    KEY_TABLE
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, code)| *code)
}

#[allow(dead_code)] // Lookup API for stored bindings; the editor only needs keycode() today.
pub fn key_name(code: u16) -> Option<&'static str> {
    KEY_TABLE
        .iter()
        .find(|(_, candidate)| *candidate == code)
        .map(|(name, _)| *name)
}

/// Deduplicated, sorted list of every registered key code.
pub fn all_codes() -> &'static [u16] {
    ALL_CODES.as_slice()
}

/// Maps an egui key to a canonical name.
///
/// egui does not expose modifier keys or distinct keypad scancodes, so those
/// cannot be captured here. Physical keypad digits collapse onto the main row.
pub fn from_egui(key: egui::Key) -> Option<&'static str> {
    use egui::Key;

    Some(match key {
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",

        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",

        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",

        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",

        Key::Escape => "Esc",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Enter => "Enter",
        Key::Space => "Space",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",

        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Semicolon => "Semicolon",
        Key::Quote => "Apostrophe",
        Key::Backslash => "Backslash",
        Key::Slash => "Slash",
        Key::OpenBracket => "LeftBracket",
        Key::CloseBracket => "RightBracket",
        Key::Backtick => "Grave",
        Key::Minus => "Minus",
        Key::Equals => "Equal",
        Key::Plus => "KeypadPlus",

        // Shifted logical variants; the physical key normally wins in capture.
        Key::Colon => "Semicolon",
        Key::Pipe => "Backslash",
        Key::Questionmark => "Slash",
        Key::Exclamationmark => "1",
        Key::OpenCurlyBracket => "LeftBracket",
        Key::CloseCurlyBracket => "RightBracket",

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_round_trip() {
        assert_eq!(keycode("A"), Some(30));
        assert_eq!(key_name(30), Some("A"));
    }

    #[test]
    fn named_keys() {
        assert_eq!(keycode("Space"), Some(57));
        assert_eq!(keycode("LeftShift"), Some(42));
        assert_eq!(keycode("F12"), Some(88));
        assert_eq!(key_name(57), Some("Space"));
    }

    #[test]
    fn all_codes_covers_keyboard_block() {
        let codes = all_codes();
        for code in 1..=32 {
            assert!(codes.contains(&code), "missing key code {code}");
        }
    }

    #[test]
    fn all_codes_is_sorted_and_deduplicated() {
        let codes = all_codes();
        assert!(codes.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn egui_letter_maps() {
        assert_eq!(from_egui(egui::Key::A), Some("A"));
        assert_eq!(from_egui(egui::Key::Escape), Some("Esc"));
    }
}
