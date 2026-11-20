//! Shared Steam Controller geometry and hit testing for the live and mapping views.
//! Coordinates mirror `assets/steam_controller.svg` (120 x 90 viewBox).

use egui::{Pos2, Rect, pos2, vec2};

use crate::inputs::{Button, ControllerState};

pub const VIEWBOX_W: f32 = 120.0;
pub const VIEWBOX_H: f32 = 90.0;

/// Small capacitive and trigger-click targets stay selectable at small sizes.
pub const MIN_HIT_RADIUS_PX: f32 = 9.0;

#[derive(Clone, Copy, Debug)]
pub struct SteamRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl SteamRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

pub const LEFT_TRIGGER_BAR: SteamRect = SteamRect::new(17.2, 0.7, 21.0, 6.6);
pub const RIGHT_TRIGGER_BAR: SteamRect = SteamRect::new(81.8, 0.7, 21.0, 6.6);
pub const LEFT_BUMPER: SteamRect = SteamRect::new(20.7, 3.9, 14.0, 3.8);
pub const RIGHT_BUMPER: SteamRect = SteamRect::new(85.3, 3.9, 14.0, 3.8);

#[derive(Clone, Copy, Debug)]
pub struct SteamPad {
    pub center: (f32, f32),
    pub width: f32,
    pub height: f32,
    pub rotation_deg: f32,
}

pub const LEFT_PAD: SteamPad = SteamPad {
    center: (37.2, 48.6),
    width: 21.2,
    height: 20.8,
    rotation_deg: -6.0,
};

pub const RIGHT_PAD: SteamPad = SteamPad {
    center: (82.8, 48.6),
    width: 21.2,
    height: 20.8,
    rotation_deg: 6.0,
};

/// Mapping-marker positions for 0.27/0.27 normalized touches, rotated with each pad.
pub const LEFT_PAD_TOUCH: (f32, f32) = (32.6, 44.9);
pub const RIGHT_PAD_TOUCH: (f32, f32) = (87.4, 44.9);

impl SteamPad {
    fn local_offset(&self, point: (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        let dx = point.0 - self.center.0;
        let dy = point.1 - self.center.1;
        (dx * cos + dy * sin, -dx * sin + dy * cos)
    }

    fn rotate(&self, local: (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        (
            self.center.0 + local.0 * cos - local.1 * sin,
            self.center.1 + local.0 * sin + local.1 * cos,
        )
    }

    pub fn contains(&self, point: (f32, f32)) -> bool {
        let local = self.local_offset(point);
        local.0.abs() <= self.width * 0.5 && local.1.abs() <= self.height * 0.5
    }

    /// Normalized pad position (0..1, 0..1) to viewBox coordinates.
    pub fn point(&self, normalized: (f32, f32)) -> (f32, f32) {
        let local = (
            (normalized.0 - 0.5) * self.width * 0.86,
            (normalized.1 - 0.5) * self.height * 0.86,
        );
        self.rotate(local)
    }

    pub fn corners(&self) -> [Pos2; 4] {
        let hw = self.width * 0.5;
        let hh = self.height * 0.5;
        [
            self.rotate((-hw, -hh)),
            self.rotate((hw, -hh)),
            self.rotate((hw, hh)),
            self.rotate((-hw, hh)),
        ]
        .map(|(x, y)| pos2(x, y))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HotspotShape {
    Circle { center: (f32, f32), radius: f32 },
    Pad { pad: SteamPad },
}

#[derive(Clone, Copy, Debug)]
pub struct SteamHotspot {
    pub button: Button,
    pub shape: HotspotShape,
    /// Higher priority wins when two hit scores are effectively tied.
    pub priority: u8,
}

const fn circle(button: Button, center: (f32, f32), radius: f32, priority: u8) -> SteamHotspot {
    SteamHotspot {
        button,
        shape: HotspotShape::Circle { center, radius },
        priority,
    }
}

pub const STEAM_HOTSPOTS: &[SteamHotspot] = &[
    circle(Button::L2, (21.6, 3.0), 2.4, 1),
    circle(Button::LeftTriggerClick, (33.4, 3.2), 1.6, 2),
    circle(Button::L1, (27.7, 6.3), 2.4, 1),
    circle(Button::R2, (98.4, 3.0), 2.4, 1),
    circle(Button::RightTriggerClick, (86.6, 3.2), 1.6, 2),
    circle(Button::R1, (92.3, 6.3), 2.4, 1),
    circle(Button::DPadUp, (26.1, 17.2), 2.0, 1),
    circle(Button::DPadRight, (30.4, 21.5), 2.0, 1),
    circle(Button::DPadDown, (26.1, 25.8), 2.0, 1),
    circle(Button::DPadLeft, (21.8, 21.5), 2.0, 1),
    circle(Button::Square, (87.2, 21.5), 2.5, 1),
    circle(Button::Circle, (98.2, 21.5), 2.5, 1),
    circle(Button::Triangle, (92.6, 15.1), 2.5, 1),
    circle(Button::Cross, (92.6, 27.2), 2.5, 1),
    circle(Button::Steam, (60.0, 19.6), 3.3, 1),
    circle(Button::Create, (44.3, 14.0), 2.1, 1),
    circle(Button::Options, (75.7, 14.0), 2.1, 1),
    circle(Button::L3, (44.3, 26.9), 5.2, 1),
    circle(Button::R3, (75.7, 26.9), 5.2, 1),
    circle(Button::LeftStickTouch, (41.3, 23.9), 1.8, 2),
    circle(Button::RightStickTouch, (72.7, 23.9), 1.8, 2),
    SteamHotspot {
        button: Button::LeftPad,
        shape: HotspotShape::Pad { pad: LEFT_PAD },
        priority: 1,
    },
    SteamHotspot {
        button: Button::RightPad,
        shape: HotspotShape::Pad { pad: RIGHT_PAD },
        priority: 1,
    },
    circle(Button::LeftPadTouch, LEFT_PAD_TOUCH, 2.4, 2),
    circle(Button::RightPadTouch, RIGHT_PAD_TOUCH, 2.4, 2),
    circle(Button::Qam, (60.0, 50.2), 2.8, 1),
];

pub const REAR_BUTTONS: [Button; 6] = [
    Button::L4,
    Button::L5,
    Button::R4,
    Button::R5,
    Button::LeftGrip,
    Button::RightGrip,
];

impl SteamHotspot {
    pub fn center(&self) -> (f32, f32) {
        match self.shape {
            HotspotShape::Circle { center, .. } => center,
            HotspotShape::Pad { pad } => pad.center,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SteamLayout {
    origin: Pos2,
    scale: f32,
}

impl SteamLayout {
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

    pub fn rect(&self, rect: SteamRect) -> Rect {
        Rect::from_min_size(
            self.point((rect.x, rect.y)),
            vec2(rect.w, rect.h) * self.scale,
        )
    }

    pub fn pad_polygon(&self, pad: SteamPad) -> Vec<Pos2> {
        pad.corners()
            .iter()
            .map(|p| self.point((p.x, p.y)))
            .collect()
    }

    /// Nearest hotspot, preferring small markers on close ties.
    pub fn hit_test(&self, point: Pos2) -> Option<Button> {
        let view = self.view_pos(point);
        let min_radius = MIN_HIT_RADIUS_PX / self.scale.max(f32::EPSILON);
        let mut best: Option<(f32, u8, f32, bool, Button)> = None;

        for hotspot in STEAM_HOTSPOTS {
            let (score, radius, within_min) = match hotspot.shape {
                HotspotShape::Circle { center, radius } => {
                    let distance =
                        ((view.0 - center.0).powi(2) + (view.1 - center.1).powi(2)).sqrt();
                    (
                        distance / radius.max(min_radius),
                        radius,
                        hotspot.priority > 1 && distance <= min_radius,
                    )
                }
                HotspotShape::Pad { pad } => {
                    if !pad.contains(view) {
                        continue;
                    }
                    let local = pad.local_offset(view);
                    let radial = ((local.0 / (pad.width * 0.5)).powi(2)
                        + (local.1 / (pad.height * 0.5)).powi(2))
                    .sqrt();
                    (1.0 + 0.2 * radial.min(1.0), pad.width * 0.5, false)
                }
            };

            let better = match best {
                None => true,
                Some((best_score, best_priority, best_radius, best_within_min, _)) => {
                    if within_min && !best_within_min {
                        true
                    } else if best_within_min && !within_min {
                        false
                    } else if (score - best_score).abs() <= 0.05 {
                        hotspot.priority > best_priority
                            || (hotspot.priority == best_priority && radius < best_radius)
                    } else {
                        score < best_score
                    }
                }
            };
            if better {
                best = Some((score, hotspot.priority, radius, within_min, hotspot.button));
            }
        }

        best.map(|(_, _, _, _, button)| button)
    }
}

/// Press state used by both live and mapping views.
pub fn steam_button_active(state: &ControllerState, button: Button) -> bool {
    match button {
        Button::LeftPad => state.steam_left_pad.click,
        Button::RightPad => state.steam_right_pad.click,
        Button::LeftPadTouch => state.steam_left_pad.touch,
        Button::RightPadTouch => state.steam_right_pad.touch,
        _ => button.is_pressed(state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::STEAM_TARGETS;

    fn layout() -> SteamLayout {
        SteamLayout::fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(760.0, 570.0)))
    }

    #[test]
    fn fit_preserves_aspect_ratio_and_centers() {
        let layout = SteamLayout::fit(Rect::from_min_size(pos2(10.0, 20.0), vec2(700.0, 400.0)));
        let art = layout.art_rect();
        assert!((art.width() / art.height() - VIEWBOX_W / VIEWBOX_H).abs() < 1e-4);
        assert!((art.center().x - 360.0).abs() < 1e-4);
        assert!((art.center().y - 220.0).abs() < 1e-4);
        assert!((layout.scale() - 400.0 / VIEWBOX_H).abs() < 1e-4);

        let top_left = layout.point((0.0, 0.0));
        let bottom_right = layout.point((VIEWBOX_W, VIEWBOX_H));
        assert!(
            ((bottom_right.x - top_left.x) / (bottom_right.y - top_left.y) - 4.0 / 3.0).abs()
                < 1e-4
        );
    }

    #[test]
    fn hotspots_stay_inside_viewbox() {
        for hotspot in STEAM_HOTSPOTS {
            match hotspot.shape {
                HotspotShape::Circle { center, radius } => {
                    assert!(center.0 - radius >= 0.0 && center.0 + radius <= VIEWBOX_W);
                    assert!(center.1 - radius >= 0.0 && center.1 + radius <= VIEWBOX_H);
                }
                HotspotShape::Pad { pad } => {
                    for corner in pad.corners() {
                        assert!(corner.x >= 0.0 && corner.x <= VIEWBOX_W);
                        assert!(corner.y >= 0.0 && corner.y <= VIEWBOX_H);
                    }
                }
            }
        }
    }

    #[test]
    fn every_steam_target_is_reachable() {
        let mut required: Vec<Button> = STEAM_TARGETS.to_vec();
        required.push(Button::LeftTriggerClick);
        required.push(Button::RightTriggerClick);

        for button in required {
            let in_hotspots = STEAM_HOTSPOTS.iter().any(|h| h.button == button);
            assert!(
                in_hotspots || REAR_BUTTONS.contains(&button),
                "{} is not reachable",
                button.label()
            );
        }
        for button in REAR_BUTTONS {
            assert!(STEAM_TARGETS.contains(&button));
        }
    }

    #[test]
    fn pad_click_and_touch_markers_hit_correctly() {
        let layout = layout();
        for (pad, touch, click, marker) in [
            (
                LEFT_PAD,
                Button::LeftPadTouch,
                Button::LeftPad,
                LEFT_PAD_TOUCH,
            ),
            (
                RIGHT_PAD,
                Button::RightPadTouch,
                Button::RightPad,
                RIGHT_PAD_TOUCH,
            ),
        ] {
            assert_eq!(layout.hit_test(layout.point(pad.center)), Some(click));
            assert_eq!(layout.hit_test(layout.point(marker)), Some(touch));
            assert!(pad.contains(marker));
        }
    }

    #[test]
    fn pad_normalized_touch_maps_inside_rotated_pad() {
        for pad in [LEFT_PAD, RIGHT_PAD] {
            let top_left = pad.point((0.0, 0.0));
            let bottom_right = pad.point((1.0, 1.0));
            let center = pad.point((0.5, 0.5));
            assert!(pad.contains(top_left));
            assert!(pad.contains(bottom_right));
            assert!((center.0 - pad.center.0).abs() < 1e-4);
            assert!((center.1 - pad.center.1).abs() < 1e-4);
        }

        let left = LEFT_PAD.point((0.27, 0.27));
        assert!((left.0 - LEFT_PAD_TOUCH.0).abs() < 0.1);
        assert!((left.1 - LEFT_PAD_TOUCH.1).abs() < 0.1);
        let right = RIGHT_PAD.point((0.73, 0.27));
        assert!((right.0 - RIGHT_PAD_TOUCH.0).abs() < 0.1);
        assert!((right.1 - RIGHT_PAD_TOUCH.1).abs() < 0.1);
    }

    #[test]
    fn small_trigger_and_capacitive_markers_resolve() {
        let layout = layout();
        assert_eq!(layout.hit_test(layout.point((27.7, 6.3))), Some(Button::L1));
        assert_eq!(layout.hit_test(layout.point((21.6, 3.0))), Some(Button::L2));
        assert_eq!(
            layout.hit_test(layout.point((33.4, 3.2))),
            Some(Button::LeftTriggerClick)
        );
        assert_eq!(
            layout.hit_test(layout.point((86.6, 3.2))),
            Some(Button::RightTriggerClick)
        );
        assert_eq!(
            layout.hit_test(layout.point((41.3, 23.9))),
            Some(Button::LeftStickTouch)
        );
        assert_eq!(
            layout.hit_test(layout.point((72.7, 23.9))),
            Some(Button::RightStickTouch)
        );
        assert_eq!(
            layout.hit_test(layout.point((44.3, 26.9))),
            Some(Button::L3)
        );
    }

    #[test]
    fn minimum_hit_radius_keeps_small_targets_selectable() {
        let layout = SteamLayout::fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 240.0)));
        let center = layout.point((41.3, 23.9));
        assert_eq!(
            layout.hit_test(center + vec2(6.0, 6.0)),
            Some(Button::LeftStickTouch)
        );
        let trigger = layout.point((33.4, 3.2));
        assert_eq!(
            layout.hit_test(trigger + vec2(8.0, 0.0)),
            Some(Button::LeftTriggerClick)
        );
        let pad = layout.point(LEFT_PAD_TOUCH);
        assert_eq!(
            layout.hit_test(pad + vec2(6.0, 6.0)),
            Some(Button::LeftPadTouch)
        );
    }

    #[test]
    fn no_relevant_visual_overlap() {
        let pads = [LEFT_PAD, RIGHT_PAD];
        for (index, a) in STEAM_HOTSPOTS.iter().enumerate() {
            for b in &STEAM_HOTSPOTS[index + 1..] {
                let HotspotShape::Circle {
                    center: ac,
                    radius: ar,
                } = a.shape
                else {
                    continue;
                };
                let HotspotShape::Circle {
                    center: bc,
                    radius: br,
                } = b.shape
                else {
                    continue;
                };
                let distance = ((ac.0 - bc.0).powi(2) + (ac.1 - bc.1).powi(2)).sqrt();
                let allowed = matches!(
                    (a.button, b.button),
                    (Button::L3, Button::LeftStickTouch)
                        | (Button::R3, Button::RightStickTouch)
                        | (Button::LeftStickTouch, Button::L3)
                        | (Button::RightStickTouch, Button::R3)
                );
                assert!(
                    allowed || distance >= ar + br,
                    "{} and {} overlap",
                    a.button.label(),
                    b.button.label()
                );
            }
        }

        for hotspot in STEAM_HOTSPOTS {
            if matches!(hotspot.button, Button::LeftPad | Button::RightPad) {
                continue;
            }
            match hotspot.button {
                Button::LeftPadTouch => {
                    assert!(LEFT_PAD.contains(hotspot.center()));
                    assert!(!RIGHT_PAD.contains(hotspot.center()));
                }
                Button::RightPadTouch => {
                    assert!(RIGHT_PAD.contains(hotspot.center()));
                    assert!(!LEFT_PAD.contains(hotspot.center()));
                }
                _ => {
                    for pad in pads {
                        assert!(!pad.contains(hotspot.center()));
                    }
                }
            }
        }
    }
}
