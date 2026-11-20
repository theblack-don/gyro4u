use egui::{
    Align2, Color32, CornerRadius, FontId, Image, Painter, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, Ui, include_image, pos2, vec2,
};

use crate::app::DS4UApp;
use crate::inputs::*;
use crate::theme::ThemeColors;
use crate::trackpad_mouse::{SteamPadHapticStrength, SteamPadMouseMode};
use crate::ui::steam_layout::*;
use crate::ui::widgets::{ds_label, ds_pill_button, ds_row, ds_section, ds_slider, ds_toggle};

impl DS4UApp {
    pub(crate) fn render_steam_inputs_section(&self, ui: &mut Ui) {
        let c = &self.theme.colors;
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("LIVE VISUALISATION")
                    .size(15.0)
                    .strong()
                    .color(c.accent())
                    .extra_letter_spacing(2.0),
            );
            ui.label(
                RichText::new("Steam Controller (2026) physical state (pre-remap)")
                    .size(12.0)
                    .italics()
                    .color(c.text_dim()),
            );
        });
        ui.add_space(20.0);

        let state = self.input.controller_state.unwrap_or_default();
        let width = ui.available_width().min(760.0).clamp(320.0, 760.0);
        ui.vertical_centered(|ui| {
            let (response, painter) =
                ui.allocate_painter(vec2(width, width * VIEWBOX_H / VIEWBOX_W), Sense::hover());
            let layout = SteamLayout::fit(response.rect);
            Image::new(include_image!("../../assets/steam_controller.svg"))
                .maintain_aspect_ratio(true)
                .paint_at(ui, layout.art_rect());

            Self::render_steam_front(&painter, &layout, &state, c);
            ui.add_space(8.0);
            Self::render_steam_rear_strip(ui, &state, c);
        });
    }

    fn render_steam_front(
        painter: &Painter,
        layout: &SteamLayout,
        state: &ControllerState,
        c: &ThemeColors,
    ) {
        let accent = c.accent();
        let white = Color32::from_white_alpha(235);
        let dim = Color32::from_white_alpha(150);
        let accent_alpha =
            |alpha: u8| Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha);

        for (bar, value, label) in [
            (LEFT_TRIGGER_BAR, state.l2, "L2"),
            (RIGHT_TRIGGER_BAR, state.r2, "R2"),
        ] {
            let rect = layout.rect(bar);
            painter.rect_filled(rect, CornerRadius::same(2), Color32::from_black_alpha(70));
            let amount = value as f32 / 255.0;
            if amount > 0.0 {
                let fill = Rect::from_min_max(
                    pos2(rect.min.x, rect.max.y - rect.height() * amount),
                    rect.max,
                );
                painter.rect_filled(fill, CornerRadius::same(2), accent_alpha(190));
            }
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, dim),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.0),
                white,
            );
        }

        for (rect, pressed) in [
            (LEFT_BUMPER, state.buttons & BTN_L1 != 0),
            (RIGHT_BUMPER, state.buttons & BTN_R1 != 0),
        ] {
            if pressed {
                let rect = layout.rect(rect);
                painter.rect_filled(rect, CornerRadius::same(4), accent_alpha(190));
            }
        }

        for (pad, pad_state, label) in [
            (LEFT_PAD, state.steam_left_pad, "LEFT PAD"),
            (RIGHT_PAD, state.steam_right_pad, "RIGHT PAD"),
        ] {
            Self::render_steam_pad(painter, layout, pad, pad_state, label, &accent_alpha);
        }

        for (button, raw, touch, label) in [
            (
                Button::L3,
                (state.left_x, state.left_y),
                state.extra_buttons & EXTRA_BTN_LEFT_STICK_TOUCH != 0,
                "L3",
            ),
            (
                Button::R3,
                (state.right_x, state.right_y),
                state.extra_buttons & EXTRA_BTN_RIGHT_STICK_TOUCH != 0,
                "R3",
            ),
        ] {
            if let Some(hotspot) = STEAM_HOTSPOTS.iter().find(|h| h.button == button) {
                let center = layout.point(hotspot.center());
                let radius = 5.2 * layout.scale();
                if steam_button_active(state, button) {
                    painter.circle_filled(center, radius, accent_alpha(120));
                }
                if touch {
                    painter.circle_stroke(center, radius + 2.5, Stroke::new(2.0_f32, accent));
                }
                painter.circle_stroke(center, radius, Stroke::new(1.2_f32, dim));
                let nx = (raw.0 as f32 - 128.0) / 128.0;
                let ny = (raw.1 as f32 - 128.0) / 128.0;
                let dot = center + vec2(nx, ny) * (radius * 0.58);
                painter.circle_filled(dot, 4.5, white);
                painter.circle_stroke(dot, 4.5, Stroke::new(1.0_f32, Color32::BLACK));
                painter.text(
                    center + vec2(0.0, radius + 2.0),
                    Align2::CENTER_TOP,
                    if touch {
                        format!("{label} touch")
                    } else {
                        label.to_string()
                    },
                    FontId::proportional(9.0),
                    dim,
                );
            }
        }

        for hotspot in STEAM_HOTSPOTS {
            if matches!(
                hotspot.button,
                Button::LeftPad
                    | Button::RightPad
                    | Button::LeftPadTouch
                    | Button::RightPadTouch
                    | Button::L2
                    | Button::R2
                    | Button::L1
                    | Button::R1
                    | Button::L3
                    | Button::R3
                    | Button::LeftStickTouch
                    | Button::RightStickTouch
            ) {
                continue;
            }
            let center = layout.point(hotspot.center());
            let radius = match hotspot.shape {
                HotspotShape::Circle { radius, .. } => radius * layout.scale(),
                HotspotShape::Pad { .. } => continue,
            };
            let active = steam_button_active(state, hotspot.button);

            match hotspot.button {
                Button::LeftTriggerClick | Button::RightTriggerClick => {
                    if active {
                        painter.circle_filled(center, 5.0, accent);
                        painter.circle_stroke(center, 5.0, Stroke::new(1.0_f32, white));
                    }
                }
                Button::Cross | Button::Circle | Button::Square | Button::Triangle => {
                    if active {
                        painter.circle_filled(center, radius, accent);
                    }
                    painter.text(
                        center,
                        Align2::CENTER_CENTER,
                        hotspot.button.device_label(ControllerKind::SteamController),
                        FontId::proportional(11.0),
                        white,
                    );
                }
                Button::Create | Button::Options | Button::Steam | Button::Qam => {
                    if active {
                        painter.circle_filled(center, radius, accent);
                    }
                    let label = hotspot.button.device_label(ControllerKind::SteamController);
                    let offset = if hotspot.button == Button::Qam {
                        vec2(0.0, radius + 4.0)
                    } else {
                        vec2(0.0, -radius - 4.0)
                    };
                    painter.text(
                        center + offset,
                        if hotspot.button == Button::Qam {
                            Align2::CENTER_TOP
                        } else {
                            Align2::CENTER_BOTTOM
                        },
                        label,
                        FontId::proportional(9.0),
                        dim,
                    );
                }
                _ => {
                    if active {
                        painter.circle_filled(center, radius, accent);
                        painter.circle_stroke(center, radius, Stroke::new(1.0_f32, white));
                    }
                }
            }
        }
    }

    fn render_steam_pad(
        painter: &Painter,
        layout: &SteamLayout,
        pad: SteamPad,
        state: SteamPadState,
        label: &str,
        accent_alpha: &impl Fn(u8) -> Color32,
    ) {
        let accent = accent_alpha(255);
        let white = Color32::from_white_alpha(225);
        let points = layout.pad_polygon(pad);

        if state.touch && state.pressure > 0 {
            let pressure = state.pressure_normalized().sqrt();
            let inset = 0.30 + 0.70 * pressure;
            let lo = 0.5 - 0.43 * inset;
            let hi = 0.5 + 0.43 * inset;
            let inner: Vec<Pos2> = [(lo, lo), (hi, lo), (hi, hi), (lo, hi)]
                .into_iter()
                .map(|(nx, ny)| layout.point(pad.point((nx, ny))))
                .collect();
            painter.add(Shape::convex_polygon(
                inner,
                accent_alpha((45.0 + 110.0 * pressure) as u8),
                Stroke::NONE,
            ));
        }

        let border = if state.click {
            Stroke::new(2.6_f32, accent)
        } else {
            Stroke::new(1.0_f32, Color32::from_white_alpha(100))
        };
        painter.add(Shape::closed_line(points, border));

        if state.touch {
            let (nx, ny) = state.normalized();
            let marker = layout.point(pad.point((nx, ny)));
            painter.circle_filled(marker, 5.5, accent);
            painter.circle_stroke(marker, 5.5, Stroke::new(1.0_f32, white));
        }

        let label_pos = layout.point((pad.center.0, pad.center.1 + pad.height * 0.5 + 2.6));
        painter.text(
            label_pos,
            Align2::CENTER_TOP,
            label,
            FontId::proportional(9.0),
            Color32::from_white_alpha(170),
        );
    }

    fn render_steam_rear_strip(ui: &mut Ui, state: &ControllerState, c: &ThemeColors) {
        let width = ui.available_width().min(760.0).clamp(320.0, 760.0);
        let height = 82.0;
        let (response, painter) = ui.allocate_painter(vec2(width, height), Sense::hover());
        let rect = response.rect;
        painter.rect_filled(rect, CornerRadius::same(8), c.extreme_bg());
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, c.widget_inactive()),
            StrokeKind::Inside,
        );
        painter.text(
            rect.min + vec2(14.0, 9.0),
            Align2::LEFT_TOP,
            "BACK BUTTONS / GRIPS",
            FontId::proportional(11.0),
            c.text_dim(),
        );

        let items = [
            (Button::L4, "L4"),
            (Button::L5, "L5"),
            (Button::R4, "R4"),
            (Button::R5, "R5"),
            (Button::LeftGrip, "GRIP L"),
            (Button::RightGrip, "GRIP R"),
        ];
        let gap = 6.0;
        let chip_w =
            ((rect.width() - 28.0 - gap * (items.len() - 1) as f32) / items.len() as f32).max(36.0);
        let chip_h = 34.0;
        let top = rect.max.y - chip_h - 10.0;
        for (index, (button, label)) in items.into_iter().enumerate() {
            let chip = Rect::from_min_size(
                pos2(rect.min.x + 14.0 + index as f32 * (chip_w + gap), top),
                vec2(chip_w, chip_h),
            );
            let pressed = steam_button_active(state, button);
            painter.rect_filled(
                chip,
                CornerRadius::same(6),
                if pressed {
                    Color32::from_rgba_unmultiplied(
                        c.accent().r(),
                        c.accent().g(),
                        c.accent().b(),
                        190,
                    )
                } else {
                    c.panel_bg()
                },
            );
            painter.rect_stroke(
                chip,
                CornerRadius::same(6),
                Stroke::new(
                    if pressed { 1.6_f32 } else { 1.0_f32 },
                    if pressed {
                        c.accent()
                    } else {
                        c.widget_inactive()
                    },
                ),
                StrokeKind::Inside,
            );
            painter.text(
                chip.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.0),
                if pressed { Color32::WHITE } else { c.text() },
            );
        }
    }

    pub(crate) fn render_steam_trackpads_section(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        let mut changed = false;
        let state = self.input.controller_state.unwrap_or_default();
        let trackpad_haptics = self
            .capabilities()
            .is_some_and(|caps| caps.trackpad_haptics);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ds_section(ui, &c, "Steam Trackpads");
                ui.horizontal(|ui| {
                    ui.add_space(crate::ui::widgets::ROW_PAD_X);
                    ui.label(
                        RichText::new(
                            "The two square pressure-sensitive trackpads are independent \
                             single-contact pads. Each can drive the mouse, scroll \
                             vertically, or stay a raw visual/touch surface.",
                        )
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                    );
                });
                if !self.using_daemon() {
                    ui.horizontal(|ui| {
                        ui.add_space(crate::ui::widgets::ROW_PAD_X);
                        ui.label(
                            RichText::new(
                                "Mouse/Scroll output and pad haptics require the gyro4u \
                                 daemon; raw state is visible either way.",
                            )
                            .size(12.0)
                            .italics()
                            .color(c.warning()),
                        );
                    });
                }

                for (title, pad, config, geometry) in [
                    (
                        "Left Pad",
                        state.steam_left_pad,
                        &mut self.touchpad.left_mouse,
                        LEFT_PAD,
                    ),
                    (
                        "Right Pad",
                        state.steam_right_pad,
                        &mut self.touchpad.right_mouse,
                        RIGHT_PAD,
                    ),
                ] {
                    ui.add_space(8.0);
                    ds_section(ui, &c, title);
                    Self::render_steam_pad_visual(ui, &c, &pad, title, geometry.rotation_deg);
                    ds_row(ui, |ui| {
                        ds_label(ui, "Mode");
                        for mode in [
                            SteamPadMouseMode::Disabled,
                            SteamPadMouseMode::Mouse,
                            SteamPadMouseMode::Scroll,
                        ] {
                            let label = match mode {
                                SteamPadMouseMode::Disabled => "Raw",
                                SteamPadMouseMode::Mouse => "Mouse",
                                SteamPadMouseMode::Scroll => "Scroll",
                            };
                            if ds_pill_button(ui, &c, label, config.mode == mode).clicked()
                                && config.mode != mode
                            {
                                config.mode = mode;
                                changed = true;
                            }
                            ui.add_space(4.0);
                        }
                    });
                    if config.mode == SteamPadMouseMode::Mouse {
                        ds_row(ui, |ui| {
                            ds_label(ui, "Sensitivity");
                            if ds_slider(ui, &c, &mut config.sensitivity, 100.0..=5000.0).changed()
                            {
                                changed = true;
                            }
                            crate::ui::widgets::ds_value_text(
                                ui,
                                &format!("{:.0}", config.sensitivity),
                            );
                        });
                        ds_row(ui, |ui| {
                            ds_label(ui, "Invert X");
                            if ds_toggle(ui, &c, &mut config.invert_x).changed() {
                                changed = true;
                            }
                            ds_label(ui, "Invert Y");
                            if ds_toggle(ui, &c, &mut config.invert_y).changed() {
                                changed = true;
                            }
                        });
                    } else if config.mode == SteamPadMouseMode::Scroll {
                        ds_row(ui, |ui| {
                            ds_label(ui, "Scroll speed");
                            if ds_slider(ui, &c, &mut config.scroll_sensitivity, 10.0..=400.0)
                                .changed()
                            {
                                changed = true;
                            }
                            crate::ui::widgets::ds_value_text(
                                ui,
                                &format!("{:.0}%", config.scroll_sensitivity),
                            );
                        });
                        ds_row(ui, |ui| {
                            ds_label(ui, "Natural scrolling");
                            if ds_toggle(ui, &c, &mut config.scroll_natural).changed() {
                                changed = true;
                            }
                        });
                    }
                    if trackpad_haptics {
                        ds_row(ui, |ui| {
                            ds_label(ui, "Haptics");
                            for strength in [
                                SteamPadHapticStrength::Off,
                                SteamPadHapticStrength::Low,
                                SteamPadHapticStrength::Medium,
                                SteamPadHapticStrength::High,
                            ] {
                                let label = match strength {
                                    SteamPadHapticStrength::Off => "Off",
                                    SteamPadHapticStrength::Low => "Low",
                                    SteamPadHapticStrength::Medium => "Medium",
                                    SteamPadHapticStrength::High => "High",
                                };
                                if ds_pill_button(ui, &c, label, config.haptic_strength == strength)
                                    .clicked()
                                    && config.haptic_strength != strength
                                {
                                    config.haptic_strength = strength;
                                    changed = true;
                                }
                                ui.add_space(4.0);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(crate::ui::widgets::ROW_PAD_X);
                        ui.label(
                            RichText::new(format!(
                                "touch {}  click {}  x {}  y {}  pressure {:.0}%",
                                pad.touch,
                                pad.click,
                                pad.x,
                                pad.y,
                                pad.pressure_normalized() * 100.0
                            ))
                            .size(12.0)
                            .monospace()
                            .color(c.text_dim()),
                        );
                    });
                }
            });

        if changed {
            self.apply_input_transform();
            self.sync_profile();
        }
    }

    fn render_steam_pad_visual(
        ui: &mut Ui,
        c: &ThemeColors,
        pad: &SteamPadState,
        label: &str,
        rotation_deg: f32,
    ) {
        let size = 220.0;
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        let painter = ui.painter();
        let center = rect.center();
        let radius = size * 0.5 - 6.0;

        let side = radius * 1.55;
        let rotation = rotation_deg.to_radians();
        let corners =
            [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)].map(|(x, y): (f32, f32)| {
                let (x, y) = (x * side, y * side);
                pos2(
                    center.x + x * rotation.cos() - y * rotation.sin(),
                    center.y + x * rotation.sin() + y * rotation.cos(),
                )
            });
        painter.add(Shape::convex_polygon(
            corners.to_vec(),
            c.extreme_bg(),
            Stroke::new(1.0_f32, Color32::from_white_alpha(120)),
        ));

        let pressure = pad.pressure_normalized();
        if pad.touch && pressure > 0.0 {
            let inset = 0.32 + 0.68 * pressure.sqrt();
            let inset_side = side * inset;
            let inset_corners =
                [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)].map(|(x, y): (f32, f32)| {
                    let (x, y) = (x * inset_side, y * inset_side);
                    pos2(
                        center.x + x * rotation.cos() - y * rotation.sin(),
                        center.y + x * rotation.sin() + y * rotation.cos(),
                    )
                });
            painter.add(Shape::convex_polygon(
                inset_corners.to_vec(),
                Color32::from_rgba_unmultiplied(90, 160, 255, 50),
                Stroke::NONE,
            ));
        }

        painter.add(Shape::closed_line(
            corners.to_vec(),
            Stroke::new(
                if pad.click { 3.0_f32 } else { 1.0_f32 },
                if pad.click {
                    c.accent()
                } else {
                    Color32::from_white_alpha(110)
                },
            ),
        ));

        let (nx, ny) = pad.normalized();
        let (local_x, local_y) = ((nx - 0.5) * side * 0.86, (ny - 0.5) * side * 0.86);
        let dot = pos2(
            center.x + local_x * rotation.cos() - local_y * rotation.sin(),
            center.y + local_x * rotation.sin() + local_y * rotation.cos(),
        );
        if pad.touch {
            painter.circle_filled(dot, 12.0, c.accent());
            painter.circle_stroke(dot, 12.0, Stroke::new(1.0_f32, Color32::WHITE));
        }
        painter.text(
            center + vec2(0.0, radius + 6.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(11.0),
            c.text_dim(),
        );
    }
}
