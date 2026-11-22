//! Front-view painting for the 8BitDo Pro 3. Geometry and hit testing live in
//! `eightbitdo_layout`; the SVG asset uses the same coordinates.

use egui::{
    Align2, Color32, CornerRadius, FontId, Image, Painter, Pos2, Rect, Stroke, StrokeKind, Ui,
    include_image, vec2,
};

use crate::inputs::*;
use crate::theme::ThemeColors;
use crate::ui::eightbitdo_layout::*;
use crate::ui::widgets::accent_alpha;

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint(
    ui: &mut Ui,
    painter: &Painter,
    layout: &EightBitDoLayout,
    state: Option<&ControllerState>,
    selected: Option<Button>,
    hovered: Option<Button>,
    abxy: AbxyLayout,
    c: &ThemeColors,
) {
    Image::new(include_image!("../../assets/eightbitdo_pro3.svg"))
        .maintain_aspect_ratio(true)
        .paint_at(ui, layout.art_rect());

    let state = state.copied().unwrap_or_default();
    paint_overlays(painter, layout, &state, selected, hovered, abxy, c);
}

fn scaled_font(scale: f32, base: f32, min: f32, max: f32) -> FontId {
    FontId::proportional((base * scale).clamp(min, max))
}

fn scaled_corner(scale: f32, units: f32) -> CornerRadius {
    CornerRadius::same((units * scale).round().clamp(0.0, 255.0) as u8)
}

fn paint_overlays(
    painter: &Painter,
    layout: &EightBitDoLayout,
    state: &ControllerState,
    selected: Option<Button>,
    hovered: Option<Button>,
    abxy: AbxyLayout,
    c: &ThemeColors,
) {
    let scale = layout.scale();
    let accent = c.accent();
    let white = Color32::from_white_alpha(235);
    let dim = Color32::from_white_alpha(165);

    for (rect, value, label) in [
        (LEFT_TRIGGER_BAR, state.l2, "L2"),
        (RIGHT_TRIGGER_BAR, state.r2, "R2"),
    ] {
        let rect = layout.rect(rect);
        painter.rect_filled(
            rect,
            scaled_corner(scale, 3.2),
            Color32::from_black_alpha(65),
        );
        let amount = value as f32 / 255.0;
        if amount > 0.0 {
            let fill = Rect::from_min_max(
                Pos2::new(rect.min.x, rect.max.y - rect.height() * amount),
                rect.max,
            );
            painter.rect_filled(fill, scaled_corner(scale, 3.2), accent_alpha(c, 190));
        }
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

    for (rect, button) in [
        (LEFT_BUMPER, Button::L1),
        (RIGHT_BUMPER, Button::R1),
        (LEFT_EXTRA_BUMPER, Button::EightBitDoL4),
        (RIGHT_EXTRA_BUMPER, Button::EightBitDoR4),
    ] {
        if button.is_pressed(state) {
            painter.rect_filled(
                layout.rect(rect),
                scaled_corner(scale, 3.4),
                accent_alpha(c, 190),
            );
        }
    }
    for (rect, label) in [
        (LEFT_BUMPER, "L1"),
        (RIGHT_BUMPER, "R1"),
        (LEFT_EXTRA_BUMPER, "L4"),
        (RIGHT_EXTRA_BUMPER, "R4"),
    ] {
        let rect = layout.rect(rect);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.2, 8.0, 13.0),
            white,
        );
    }

    for (rect, button, glyph) in [
        (CREATE_PILL, Button::Create, "−"),
        (OPTIONS_PILL, Button::Options, "+"),
    ] {
        let rect = layout.rect(rect);
        if button.is_pressed(state) {
            painter.rect_filled(rect, scaled_corner(scale, 2.5), accent_alpha(c, 190));
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            glyph,
            scaled_font(scale, 2.2, 8.0, 12.0),
            white,
        );
    }

    if Button::PS.is_pressed(state) {
        painter.circle_filled(
            layout.point(HOME_CENTER),
            HOME_RADIUS * scale,
            accent_alpha(c, 175),
        );
    }

    for (button, center, glyph) in [
        (Button::DPadUp, DPAD_UP, "▲"),
        (Button::DPadRight, DPAD_RIGHT, "▶"),
        (Button::DPadDown, DPAD_DOWN, "▼"),
        (Button::DPadLeft, DPAD_LEFT, "◀"),
    ] {
        let center = layout.point(center);
        let pressed = button.is_pressed(state);
        if pressed {
            painter.circle_filled(center, DPAD_HIT_RADIUS * scale, accent_alpha(c, 190));
        }
        painter.text(
            center,
            Align2::CENTER_CENTER,
            glyph,
            scaled_font(scale, 2.0, 7.0, 11.0),
            if pressed {
                white
            } else {
                Color32::from_white_alpha(165)
            },
        );
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
            button.device_label_with_layout(ControllerKind::EightBitDoPro3, abxy),
            scaled_font(scale, 2.8, 9.0, 14.0),
            white,
        );
    }

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
        let nx = (raw.0 as f32 - 128.0) / 128.0;
        let ny = (raw.1 as f32 - 128.0) / 128.0;
        let dot = center + vec2(nx, ny) * (radius * 0.55);
        let dot_radius = (radius * 0.22).max(3.0);
        painter.circle_filled(dot, dot_radius, white);
        painter.circle_stroke(
            dot,
            dot_radius,
            Stroke::new(1.0_f32, Color32::from_black_alpha(180)),
        );
        painter.text(
            if button == Button::L3 {
                center + vec2(-(radius + 5.0), 0.0)
            } else {
                center + vec2(radius + 5.0, 0.0)
            },
            if button == Button::L3 {
                Align2::RIGHT_CENTER
            } else {
                Align2::LEFT_CENTER
            },
            label,
            scaled_font(scale, 2.2, 8.0, 12.0),
            dim,
        );
    }

    for (rect, button, label) in [
        (PL_PADDLE, Button::EightBitDoPL, "PL"),
        (PR_PADDLE, Button::EightBitDoPR, "PR"),
    ] {
        let rect = layout.rect(rect);
        if button.is_pressed(state) {
            painter.rect_filled(rect, scaled_corner(scale, 3.4), accent_alpha(c, 175));
        }
        painter.text(
            rect.center() + vec2(0.0, -1.8 * scale),
            Align2::CENTER_CENTER,
            label,
            scaled_font(scale, 2.4, 9.0, 14.0),
            white,
        );
        painter.text(
            rect.center() + vec2(0.0, 4.6 * scale),
            Align2::CENTER_CENTER,
            "rear",
            scaled_font(scale, 1.6, 6.0, 9.0),
            dim,
        );
    }

    for hotspot in EIGHTBITDO_HOTSPOTS {
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
            EightBitDoShape::Circle { center, radius } => {
                let center = layout.point(center);
                let radius = match hotspot.button {
                    Button::Triangle | Button::Square | Button::Circle | Button::Cross => {
                        FACE_RADIUS
                    }
                    Button::L3 | Button::R3 => STICK_RADIUS,
                    Button::PS => HOME_RADIUS,
                    _ => radius,
                };
                painter.circle_stroke(center, radius * scale + 2.5, Stroke::new(width, color));
            }
            EightBitDoShape::Rect(rect) => {
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
        let bytes = include_bytes!("../../assets/eightbitdo_pro3.svg");
        let image = egui_extras::image::load_svg_bytes(bytes, &Default::default())
            .expect("8BitDo Pro 3 SVG should parse");
        assert!(image.size[0] > 0 && image.size[1] > 0);
    }
}
