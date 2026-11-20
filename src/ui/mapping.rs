use std::collections::HashMap;
use std::f32::consts::TAU;

use egui::{
    Align2, FontId, Image, Pos2, Rect, RichText, Sense, Stroke, Ui, include_image, pos2, vec2,
};

use crate::app::DS4UApp;
use crate::common::TouchpadMode;
use crate::inputs::{ALL_BUTTONS, Button, GAMEPAD_TARGETS};
use crate::keycodes::KEY_TABLE;
use crate::remap::{
    Binding, BindingSlot, Key, Layer, MouseButton, PressType, ProfileAction, STICK_DIRS, WheelDir,
    active_stick_dirs, resolve_binding, top_active_layer,
};
use crate::state::{CaptureKind, CaptureRequest};
use crate::theme::ThemeColors;
use crate::ui::widgets::{
    ROW_HEIGHT, ROW_PAD_X, accent_alpha, ds_label, ds_pill_button, ds_row, ds_section, ds_slider,
    ds_slider_int, ds_toggle, ds_value_pct, ds_value_text, text_alpha,
};

include!(concat!(env!("OUT_DIR"), "/svg_coords.rs"));

const SVG_VIEWPORT: f32 = 128.0;

const GROUPS: &[(&str, &[Button])] = &[
    (
        "Face Buttons",
        &[
            Button::Cross,
            Button::Circle,
            Button::Square,
            Button::Triangle,
        ],
    ),
    (
        "D-Pad",
        &[
            Button::DPadUp,
            Button::DPadRight,
            Button::DPadDown,
            Button::DPadLeft,
        ],
    ),
    (
        "Shoulders & Triggers",
        &[Button::L1, Button::R1, Button::L2, Button::R2],
    ),
    ("Sticks", &[Button::L3, Button::R3]),
    (
        "Left Stick (Digital)",
        &[
            Button::LeftStickUp,
            Button::LeftStickDown,
            Button::LeftStickLeft,
            Button::LeftStickRight,
        ],
    ),
    (
        "Right Stick (Digital)",
        &[
            Button::RightStickUp,
            Button::RightStickDown,
            Button::RightStickLeft,
            Button::RightStickRight,
        ],
    ),
    (
        "System",
        &[
            Button::Create,
            Button::Options,
            Button::PS,
            Button::Mute,
            Button::Touchpad,
        ],
    ),
    (
        "DualSense Edge",
        &[
            Button::EdgeFnLeft,
            Button::EdgeFnRight,
            Button::EdgePaddleLeft,
            Button::EdgePaddleRight,
        ],
    ),
];

const STANDARD_HOTSPOTS: &[(Button, (f32, f32), f32)] = &[
    (Button::Square, SVG_SQUARE, 3.2),
    (Button::Cross, SVG_CROSS, 3.2),
    (Button::Circle, SVG_CIRCLE, 3.2),
    (Button::Triangle, SVG_TRIANGLE, 3.2),
    (Button::DPadUp, SVG_DPAD_T, 3.4),
    (Button::DPadRight, SVG_DPAD_R, 3.4),
    (Button::DPadDown, SVG_DPAD_B, 3.4),
    (Button::DPadLeft, SVG_DPAD_L, 3.4),
    (Button::L1, SVG_L1, 4.0),
    (Button::R1, SVG_R1, 4.0),
    (Button::L2, SVG_L2, 4.0),
    (Button::R2, SVG_R2, 4.0),
    (Button::L3, SVG_STICK_L, 5.0),
    (Button::R3, SVG_STICK_R, 5.0),
    (Button::Create, SVG_CREATE_BTN, 2.6),
    (Button::Options, SVG_OPTIONS_BTN, 2.6),
    (Button::PS, SVG_PS_BTN, 3.0),
    (Button::Mute, SVG_MIC_BTN, 2.6),
    (Button::Touchpad, SVG_TOUCHPAD, 10.0),
];

const EDGE_HOTSPOTS: &[(Button, (f32, f32), f32)] = &[
    (Button::EdgeFnLeft, SVG_EDGE_FN_L, 3.0),
    (Button::EdgeFnRight, SVG_EDGE_FN_R, 3.0),
    (Button::EdgePaddleLeft, SVG_EDGE_PADDLE_L, 3.0),
    (Button::EdgePaddleRight, SVG_EDGE_PADDLE_R, 3.0),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Gamepad,
    Key,
    Modifier,
    Mouse,
    Wheel,
    Profile,
}

impl TargetKind {
    fn label(self) -> &'static str {
        match self {
            Self::Gamepad => "Gamepad",
            Self::Key => "Key",
            Self::Modifier => "Modifier",
            Self::Mouse => "Mouse",
            Self::Wheel => "Wheel",
            Self::Profile => "Profile",
        }
    }
}

const TARGET_KINDS: [TargetKind; 6] = [
    TargetKind::Gamepad,
    TargetKind::Key,
    TargetKind::Modifier,
    TargetKind::Mouse,
    TargetKind::Wheel,
    TargetKind::Profile,
];

const MODIFIERS: [(&str, &str); 8] = [
    ("LeftCtrl", "Left Ctrl"),
    ("RightCtrl", "Right Ctrl"),
    ("LeftShift", "Left Shift"),
    ("RightShift", "Right Shift"),
    ("LeftAlt", "Left Alt"),
    ("RightAlt", "Right Alt"),
    ("LeftMeta", "Left Meta"),
    ("RightMeta", "Right Meta"),
];

const MOUSE_BUTTONS: [MouseButton; 5] = [
    MouseButton::Left,
    MouseButton::Right,
    MouseButton::Middle,
    MouseButton::Back,
    MouseButton::Forward,
];

const WHEEL_DIRS: [WheelDir; 4] = [
    WheelDir::Up,
    WheelDir::Down,
    WheelDir::Left,
    WheelDir::Right,
];

const PRESS_TYPES: [PressType; 3] = [
    PressType::Regular,
    PressType::LongPress,
    PressType::DoubleTap,
];

fn press_label(press: PressType) -> &'static str {
    match press {
        PressType::Regular => "Regular",
        PressType::LongPress => "Long Press",
        PressType::DoubleTap => "Double Tap",
    }
}

fn press_short(press: PressType) -> &'static str {
    match press {
        PressType::Regular => "Regular",
        PressType::LongPress => "Long",
        PressType::DoubleTap => "Double",
    }
}

impl DS4UApp {
    pub(crate) fn render_mapping_section(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        let mut changed = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ds_section(ui, &c, "Editing");
                changed |= self.render_editing_selector(ui, &c);

                ds_section(ui, &c, "Press Timing");
                ds_row(ui, |ui| {
                    ds_label(ui, "Long Press");
                    let mut ms = self.remap.press_timing.long_press_ms.clamp(200, 1000) as i32;
                    if ds_slider_int(ui, &c, &mut ms, 200..=1000).changed() {
                        self.remap.press_timing.long_press_ms = ms as u32;
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{ms} ms"));
                });
                ds_row(ui, |ui| {
                    ds_label(ui, "Double Tap");
                    let mut ms = self.remap.press_timing.double_tap_ms.clamp(150, 600) as i32;
                    if ds_slider_int(ui, &c, &mut ms, 150..=600).changed() {
                        self.remap.press_timing.double_tap_ms = ms as u32;
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{ms} ms"));
                });

                ds_section(ui, &c, "Controller Map");
                self.render_mapping_diagram(ui, &c);

                ui.horizontal(|ui| {
                    ui.add_space(ROW_PAD_X);
                    ui.label(
                        RichText::new(
                            "Click a control on the diagram or a row below to open the editor \
                             popup. Unbound controls pass through unchanged. Dashed markers \
                             are DualSense Edge controls (Fn L/R over the sticks, Paddle L/R \
                             on the rear grips).",
                        )
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                    );
                });

                for (title, buttons) in GROUPS {
                    ds_section(ui, &c, title);
                    for btn in *buttons {
                        let summary = self.control_summary(*btn);
                        let active = self.remap.selected == Some(*btn);
                        let row_rect = Rect::from_min_size(
                            ui.cursor().min,
                            vec2(ui.available_width(), ROW_HEIGHT),
                        );
                        let mut clicked = false;
                        ds_row(ui, |ui| {
                            ds_label(ui, btn.label());
                            clicked = ds_pill_button(ui, &c, &summary, active).clicked();
                        });
                        clicked |= ui
                            .interact(
                                row_rect,
                                ui.id().with(("mapping_row", *btn)),
                                Sense::click(),
                            )
                            .clicked();
                        if clicked {
                            self.remap.selected = Some(*btn);
                        }
                    }
                    match *title {
                        "Left Stick (Digital)" => {
                            changed |= self.render_digital_stick_controls(ui, &c, true);
                        }
                        "Right Stick (Digital)" => {
                            changed |= self.render_digital_stick_controls(ui, &c, false);
                        }
                        _ => {}
                    }
                }
            });

        if self.remap.selected.is_some() {
            let ctx = ui.ctx().clone();
            changed |= self.render_mapping_editor_popup(&ctx, &c);
        }

        if changed {
            self.apply_input_transform();
            self.sync_profile();
        }
    }

    fn render_editing_selector(&mut self, ui: &mut Ui, c: &ThemeColors) -> bool {
        let mut changed = false;
        let mut select: Option<Option<usize>> = None;
        let mut add_layer = false;
        let mut remove_layer = false;
        let mut activator_change: Option<(usize, Button)> = None;
        let can_add = self.next_free_activator().is_some();

        ds_row(ui, |ui| {
            ds_label(ui, "Layer");
            if ds_pill_button(ui, c, "Base", self.remap.selected_layer.is_none()).clicked() {
                select = Some(None);
            }
            ui.add_space(4.0);
            for (index, layer) in self.remap.layers.iter().enumerate() {
                let active = self.remap.selected_layer == Some(index);
                if ds_pill_button(ui, c, layer.activator.label(), active).clicked() {
                    select = Some(Some(index));
                }
                ui.add_space(4.0);
            }
            if ds_pill_button(ui, c, "Add layer", false).clicked() {
                add_layer = true;
            }
        });

        if !can_add {
            ui.horizontal(|ui| {
                ui.add_space(ROW_PAD_X);
                ui.label(
                    RichText::new("Every layer key is already in use.")
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                );
            });
        }

        if let Some(index) = self.remap.selected_layer
            && let Some(layer) = self.remap.layers.get(index)
        {
            let own = layer.activator;
            let used_by_others: Vec<Button> = self
                .remap
                .layers
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, layer)| layer.activator)
                .collect();
            let choices: Vec<Button> = GAMEPAD_TARGETS
                .iter()
                .copied()
                .filter(|btn| !used_by_others.contains(btn))
                .collect();

            ds_row(ui, |ui| {
                ds_label(ui, "Layer key");
                egui::ComboBox::from_id_salt("ds4u_mapping_layer_activator")
                    .selected_text(own.label())
                    .show_ui(ui, |ui| {
                        for btn in &choices {
                            if ui.selectable_label(own == *btn, btn.label()).clicked()
                                && own != *btn
                            {
                                activator_change = Some((index, *btn));
                            }
                        }
                    });
                ui.add_space(8.0);
                if ds_pill_button(ui, c, "Remove layer", false).clicked() {
                    remove_layer = true;
                }
            });
        }

        if add_layer && let Some(activator) = self.next_free_activator() {
            self.remap.layers.push(Layer {
                activator,
                bindings: HashMap::new(),
            });
            self.remap.selected_layer = Some(self.remap.layers.len() - 1);
            self.remap.capture = None;
            changed = true;
        }

        if let Some((index, activator)) = activator_change
            && self
                .remap
                .layers
                .get(index)
                .is_some_and(|layer| layer.activator != activator)
            && !self
                .remap
                .layers
                .iter()
                .enumerate()
                .any(|(other, layer)| other != index && layer.activator == activator)
        {
            self.remap.layers[index].activator = activator;
            changed = true;
        }

        if remove_layer && let Some(index) = self.remap.selected_layer {
            if index < self.remap.layers.len() {
                self.remap.layers.remove(index);
            }
            self.remap.selected_layer = None;
            self.remap.capture = None;
            changed = true;
        }

        if let Some(selection) = select {
            self.remap.selected_layer = selection;
        }

        changed
    }

    fn next_free_activator(&self) -> Option<Button> {
        let used: Vec<Button> = self.remap.layers.iter().map(|l| l.activator).collect();
        for preferred in [Button::L1, Button::R1] {
            if !used.contains(&preferred) {
                return Some(preferred);
            }
        }
        GAMEPAD_TARGETS
            .iter()
            .copied()
            .find(|btn| !used.contains(btn))
    }

    fn selected_bindings(&self) -> Option<&HashMap<Button, Vec<BindingSlot>>> {
        match self.remap.selected_layer {
            Some(index) => self.remap.layers.get(index).map(|layer| &layer.bindings),
            None => Some(&self.remap.bindings),
        }
    }

    fn selected_bindings_mut(&mut self) -> Option<&mut HashMap<Button, Vec<BindingSlot>>> {
        match self.remap.selected_layer {
            Some(index) => self
                .remap
                .layers
                .get_mut(index)
                .map(|layer| &mut layer.bindings),
            None => Some(&mut self.remap.bindings),
        }
    }

    fn control_summary(&self, btn: Button) -> String {
        let base = Self::binding_summary(self.remap.bindings.get(&btn).map(Vec::as_slice));
        let Some(index) = self.remap.selected_layer else {
            return base;
        };
        let Some(layer) = self.remap.layers.get(index) else {
            return base;
        };
        match layer.bindings.get(&btn) {
            Some(list) => Self::binding_summary(Some(list)),
            None => format!("Fallback: {base}"),
        }
    }

    fn render_mapping_diagram(&mut self, ui: &mut Ui, c: &ThemeColors) {
        let side = ui.available_width().min(760.0).clamp(320.0, 620.0);
        let stick_cfg = self.stick_config();
        let stick_mouse_enabled = self.sticks.stick_mouse.enabled;
        let controller = self.input.controller_state;
        let pressed: Vec<Button> = controller
            .map(|s| {
                let mut pressed: Vec<Button> = ALL_BUTTONS
                    .iter()
                    .copied()
                    .filter(|b| b.is_pressed(&s))
                    .collect();
                pressed.extend(
                    active_stick_dirs(&s, &stick_cfg)
                        .into_iter()
                        .filter(|btn| !(stick_mouse_enabled && btn.is_right_stick())),
                );
                pressed
            })
            .unwrap_or_default();

        ui.vertical_centered(|ui| {
            let (response, painter) = ui.allocate_painter(vec2(side, side), Sense::click());
            let canvas = response.rect;
            Image::new(include_image!("../../assets/controller_body.svg"))
                .maintain_aspect_ratio(true)
                .paint_at(ui, canvas);

            let scale = canvas.width() / SVG_VIEWPORT;
            let map = |x: f32, y: f32| -> Pos2 {
                pos2(
                    canvas.min.x + (x / SVG_VIEWPORT) * canvas.width(),
                    canvas.min.y + (y / SVG_VIEWPORT) * canvas.width(),
                )
            };

            let pointer = response.hover_pos();

            for (button, pos, r) in STANDARD_HOTSPOTS.iter().chain(EDGE_HOTSPOTS.iter()) {
                let center = map(pos.0, pos.1);
                let radius = r * scale;
                let selected = self.remap.selected == Some(*button);
                let hovered = pointer.is_some_and(|p| p.distance(center) <= radius + 3.0);
                let is_pressed = pressed.contains(button);
                let edge = EDGE_HOTSPOTS.iter().any(|(b, _, _)| b == button);

                painter.circle_filled(
                    center,
                    radius,
                    accent_alpha(
                        c,
                        if is_pressed {
                            130
                        } else if selected {
                            70
                        } else {
                            22
                        },
                    ),
                );

                if is_pressed {
                    painter.circle_filled(center, (radius * 0.33).max(1.5), c.text());
                }

                if edge {
                    let ring: Vec<Pos2> = (0..=24)
                        .map(|i| {
                            let a = i as f32 / 24.0 * TAU;
                            center + vec2(a.cos(), a.sin()) * (radius + 2.0)
                        })
                        .collect();
                    let color = if is_pressed {
                        c.accent()
                    } else {
                        accent_alpha(c, 170)
                    };
                    painter.extend(egui::Shape::dashed_line(
                        &ring,
                        Stroke::new(1.2_f32, color),
                        4.0,
                        3.0,
                    ));
                } else if is_pressed {
                    painter.circle_stroke(center, radius, Stroke::new(1.8_f32, c.accent()));
                } else {
                    painter.circle_stroke(center, radius, Stroke::new(1.2_f32, text_alpha(c, 130)));
                }

                if hovered {
                    painter.circle_stroke(
                        center,
                        radius + 2.0,
                        Stroke::new(1.2_f32, text_alpha(c, 200)),
                    );
                }
                if selected {
                    painter.circle_stroke(center, radius + 4.0, Stroke::new(2.0_f32, c.accent()));
                }
            }

            for (button, (x, y), r) in EDGE_HOTSPOTS {
                let label = match button {
                    Button::EdgeFnLeft => "Fn L",
                    Button::EdgeFnRight => "Fn R",
                    Button::EdgePaddleLeft => "Paddle L",
                    Button::EdgePaddleRight => "Paddle R",
                    _ => "",
                };
                painter.text(
                    map(*x, *y) + vec2(0.0, r * scale + 6.0),
                    Align2::CENTER_TOP,
                    label,
                    FontId::proportional((6.5 * scale).clamp(9.0, 12.0)),
                    c.text_dim(),
                );
            }

            {
                let touchpad_allowed = matches!(
                    self.touchpad.mode,
                    TouchpadMode::Disabled | TouchpadMode::GesturesOnly
                );
                let mut outputs: Vec<String> = Vec::new();
                let active_layer = controller
                    .and_then(|s| top_active_layer(&self.remap.layers, &s, touchpad_allowed))
                    .and_then(|index| self.remap.layers.get(index))
                    .map(|layer| layer.activator.label());

                if let Some(s) = controller {
                    for btn in &pressed {
                        match resolve_binding(
                            &self.remap.bindings,
                            &self.remap.layers,
                            &s,
                            *btn,
                            touchpad_allowed,
                        ) {
                            None => push_unique(&mut outputs, btn.label().to_string()),
                            Some(list) => {
                                if *btn == Button::Touchpad && !touchpad_allowed {
                                    continue;
                                }
                                for slot in list {
                                    push_unique(&mut outputs, slot_label(slot));
                                }
                            }
                        }
                    }
                }

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Physical: {}", label_list(&pressed)))
                            .size(12.0)
                            .color(c.text_dim()),
                    );
                    ui.label(RichText::new("→").size(12.0).color(c.text_dim()));
                    ui.label(
                        RichText::new(format!("Output: {}", join_labels(&outputs)))
                            .size(12.0)
                            .color(c.accent()),
                    );
                    if let Some(name) = active_layer {
                        ui.label(
                            RichText::new(format!("Layer: {name}"))
                                .size(12.0)
                                .color(c.accent()),
                        );
                    }
                });
            }

            if let Some(p) = response
                .clicked()
                .then(|| response.interact_pointer_pos())
                .flatten()
            {
                let hit = STANDARD_HOTSPOTS
                    .iter()
                    .chain(EDGE_HOTSPOTS.iter())
                    .find(|(_, pos, r)| p.distance(map(pos.0, pos.1)) <= r * scale + 3.0);
                if let Some((button, _, _)) = hit {
                    self.remap.selected = Some(*button);
                }
            }
        });
    }

    fn render_digital_stick_controls(&mut self, ui: &mut Ui, c: &ThemeColors, left: bool) -> bool {
        let mut changed = false;
        let dirs = if left {
            &STICK_DIRS[0..4]
        } else {
            &STICK_DIRS[4..8]
        };
        let preset = if left {
            ["W", "S", "A", "D"]
        } else {
            ["Up", "Down", "Left", "Right"]
        };

        ds_row(ui, |ui| {
            ds_label(ui, "Preset");
            let preset_label = if left { "WASD" } else { "Arrows" };
            if ds_pill_button(ui, c, preset_label, false).clicked() {
                if let Some(map) = self.selected_bindings_mut() {
                    for (dir, name) in dirs.iter().zip(preset) {
                        map.insert(
                            *dir,
                            vec![BindingSlot::from(Binding::Key(
                                Key::new(name).expect("preset key in table"),
                            ))],
                        );
                    }
                }
                changed = true;
            }
            ui.add_space(6.0);
            if ds_pill_button(ui, c, "Clear", false).clicked() {
                if let Some(map) = self.selected_bindings_mut() {
                    for dir in dirs {
                        map.remove(dir);
                    }
                }
                changed = true;
            }
        });

        ds_row(ui, |ui| {
            ds_label(ui, "Digital deadzone");
            let deadzone = if left {
                &mut self.sticks.digital_left_deadzone
            } else {
                &mut self.sticks.digital_right_deadzone
            };
            if ds_slider(ui, c, deadzone, 0.0..=0.9).changed() {
                changed = true;
            }
            ds_value_pct(ui, *deadzone * 100.0);
        });

        ds_row(ui, |ui| {
            ds_label(ui, "Keep analog output");
            let analog = if left {
                &mut self.sticks.digital_left_analog
            } else {
                &mut self.sticks.digital_right_analog
            };
            if ds_toggle(ui, c, analog).changed() {
                changed = true;
            }
        });

        if !left && self.sticks.stick_mouse.enabled {
            ui.horizontal(|ui| {
                ui.add_space(ROW_PAD_X);
                ui.label(
                    RichText::new("Ignored while Right Stick Mouse is enabled.")
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                );
            });
        }

        changed
    }

    fn render_mapping_editor_content(&mut self, ui: &mut Ui, c: &ThemeColors) -> bool {
        let Some(selected) = self.remap.selected else {
            return false;
        };

        let mut changed = false;
        let in_layer = self.remap.selected_layer.is_some();
        let current = self
            .selected_bindings()
            .and_then(|map| map.get(&selected))
            .cloned();
        let base_summary =
            Self::binding_summary(self.remap.bindings.get(&selected).map(Vec::as_slice));
        let summary = if in_layer && current.is_none() {
            format!("Fall back to base: {base_summary}")
        } else {
            Self::binding_summary(current.as_deref())
        };

        ds_row(ui, |ui| {
            ds_label(ui, selected.label());
            ui.label(RichText::new(summary).size(17.0).strong());
        });

        let pick_id = egui::Id::new("ds4u_mapping_target");
        let kind_id = egui::Id::new("ds4u_mapping_target_kind");
        let mouse_id = egui::Id::new("ds4u_mapping_mouse_target");
        let wheel_id = egui::Id::new("ds4u_mapping_wheel_target");
        let modifier_id = egui::Id::new("ds4u_mapping_modifier_target");
        let key_id = egui::Id::new("ds4u_mapping_key_target");
        let profile_id = egui::Id::new("ds4u_mapping_profile_target");
        let press_id = egui::Id::new("ds4u_mapping_press_target");

        let mut pick = ui.data_mut(|d| d.get_temp::<Button>(pick_id).unwrap_or(Button::Cross));
        let mut kind = ui.data_mut(|d| {
            d.get_temp::<TargetKind>(kind_id)
                .unwrap_or(TargetKind::Gamepad)
        });
        let mut press = ui.data_mut(|d| d.get_temp::<PressType>(press_id).unwrap_or_default());
        let mut mouse_pick = ui.data_mut(|d| {
            d.get_temp::<MouseButton>(mouse_id)
                .unwrap_or(MouseButton::Left)
        });
        let mut wheel_pick =
            ui.data_mut(|d| d.get_temp::<WheelDir>(wheel_id).unwrap_or(WheelDir::Up));
        let mut modifier_pick = ui.data_mut(|d| {
            d.get_temp::<usize>(modifier_id)
                .unwrap_or(0)
                .min(MODIFIERS.len() - 1)
        });
        let mut key_pick = ui.data_mut(|d| {
            d.get_temp::<usize>(key_id)
                .unwrap_or(0)
                .min(KEY_TABLE.len() - 1)
        });
        let mut profile_pick =
            ui.data_mut(|d| d.get_temp::<String>(profile_id).unwrap_or_default());
        let capturing = self
            .remap
            .capture
            .as_ref()
            .is_some_and(|capture| matches!(capture.kind, CaptureKind::Key));

        let mut add = false;
        let mut start_capture = false;
        let mut cancel_capture = false;

        ds_row(ui, |ui| {
            ds_label(ui, "Type");
            egui::ComboBox::from_id_salt("ds4u_mapping_target_kind_pick")
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for option in TARGET_KINDS {
                        if ui
                            .selectable_label(kind == option, option.label())
                            .clicked()
                        {
                            kind = option;
                        }
                    }
                });
        });

        ds_row(ui, |ui| {
            ds_label(ui, "Press");
            egui::ComboBox::from_id_salt("ds4u_mapping_press_pick")
                .selected_text(press_label(press))
                .show_ui(ui, |ui| {
                    for option in PRESS_TYPES {
                        if ui
                            .selectable_label(press == option, press_label(option))
                            .clicked()
                        {
                            press = option;
                        }
                    }
                });
        });

        let profile_names: Vec<String> = if kind == TargetKind::Profile {
            self.profile_manager
                .list_profiles()
                .into_iter()
                .map(|p| p.name)
                .collect()
        } else {
            Vec::new()
        };

        ds_row(ui, |ui| {
            ds_label(ui, "Target");
            match kind {
                TargetKind::Gamepad => {
                    egui::ComboBox::from_id_salt("ds4u_mapping_target_pick")
                        .selected_text(pick.label())
                        .show_ui(ui, |ui| {
                            for btn in GAMEPAD_TARGETS {
                                if ui.selectable_label(pick == btn, btn.label()).clicked() {
                                    pick = btn;
                                }
                            }
                        });
                    ui.add_space(8.0);
                    if ds_pill_button(ui, c, "Add binding", false).clicked() {
                        add = true;
                    }
                }
                TargetKind::Key => {
                    if capturing {
                        ui.label(
                            RichText::new("Press a key… (Esc cancels)")
                                .size(15.0)
                                .color(c.accent()),
                        );
                        ui.add_space(8.0);
                        if ds_pill_button(ui, c, "Cancel", false).clicked() {
                            cancel_capture = true;
                        }
                    } else {
                        egui::ComboBox::from_id_salt("ds4u_mapping_key_pick")
                            .selected_text(KEY_TABLE[key_pick].0)
                            .show_ui(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(240.0)
                                    .show(ui, |ui| {
                                        for (i, (name, _)) in KEY_TABLE.iter().enumerate() {
                                            if ui.selectable_label(key_pick == i, *name).clicked() {
                                                key_pick = i;
                                            }
                                        }
                                    });
                            });
                        ui.add_space(8.0);
                        if ds_pill_button(ui, c, "Add binding", false).clicked() {
                            add = true;
                        }
                        ui.add_space(6.0);
                        if ds_pill_button(ui, c, "Capture key…", false).clicked() {
                            start_capture = true;
                        }
                    }
                }
                TargetKind::Modifier => {
                    egui::ComboBox::from_id_salt("ds4u_mapping_modifier_pick")
                        .selected_text(MODIFIERS[modifier_pick].1)
                        .show_ui(ui, |ui| {
                            for (i, (_, label)) in MODIFIERS.iter().enumerate() {
                                if ui.selectable_label(modifier_pick == i, *label).clicked() {
                                    modifier_pick = i;
                                }
                            }
                        });
                    ui.add_space(8.0);
                    if ds_pill_button(ui, c, "Add binding", false).clicked() {
                        add = true;
                    }
                }
                TargetKind::Mouse => {
                    egui::ComboBox::from_id_salt("ds4u_mapping_mouse_pick")
                        .selected_text(mouse_pick.label())
                        .show_ui(ui, |ui| {
                            for option in MOUSE_BUTTONS {
                                if ui
                                    .selectable_label(mouse_pick == option, option.label())
                                    .clicked()
                                {
                                    mouse_pick = option;
                                }
                            }
                        });
                    ui.add_space(8.0);
                    if ds_pill_button(ui, c, "Add binding", false).clicked() {
                        add = true;
                    }
                }
                TargetKind::Wheel => {
                    egui::ComboBox::from_id_salt("ds4u_mapping_wheel_pick")
                        .selected_text(wheel_pick.label())
                        .show_ui(ui, |ui| {
                            for option in WHEEL_DIRS {
                                if ui
                                    .selectable_label(wheel_pick == option, option.label())
                                    .clicked()
                                {
                                    wheel_pick = option;
                                }
                            }
                        });
                    ui.add_space(8.0);
                    if ds_pill_button(ui, c, "Add binding", false).clicked() {
                        add = true;
                    }
                }
                TargetKind::Profile => {
                    let selected_text = if profile_pick.is_empty() {
                        "Cycle profiles".to_string()
                    } else {
                        profile_pick.clone()
                    };
                    egui::ComboBox::from_id_salt("ds4u_mapping_profile_pick")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(profile_pick.is_empty(), "Cycle profiles")
                                .clicked()
                            {
                                profile_pick.clear();
                            }
                            for name in &profile_names {
                                if ui.selectable_label(&profile_pick == name, name).clicked() {
                                    profile_pick = name.clone();
                                }
                            }
                        });
                    ui.add_space(8.0);
                    if ds_pill_button(ui, c, "Add binding", false).clicked() {
                        add = true;
                    }
                }
            }
        });

        if kind == TargetKind::Modifier {
            ui.horizontal(|ui| {
                ui.add_space(ROW_PAD_X);
                ui.label(
                    RichText::new("Held while the bound control is held.")
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                );
            });
        }

        ui.data_mut(|d| {
            d.insert_temp(pick_id, pick);
            d.insert_temp(kind_id, kind);
            d.insert_temp(mouse_id, mouse_pick);
            d.insert_temp(wheel_id, wheel_pick);
            d.insert_temp(modifier_id, modifier_pick);
            d.insert_temp(key_id, key_pick);
            d.insert_temp(profile_id, profile_pick.clone());
            d.insert_temp(press_id, press);
        });

        if start_capture {
            self.remap.capture = Some(CaptureRequest {
                control: selected,
                kind: CaptureKind::Key,
                layer: self.remap.selected_layer,
            });
        }
        if cancel_capture {
            self.remap.capture = None;
        }

        if add {
            let binding = match kind {
                TargetKind::Gamepad => Binding::Gamepad(pick),
                TargetKind::Modifier => {
                    let (name, _) = MODIFIERS[modifier_pick];
                    Binding::Key(Key::new(name).expect("modifier in key table"))
                }
                TargetKind::Mouse => Binding::Mouse(mouse_pick),
                TargetKind::Wheel => Binding::Wheel(wheel_pick),
                TargetKind::Key => {
                    Binding::Key(Key::new(KEY_TABLE[key_pick].0).expect("key in table"))
                }
                TargetKind::Profile => {
                    if profile_pick.is_empty() {
                        Binding::Profile(ProfileAction::Cycle)
                    } else {
                        Binding::Profile(ProfileAction::Switch(profile_pick.clone()))
                    }
                }
            };
            if let Some(map) = self.selected_bindings_mut() {
                map.entry(selected)
                    .or_default()
                    .push(BindingSlot { binding, press });
            }
            changed = true;
        }

        let list = self
            .selected_bindings()
            .and_then(|map| map.get(&selected))
            .cloned()
            .unwrap_or_default();
        let mut remove = None;
        let mut press_change = None;
        for (i, slot) in list.iter().enumerate() {
            let mut remove_clicked = false;
            let mut slot_press = slot.press;
            ds_row(ui, |ui| {
                ds_label(ui, &format!("Slot {}", i + 1));
                egui::ComboBox::from_id_salt(("ds4u_mapping_slot_press", i))
                    .width(112.0)
                    .selected_text(press_label(slot_press))
                    .show_ui(ui, |ui| {
                        for option in PRESS_TYPES {
                            if ui
                                .selectable_label(slot_press == option, press_label(option))
                                .clicked()
                            {
                                slot_press = option;
                            }
                        }
                    });
                ui.add_space(8.0);
                ui.label(RichText::new(binding_label(&slot.binding)).size(17.0));
                ui.add_space(8.0);
                if ds_pill_button(ui, c, "Remove", false).clicked() {
                    remove_clicked = true;
                }
            });
            if slot_press != slot.press {
                press_change = Some((i, slot_press));
            }
            if remove_clicked {
                remove = Some(i);
            }
        }

        if let Some((i, press)) = press_change
            && let Some(map) = self.selected_bindings_mut()
            && let Some(list) = map.get_mut(&selected)
            && let Some(slot) = list.get_mut(i)
        {
            slot.press = press;
            changed = true;
        }

        if let Some(i) = remove
            && let Some(map) = self.selected_bindings_mut()
            && let Some(list) = map.get_mut(&selected)
            && i < list.len()
        {
            list.remove(i);
            changed = true;
        }

        let mut disable = false;
        let mut use_base = false;
        let mut reset = false;
        ds_row(ui, |ui| {
            ds_label(ui, "Actions");
            let disabled = current.as_ref().is_some_and(Vec::is_empty);
            if in_layer {
                if ds_pill_button(ui, c, "Disable in layer", disabled).clicked() {
                    disable = true;
                }
                ui.add_space(6.0);
                if ds_pill_button(ui, c, "Use base", current.is_none()).clicked() {
                    use_base = true;
                }
            } else {
                if ds_pill_button(ui, c, "Disable", disabled).clicked() {
                    disable = true;
                }
                ui.add_space(6.0);
                if ds_pill_button(ui, c, "Reset to identity", current.is_none()).clicked() {
                    reset = true;
                }
            }
        });

        if disable {
            if let Some(map) = self.selected_bindings_mut() {
                map.insert(selected, Vec::new());
            }
            changed = true;
        } else if use_base {
            if let Some(map) = self.selected_bindings_mut() {
                map.remove(&selected);
            }
            changed = true;
        } else if reset {
            self.remap.bindings.remove(&selected);
            changed = true;
        }

        if selected == Button::Touchpad {
            ui.horizontal(|ui| {
                ui.add_space(ROW_PAD_X);
                ui.label(
                    RichText::new(
                        "Native pointer motion is disabled while gyro4u owns the touchpad. \
                         This click binding is applied once, when the touchpad is owned.",
                    )
                    .size(12.0)
                    .italics()
                    .color(c.text_dim()),
                );
            });
        }

        changed
    }

    fn render_mapping_editor_popup(&mut self, ctx: &egui::Context, c: &ThemeColors) -> bool {
        if self.remap.selected.is_none() {
            return false;
        }

        let mut changed = false;
        let mut close = false;
        let mut open = true;
        let max_width = (ctx.input(|i| i.content_rect().width()) - 40.0).min(760.0);
        let min_width = 560.0_f32.min(max_width);
        let default_width = 660.0_f32.min(max_width);

        egui::Window::new("Edit Mapping")
            .id(egui::Id::new("ds4u_mapping_editor"))
            .collapsible(false)
            .resizable(true)
            .default_width(default_width)
            .min_width(min_width)
            .max_width(max_width)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(560.0)
                    .show(ui, |ui| {
                        changed |= self.render_mapping_editor_content(ui, c);
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ds_pill_button(ui, c, "Close", false).clicked() {
                        close = true;
                    }
                });
            });

        if close {
            open = false;
        }
        if !open {
            self.remap.selected = None;
        }

        changed
    }

    fn binding_summary(slots: Option<&[BindingSlot]>) -> String {
        match slots {
            None => "Identity".to_string(),
            Some([]) => "Disabled".to_string(),
            Some(list) => {
                let mut labels: Vec<String> = Vec::new();
                for slot in list {
                    push_unique(&mut labels, slot_label(slot));
                }
                labels.join(" + ")
            }
        }
    }
}

fn slot_label(slot: &BindingSlot) -> String {
    format!(
        "{} → {}",
        press_short(slot.press),
        binding_label(&slot.binding)
    )
}

fn binding_label(binding: &Binding) -> String {
    match binding {
        Binding::Gamepad(btn) => btn.label().to_string(),
        Binding::Key(key) => format!("Key {}", key.label()),
        Binding::Mouse(button) => format!("Mouse {}", button.label()),
        Binding::Wheel(dir) => format!("Wheel {}", dir.label()),
        Binding::Profile(ProfileAction::Cycle) => "Profile Cycle".to_string(),
        Binding::Profile(ProfileAction::Switch(name)) => format!("Profile {}", name),
    }
}

fn push_unique(out: &mut Vec<String>, label: String) {
    if !out.contains(&label) {
        out.push(label);
    }
}

fn join_labels(labels: &[String]) -> String {
    if labels.is_empty() {
        "—".to_string()
    } else {
        labels.join(", ")
    }
}

fn label_list(buttons: &[Button]) -> String {
    if buttons.is_empty() {
        "—".to_string()
    } else {
        buttons
            .iter()
            .map(|b| b.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
