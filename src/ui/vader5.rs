//! Front-view painting for the Flydigi Vader 5 Pro. Geometry and hit testing
//! live in `vader5_layout`; the SVG asset uses the same coordinates.

use egui::{
    Align2, Color32, CornerRadius, FontId, Image, Painter, Pos2, Rect, Stroke, StrokeKind, Ui,
    include_image, vec2,
};

use crate::inputs::*;
use crate::theme::ThemeColors;
use crate::ui::vader5_layout::*;
use crate::ui::widgets::accent_alpha;

pub(crate) fn paint(
    ui: &mut Ui,
    painter: &Painter,
    layout: &Vader5Layout,
    state: Option<&ControllerState>,
    selected: Option<Button>,
    hovered: Option<Button>,
    c: &ThemeColors,
) {
    Image::new(include_image!("../../assets/vader5_pro.svg"))
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

fn paint_overlays(
    painter: &Painter,
    layout: &Vader5Layout,
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
            scaled_corner(scale, 3.2),
            Color32::from_black_alpha(65),
        );
        let fill = Rect::from_min_max(
            Pos2::new(rect.min.x, rect.max.y - rect.height() * amount),
            rect.max,
        );
        painter.rect_filled(fill, scaled_corner(scale, 3.2), accent_alpha(c, 190));
        painter.rect_stroke(
            rect,
            scaled_corner(scale, 3.2),
            Stroke::new(1.0_f32, dim),
            StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.8, 9.0, 15.0),
            white,
        );
    }

    for (rect, button, label) in [
        (LEFT_BUMPER, Button::L1, "LB"),
        (RIGHT_BUMPER, Button::R1, "RB"),
        (LEFT_EXTRA_BUMPER, Button::ApexLM, "LM"),
        (RIGHT_EXTRA_BUMPER, Button::ApexRM, "RM"),
    ] {
        if !button.is_pressed(state) {
            continue;
        }
        let rect = layout.rect(rect);
        painter.rect_filled(rect, scaled_corner(scale, 3.4), accent_alpha(c, 190));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.2, 8.0, 13.0),
            white,
        );
    }

    for (rect, button) in [
        (CREATE_PILL, Button::Create),
        (OPTIONS_PILL, Button::Options),
    ] {
        if button.is_pressed(state) {
            painter.rect_filled(
                layout.rect(rect),
                scaled_corner(scale, 2.5),
                accent_alpha(c, 190),
            );
        }
    }

    if Button::PS.is_pressed(state) {
        painter.circle_filled(
            layout.point(GUIDE_CENTER),
            GUIDE_RADIUS * scale,
            accent_alpha(c, 175),
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
                DPAD_HIT_RADIUS * scale,
                accent_alpha(c, 190),
            );
        }
    }

    for (button, center, label) in [
        (Button::Triangle, FACE_Y, "Y"),
        (Button::Square, FACE_X, "X"),
        (Button::Circle, FACE_B, "B"),
        (Button::Cross, FACE_A, "A"),
    ] {
        let center = layout.point(center);
        if button.is_pressed(state) {
            painter.circle_filled(center, FACE_RADIUS * scale, accent_alpha(c, 140));
        }
        painter.text(
            center,
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.8, 9.0, 14.0),
            white,
        );
    }

    for (button, center, label) in [
        (Button::VaderC, C_BUTTON, "C"),
        (Button::VaderZ, Z_BUTTON, "Z"),
    ] {
        let center = layout.point(center);
        if button.is_pressed(state) {
            painter.circle_filled(center, CZ_RADIUS * scale, accent_alpha(c, 140));
        }
        painter.text(
            center,
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.5, 8.0, 13.0),
            white,
        );
    }

    let circle_center = layout.point(VADER_CIRCLE_BUTTON);
    let circle_radius = VADER_CIRCLE_RADIUS * scale;
    if Button::VaderCircle.is_pressed(state) {
        painter.circle_filled(circle_center, circle_radius, accent_alpha(c, 140));
    }
    painter.circle_stroke(circle_center, circle_radius, Stroke::new(1.0_f32, dim));
    painter.text(
        circle_center,
        Align2::CENTER_CENTER,
        "Circle",
        scaled_font(scale, 2.0, 7.0, 12.0),
        white,
    );

    for (button, center, raw) in [
        (Button::L3, LEFT_STICK, (state.left_x, state.left_y)),
        (Button::R3, RIGHT_STICK, (state.right_x, state.right_y)),
    ] {
        let center = layout.point(center);
        let radius = STICK_RADIUS * scale;
        if button.is_pressed(state) {
            painter.circle_stroke(center, radius + 2.0, Stroke::new(2.0_f32, accent));
        }
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

    for (rect, button) in [
        (M1_CHIP, Button::ApexM1),
        (M2_CHIP, Button::ApexM2),
        (M3_CHIP, Button::ApexM3),
        (M4_CHIP, Button::ApexM4),
    ] {
        if button.is_pressed(state) {
            painter.rect_filled(
                layout.rect(rect),
                scaled_corner(scale, 3.4),
                accent_alpha(c, 175),
            );
        }
    }

    for hotspot in VADER5_HOTSPOTS {
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
            Vader5Shape::Circle { center, radius } => {
                let center = layout.point(center);
                let radius = match hotspot.button {
                    Button::Triangle | Button::Square | Button::Circle | Button::Cross => {
                        FACE_RADIUS
                    }
                    Button::VaderC | Button::VaderZ => CZ_RADIUS,
                    Button::L3 | Button::R3 => STICK_RADIUS,
                    Button::PS => GUIDE_RADIUS,
                    _ => radius,
                };
                painter.circle_stroke(center, radius * scale + 2.5, Stroke::new(width, color));
            }
            Vader5Shape::Rect(rect) => {
                painter.rect_stroke(
                    layout.rect(rect).expand(1.5),
                    scaled_corner(scale, 3.0),
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
        let bytes = include_bytes!("../../assets/vader5_pro.svg");
        let image = egui_extras::image::load_svg_bytes(bytes, &Default::default())
            .expect("Flydigi Vader 5 Pro SVG should parse");
        assert!(image.size[0] > 0 && image.size[1] > 0);
    }
}
