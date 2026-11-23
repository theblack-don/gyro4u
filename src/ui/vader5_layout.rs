//! Shared Flydigi Vader 5 Pro geometry and hit testing for the live and mapping
//! views. Coordinates mirror `assets/vader5_pro.svg` (160 x 115 viewBox).

use egui::{Pos2, Rect, vec2};

use crate::inputs::Button;

pub const VIEWBOX_W: f32 = 160.0;
pub const VIEWBOX_H: f32 = 115.0;

/// Small system buttons stay selectable when the diagram is narrow.
pub const MIN_HIT_RADIUS_PX: f32 = 9.0;

#[derive(Clone, Copy, Debug)]
pub struct Vader5Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Vader5Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[cfg(test)]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    fn distance(&self, point: (f32, f32)) -> f32 {
        let dx = (self.x - point.0).max(point.0 - (self.x + self.w)).max(0.0);
        let dy = (self.y - point.1).max(point.1 - (self.y + self.h)).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }

    #[cfg(test)]
    fn bounds(&self) -> Self {
        *self
    }

    #[cfg(test)]
    fn overlaps(&self, other: &Self) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

pub const LEFT_TRIGGER_BAR: Vader5Rect = Vader5Rect::new(31.5, 1.0, 27.0, 7.0);
pub const RIGHT_TRIGGER_BAR: Vader5Rect = Vader5Rect::new(101.5, 1.0, 27.0, 7.0);
pub const LEFT_BUMPER: Vader5Rect = Vader5Rect::new(31.5, 9.2, 36.0, 10.8);
pub const RIGHT_BUMPER: Vader5Rect = Vader5Rect::new(92.5, 9.2, 36.0, 10.8);
pub const LEFT_EXTRA_BUMPER: Vader5Rect = Vader5Rect::new(59.5, 1.0, 8.0, 7.0);
pub const RIGHT_EXTRA_BUMPER: Vader5Rect = Vader5Rect::new(92.5, 1.0, 8.0, 7.0);

pub const DPAD_UP: (f32, f32) = (62.5, 53.0);
pub const DPAD_RIGHT: (f32, f32) = (71.0, 61.5);
pub const DPAD_DOWN: (f32, f32) = (62.5, 70.0);
pub const DPAD_LEFT: (f32, f32) = (54.0, 61.5);
pub const DPAD_HIT_RADIUS: f32 = 4.2;

pub const LEFT_STICK: (f32, f32) = (44.5, 39.0);
pub const RIGHT_STICK: (f32, f32) = (99.5, 61.5);
pub const STICK_RADIUS: f32 = 13.2;
pub const STICK_HIT_RADIUS: f32 = 11.0;

pub const FACE_RADIUS: f32 = 5.0;
pub const FACE_HIT_RADIUS: f32 = 4.6;
pub const FACE_Y: (f32, f32) = (118.0, 29.0);
pub const FACE_X: (f32, f32) = (108.0, 39.0);
pub const FACE_B: (f32, f32) = (128.0, 39.0);
pub const FACE_A: (f32, f32) = (118.0, 49.0);

pub const C_BUTTON: (f32, f32) = (123.5, 64.0);
pub const Z_BUTTON: (f32, f32) = (133.0, 56.0);
pub const CZ_RADIUS: f32 = 4.4;
pub const CZ_HIT_RADIUS: f32 = 4.2;

pub const VADER_CIRCLE_BUTTON: (f32, f32) = (80.0, 48.0);
pub const VADER_CIRCLE_RADIUS: f32 = 4.2;
pub const VADER_CIRCLE_HIT_RADIUS: f32 = 4.2;

pub const CREATE_PILL: Vader5Rect = Vader5Rect::new(64.5, 33.8, 10.0, 5.4);
pub const OPTIONS_PILL: Vader5Rect = Vader5Rect::new(87.5, 33.8, 10.0, 5.4);
pub const GUIDE_CENTER: (f32, f32) = (80.0, 26.0);
pub const GUIDE_RADIUS: f32 = 5.5;

pub const M1_CHIP: Vader5Rect = Vader5Rect::new(18.0, 80.0, 10.0, 10.0);
pub const M2_CHIP: Vader5Rect = Vader5Rect::new(14.0, 93.0, 10.0, 10.0);
pub const M3_CHIP: Vader5Rect = Vader5Rect::new(132.0, 80.0, 10.0, 10.0);
pub const M4_CHIP: Vader5Rect = Vader5Rect::new(136.0, 93.0, 10.0, 10.0);

#[derive(Clone, Copy, Debug)]
pub enum Vader5Shape {
    Circle { center: (f32, f32), radius: f32 },
    Rect(Vader5Rect),
}

impl Vader5Shape {
    #[cfg(test)]
    pub fn center(&self) -> (f32, f32) {
        match self {
            Self::Circle { center, .. } => *center,
            Self::Rect(rect) => rect.center(),
        }
    }

    #[cfg(test)]
    fn bounds(&self) -> Vader5Rect {
        match self {
            Self::Circle { center, radius } => Vader5Rect::new(
                center.0 - radius,
                center.1 - radius,
                radius * 2.0,
                radius * 2.0,
            ),
            Self::Rect(rect) => rect.bounds(),
        }
    }

    #[cfg(test)]
    fn overlaps(&self, other: &Self) -> bool {
        self.bounds().overlaps(&other.bounds())
    }

    fn distance(&self, point: (f32, f32)) -> f32 {
        match self {
            Self::Circle { center, .. } => {
                ((point.0 - center.0).powi(2) + (point.1 - center.1).powi(2)).sqrt()
            }
            Self::Rect(rect) => rect.distance(point),
        }
    }

    fn reach(&self, min_radius: f32) -> f32 {
        match self {
            Self::Circle { radius, .. } => radius.max(min_radius),
            Self::Rect(_) => min_radius,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Vader5Hotspot {
    pub button: Button,
    pub shape: Vader5Shape,
}

const fn circle(button: Button, center: (f32, f32), radius: f32) -> Vader5Hotspot {
    Vader5Hotspot {
        button,
        shape: Vader5Shape::Circle { center, radius },
    }
}

const fn rect(button: Button, rect: Vader5Rect) -> Vader5Hotspot {
    Vader5Hotspot {
        button,
        shape: Vader5Shape::Rect(rect),
    }
}

pub const VADER5_HOTSPOTS: &[Vader5Hotspot] = &[
    rect(Button::L2, LEFT_TRIGGER_BAR),
    rect(Button::R2, RIGHT_TRIGGER_BAR),
    rect(Button::L1, LEFT_BUMPER),
    rect(Button::R1, RIGHT_BUMPER),
    rect(Button::ApexLM, LEFT_EXTRA_BUMPER),
    rect(Button::ApexRM, RIGHT_EXTRA_BUMPER),
    circle(Button::DPadUp, DPAD_UP, DPAD_HIT_RADIUS),
    circle(Button::DPadRight, DPAD_RIGHT, DPAD_HIT_RADIUS),
    circle(Button::DPadDown, DPAD_DOWN, DPAD_HIT_RADIUS),
    circle(Button::DPadLeft, DPAD_LEFT, DPAD_HIT_RADIUS),
    circle(Button::Triangle, FACE_Y, FACE_HIT_RADIUS),
    circle(Button::Square, FACE_X, FACE_HIT_RADIUS),
    circle(Button::Circle, FACE_B, FACE_HIT_RADIUS),
    circle(Button::Cross, FACE_A, FACE_HIT_RADIUS),
    circle(Button::VaderC, C_BUTTON, CZ_HIT_RADIUS),
    circle(Button::VaderZ, Z_BUTTON, CZ_HIT_RADIUS),
    circle(
        Button::VaderCircle,
        VADER_CIRCLE_BUTTON,
        VADER_CIRCLE_HIT_RADIUS,
    ),
    circle(Button::L3, LEFT_STICK, STICK_HIT_RADIUS),
    circle(Button::R3, RIGHT_STICK, STICK_HIT_RADIUS),
    rect(Button::Create, CREATE_PILL),
    rect(Button::Options, OPTIONS_PILL),
    circle(Button::PS, GUIDE_CENTER, GUIDE_RADIUS),
    rect(Button::ApexM1, M1_CHIP),
    rect(Button::ApexM2, M2_CHIP),
    rect(Button::ApexM3, M3_CHIP),
    rect(Button::ApexM4, M4_CHIP),
];

#[derive(Clone, Copy, Debug)]
pub struct Vader5Layout {
    origin: Pos2,
    scale: f32,
}

impl Vader5Layout {
    pub fn fit(rect: Rect) -> Self {
        let scale = (rect.width() / VIEWBOX_W).min(rect.height() / VIEWBOX_H);
        let size = vec2(VIEWBOX_W * scale, VIEWBOX_H * scale);
        Self {
            origin: rect.center() - size * 0.5,
            scale,
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn art_rect(&self) -> Rect {
        Rect::from_min_size(self.origin, vec2(VIEWBOX_W, VIEWBOX_H) * self.scale)
    }

    pub fn point(&self, (x, y): (f32, f32)) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    pub fn view_pos(&self, point: Pos2) -> (f32, f32) {
        let local = (point - self.origin) / self.scale;
        (local.x, local.y)
    }

    pub fn rect(&self, rect: Vader5Rect) -> Rect {
        Rect::from_min_size(
            self.point((rect.x, rect.y)),
            vec2(rect.w, rect.h) * self.scale,
        )
    }

    /// Nearest hotspot; small controls keep a minimum screen-space reach.
    pub fn hit_test(&self, point: Pos2) -> Option<Button> {
        let view = self.view_pos(point);
        let min_radius = MIN_HIT_RADIUS_PX / self.scale.max(f32::EPSILON);
        let mut best: Option<(f32, Button)> = None;

        for hotspot in VADER5_HOTSPOTS {
            let reach = hotspot.shape.reach(min_radius);
            let distance = hotspot.shape.distance(view);
            if distance > reach {
                continue;
            }
            let score = distance / reach;
            if best.is_none_or(|(best_score, _)| score < best_score) {
                best = Some((score, hotspot.button));
            }
        }

        best.map(|(_, button)| button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::ControllerKind;
    use crate::ui::model_art_tests::{
        assert_circle, assert_no_text, assert_path_bounds, assert_rect, assert_viewbox,
    };
    use egui::pos2;
    use std::collections::HashSet;

    const SVG: &str = include_str!("../../assets/vader5_pro.svg");

    #[test]
    fn svg_art_matches_layout_constants() {
        assert_viewbox(SVG, VIEWBOX_W, VIEWBOX_H);
        assert_no_text(SVG);

        for (id, rect) in [
            ("vader-create", CREATE_PILL),
            ("vader-options", OPTIONS_PILL),
        ] {
            assert_rect(SVG, id, rect.x, rect.y, rect.w, rect.h);
        }

        for (id, rect) in [("vader-l1", LEFT_BUMPER), ("vader-r1", RIGHT_BUMPER)] {
            assert_path_bounds(SVG, id, rect.x, rect.y, rect.w, rect.h);
        }

        for (id, (cx, cy), radius) in [
            ("vader-stick-l", LEFT_STICK, STICK_RADIUS),
            ("vader-stick-r", RIGHT_STICK, STICK_RADIUS),
            ("vader-face-y", FACE_Y, FACE_RADIUS),
            ("vader-face-x", FACE_X, FACE_RADIUS),
            ("vader-face-b", FACE_B, FACE_RADIUS),
            ("vader-face-a", FACE_A, FACE_RADIUS),
            ("vader-c", C_BUTTON, CZ_RADIUS),
            ("vader-z", Z_BUTTON, CZ_RADIUS),
        ] {
            assert_circle(SVG, id, cx, cy, radius);
        }
    }

    fn layout() -> Vader5Layout {
        Vader5Layout::fit(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(760.0, 760.0 * VIEWBOX_H / VIEWBOX_W),
        ))
    }

    #[test]
    fn fit_preserves_aspect_ratio_and_centers_in_non_square_canvas() {
        let layout = Vader5Layout::fit(Rect::from_min_size(pos2(10.0, 20.0), vec2(700.0, 400.0)));
        let art = layout.art_rect();
        assert!((art.width() / art.height() - VIEWBOX_W / VIEWBOX_H).abs() < 1e-4);
        assert!((art.center().x - 360.0).abs() < 1e-4);
        assert!((art.center().y - 220.0).abs() < 1e-4);
        assert!((layout.scale() - 400.0 / VIEWBOX_H).abs() < 1e-4);

        let top_left = layout.point((0.0, 0.0));
        let bottom_right = layout.point((VIEWBOX_W, VIEWBOX_H));
        assert!(
            ((bottom_right.x - top_left.x) / (bottom_right.y - top_left.y) - VIEWBOX_W / VIEWBOX_H)
                .abs()
                < 1e-4
        );
    }

    #[test]
    fn hotspots_cover_every_vader5_target_once() {
        let hotspots: HashSet<Button> = VADER5_HOTSPOTS.iter().map(|h| h.button).collect();
        let targets: HashSet<Button> = ControllerKind::FlydigiVader5Pro
            .target_buttons()
            .iter()
            .copied()
            .collect();
        assert_eq!(hotspots, targets);
        assert_eq!(VADER5_HOTSPOTS.len(), targets.len());
        assert_eq!(targets.len(), 26);
    }

    #[test]
    fn hotspots_stay_inside_viewbox() {
        for hotspot in VADER5_HOTSPOTS {
            let bounds = hotspot.shape.bounds();
            assert!(
                bounds.x >= 0.0 && bounds.x + bounds.w <= VIEWBOX_W,
                "{:?} x",
                hotspot.button
            );
            assert!(
                bounds.y >= 0.0 && bounds.y + bounds.h <= VIEWBOX_H,
                "{:?} y",
                hotspot.button
            );
        }
    }

    #[test]
    fn hotspot_bounds_do_not_overlap() {
        for (index, first) in VADER5_HOTSPOTS.iter().enumerate() {
            for second in &VADER5_HOTSPOTS[index + 1..] {
                assert!(
                    !first.shape.overlaps(&second.shape),
                    "{} and {} overlap",
                    first.button.label(),
                    second.button.label()
                );
            }
        }
    }

    #[test]
    fn hit_testing_returns_the_expected_button_at_each_center() {
        let layout = layout();
        for hotspot in VADER5_HOTSPOTS {
            let center = layout.point(hotspot.shape.center());
            assert_eq!(
                layout.hit_test(center),
                Some(hotspot.button),
                "{} at its center",
                hotspot.button.label()
            );
        }
    }

    #[test]
    fn small_controls_keep_a_minimum_hit_reach() {
        let layout = Vader5Layout::fit(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(320.0, 320.0 * VIEWBOX_H / VIEWBOX_W),
        ));
        let create = layout.point(CREATE_PILL.center());
        assert_eq!(
            layout.hit_test(create + vec2(4.0, 0.0)),
            Some(Button::Create)
        );
        let m1 = layout.point(M1_CHIP.center());
        assert_eq!(layout.hit_test(m1 + vec2(0.0, 4.0)), Some(Button::ApexM1));
    }
}
