#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyStroke {
    pub vk: u16,
    pub scan: u16,
    pub extended: bool,
    pub scancode: bool,
}

const fn scan(scan: u16) -> KeyStroke {
    KeyStroke {
        vk: 0,
        scan,
        extended: false,
        scancode: true,
    }
}

const fn extended_scan(scan: u16) -> KeyStroke {
    KeyStroke {
        vk: 0,
        scan,
        extended: true,
        scancode: true,
    }
}

const fn virtual_key(vk: u16) -> KeyStroke {
    KeyStroke {
        vk,
        scan: 0,
        extended: false,
        scancode: false,
    }
}

pub(crate) fn key_stroke(code: u16) -> Option<KeyStroke> {
    Some(match code {
        1 => scan(0x01),            // Esc
        2 => scan(0x02),            // 1
        3 => scan(0x03),            // 2
        4 => scan(0x04),            // 3
        5 => scan(0x05),            // 4
        6 => scan(0x06),            // 5
        7 => scan(0x07),            // 6
        8 => scan(0x08),            // 7
        9 => scan(0x09),            // 8
        10 => scan(0x0a),           // 9
        11 => scan(0x0b),           // 0
        12 => scan(0x0c),           // Minus
        13 => scan(0x0d),           // Equal
        14 => scan(0x0e),           // Backspace
        15 => scan(0x0f),           // Tab
        16 => scan(0x10),           // Q
        17 => scan(0x11),           // W
        18 => scan(0x12),           // E
        19 => scan(0x13),           // R
        20 => scan(0x14),           // T
        21 => scan(0x15),           // Y
        22 => scan(0x16),           // U
        23 => scan(0x17),           // I
        24 => scan(0x18),           // O
        25 => scan(0x19),           // P
        26 => scan(0x1a),           // LeftBracket
        27 => scan(0x1b),           // RightBracket
        28 => scan(0x1c),           // Enter
        29 => scan(0x1d),           // LeftCtrl
        30 => scan(0x1e),           // A
        31 => scan(0x1f),           // S
        32 => scan(0x20),           // D
        33 => scan(0x21),           // F
        34 => scan(0x22),           // G
        35 => scan(0x23),           // H
        36 => scan(0x24),           // J
        37 => scan(0x25),           // K
        38 => scan(0x26),           // L
        39 => scan(0x27),           // Semicolon
        40 => scan(0x28),           // Apostrophe
        41 => scan(0x29),           // Grave
        42 => scan(0x2a),           // LeftShift
        43 => scan(0x2b),           // Backslash
        44 => scan(0x2c),           // Z
        45 => scan(0x2d),           // X
        46 => scan(0x2e),           // C
        47 => scan(0x2f),           // V
        48 => scan(0x30),           // B
        49 => scan(0x31),           // N
        50 => scan(0x32),           // M
        51 => scan(0x33),           // Comma
        52 => scan(0x34),           // Period
        53 => scan(0x35),           // Slash
        54 => scan(0x36),           // RightShift
        55 => scan(0x37),           // KeypadAsterisk
        56 => scan(0x38),           // LeftAlt
        57 => scan(0x39),           // Space
        58 => scan(0x3a),           // CapsLock
        59 => scan(0x3b),           // F1
        60 => scan(0x3c),           // F2
        61 => scan(0x3d),           // F3
        62 => scan(0x3e),           // F4
        63 => scan(0x3f),           // F5
        64 => scan(0x40),           // F6
        65 => scan(0x41),           // F7
        66 => scan(0x42),           // F8
        67 => scan(0x43),           // F9
        68 => scan(0x44),           // F10
        69 => scan(0x45),           // NumLock
        70 => scan(0x46),           // ScrollLock
        71 => scan(0x47),           // Keypad7
        72 => scan(0x48),           // Keypad8
        73 => scan(0x49),           // Keypad9
        74 => scan(0x4a),           // KeypadMinus
        75 => scan(0x4b),           // Keypad4
        76 => scan(0x4c),           // Keypad5
        77 => scan(0x4d),           // Keypad6
        78 => scan(0x4e),           // KeypadPlus
        79 => scan(0x4f),           // Keypad1
        80 => scan(0x50),           // Keypad2
        81 => scan(0x51),           // Keypad3
        82 => scan(0x52),           // Keypad0
        83 => scan(0x53),           // KeypadPeriod
        87 => scan(0x57),           // F11
        88 => scan(0x58),           // F12
        97 => extended_scan(0x1d),  // RightCtrl
        98 => extended_scan(0x35),  // KeypadSlash
        99 => extended_scan(0x37),  // PrintScreen
        100 => extended_scan(0x38), // RightAlt
        102 => extended_scan(0x47), // Home
        103 => extended_scan(0x48), // Up
        104 => extended_scan(0x49), // PageUp
        105 => extended_scan(0x4b), // Left
        106 => extended_scan(0x4d), // Right
        107 => extended_scan(0x4f), // End
        108 => extended_scan(0x50), // Down
        109 => extended_scan(0x51), // PageDown
        110 => extended_scan(0x52), // Insert
        111 => extended_scan(0x53), // Delete
        113 => virtual_key(0xad),   // Mute
        114 => virtual_key(0xae),   // VolumeDown
        115 => virtual_key(0xaf),   // VolumeUp
        119 => virtual_key(0x13),   // Pause
        125 => extended_scan(0x5b), // LeftMeta
        126 => extended_scan(0x5c), // RightMeta
        163 => virtual_key(0xb0),   // NextSong
        164 => virtual_key(0xb3),   // PlayPause
        165 => virtual_key(0xb1),   // PreviousSong
        166 => virtual_key(0xb2),   // Stop
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keycodes::KEY_TABLE;

    #[test]
    fn covers_every_key_table_code() {
        for (name, code) in KEY_TABLE {
            assert!(
                key_stroke(*code).is_some(),
                "{} ({code}) has no Windows mapping",
                name
            );
        }
    }

    #[test]
    fn extended_keys_use_extended_scancodes() {
        for code in [
            97, 98, 99, 100, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 125, 126,
        ] {
            let stroke = key_stroke(code).expect("extended mapping");
            assert!(stroke.scancode, "{code} must use a scancode");
            assert!(stroke.extended, "{code} must set KEYEVENTF_EXTENDEDKEY");
        }
    }

    #[test]
    fn media_keys_fall_back_to_virtual_keys() {
        for code in [113, 114, 115, 163, 164, 165, 166] {
            let stroke = key_stroke(code).expect("media mapping");
            assert!(!stroke.scancode, "{code} must use a virtual key");
            assert!(!stroke.extended, "{code} must not be an extended key");
        }
    }
}
