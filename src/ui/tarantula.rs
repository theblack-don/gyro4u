//! Front-view painting for the GameSir Tarantula Pro. Geometry and hit testing
//! live in `tarantula_layout`; the SVG asset uses the same coordinates.

use egui::{
    Align2, Color32, CornerRadius, FontId, Image, Painter, Pos2, Rect, Stroke, StrokeKind, Ui,
    include_image, vec2,
};

use crate::inputs::*;
use crate::theme::ThemeColors;
use crate::ui::tarantula_layout::*;
use crate::ui::widgets::accent_alpha;

pub(crate) fn paint(
    ui: &mut Ui,
    painter: &Painter,
    layout: &TarantulaLayout,
    state: Option<&ControllerState>,
    selected: Option<Button>,
    hovered: Option<Button>,
    c: &ThemeColors,
) {
    Image::new(include_image!("../../assets/tarantula_pro.svg"))
        .maintain_aspect_ratio(true)
        .paint_at(ui, layout.art_rect());

    let has_live_state = state.is_some();
    let state = state.copied().unwrap_or_default();
    paint_overlays(
        painter,
        layout,
        &state,
        has_live_state,
        selected,
        hovered,
        c,
    );
}

fn scaled_font(scale: f32, base: f32, min: f32, max: f32) -> FontId {
    FontId::proportional((base * scale).clamp(min, max))
}

fn scaled_corner(scale: f32, units: f32) -> CornerRadius {
    CornerRadius::same((units * scale).round().clamp(0.0, 255.0) as u8)
}

fn visual_radius(button: Button) -> f32 {
    match button {
        Button::Triangle | Button::Square | Button::Circle | Button::Cross => FACE_RADIUS,
        Button::L3 | Button::R3 => STICK_RADIUS,
        Button::PS => HOME_RADIUS,
        Button::GameSirT1 | Button::GameSirT2 | Button::GameSirT3 => T_BUTTON_RADIUS,
        _ => DPAD_RADIUS,
    }
}

fn paint_overlays(
    painter: &Painter,
    layout: &TarantulaLayout,
    state: &ControllerState,
    has_live_state: bool,
    selected: Option<Button>,
    hovered: Option<Button>,
    c: &ThemeColors,
) {
    let scale = layout.scale();
    let accent = c.accent();
    let white = Color32::from_white_alpha(235);
    let dim = Color32::from_white_alpha(165);

    for (rect, value, label) in [
        (LEFT_TRIGGER_BAR, state.l2, "LT"),
        (RIGHT_TRIGGER_BAR, state.r2, "RT"),
    ] {
        let amount = value as f32 / 255.0;
        if amount <= 0.0 {
            continue;
        }
        let rect = layout.rect(rect);
        painter.rect_filled(
            rect,
            scaled_corner(scale, 2.4),
            Color32::from_black_alpha(65),
        );
        let fill = Rect::from_min_max(
            Pos2::new(rect.min.x, rect.max.y - rect.height() * amount),
            rect.max,
        );
        painter.rect_filled(fill, scaled_corner(scale, 2.4), accent_alpha(c, 190));
        painter.rect_stroke(
            rect,
            scaled_corner(scale, 2.4),
            Stroke::new(1.0_f32, dim),
            StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.6, 9.0, 15.0),
            white,
        );
    }

    for (rect, button, label) in [
        (LEFT_BUMPER, Button::L1, "LB"),
        (RIGHT_BUMPER, Button::R1, "RB"),
        (C1_CHIP, Button::GameSirC1, "C1"),
        (C2_CHIP, Button::GameSirC2, "C2"),
        (C3_CHIP, Button::GameSirC3, "C3"),
        (C4_CHIP, Button::GameSirC4, "C4"),
        (VIEW_PILL, Button::Create, "View"),
        (MENU_PILL, Button::Options, "Menu"),
        (SHARE_PILL, Button::GameSirShare, "Share"),
        (LEFT_REAR_CHIP, Button::GameSirL4, "L4"),
        (RIGHT_REAR_CHIP, Button::GameSirR4, "R4"),
    ] {
        let rect = layout.rect(rect);
        if button.is_pressed(state) {
            painter.rect_filled(rect, scaled_corner(scale, 2.0), accent_alpha(c, 190));
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 1.9, 8.0, 12.0),
            white,
        );
    }

    for (button, center) in [
        (Button::GameSirT1, T1_CENTER),
        (Button::GameSirT2, T2_CENTER),
        (Button::GameSirT3, T3_CENTER),
    ] {
        let center = layout.point(center);
        if button.is_pressed(state) {
            painter.circle_filled(center, T_BUTTON_RADIUS * scale, accent_alpha(c, 190));
        }
        painter.text(
            center,
            Align2::CENTER_CENTER,
            button.label(),
            scaled_font(scale, 1.9, 8.0, 12.0),
            white,
        );
    }

    for (button, center) in [
        (Button::DPadUp, DPAD_UP),
        (Button::DPadRight, DPAD_RIGHT),
        (Button::DPadDown, DPAD_DOWN),
        (Button::DPadLeft, DPAD_LEFT),
    ] {
        if button.is_pressed(state) {
            painter.circle_filled(
                layout.point(center),
                DPAD_RADIUS * scale,
                accent_alpha(c, 190),
            );
        }
    }

    for (button, center) in [
        (Button::Triangle, FACE_Y),
        (Button::Square, FACE_X),
        (Button::Circle, FACE_B),
        (Button::Cross, FACE_A),
    ] {
        let center = layout.point(center);
        if button.is_pressed(state) {
            painter.circle_filled(center, FACE_RADIUS * scale, accent_alpha(c, 140));
        }
        painter.text(
            center,
            Align2::CENTER_CENTER,
            button.device_label(ControllerKind::GameSirTarantulaPro),
            scaled_font(scale, 2.8, 9.0, 14.0),
            white,
        );
    }

    let home = layout.point(HOME_CENTER);
    if Button::PS.is_pressed(state) {
        painter.circle_filled(home, HOME_RADIUS * scale, accent_alpha(c, 175));
    }
    painter.text(
        home,
        Align2::CENTER_CENTER,
        Button::PS.device_label(ControllerKind::GameSirTarantulaPro),
        scaled_font(scale, 1.7, 7.0, 11.0),
        white,
    );

    for (button, center, raw, label) in [
        (Button::L3, LEFT_STICK, (state.left_x, state.left_y), "L3"),
        (
            Button::R3,
            RIGHT_STICK,
            (state.right_x, state.right_y),
            "R3",
        ),
    ] {
        let center = layout.point(center);
        let radius = STICK_RADIUS * scale;
        if button.is_pressed(state) {
            painter.circle_stroke(center, radius + 2.0, Stroke::new(2.0_f32, accent));
        }
        painter.text(
            center,
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.3, 8.0, 13.0),
            white,
        );
        let nx = (raw.0 as f32 - 128.0) / 128.0;
        let ny = (raw.1 as f32 - 128.0) / 128.0;
        if has_live_state && (nx.abs() > 0.05 || ny.abs() > 0.05) {
            let dot = center + vec2(nx, ny) * (radius * 0.55);
            let dot_radius = (radius * 0.22).max(3.0);
            painter.circle_filled(dot, dot_radius, white);
            painter.circle_stroke(
                dot,
                dot_radius,
                Stroke::new(1.0_f32, Color32::from_black_alpha(180)),
            );
        }
    }

    for hotspot in TARANTULA_HOTSPOTS {
        let is_selected = selected == Some(hotspot.button);
        let is_hovered = hovered == Some(hotspot.button);
        if !is_selected && !is_hovered {
            continue;
        }
        let color = if is_selected {
            accent
        } else {
            Color32::from_white_alpha(220)
        };
        let width = if is_selected { 2.0_f32 } else { 1.2_f32 };

        match hotspot.shape {
            TarantulaShape::Circle { center, .. } => {
                painter.circle_stroke(
                    layout.point(center),
                    visual_radius(hotspot.button) * scale + 2.5,
                    Stroke::new(width, color),
                );
            }
            TarantulaShape::Rect(rect) => {
                painter.rect_stroke(
                    layout.rect(rect).expand(1.5),
                    scaled_corner(scale, 2.0),
                    Stroke::new(width, color),
                    StrokeKind::Outside,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn svg_asset_parses_with_the_svg_renderer() {
        let bytes = include_bytes!("../../assets/tarantula_pro.svg");
        let image = egui_extras::image::load_svg_bytes(bytes, &Default::default())
            .expect("GameSir Tarantula Pro SVG should parse");
        assert!(image.size[0] > 0 && image.size[1] > 0);
    }
}
