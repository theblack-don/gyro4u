//! Shared GameSir Tarantula Pro geometry and hit testing for the live and
//! mapping views. Coordinates mirror `assets/tarantula_pro.svg` (160 x 112).

use egui::{Pos2, Rect, vec2};

use crate::inputs::Button;

pub const VIEWBOX_W: f32 = 160.0;
pub const VIEWBOX_H: f32 = 112.0;

/// Small system buttons stay selectable when the diagram is narrow.
pub const MIN_HIT_RADIUS_PX: f32 = 9.0;

#[derive(Clone, Copy, Debug)]
pub struct TarantulaRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl TarantulaRect {
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

pub const LEFT_TRIGGER_BAR: TarantulaRect = TarantulaRect::new(26.0, 1.0, 27.0, 7.0);
pub const RIGHT_TRIGGER_BAR: TarantulaRect = TarantulaRect::new(107.0, 1.0, 27.0, 7.0);
pub const LEFT_BUMPER: TarantulaRect = TarantulaRect::new(26.0, 9.0, 27.0, 11.0);
pub const RIGHT_BUMPER: TarantulaRect = TarantulaRect::new(107.0, 9.0, 27.0, 11.0);

pub const DPAD_UP: (f32, f32) = (45.0, 32.0);
pub const DPAD_RIGHT: (f32, f32) = (55.0, 42.0);
pub const DPAD_DOWN: (f32, f32) = (45.0, 52.0);
pub const DPAD_LEFT: (f32, f32) = (35.0, 42.0);
pub const DPAD_RADIUS: f32 = 4.2;
pub const DPAD_HIT_RADIUS: f32 = 4.2;

pub const LEFT_STICK: (f32, f32) = (44.0, 78.0);
pub const RIGHT_STICK: (f32, f32) = (116.0, 78.0);
pub const STICK_RADIUS: f32 = 10.8;
pub const STICK_HIT_RADIUS: f32 = 10.8;

pub const FACE_RADIUS: f32 = 4.6;
pub const FACE_HIT_RADIUS: f32 = 4.4;
pub const FACE_Y: (f32, f32) = (117.0, 31.0);
pub const FACE_X: (f32, f32) = (106.0, 42.0);
pub const FACE_B: (f32, f32) = (128.0, 42.0);
pub const FACE_A: (f32, f32) = (117.0, 53.0);

pub const T_BUTTON_RADIUS: f32 = 3.0;
pub const T1_CENTER: (f32, f32) = (70.0, 25.0);
pub const T2_CENTER: (f32, f32) = (80.0, 21.0);
pub const T3_CENTER: (f32, f32) = (90.0, 25.0);

pub const C1_CHIP: TarantulaRect = TarantulaRect::new(63.0, 38.0, 8.0, 6.0);
pub const C2_CHIP: TarantulaRect = TarantulaRect::new(72.0, 38.0, 8.0, 6.0);
pub const C3_CHIP: TarantulaRect = TarantulaRect::new(82.0, 38.0, 8.0, 6.0);
pub const C4_CHIP: TarantulaRect = TarantulaRect::new(91.0, 38.0, 8.0, 6.0);

pub const HOME_CENTER: (f32, f32) = (80.0, 51.0);
pub const HOME_RADIUS: f32 = 4.2;
pub const VIEW_PILL: TarantulaRect = TarantulaRect::new(57.0, 56.0, 11.0, 6.0);
pub const MENU_PILL: TarantulaRect = TarantulaRect::new(92.0, 56.0, 11.0, 6.0);
pub const SHARE_PILL: TarantulaRect = TarantulaRect::new(74.0, 58.0, 12.0, 6.0);

pub const LEFT_REAR_CHIP: TarantulaRect = TarantulaRect::new(16.0, 90.0, 14.0, 10.0);
pub const RIGHT_REAR_CHIP: TarantulaRect = TarantulaRect::new(130.0, 90.0, 14.0, 10.0);

#[derive(Clone, Copy, Debug)]
pub enum TarantulaShape {
    Circle { center: (f32, f32), radius: f32 },
    Rect(TarantulaRect),
}

impl TarantulaShape {
    #[cfg(test)]
    pub fn center(&self) -> (f32, f32) {
        match self {
            Self::Circle { center, .. } => *center,
            Self::Rect(rect) => rect.center(),
        }
    }

    #[cfg(test)]
    fn bounds(&self) -> TarantulaRect {
        match self {
            Self::Circle { center, radius } => TarantulaRect::new(
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
pub struct TarantulaHotspot {
    pub button: Button,
    pub shape: TarantulaShape,
}

const fn circle(button: Button, center: (f32, f32), radius: f32) -> TarantulaHotspot {
    TarantulaHotspot {
        button,
        shape: TarantulaShape::Circle { center, radius },
    }
}

const fn rect(button: Button, rect: TarantulaRect) -> TarantulaHotspot {
    TarantulaHotspot {
        button,
        shape: TarantulaShape::Rect(rect),
    }
}

pub const TARANTULA_HOTSPOTS: &[TarantulaHotspot] = &[
    rect(Button::L2, LEFT_TRIGGER_BAR),
    rect(Button::R2, RIGHT_TRIGGER_BAR),
    rect(Button::L1, LEFT_BUMPER),
    rect(Button::R1, RIGHT_BUMPER),
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
    circle(Button::GameSirT1, T1_CENTER, T_BUTTON_RADIUS),
    circle(Button::GameSirT2, T2_CENTER, T_BUTTON_RADIUS),
    circle(Button::GameSirT3, T3_CENTER, T_BUTTON_RADIUS),
    rect(Button::GameSirC1, C1_CHIP),
    rect(Button::GameSirC2, C2_CHIP),
    rect(Button::GameSirC3, C3_CHIP),
    rect(Button::GameSirC4, C4_CHIP),
    circle(Button::PS, HOME_CENTER, HOME_RADIUS),
    rect(Button::Create, VIEW_PILL),
    rect(Button::Options, MENU_PILL),
    rect(Button::GameSirShare, SHARE_PILL),
    rect(Button::GameSirL4, LEFT_REAR_CHIP),
    rect(Button::GameSirR4, RIGHT_REAR_CHIP),
];

#[derive(Clone, Copy, Debug)]
pub struct TarantulaLayout {
    origin: Pos2,
    scale: f32,
}

impl TarantulaLayout {
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

    pub fn rect(&self, rect: TarantulaRect) -> Rect {
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

        for hotspot in TARANTULA_HOTSPOTS {
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

    const SVG: &str = include_str!("../../assets/tarantula_pro.svg");

    #[test]
    fn svg_art_matches_layout_constants() {
        assert_viewbox(SVG, VIEWBOX_W, VIEWBOX_H);
        assert_no_text(SVG);

        for (id, rect) in [
            ("tarantula-l2", LEFT_TRIGGER_BAR),
            ("tarantula-r2", RIGHT_TRIGGER_BAR),
            ("tarantula-c1", C1_CHIP),
            ("tarantula-c2", C2_CHIP),
            ("tarantula-c3", C3_CHIP),
            ("tarantula-c4", C4_CHIP),
            ("tarantula-view", VIEW_PILL),
            ("tarantula-menu", MENU_PILL),
            ("tarantula-share", SHARE_PILL),
            ("tarantula-l4", LEFT_REAR_CHIP),
            ("tarantula-r4", RIGHT_REAR_CHIP),
        ] {
            assert_rect(SVG, id, rect.x, rect.y, rect.w, rect.h);
        }

        for (id, rect) in [
            ("tarantula-l1", LEFT_BUMPER),
            ("tarantula-r1", RIGHT_BUMPER),
        ] {
            assert_path_bounds(SVG, id, rect.x, rect.y, rect.w, rect.h);
        }

        for (id, (cx, cy), radius) in [
            ("tarantula-stick-l", LEFT_STICK, STICK_RADIUS),
            ("tarantula-stick-r", RIGHT_STICK, STICK_RADIUS),
            ("tarantula-face-y", FACE_Y, FACE_RADIUS),
            ("tarantula-face-x", FACE_X, FACE_RADIUS),
            ("tarantula-face-b", FACE_B, FACE_RADIUS),
            ("tarantula-face-a", FACE_A, FACE_RADIUS),
            ("tarantula-t1", T1_CENTER, T_BUTTON_RADIUS),
            ("tarantula-t2", T2_CENTER, T_BUTTON_RADIUS),
            ("tarantula-t3", T3_CENTER, T_BUTTON_RADIUS),
            ("tarantula-home", HOME_CENTER, HOME_RADIUS),
        ] {
            assert_circle(SVG, id, cx, cy, radius);
        }
    }

    fn layout() -> TarantulaLayout {
        TarantulaLayout::fit(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(760.0, 760.0 * VIEWBOX_H / VIEWBOX_W),
        ))
    }

    #[test]
    fn fit_preserves_aspect_ratio_and_centers_in_non_square_canvas() {
        let layout =
            TarantulaLayout::fit(Rect::from_min_size(pos2(10.0, 20.0), vec2(700.0, 400.0)));
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
    fn hotspots_cover_every_tarantula_target_once() {
        let hotspots: HashSet<Button> = TARANTULA_HOTSPOTS.iter().map(|h| h.button).collect();
        let targets: HashSet<Button> = ControllerKind::GameSirTarantulaPro
            .target_buttons()
            .iter()
            .copied()
            .collect();
        assert_eq!(hotspots, targets);
        assert_eq!(TARANTULA_HOTSPOTS.len(), targets.len());
        assert_eq!(targets.len(), 27);
    }

    #[test]
    fn hotspots_stay_inside_viewbox() {
        for hotspot in TARANTULA_HOTSPOTS {
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
        for (index, first) in TARANTULA_HOTSPOTS.iter().enumerate() {
            for second in &TARANTULA_HOTSPOTS[index + 1..] {
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
        for hotspot in TARANTULA_HOTSPOTS {
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
        let layout = TarantulaLayout::fit(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(320.0, 320.0 * VIEWBOX_H / VIEWBOX_W),
        ));
        let t1 = layout.point(T1_CENTER);
        assert_eq!(
            layout.hit_test(t1 + vec2(4.0, 0.0)),
            Some(Button::GameSirT1)
        );
        let l4 = layout.point(LEFT_REAR_CHIP.center());
        assert_eq!(
            layout.hit_test(l4 + vec2(4.0, 0.0)),
            Some(Button::GameSirL4)
        );
    }
}
