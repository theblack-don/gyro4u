use std::{collections::HashMap, fs, path::PathBuf, time::SystemTime};

use crate::theme::{Theme, ThemeColors};

pub const ID: &str = "noctalia";

#[derive(Clone, Copy, Default)]
struct Seed {
    window_bg: Option<[u8; 3]>,
    panel_bg: Option<[u8; 3]>,
    accent: Option<[u8; 3]>,
    widget_hovered: Option<[u8; 3]>,
    widget_inactive: Option<[u8; 3]>,
    text: Option<[u8; 3]>,
    text_dim: Option<[u8; 3]>,
    success: Option<[u8; 3]>,
    error: Option<[u8; 3]>,
    warning: Option<[u8; 3]>,
}

impl Seed {
    fn or(self, fallback: Seed) -> Seed {
        Seed {
            window_bg: self.window_bg.or(fallback.window_bg),
            panel_bg: self.panel_bg.or(fallback.panel_bg),
            accent: self.accent.or(fallback.accent),
            widget_hovered: self.widget_hovered.or(fallback.widget_hovered),
            widget_inactive: self.widget_inactive.or(fallback.widget_inactive),
            text: self.text.or(fallback.text),
            text_dim: self.text_dim.or(fallback.text_dim),
            success: self.success.or(fallback.success),
            error: self.error.or(fallback.error),
            warning: self.warning.or(fallback.warning),
        }
    }

    fn finish(self) -> ThemeColors {
        let window_bg = self.window_bg.unwrap_or([7, 7, 34]);
        let text = self.text.unwrap_or([243, 237, 247]);
        let accent = self.accent.unwrap_or([255, 245, 155]);
        let panel_bg = self.panel_bg.unwrap_or_else(|| mix(window_bg, text, 0.07));
        let widget_hovered = self.widget_hovered.unwrap_or(accent);
        let widget_inactive = self
            .widget_inactive
            .unwrap_or_else(|| mix(panel_bg, text, 0.18));
        let text_dim = self.text_dim.unwrap_or_else(|| mix(window_bg, text, 0.42));

        ThemeColors {
            window_bg,
            panel_bg,
            extreme_bg: mix(window_bg, [0, 0, 0], 0.22),
            accent,
            widget_hovered,
            widget_inactive,
            text,
            text_dim,
            success: self.success.unwrap_or(accent),
            error: self.error.unwrap_or([253, 70, 99]),
            warning: self.warning.unwrap_or(widget_hovered),
        }
    }
}

pub fn theme() -> Option<Theme> {
    let colors = gtk_seed().or(kde_seed()).finish();

    Some(Theme {
        id: ID.into(),
        name: "Noctalia".into(),
        dark_mode: is_dark(colors.window_bg),
        colors,
    })
}

pub fn source_mtime() -> Option<SystemTime> {
    source_paths()
        .into_iter()
        .filter_map(|path| fs::metadata(path).ok()?.modified().ok())
        .max()
}

fn source_paths() -> Vec<PathBuf> {
    let mut paths = css_paths();
    if let Some(data) = dirs::data_dir() {
        paths.push(data.join("color-schemes/noctalia.colors"));
    }
    paths
}

fn css_paths() -> Vec<PathBuf> {
    let Some(config) = dirs::config_dir() else {
        return Vec::new();
    };

    vec![
        config.join("gtk-4.0/noctalia.css"),
        config.join("gtk-3.0/noctalia.css"),
    ]
}

fn gtk_seed() -> Seed {
    for path in css_paths() {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let map = parse_declarations(&text);
        let seed = Seed {
            window_bg: lookup(&map, &["window_bg_color", "window-bg-color"]),
            panel_bg: lookup(&map, &["sidebar_bg_color", "sidebar-bg-color"])
                .or_else(|| lookup(&map, &["popover_bg_color", "popover-bg-color"]))
                .or_else(|| lookup(&map, &["card_bg_color", "card-bg-color"]))
                .or_else(|| lookup(&map, &["headerbar_bg_color", "headerbar-bg-color"])),
            accent: lookup(
                &map,
                &[
                    "accent_bg_color",
                    "accent-bg-color",
                    "accent_color",
                    "accent-color",
                ],
            ),
            widget_hovered: lookup(&map, &["warning_color", "warning-color"]),
            text: lookup(
                &map,
                &[
                    "window_fg_color",
                    "window-fg-color",
                    "view_fg_color",
                    "view-fg-color",
                ],
            ),
            text_dim: lookup(&map, &["text_dim_color", "text-dim-color"]),
            success: lookup(&map, &["success_color", "success-color"]),
            error: lookup(
                &map,
                &[
                    "error_bg_color",
                    "error-bg-color",
                    "error_color",
                    "error-color",
                ],
            ),
            warning: lookup(&map, &["warning_color", "warning-color"]),
            ..Seed::default()
        };

        if seed.window_bg.is_some() || seed.accent.is_some() {
            return seed;
        }
    }

    Seed::default()
}

fn kde_seed() -> Seed {
    let Some(data) = dirs::data_dir() else {
        return Seed::default();
    };
    let Ok(text) = fs::read_to_string(data.join("color-schemes/noctalia.colors")) else {
        return Seed::default();
    };
    let map = parse_ini(&text);
    let get = |section: &str, key: &str| -> Option<[u8; 3]> {
        let key = (section.to_ascii_lowercase(), key.to_ascii_lowercase());
        map.get(&key).and_then(|value| parse_kde_color(value))
    };

    Seed {
        window_bg: get("colors:view", "backgroundnormal")
            .or_else(|| get("colors:window", "backgroundnormal")),
        panel_bg: get("colors:window", "backgroundnormal")
            .or_else(|| get("colors:header", "backgroundnormal")),
        accent: get("colors:selection", "backgroundnormal"),
        widget_hovered: get("colors:button", "decorationhover")
            .or_else(|| get("colors:selection", "decorationhover")),
        widget_inactive: get("colors:button", "backgroundnormal"),
        text: get("colors:window", "foregroundnormal")
            .or_else(|| get("colors:view", "foregroundnormal")),
        success: get("colors:window", "foregroundpositive"),
        error: get("colors:window", "foregroundnegative"),
        warning: get("colors:window", "foregroundneutral"),
        ..Seed::default()
    }
}

fn parse_declarations(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for raw in text.lines() {
        let line = raw.trim();

        if let Some(rest) = line.strip_prefix("@define-color") {
            let rest = rest.trim();
            let Some(pos) = rest.find(char::is_whitespace) else {
                continue;
            };
            let name = rest[..pos].trim().to_ascii_lowercase();
            let value = rest[pos..].trim().trim_end_matches(';').trim();
            map.insert(name, value.to_string());
        } else if let Some(rest) = line.strip_prefix("--")
            && let Some((name, value)) = rest.split_once(':')
        {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().trim_end_matches(';').trim();
            map.insert(name, value.to_string());
        }
    }

    map
}

fn parse_ini(text: &str) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    let mut section = String::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if let Some(inner) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            section = inner.trim().to_ascii_lowercase();
        } else if let Some((key, value)) = line.split_once('=') {
            map.insert(
                (section.clone(), key.trim().to_ascii_lowercase()),
                value.trim().to_string(),
            );
        }
    }

    map
}

fn lookup(map: &HashMap<String, String>, keys: &[&str]) -> Option<[u8; 3]> {
    keys.iter().find_map(|key| resolve(map, key))
}

fn resolve(map: &HashMap<String, String>, key: &str) -> Option<[u8; 3]> {
    let mut key = key.to_ascii_lowercase();

    for _ in 0..8 {
        let value = map.get(&key)?;
        if let Some(alias) = value.strip_prefix('@') {
            key = alias.trim().to_ascii_lowercase();
        } else {
            return parse_css_color(value);
        }
    }

    None
}

fn parse_css_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();

    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex);
    }

    let inner = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))?
        .trim_end_matches(')');
    let mut channels = inner
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty());

    Some([
        parse_channel(channels.next()?)?,
        parse_channel(channels.next()?)?,
        parse_channel(channels.next()?)?,
    ])
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim();
    if !hex.is_ascii() {
        return None;
    }

    match hex.len() {
        3 => {
            let bytes = hex.as_bytes();
            Some([
                nibble(bytes[0])? * 17,
                nibble(bytes[1])? * 17,
                nibble(bytes[2])? * 17,
            ])
        }
        6 | 8 => Some([
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ]),
        _ => None,
    }
}

fn nibble(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|value| value as u8)
}

fn parse_channel(value: &str) -> Option<u8> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        let value = percent.trim().parse::<f32>().ok()?;
        Some((value.clamp(0.0, 100.0) * 2.55).round() as u8)
    } else {
        let value = value.trim().parse::<f32>().ok()?;
        Some(value.round().clamp(0.0, 255.0) as u8)
    }
}

fn parse_kde_color(value: &str) -> Option<[u8; 3]> {
    let mut channels = value.split(',').map(|part| part.trim());

    Some([
        channels.next()?.parse::<u8>().ok()?,
        channels.next()?.parse::<u8>().ok()?,
        channels.next()?.parse::<u8>().ok()?,
    ])
}

fn mix(from: [u8; 3], to: [u8; 3], amount: f32) -> [u8; 3] {
    let channel = |from: u8, to: u8| {
        (from as f32 + (to as f32 - from as f32) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };

    [
        channel(from[0], to[0]),
        channel(from[1], to[1]),
        channel(from[2], to[2]),
    ]
}

fn is_dark([r, g, b]: [u8; 3]) -> bool {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) < 128.0
}
