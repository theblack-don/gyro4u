use std::time::{Duration, Instant};

use egui::{Color32, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::app::DS4UApp;
use crate::gyro::{GyroAcceleration, GyroActivation, GyroConfig, RAW_COUNTS_PER_DPS};
use crate::inputs::{AbxyLayout, Button, ControllerKind, ControllerState};
use crate::state::GyroCalibration;
use crate::theme::ThemeColors;
use crate::ui::widgets::{
    ROW_PAD_X, ds_label, ds_pill_button, ds_row, ds_section, ds_slider, ds_slider_int, ds_toggle,
    ds_value_text,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActivationKind {
    Always,
    Hold,
    Toggle,
    Touchpad,
    LeftTrackpad,
    RightTrackpad,
    Trigger,
}

impl ActivationKind {
    const DUAL_SENSE: [Self; 5] = [
        Self::Always,
        Self::Touchpad,
        Self::Hold,
        Self::Toggle,
        Self::Trigger,
    ];
    const STEAM: [Self; 7] = [
        Self::Always,
        Self::Touchpad,
        Self::LeftTrackpad,
        Self::RightTrackpad,
        Self::Hold,
        Self::Toggle,
        Self::Trigger,
    ];
    const EIGHTBITDO: [Self; 4] = [Self::Always, Self::Hold, Self::Toggle, Self::Trigger];

    fn choices(kind: ControllerKind) -> &'static [Self] {
        match kind.family() {
            crate::inputs::ControllerFamily::Steam => &Self::STEAM,
            crate::inputs::ControllerFamily::EightBitDo => &Self::EIGHTBITDO,
            crate::inputs::ControllerFamily::DualSense => &Self::DUAL_SENSE,
        }
    }

    fn label(self, kind: ControllerKind) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::Hold => "Hold",
            Self::Toggle => "Toggle",
            Self::Touchpad => {
                if kind.is_steam() {
                    "Either Trackpad"
                } else {
                    "Touchpad"
                }
            }
            Self::LeftTrackpad => "Left Trackpad",
            Self::RightTrackpad => "Right Trackpad",
            Self::Trigger => "Trigger",
        }
    }

    fn of(activation: &GyroActivation) -> Option<Self> {
        match activation {
            GyroActivation::Always => Some(Self::Always),
            GyroActivation::Hold { .. } => Some(Self::Hold),
            GyroActivation::Toggle { .. } => Some(Self::Toggle),
            GyroActivation::Touchpad => Some(Self::Touchpad),
            GyroActivation::LeftTrackpad => Some(Self::LeftTrackpad),
            GyroActivation::RightTrackpad => Some(Self::RightTrackpad),
            GyroActivation::Trigger { .. } => Some(Self::Trigger),
            GyroActivation::AnyOf(_) => None,
        }
    }
}

fn flatten_sources(activation: &GyroActivation, out: &mut Vec<GyroActivation>) {
    match activation {
        GyroActivation::AnyOf(list) => {
            for source in list {
                flatten_sources(source, out);
            }
        }
        source => out.push(source.clone()),
    }
}

fn contains_kind(activation: &GyroActivation, kind: ActivationKind) -> bool {
    ActivationKind::of(activation) == Some(kind)
        || matches!(
            activation,
            GyroActivation::AnyOf(list) if list.iter().any(|source| contains_kind(source, kind))
        )
}

fn activation_state(s: &ControllerState, activation: &GyroActivation) -> Option<bool> {
    match activation {
        GyroActivation::Always => Some(true),
        GyroActivation::Hold { button } => Some(button.is_pressed(s)),
        GyroActivation::Toggle { .. } => None,
        GyroActivation::Touchpad => {
            Some(s.touch_count == 1 || s.steam_left_pad.touch || s.steam_right_pad.touch)
        }
        GyroActivation::LeftTrackpad => Some(s.steam_left_pad.touch),
        GyroActivation::RightTrackpad => Some(s.steam_right_pad.touch),
        GyroActivation::Trigger { button, threshold } => Some(match button {
            Button::L2 => s.l2 >= *threshold,
            Button::R2 => s.r2 >= *threshold,
            _ => false,
        }),
        GyroActivation::AnyOf(list) => {
            let mut neutral = false;
            for source in list {
                match activation_state(s, source) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => neutral = true,
                }
            }
            if neutral { None } else { Some(false) }
        }
    }
}

fn source_hint(source: &GyroActivation, kind: ControllerKind, abxy: AbxyLayout) -> String {
    match source {
        GyroActivation::Always => "the controller is connected".to_string(),
        GyroActivation::Hold { button } => {
            format!("{} is held", button.device_label_with_layout(kind, abxy))
        }
        GyroActivation::Toggle { button } => format!(
            "{} is toggled on",
            button.device_label_with_layout(kind, abxy)
        ),
        GyroActivation::Touchpad => {
            if kind.is_steam() {
                "either Steam trackpad is touched".to_string()
            } else {
                "a finger rests on the owned touchpad".to_string()
            }
        }
        GyroActivation::LeftTrackpad => "the left Steam trackpad is touched".to_string(),
        GyroActivation::RightTrackpad => "the right Steam trackpad is touched".to_string(),
        GyroActivation::Trigger { button, .. } => format!(
            "{} is at or past the threshold",
            button.device_label_with_layout(kind, abxy)
        ),
        GyroActivation::AnyOf(_) => "any source is active".to_string(),
    }
}

fn activation_hint(activation: &GyroActivation, kind: ControllerKind, abxy: AbxyLayout) -> String {
    match activation {
        GyroActivation::Always => {
            "Gyro is always active while the controller is connected.".to_string()
        }
        GyroActivation::Hold { .. } => "Gyro is active while the button is held.".to_string(),
        GyroActivation::Toggle { .. } => "Press the button to toggle gyro on or off.".to_string(),
        GyroActivation::Touchpad => {
            if kind.is_steam() {
                "Gyro is active while either Steam trackpad is touched.".to_string()
            } else {
                "Gyro is active while exactly one finger rests on an owned DualSense touchpad."
                    .to_string()
            }
        }
        GyroActivation::LeftTrackpad => {
            "Gyro is active while the left Steam trackpad is touched.".to_string()
        }
        GyroActivation::RightTrackpad => {
            "Gyro is active while the right Steam trackpad is touched.".to_string()
        }
        GyroActivation::Trigger {
            button: Button::L2, ..
        } => "Gyro is active while L2 is at or past the threshold.".to_string(),
        GyroActivation::Trigger { .. } => {
            "Gyro is active while R2 is at or past the threshold.".to_string()
        }
        GyroActivation::AnyOf(_) => {
            let mut sources = Vec::new();
            flatten_sources(activation, &mut sources);
            if sources.is_empty() {
                "No activation sources: gyro stays off. Add a source below.".to_string()
            } else {
                let parts: Vec<String> = sources
                    .iter()
                    .map(|source| source_hint(source, kind, abxy))
                    .collect();
                format!("Active while {}.", parts.join(" or "))
            }
        }
    }
}

impl DS4UApp {
    fn render_gyro_visual(
        ui: &mut Ui,
        dps: [f32; 3],
        accel: [i16; 3],
        accel_counts_per_g: f32,
        c: &ThemeColors,
    ) {
        ui.horizontal(|ui| {
            let size = 160.0;
            let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
            let p = ui.painter();
            let center = rect.center();
            let radius = size * 0.5 - 4.0;

            p.circle_filled(center, radius, c.extreme_bg());
            p.circle_stroke(center, radius, Stroke::new(1.0_f32, Color32::WHITE));
            p.line_segment(
                [
                    pos2(center.x - radius, center.y),
                    pos2(center.x + radius, center.y),
                ],
                Stroke::new(0.5_f32, c.widget_inactive()),
            );
            p.line_segment(
                [
                    pos2(center.x, center.y - radius),
                    pos2(center.x, center.y + radius),
                ],
                Stroke::new(0.5_f32, c.widget_inactive()),
            );

            let scale = if accel_counts_per_g.is_finite() && accel_counts_per_g > 0.0 {
                accel_counts_per_g
            } else {
                crate::inputs::DS_ACCEL_COUNTS_PER_G
            };
            let ax = (accel[0] as f32 / scale).clamp(-1.0, 1.0);
            let az = (accel[2] as f32 / scale).clamp(-1.0, 1.0);
            let dot = pos2(
                center.x + ax * (radius - 10.0),
                center.y + az * (radius - 10.0),
            );
            p.circle_filled(dot, 8.0, c.accent());
            p.circle_stroke(dot, 8.0, Stroke::new(1.0_f32, Color32::WHITE));

            ui.add_space(12.0);

            ui.vertical(|ui| {
                let bar_w = 240.0;
                let bar_h = 18.0;
                let labels = ["Pitch (RX)", "Yaw (RY)", "Roll (RZ)"];
                for (i, &val) in dps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(labels[i]).size(11.0).color(c.text_dim()));
                        let (bar_rect, _) =
                            ui.allocate_exact_size(vec2(bar_w, bar_h), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(bar_rect, 3.0, c.extreme_bg());
                        p.rect_stroke(
                            bar_rect,
                            3.0,
                            Stroke::new(1.0_f32, Color32::WHITE),
                            egui::StrokeKind::Outside,
                        );
                        let center_x = bar_rect.center().x;
                        p.line_segment(
                            [
                                pos2(center_x, bar_rect.min.y),
                                pos2(center_x, bar_rect.max.y),
                            ],
                            Stroke::new(0.5_f32, c.text_dim()),
                        );
                        let n = (val / 500.0).clamp(-1.0, 1.0);
                        let half = bar_rect.width() * 0.5;
                        let fill = if n >= 0.0 {
                            egui::Rect::from_min_max(
                                pos2(center_x, bar_rect.min.y + 2.0),
                                pos2(center_x + half * n, bar_rect.max.y - 2.0),
                            )
                        } else {
                            egui::Rect::from_min_max(
                                pos2(center_x + half * n, bar_rect.min.y + 2.0),
                                pos2(center_x, bar_rect.max.y - 2.0),
                            )
                        };
                        p.rect_filled(fill, 2.0, c.accent());
                        ui.label(
                            RichText::new(format!("{:+.0}", val))
                                .size(11.0)
                                .color(c.text_dim()),
                        );
                    });
                }
            });
        });
    }

    fn render_gyro_source_row(
        ui: &mut Ui,
        c: &ThemeColors,
        index: usize,
        source: &mut GyroActivation,
        choices: &[Button],
        kind: ControllerKind,
        abxy: AbxyLayout,
    ) -> (bool, bool) {
        let mut changed = false;
        let mut remove = false;

        ds_row(ui, |ui| {
            let label =
                ActivationKind::of(source).map_or("Source", |activation| activation.label(kind));
            ds_label(ui, label);
            match source {
                GyroActivation::Hold { button } | GyroActivation::Toggle { button } => {
                    let mut picked = *button;
                    egui::ComboBox::from_id_salt(("ds4u_gyro_source_button", index))
                        .selected_text(picked.device_label_with_layout(kind, abxy))
                        .show_ui(ui, |ui| {
                            for option in choices {
                                if ui
                                    .selectable_label(
                                        picked == *option,
                                        option.device_label_with_layout(kind, abxy),
                                    )
                                    .clicked()
                                {
                                    picked = *option;
                                }
                            }
                        });
                    if picked != *button {
                        *button = picked;
                        changed = true;
                    }
                }
                GyroActivation::Trigger { button, threshold } => {
                    let mut picked = if matches!(button, Button::L2 | Button::R2) {
                        *button
                    } else {
                        Button::L2
                    };
                    egui::ComboBox::from_id_salt(("ds4u_gyro_source_trigger", index))
                        .selected_text(picked.label())
                        .show_ui(ui, |ui| {
                            for option in [Button::L2, Button::R2] {
                                if ui
                                    .selectable_label(picked == option, option.label())
                                    .clicked()
                                {
                                    picked = option;
                                }
                            }
                        });
                    ui.add_space(8.0);
                    let mut threshold_value = *threshold as i32;
                    if ds_slider_int(ui, c, &mut threshold_value, 1..=255).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &threshold_value.to_string());
                    let new_threshold = threshold_value.clamp(1, 255) as u8;
                    if picked != *button || new_threshold != *threshold {
                        *button = picked;
                        *threshold = new_threshold;
                        changed = true;
                    }
                }
                GyroActivation::Always
                | GyroActivation::Touchpad
                | GyroActivation::LeftTrackpad
                | GyroActivation::RightTrackpad
                | GyroActivation::AnyOf(_) => {}
            }
            ui.add_space(8.0);
            if ds_pill_button(ui, c, "Remove", false).clicked() {
                remove = true;
            }
        });

        (changed, remove)
    }

    fn render_gyro_activation(
        &mut self,
        ui: &mut Ui,
        c: &ThemeColors,
        cfg: &mut GyroConfig,
    ) -> bool {
        let mut changed = false;
        let mut sources = Vec::new();
        flatten_sources(&cfg.activation, &mut sources);

        let kind = self.controller_kind.unwrap_or(ControllerKind::DualSense);
        let abxy = self.abxy_layout();
        let choices = kind.target_buttons();
        let mut remove = None;
        for (index, source) in sources.iter_mut().enumerate() {
            let (row_changed, row_remove) =
                Self::render_gyro_source_row(ui, c, index, source, choices, kind, abxy);
            changed |= row_changed;
            if row_remove {
                remove = Some(index);
            }
        }
        if let Some(index) = remove {
            sources.remove(index);
            changed = true;
        }

        if sources.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(ROW_PAD_X);
                ui.label(
                    RichText::new("No sources: gyro stays off until you add one.")
                        .size(12.0)
                        .italics()
                        .color(c.warning()),
                );
            });
        }

        let kind_id = egui::Id::new("ds4u_gyro_add_kind");
        let button_id = egui::Id::new("ds4u_gyro_add_button");
        let trigger_id = egui::Id::new("ds4u_gyro_add_trigger");
        let threshold_id = egui::Id::new("ds4u_gyro_add_threshold");
        let mut add_kind = ui
            .data_mut(|d| d.get_temp::<ActivationKind>(kind_id))
            .unwrap_or(ActivationKind::Hold);
        if !ActivationKind::choices(kind).contains(&add_kind) {
            add_kind = ActivationKind::Hold;
        }
        let mut add_button = ui
            .data_mut(|d| d.get_temp::<Button>(button_id))
            .unwrap_or(Button::R3);
        let mut add_trigger = ui
            .data_mut(|d| d.get_temp::<Button>(trigger_id))
            .unwrap_or(Button::L2);
        let mut add_threshold = ui
            .data_mut(|d| d.get_temp::<i32>(threshold_id))
            .unwrap_or(128);
        let mut add = false;

        ds_row(ui, |ui| {
            ds_label(ui, "Add source");
            egui::ComboBox::from_id_salt("ds4u_gyro_add_kind_pick")
                .selected_text(add_kind.label(kind))
                .show_ui(ui, |ui| {
                    for option in ActivationKind::choices(kind) {
                        if ui
                            .selectable_label(add_kind == *option, option.label(kind))
                            .clicked()
                        {
                            add_kind = *option;
                        }
                    }
                });
            if matches!(add_kind, ActivationKind::Hold | ActivationKind::Toggle) {
                ui.add_space(8.0);
                egui::ComboBox::from_id_salt("ds4u_gyro_add_button_pick")
                    .selected_text(add_button.device_label_with_layout(kind, abxy))
                    .show_ui(ui, |ui| {
                        for option in choices {
                            if ui
                                .selectable_label(
                                    add_button == *option,
                                    option.device_label_with_layout(kind, abxy),
                                )
                                .clicked()
                            {
                                add_button = *option;
                            }
                        }
                    });
            } else if add_kind == ActivationKind::Trigger {
                ui.add_space(8.0);
                egui::ComboBox::from_id_salt("ds4u_gyro_add_trigger_pick")
                    .selected_text(add_trigger.label())
                    .show_ui(ui, |ui| {
                        for option in [Button::L2, Button::R2] {
                            if ui
                                .selectable_label(add_trigger == option, option.label())
                                .clicked()
                            {
                                add_trigger = option;
                            }
                        }
                    });
            }
            ui.add_space(8.0);
            if ds_pill_button(ui, c, "Add", false).clicked() {
                add = true;
            }
        });

        if add_kind == ActivationKind::Trigger {
            ds_row(ui, |ui| {
                ds_label(ui, "Threshold");
                if ds_slider_int(ui, c, &mut add_threshold, 1..=255).changed() {
                    add_threshold = add_threshold.clamp(1, 255);
                }
                ds_value_text(ui, &add_threshold.to_string());
            });
        }

        ui.data_mut(|d| {
            d.insert_temp(kind_id, add_kind);
            d.insert_temp(button_id, add_button);
            d.insert_temp(trigger_id, add_trigger);
            d.insert_temp(threshold_id, add_threshold);
        });

        if add {
            let source = match add_kind {
                ActivationKind::Always => GyroActivation::Always,
                ActivationKind::Touchpad => GyroActivation::Touchpad,
                ActivationKind::LeftTrackpad => GyroActivation::LeftTrackpad,
                ActivationKind::RightTrackpad => GyroActivation::RightTrackpad,
                ActivationKind::Hold => GyroActivation::Hold { button: add_button },
                ActivationKind::Toggle => GyroActivation::Toggle { button: add_button },
                ActivationKind::Trigger => GyroActivation::Trigger {
                    button: if matches!(add_trigger, Button::L2 | Button::R2) {
                        add_trigger
                    } else {
                        Button::L2
                    },
                    threshold: add_threshold.clamp(1, 255) as u8,
                },
            };
            sources.push(source);
            changed = true;
        }

        let activation = match sources.len() {
            0 => GyroActivation::AnyOf(Vec::new()),
            1 => sources.pop().expect("one source"),
            _ => GyroActivation::AnyOf(sources),
        };
        if activation != cfg.activation {
            cfg.activation = activation;
            changed = true;
        }

        ui.horizontal(|ui| {
            ui.add_space(ROW_PAD_X);
            ui.label(
                RichText::new(activation_hint(&cfg.activation, kind, abxy))
                    .size(12.0)
                    .italics()
                    .color(c.text_dim()),
            );
        });

        let trackpad_source = contains_kind(&cfg.activation, ActivationKind::Touchpad)
            || contains_kind(&cfg.activation, ActivationKind::LeftTrackpad)
            || contains_kind(&cfg.activation, ActivationKind::RightTrackpad);
        let warning = if !self.using_daemon() {
            if kind.is_steam() {
                if trackpad_source {
                    Some(
                        "Steam trackpad touch is read directly. Gyro output requires the \
                         gyro4u daemon.",
                    )
                } else {
                    Some("Gyro output requires the gyro4u daemon.")
                }
            } else if trackpad_source {
                Some("Touchpad activation requires the gyro4u daemon to own the touchpad.")
            } else {
                None
            }
        } else {
            None
        };
        if let Some(warning) = warning {
            ui.horizontal(|ui| {
                ui.add_space(ROW_PAD_X);
                ui.label(
                    RichText::new(warning)
                        .size(12.0)
                        .italics()
                        .color(c.warning()),
                );
            });
        }

        changed
    }

    pub(crate) fn render_gyroscope_section(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        let mut cfg = self.gyro.mouse.clone();
        let mut changed = false;
        let state = self.input.controller_state;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ds_section(ui, &c, "Gyro to Mouse");

                ds_row(ui, |ui| {
                    ds_label(ui, "Gyro to mouse");
                    if ds_toggle(ui, &c, &mut cfg.enabled).changed() {
                        changed = true;
                    }
                });

                if !cfg.enabled {
                    ui.horizontal(|ui| {
                        ui.add_space(ROW_PAD_X);
                        ui.label(
                            RichText::new(
                                "Enable gyro to configure activation, response, and calibration.",
                            )
                            .size(12.0)
                            .italics()
                            .color(c.text_dim()),
                        );
                    });
                    return;
                }

                ds_section(ui, &c, "Activation");
                changed |= self.render_gyro_activation(ui, &c, &mut cfg);

                ds_section(ui, &c, "Status");
                ds_row(ui, |ui| {
                    ds_label(ui, "Gyro state");
                    match state {
                        None => {
                            ui.label(
                                RichText::new("Waiting for input…")
                                    .size(17.0)
                                    .color(c.warning()),
                            );
                        }
                        Some(s) => match activation_state(&s, &cfg.activation) {
                            Some(true) => {
                                ui.label(
                                    RichText::new("Active")
                                        .size(20.0)
                                        .strong()
                                        .color(c.accent()),
                                );
                            }
                            Some(false) => {
                                ui.label(
                                    RichText::new("Inactive").size(20.0).color(c.text_dim()),
                                );
                            }
                            None => {
                                ui.label(
                                    RichText::new("Toggle — state follows button presses")
                                        .size(17.0)
                                        .color(c.text_dim()),
                                );
                            }
                        },
                    }
                });

                ds_section(ui, &c, "Response");
                ds_row(ui, |ui| {
                    ds_label(ui, "Dots per 360°");
                    if ds_slider(ui, &c, &mut cfg.dots_per_360, 1000.0..=20000.0).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{}", cfg.dots_per_360.round() as i32));
                });
                ui.horizontal(|ui| {
                    ui.add_space(ROW_PAD_X);
                    ui.label(
                        RichText::new(
                            "Mouse counts your game needs for a 360° turn; 6545 is Valve's default.",
                        )
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                    );
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Gyro ° Sensitivity");
                    if ds_slider(ui, &c, &mut cfg.sensitivity, 0.1..=10.0).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{:.2}x", cfg.sensitivity));
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Output Mixer");
                    if ds_slider(ui, &c, &mut cfg.output_mixer, -1.0..=1.0).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{}%", (cfg.output_mixer * 100.0).round() as i32));
                });
                ui.horizontal(|ui| {
                    ui.add_space(ROW_PAD_X);
                    ui.label(
                        RichText::new(
                            "0% is 1:1; negative attenuates vertical and positive attenuates horizontal output.",
                        )
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                    );
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Acceleration");
                    for option in [
                        GyroAcceleration::Off,
                        GyroAcceleration::Linear,
                        GyroAcceleration::Relaxed,
                        GyroAcceleration::Aggressive,
                    ] {
                        let label = match option {
                            GyroAcceleration::Off => "Off",
                            GyroAcceleration::Linear => "Linear",
                            GyroAcceleration::Relaxed => "Relaxed",
                            GyroAcceleration::Aggressive => "Aggressive",
                        };
                        if ds_pill_button(ui, &c, label, cfg.acceleration == option).clicked() {
                            cfg.acceleration = option;
                            changed = true;
                        }
                        ui.add_space(6.0);
                    }
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Gyro Speed Deadzone");
                    if ds_slider(ui, &c, &mut cfg.speed_deadzone, 0.0..=20.0).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{:.2}°/s", cfg.speed_deadzone));
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Gyro Precision Speed");
                    if ds_slider(ui, &c, &mut cfg.precision_speed, 0.0..=50.0).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{:.2}°/s", cfg.precision_speed));
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Smoothing");
                    if ds_slider(ui, &c, &mut cfg.smoothing, 0.0..=0.95).changed() {
                        changed = true;
                    }
                    ds_value_text(ui, &format!("{}%", (cfg.smoothing * 100.0).round() as i32));
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Invert X");
                    if ds_toggle(ui, &c, &mut cfg.invert_x).changed() {
                        changed = true;
                    }
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Invert Y");
                    if ds_toggle(ui, &c, &mut cfg.invert_y).changed() {
                        changed = true;
                    }
                });

                ds_section(ui, &c, "Calibration");
                ds_row(ui, |ui| {
                    ds_label(ui, "Center bias");
                    if self.gyro.calibration.is_some() {
                        ui.label(RichText::new("Hold still…").size(17.0).color(c.accent()));
                    } else if state.is_none() {
                        ui.label(
                            RichText::new("Waiting for input…")
                                .size(17.0)
                                .color(c.warning()),
                        );
                    } else if ds_pill_button(ui, &c, "Calibrate center", false).clicked() {
                        self.gyro.calibration = Some(GyroCalibration {
                            started: Instant::now(),
                            samples: Vec::new(),
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(ROW_PAD_X);
                    let [bx, by, bz] = cfg.bias_dps;
                    ui.label(
                        RichText::new(format!(
                            "Keep the controller still for 1.5 s. Current bias: {bx:+.2}, {by:+.2}, {bz:+.2} °/s."
                        ))
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                    );
                });

                ds_section(ui, &c, "Live Readout");
                ui.add_space(8.0);
                if let Some(s) = state {
                    let dps = cfg.dps_from_state(&s);
                    ui.horizontal(|ui| {
                        ui.add_space(ROW_PAD_X);
                        Self::render_gyro_visual(
                            ui,
                            dps,
                            s.accel,
                            s.accel_counts_per_g,
                            &c,
                        );
                    });

                    let speed = (dps[0] * dps[0] + dps[1] * dps[1]).sqrt();
                    let yaw = -dps[1];
                    let pitch = -dps[0];
                    let (h, v) = cfg.mixer_factors();
                    let accel = cfg.acceleration_multiplier(speed);
                    let precision = cfg.precision_multiplier(speed);
                    let scale = (cfg.dots_per_360 / 360.0) * cfg.sensitivity;
                    let (out_x, out_y) = if speed < cfg.speed_deadzone {
                        (0.0, 0.0)
                    } else {
                        (
                            yaw * scale * accel * precision * h,
                            pitch * scale * accel * precision * v,
                        )
                    };
                    let (out_x, out_y) = if cfg.invert_x {
                        (-out_x, out_y)
                    } else {
                        (out_x, out_y)
                    };
                    let (out_x, out_y) = if cfg.invert_y {
                        (out_x, -out_y)
                    } else {
                        (out_x, out_y)
                    };

                    ui.horizontal(|ui| {
                        ui.add_space(ROW_PAD_X);
                        ui.label(
                            RichText::new(format!(
                                "pitch {:+.1}   yaw {:+.1}   roll {:+.1} °/s",
                                dps[0], dps[1], dps[2]
                            ))
                            .size(13.0)
                            .monospace()
                            .color(c.text_dim()),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(ROW_PAD_X);
                        ui.label(
                            RichText::new(format!(
                                "speed {:.1} °/s → x {out_x:+.0}, y {out_y:+.0} counts/s",
                                speed
                            ))
                            .size(13.0)
                            .monospace()
                            .color(c.text_dim()),
                        );
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.add_space(ROW_PAD_X);
                        ui.label(
                            RichText::new("Waiting for input…")
                                .size(18.0)
                                .color(c.warning()),
                        );
                    });
                }
            });

        let mut finish_calibration = false;
        let mut bias = [0.0f32; 3];
        if let Some(cal) = self.gyro.calibration.as_mut() {
            let scale = self
                .input
                .controller_state
                .map(|s| s.gyro_counts_per_dps)
                .unwrap_or(RAW_COUNTS_PER_DPS);
            if let Some(s) = self.input.controller_state {
                cal.samples.push(s.gyro);
            }
            if cal.started.elapsed() >= Duration::from_millis(1500) {
                let count = cal.samples.len().max(1) as f32;
                for sample in &cal.samples {
                    for (i, value) in sample.iter().enumerate() {
                        bias[i] += *value as f32;
                    }
                }
                for value in &mut bias {
                    *value = *value / count / scale;
                }
                finish_calibration = true;
            } else {
                ui.ctx().request_repaint();
            }
        }
        if finish_calibration {
            cfg.bias_dps = bias;
            self.gyro.calibration = None;
            changed = true;
        }

        if changed {
            self.gyro.mouse = cfg;
            self.apply_input_transform();
            self.sync_profile();
        }
    }
}
