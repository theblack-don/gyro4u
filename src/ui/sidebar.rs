use egui::{Color32, CornerRadius, Id, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use std::time::Duration;

use crate::app::DS4UApp;
use crate::controller::ControllerFamily;
use crate::settings::ControllerPreference;
use crate::state::Section;
use crate::theme::ThemeColors;
use crate::ui::widgets::{accent_alpha, hovered_alpha, sep_color};

use super::widgets;

const NAV: &[(Section, &str)] = &[
    (Section::Inputs, "Inputs"),
    (Section::Mapping, "Mapping"),
    (Section::Sticks, "Sticks"),
    (Section::Triggers, "Triggers"),
    (Section::Haptics, "Haptics"),
    (Section::Gyroscope, "Motion"),
    (Section::Touchpad, "Touchpad"),
    (Section::Lightbar, "Lightbar"),
    (Section::Audio, "Audio"),
    (Section::Profiles, "Profiles"),
    (Section::Advanced, "Advanced"),
    (Section::Settings, "Settings"),
];

fn switcher_row(ui: &mut Ui, c: &ThemeColors, title: &str, detail: &str, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, CornerRadius::same(3), accent_alpha(c, 46));
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(3), hovered_alpha(c, 76));
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(2.0, c.accent()),
            StrokeKind::Inside,
        );
    }
    if active {
        painter.text(
            pos2(rect.min.x + 14.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "✓",
            egui::FontId::proportional(16.0),
            c.accent(),
        );
    }
    painter.text(
        pos2(rect.min.x + 34.0, rect.center().y - 8.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(15.0),
        c.text(),
    );
    painter.text(
        pos2(rect.min.x + 34.0, rect.center().y + 8.0),
        egui::Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(11.0),
        text_dim_alpha(c),
    );
    response.clicked()
        || (response.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space)))
}

fn text_dim_alpha(c: &ThemeColors) -> Color32 {
    crate::ui::widgets::with_alpha(c.text(), 170)
}

impl DS4UApp {
    fn render_controller_switcher(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        let current = self.current_family();
        let preference = self.settings.controller_preference;
        let pending = self.pending_controller_target;
        let switch_error = self.controller_switch_error.clone();
        let rows = self.controller_inventory.clone();

        let pending_other = pending.filter(|pending| current != Some(pending.family));
        let steam_wake_pending = pending_other.is_some_and(|pending| {
            pending.family == ControllerFamily::Steam
                && !rows
                    .iter()
                    .any(|row| row.family == ControllerFamily::Steam && row.live)
        });
        let label = current
            .map(ControllerFamily::label)
            .unwrap_or("Controllers");
        let font = egui::FontId::proportional(14.0);
        let galley = ui
            .painter()
            .layout_no_wrap(label.to_string(), font.clone(), c.text());
        let width = (galley.size().x + 60.0).clamp(120.0, 220.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
        let response = response.on_hover_text(match pending_other {
            Some(pending) => format!(
                "Switching to {} - click for controller options",
                pending.family.label()
            ),
            None => "Switch controller".to_string(),
        });
        let stale =
            pending_other.is_some_and(|pending| pending.since.elapsed() >= Duration::from_secs(5));
        let stroke = if stale || !switch_error.is_empty() {
            c.warning()
        } else if pending_other.is_some() {
            c.accent()
        } else if response.hovered() {
            accent_alpha(&c, 180)
        } else {
            sep_color(&c)
        };
        let fill = if response.hovered() {
            hovered_alpha(&c, 90)
        } else {
            Color32::TRANSPARENT
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(3), fill);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.2, stroke),
            StrokeKind::Inside,
        );
        painter.text(
            pos2(rect.min.x + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font,
            c.text_dim(),
        );
        let chevron = pos2(rect.max.x - 13.0, rect.center().y);
        painter.line_segment(
            [chevron + vec2(-4.0, -2.0), chevron + vec2(0.0, 2.0)],
            Stroke::new(1.5, c.text_dim()),
        );
        painter.line_segment(
            [chevron + vec2(0.0, 2.0), chevron + vec2(4.0, -2.0)],
            Stroke::new(1.5, c.text_dim()),
        );
        if pending_other.is_some() {
            painter.circle_filled(
                pos2(rect.max.x - 29.0, rect.center().y),
                3.0,
                if stale { c.warning() } else { c.accent() },
            );
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect,
                CornerRadius::same(3),
                Stroke::new(2.0, c.accent()),
                StrokeKind::Outside,
            );
        }

        let popup_id = Id::new("gyro4u_controller_switcher_popup");
        if response.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space))
        {
            egui::Popup::toggle_id(ui.ctx(), popup_id);
        }

        let mut switch_to: Option<ControllerPreference> = None;

        egui::Popup::menu(&response)
            .id(popup_id)
            .width(300.0)
            .frame(
                egui::Frame::popup(ui.style())
                    .fill(c.panel_bg())
                    .stroke(Stroke::new(1.0, sep_color(&c)))
                    .inner_margin(8.0),
            )
            .show(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
                ui.set_min_width(284.0);

                let active_row = current.and_then(|family| {
                    rows.iter()
                        .find(|row| row.family == family)
                        .map(|row| (row, family))
                });
                if let Some((row, family)) = active_row {
                    let detail = row.transport.as_deref().unwrap_or("connected");
                    let _ = switcher_row(ui, &c, family.label(), detail, true);
                } else if let Some(family) = current {
                    let _ = switcher_row(ui, &c, family.label(), "connected", true);
                }

                let mut other_count = 0;
                for row in rows.iter().filter(|row| Some(row.family) != current) {
                    other_count += 1;
                    let (title, detail) =
                        if row.family == ControllerFamily::Steam && row.receiver_only {
                            (
                                "Steam receiver".to_string(),
                                "controller not detected — click to connect".to_string(),
                            )
                        } else if row.family == ControllerFamily::Steam {
                            let transport = row
                                .transport
                                .clone()
                                .unwrap_or_else(|| "detected".to_string());
                            (
                                "Steam Controller".to_string(),
                                format!("{transport} - click to switch"),
                            )
                        } else {
                            let transport = row
                                .transport
                                .clone()
                                .unwrap_or_else(|| "detected".to_string());
                            (row.family.label().to_string(), transport)
                        };
                    if switcher_row(ui, &c, &title, &detail, false) {
                        switch_to = Some(row.family.preference());
                    }
                }
                if other_count == 0 {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("No other controllers detected")
                            .size(12.0)
                            .color(text_dim_alpha(&c)),
                    );
                }

                ui.add_space(4.0);
                let (sep_rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(sep_rect, 0.0, sep_color(&c));
                ui.add_space(4.0);

                if switcher_row(
                    ui,
                    &c,
                    "Automatic",
                    "DualSense preferred; Steam then 8BitDo",
                    preference == ControllerPreference::Auto,
                ) {
                    switch_to = Some(ControllerPreference::Auto);
                }

                if let Some(pending) = pending_other {
                    ui.add_space(4.0);
                    let (message, color) = if steam_wake_pending {
                        (crate::app::STEAM_WAKE_RETRY.to_string(), c.warning())
                    } else if pending.since.elapsed() < Duration::from_secs(5) {
                        (
                            format!("Switching to {}...", pending.family.label()),
                            c.accent(),
                        )
                    } else {
                        (
                            format!("{} not detected - retrying", pending.family.label()),
                            c.warning(),
                        )
                    };
                    ui.label(RichText::new(message).size(11.0).color(color));
                }
                if !switch_error.is_empty()
                    && !pending_other
                        .is_some_and(|pending| pending.since.elapsed() >= Duration::from_secs(5))
                {
                    ui.label(RichText::new(&switch_error).size(11.0).color(c.error()));
                }
            });

        if let Some(preference) = switch_to {
            self.set_controller_preference(preference);
            if let Some(ctx) = &self.egui_ctx {
                ctx.request_repaint();
            }
        }
    }

    pub(crate) fn section_visible(&self, section: Section) -> bool {
        if !self.is_connected() {
            return section == Section::Settings;
        }
        if self.is_steam() {
            return !matches!(
                section,
                Section::Triggers
                    | Section::Haptics
                    | Section::Lightbar
                    | Section::Audio
                    | Section::Advanced
            );
        }
        if self.is_eightbitdo() {
            return !matches!(
                section,
                Section::Triggers
                    | Section::Touchpad
                    | Section::Lightbar
                    | Section::Audio
                    | Section::Advanced
            );
        }
        true
    }

    pub(crate) fn cycle_section(&mut self, dir: i32) {
        let visible: Vec<Section> = NAV
            .iter()
            .filter(|(s, _)| self.section_visible(*s))
            .map(|(s, _)| *s)
            .collect();
        if visible.len() < 2 {
            return;
        }
        let len = visible.len() as i32;
        let cur = visible
            .iter()
            .position(|s| *s == self.active_section)
            .unwrap_or(0) as i32;
        let next = ((cur + dir) % len + len) % len;
        self.active_section = visible[next as usize];
    }

    fn render_nav_item(&mut self, ui: &mut Ui, label: &str, section: Section) {
        let active = self.active_section == section;
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 50.0), Sense::click());
        let c = &self.theme.colors;
        let p = ui.painter();

        if active {
            p.rect_filled(rect, 0.0, widgets::accent_alpha(c, 84));
            p.rect_filled(
                Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                0.0,
                c.accent(),
            );
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, widgets::hovered_alpha(c, 32));
        }

        p.rect_filled(
            Rect::from_min_size(pos2(rect.min.x, rect.max.y - 1.0), vec2(rect.width(), 1.0)),
            0.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 10),
        );

        let (color, weight) = if active {
            (Color32::WHITE, true)
        } else if resp.hovered() {
            (Color32::from_rgba_unmultiplied(255, 255, 255, 178), false)
        } else {
            (Color32::from_rgba_unmultiplied(255, 255, 255, 102), false)
        };
        let mut rt = RichText::new(label).size(20.0).color(color);
        if weight {
            rt = rt.strong();
        }
        p.text(
            pos2(rect.min.x + 20.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(20.0),
            color,
        );
        let _ = rt;

        if resp.clicked() {
            self.active_section = section;
        }
    }

    pub(crate) fn render_statusbar(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        ui.horizontal_centered(|ui| {
            let prof = self
                .current_profile
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Default".into());
            let max_title = (ui.available_width() - 620.0).max(80.0);
            ui.add_sized(
                vec2(max_title, 18.0),
                egui::Label::new(
                    RichText::new(format!("Gyro4U // {}", prof))
                        .size(15.0)
                        .color(Color32::from_rgba_unmultiplied(255, 255, 255, 115)),
                )
                .truncate(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(bat) = self.battery_info.clone() {
                    let label = match bat.status.as_str() {
                        "Charging" => format!("{}% CHG", bat.capacity),
                        "Full" => format!("{}% FULL", bat.capacity),
                        _ => format!("{}%", bat.capacity),
                    };
                    ui.label(
                        RichText::new(label)
                            .size(15.0)
                            .color(Color32::from_rgba_unmultiplied(255, 255, 255, 153)),
                    );
                    let (r, _) = ui.allocate_exact_size(vec2(32.0, 14.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_stroke(
                        r,
                        CornerRadius::same(2),
                        Stroke::new(1.3, Color32::from_rgba_unmultiplied(255, 255, 255, 102)),
                        StrokeKind::Inside,
                    );
                    let fill_w = (bat.capacity as f32 / 100.0).clamp(0.05, 1.0) * 24.0;
                    p.rect_filled(
                        Rect::from_min_size(r.min + vec2(3.0, 2.5), vec2(fill_w, 9.0)),
                        CornerRadius::same(1),
                        c.accent(),
                    );
                    p.rect_filled(
                        Rect::from_min_size(pos2(r.max.x, r.min.y + 4.0), vec2(3.0, 6.0)),
                        CornerRadius::same(1),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 80),
                    );
                    ui.add_space(26.0);
                }
                let connected = self.is_connected();
                let via_daemon = self.using_daemon();
                ui.label(
                    RichText::new(if connected {
                        "Connected"
                    } else {
                        "Searching..."
                    })
                    .size(15.0)
                    .color(Color32::from_rgba_unmultiplied(255, 255, 255, 153)),
                );
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                let col = if connected { c.success() } else { c.warning() };
                ui.painter().circle_filled(dot.center(), 4.0, col);
                if connected && !via_daemon {
                    ui.add_space(8.0);
                    let label = "DIRECT";
                    let pad = vec2(8.0, 3.0);
                    let galley = ui.painter().layout_no_wrap(
                        label.into(),
                        egui::FontId::proportional(11.0),
                        c.warning(),
                    );
                    let size = galley.size() + pad * 2.0;
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    let p = ui.painter();
                    p.rect_stroke(
                        rect,
                        CornerRadius::same(2),
                        Stroke::new(1.0, c.warning()),
                        StrokeKind::Inside,
                    );
                    p.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(11.0),
                        c.warning(),
                    );
                }
                ui.add_space(12.0);
                self.render_controller_switcher(ui);
            });
        });
    }

    pub(crate) fn render_header(&mut self, ui: &mut Ui) {
        let title = match self.active_section {
            Section::Inputs => "Controller Inputs",
            Section::Mapping => "Button Mapping",
            Section::Sticks => "Analog Sticks",
            Section::Triggers => "Adaptive Triggers",
            Section::Haptics => "Haptics & Vibration",
            Section::Gyroscope => "Motion / Gyro",
            Section::Touchpad => {
                if self.is_steam() {
                    "Trackpads"
                } else {
                    "Touchpad"
                }
            }
            Section::Lightbar => "Lightbar & LEDs",
            Section::Audio => "Audio",
            Section::Profiles => "Profiles",
            Section::Advanced => "Advanced",
            Section::Settings => "Settings",
        };
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new(title).size(22.0).strong());
        });
    }

    fn render_title(&mut self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
        let c = &self.theme.colors;
        let p = ui.painter();
        p.rect_filled(
            Rect::from_min_size(pos2(rect.min.x, rect.max.y - 1.0), vec2(rect.width(), 1.0)),
            0.0,
            sep_color(c),
        );
        p.text(
            pos2(rect.min.x + 20.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Gyro4U",
            egui::FontId::proportional(20.0),
            c.accent(),
        );
    }

    pub(crate) fn render_sidebar(&mut self, ui: &mut Ui) {
        self.render_title(ui);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (sec, label) in NAV {
                    if !self.section_visible(*sec) {
                        continue;
                    }
                    self.render_nav_item(ui, label, *sec);
                }

                ui.add_space(32.0);

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(12.0);

                    if !self.error_message.is_empty() {
                        ui.label(
                            RichText::new(&self.error_message)
                                .size(11.0)
                                .color(self.theme.colors.error()),
                        );
                    }

                    if !self.status_message.is_empty() {
                        ui.label(
                            RichText::new(&self.status_message)
                                .size(11.0)
                                .color(self.theme.colors.success()),
                        );
                    }
                    ui.add_space(8.0);
                });
            });
        let _ = accent_alpha;
    }
}
