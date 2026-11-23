//! Shared 8BitDo Pro 3 geometry and hit testing for the live and mapping views.
//! Coordinates mirror `assets/eightbitdo_pro3.svg` (160 x 105 viewBox).

use egui::{Pos2, Rect, vec2};

use crate::inputs::Button;

pub const VIEWBOX_W: f32 = 160.0;
pub const VIEWBOX_H: f32 = 105.0;

/// Small system buttons stay selectable when the diagram is narrow.
pub const MIN_HIT_RADIUS_PX: f32 = 9.0;

#[derive(Clone, Copy, Debug)]
pub struct EightBitDoRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl EightBitDoRect {
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

pub const LEFT_TRIGGER_BAR: EightBitDoRect = EightBitDoRect::new(28.0, 2.0, 34.0, 7.0);
pub const RIGHT_TRIGGER_BAR: EightBitDoRect = EightBitDoRect::new(98.0, 2.0, 34.0, 7.0);
pub const LEFT_BUMPER: EightBitDoRect = EightBitDoRect::new(28.0, 10.0, 36.0, 6.0);
pub const RIGHT_BUMPER: EightBitDoRect = EightBitDoRect::new(96.0, 10.0, 36.0, 6.0);
pub const LEFT_EXTRA_BUMPER: EightBitDoRect = EightBitDoRect::new(66.0, 8.0, 10.0, 8.0);
pub const RIGHT_EXTRA_BUMPER: EightBitDoRect = EightBitDoRect::new(84.0, 8.0, 10.0, 8.0);

pub const DPAD_UP: (f32, f32) = (42.0, 23.6);
pub const DPAD_RIGHT: (f32, f32) = (48.4, 30.0);
pub const DPAD_DOWN: (f32, f32) = (42.0, 36.4);
pub const DPAD_LEFT: (f32, f32) = (35.6, 30.0);
pub const DPAD_HIT_RADIUS: f32 = 3.2;

pub const LEFT_STICK: (f32, f32) = (59.0, 46.0);
pub const RIGHT_STICK: (f32, f32) = (101.0, 46.0);
pub const STICK_RADIUS: f32 = 9.5;
pub const STICK_HIT_RADIUS: f32 = 9.5;

/// Xbox cap positions; the physical buttons are Triangle top, Square left,
/// Circle right, and Cross bottom.
pub const FACE_RADIUS: f32 = 4.8;
pub const FACE_HIT_RADIUS: f32 = 4.5;
pub const FACE_Y: (f32, f32) = (117.0, 21.0);
pub const FACE_X: (f32, f32) = (106.5, 30.0);
pub const FACE_B: (f32, f32) = (127.5, 30.0);
pub const FACE_A: (f32, f32) = (117.0, 39.0);

pub const CREATE_PILL: EightBitDoRect = EightBitDoRect::new(69.0, 27.7, 9.5, 3.2);
pub const OPTIONS_PILL: EightBitDoRect = EightBitDoRect::new(81.5, 27.7, 9.5, 3.2);
pub const HOME_CENTER: (f32, f32) = (80.0, 47.6);
pub const HOME_RADIUS: f32 = 2.8;

pub const PL_PADDLE: EightBitDoRect = EightBitDoRect::new(22.0, 80.0, 14.0, 16.0);
pub const PR_PADDLE: EightBitDoRect = EightBitDoRect::new(124.0, 80.0, 14.0, 16.0);

#[derive(Clone, Copy, Debug)]
pub enum EightBitDoShape {
    Circle { center: (f32, f32), radius: f32 },
    Rect(EightBitDoRect),
}

impl EightBitDoShape {
    #[cfg(test)]
    pub fn center(&self) -> (f32, f32) {
        match self {
            Self::Circle { center, .. } => *center,
            Self::Rect(rect) => rect.center(),
        }
    }

    #[cfg(test)]
    fn bounds(&self) -> EightBitDoRect {
        match self {
            Self::Circle { center, radius } => EightBitDoRect::new(
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
pub struct EightBitDoHotspot {
    pub button: Button,
    pub shape: EightBitDoShape,
}

const fn circle(button: Button, center: (f32, f32), radius: f32) -> EightBitDoHotspot {
    EightBitDoHotspot {
        button,
        shape: EightBitDoShape::Circle { center, radius },
    }
}

const fn rect(button: Button, rect: EightBitDoRect) -> EightBitDoHotspot {
    EightBitDoHotspot {
        button,
        shape: EightBitDoShape::Rect(rect),
    }
}

pub const EIGHTBITDO_HOTSPOTS: &[EightBitDoHotspot] = &[
    rect(Button::L2, LEFT_TRIGGER_BAR),
    rect(Button::R2, RIGHT_TRIGGER_BAR),
    rect(Button::L1, LEFT_BUMPER),
    rect(Button::R1, RIGHT_BUMPER),
    rect(Button::EightBitDoL4, LEFT_EXTRA_BUMPER),
    rect(Button::EightBitDoR4, RIGHT_EXTRA_BUMPER),
    circle(Button::DPadUp, DPAD_UP, DPAD_HIT_RADIUS),
    circle(Button::DPadRight, DPAD_RIGHT, DPAD_HIT_RADIUS),
    circle(Button::DPadDown, DPAD_DOWN, DPAD_HIT_RADIUS),
    circle(Button::DPadLeft, DPAD_LEFT, DPAD_HIT_RADIUS),
    circle(Button::Triangle, FACE_Y, FACE_HIT_RADIUS),
    circle(Button::Square, FACE_X, FACE_HIT_RADIUS),
    circle(Button::Circle, FACE_B, FACE_HIT_RADIUS),
    circle(Button::Cross, FACE_A, FACE_HIT_RADIUS),
    circle(Button::L3, LEFT_STICK, STICK_HIT_RADIUS),
    circle(Button::R3, RIGHT_STICK, STICK_HIT_RADIUS),
    rect(Button::Create, CREATE_PILL),
    rect(Button::Options, OPTIONS_PILL),
    circle(Button::PS, HOME_CENTER, HOME_RADIUS),
    rect(Button::EightBitDoPL, PL_PADDLE),
    rect(Button::EightBitDoPR, PR_PADDLE),
];

#[derive(Clone, Copy, Debug)]
pub struct EightBitDoLayout {
    origin: Pos2,
    scale: f32,
}

impl EightBitDoLayout {
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

    pub fn rect(&self, rect: EightBitDoRect) -> Rect {
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

        for hotspot in EIGHTBITDO_HOTSPOTS {
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
    use crate::ui::model_art_tests::{assert_circle, assert_no_text, assert_rect, assert_viewbox};
    use egui::pos2;
    use std::collections::HashSet;

    const SVG: &str = include_str!("../../assets/eightbitdo_pro3.svg");

    #[test]
    fn svg_art_matches_layout_constants() {
        assert_viewbox(SVG, VIEWBOX_W, VIEWBOX_H);
        assert_no_text(SVG);

        for (id, rect) in [
            ("pro3-create", CREATE_PILL),
            ("pro3-options", OPTIONS_PILL),
            (
                "pro3-home",
                EightBitDoRect::new(
                    HOME_CENTER.0 - HOME_RADIUS,
                    HOME_CENTER.1 - 1.4,
                    HOME_RADIUS * 2.0,
                    2.8,
                ),
            ),
        ] {
            assert_rect(SVG, id, rect.x, rect.y, rect.w, rect.h);
        }

        for (id, (cx, cy), radius) in [
            ("pro3-stick-l", LEFT_STICK, STICK_RADIUS),
            ("pro3-stick-r", RIGHT_STICK, STICK_RADIUS),
            ("pro3-face-y", FACE_Y, FACE_RADIUS),
            ("pro3-face-x", FACE_X, FACE_RADIUS),
            ("pro3-face-b", FACE_B, FACE_RADIUS),
            ("pro3-face-a", FACE_A, FACE_RADIUS),
        ] {
            assert_circle(SVG, id, cx, cy, radius);
        }
    }

    fn layout() -> EightBitDoLayout {
        EightBitDoLayout::fit(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(760.0, 760.0 * VIEWBOX_H / VIEWBOX_W),
        ))
    }

    #[test]
    fn fit_preserves_aspect_ratio_and_centers_in_non_square_canvas() {
        let layout =
            EightBitDoLayout::fit(Rect::from_min_size(pos2(10.0, 20.0), vec2(700.0, 400.0)));
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
    fn hotspots_cover_every_eightbitdo_target_once() {
        let hotspots: HashSet<Button> = EIGHTBITDO_HOTSPOTS.iter().map(|h| h.button).collect();
        let targets: HashSet<Button> = ControllerKind::EightBitDoPro3
            .target_buttons()
            .iter()
            .copied()
            .collect();
        assert_eq!(hotspots, targets);
        assert_eq!(EIGHTBITDO_HOTSPOTS.len(), targets.len());
    }

    #[test]
    fn hotspots_stay_inside_viewbox() {
        for hotspot in EIGHTBITDO_HOTSPOTS {
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
        for (index, first) in EIGHTBITDO_HOTSPOTS.iter().enumerate() {
            for second in &EIGHTBITDO_HOTSPOTS[index + 1..] {
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
        for hotspot in EIGHTBITDO_HOTSPOTS {
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
        let layout = EightBitDoLayout::fit(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(320.0, 320.0 * VIEWBOX_H / VIEWBOX_W),
        ));
        let create = layout.point(CREATE_PILL.center());
        assert_eq!(
            layout.hit_test(create + vec2(4.0, 0.0)),
            Some(Button::Create)
        );
        let l4 = layout.point(LEFT_EXTRA_BUMPER.center());
        assert_eq!(
            layout.hit_test(l4 + vec2(0.0, 4.0)),
            Some(Button::EightBitDoL4)
        );
    }
}
