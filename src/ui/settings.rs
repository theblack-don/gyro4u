use egui::{
    Align, Color32, Direction, Frame, Image, Label, Layout, Margin, RichText, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, include_image, vec2,
};

use crate::app::DS4UApp;
use crate::ui::widgets::{ROW_PAD_X, ds_label, ds_pill_button, ds_row, ds_section, ds_toggle};

const GALLERY_CARD_SIZE: Vec2 = vec2(224.0, 184.0);
const GALLERY_GAP: f32 = 12.0;
const GALLERY_PAD: Vec2 = vec2(14.0, 10.0);
const GALLERY_HEADER_H: f32 = 36.0;
const GALLERY_HEADER_GAP: f32 = 6.0;
const GALLERY_RADIUS: f32 = 4.0;

fn gallery_cards() -> [(&'static str, egui::ImageSource<'static>); 6] {
    [
        (
            "DualSense / DualSense Edge",
            include_image!("../../assets/controller.svg"),
        ),
        (
            "Steam Controller (2026)",
            include_image!("../../assets/steam_controller.svg"),
        ),
        (
            "8BitDo Pro 3",
            include_image!("../../assets/eightbitdo_pro3.svg"),
        ),
        ("Flydigi Apex 5", include_image!("../../assets/apex5.svg")),
        (
            "Flydigi Vader 5 Pro",
            include_image!("../../assets/vader5_pro.svg"),
        ),
        (
            "GameSir Tarantula Pro",
            include_image!("../../assets/GAMESIR_Controller_White_Line_Art.svg"),
        ),
    ]
}

fn render_supported_controllers(ui: &mut Ui, c: &crate::theme::ThemeColors) {
    Frame::NONE
        .inner_margin(Margin::symmetric(ROW_PAD_X as i8, 0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(GALLERY_GAP, GALLERY_GAP);
                for (name, image) in gallery_cards() {
                    let (card, _) = ui.allocate_exact_size(GALLERY_CARD_SIZE, Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(card, GALLERY_RADIUS, c.panel_bg());
                    painter.rect_stroke(
                        card,
                        GALLERY_RADIUS,
                        Stroke::new(1.0, crate::ui::widgets::sep_color(c)),
                        StrokeKind::Inside,
                    );

                    let inner = card.shrink2(GALLERY_PAD);
                    let art = vec2(
                        inner.width(),
                        inner.height() - GALLERY_HEADER_H - GALLERY_HEADER_GAP,
                    );
                    let mut card_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(inner)
                            .layout(Layout::top_down(Align::Center)),
                    );
                    card_ui.spacing_mut().item_spacing.y = 0.0;
                    card_ui.allocate_ui_with_layout(
                        vec2(inner.width(), GALLERY_HEADER_H),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.add(
                                Label::new(RichText::new(name).size(14.0).strong().color(c.text()))
                                    .wrap()
                                    .halign(Align::Center)
                                    .selectable(false),
                            );
                        },
                    );
                    card_ui.add_space(GALLERY_HEADER_GAP);
                    card_ui.allocate_ui_with_layout(
                        art,
                        Layout::centered_and_justified(Direction::TopDown),
                        |ui| {
                            ui.add(
                                Image::new(image)
                                    .max_size(art)
                                    .maintain_aspect_ratio(true)
                                    .tint(c.text()),
                            );
                        },
                    );
                }
            });
        });
}

impl DS4UApp {
    pub(crate) fn render_settings_section(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ds_section(ui, &c, "Supported controllers");
                render_supported_controllers(ui, &c);
                ui.add_space(4.0);

                ds_section(ui, &c, "Theme");
                ds_row(ui, |ui| {
                    ds_label(ui, "Active");
                    ui.label(
                        RichText::new(&self.theme.name)
                            .size(20.0)
                            .strong()
                            .color(c.accent()),
                    );
                    ui.add_space(12.0);
                    if ds_pill_button(ui, &c, "Refresh", false).clicked() {
                        self.theme_manager.reload();
                        self.theme = self.theme_manager.load_by_id(&self.settings.theme_id);
                    }
                });

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(ROW_PAD_X);
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    let themes = self.theme_manager.list_all().to_vec();
                    for t in themes {
                        let selected = t.id == self.theme.id;
                        let tc = t.colors.clone();

                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(180.0, 90.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(
                            rect,
                            4.0,
                            Color32::from_rgb(tc.panel_bg[0], tc.panel_bg[1], tc.panel_bg[2]),
                        );
                        let stroke_col = if selected {
                            c.accent()
                        } else if resp.hovered() {
                            crate::ui::widgets::accent_alpha(&c, 180)
                        } else {
                            crate::ui::widgets::sep_color(&c)
                        };
                        p.rect_stroke(
                            rect,
                            4.0,
                            Stroke::new(if selected { 2.0 } else { 1.0 }, stroke_col),
                            StrokeKind::Inside,
                        );
                        for (i, col) in [tc.accent, tc.success, tc.error].iter().enumerate() {
                            let s = egui::Rect::from_min_size(
                                egui::pos2(rect.min.x + 12.0 + i as f32 * 22.0, rect.min.y + 12.0),
                                vec2(18.0, 18.0),
                            );
                            p.rect_filled(s, 3.0, Color32::from_rgb(col[0], col[1], col[2]));
                        }
                        p.text(
                            egui::pos2(rect.min.x + 12.0, rect.max.y - 14.0),
                            egui::Align2::LEFT_BOTTOM,
                            &t.name,
                            egui::FontId::proportional(16.0),
                            Color32::from_rgb(tc.text[0], tc.text[1], tc.text[2]),
                        );

                        if resp.clicked() && !selected {
                            self.settings.theme_id = t.id.clone();
                            self.theme = t.clone();
                            self.settings_manager.save(&self.settings);
                        }
                    }
                });

                ds_section(ui, &c, "Daemon");

                ds_row(ui, |ui| {
                    ds_label(ui, "Start with app");
                    if ds_toggle(ui, &c, &mut self.settings.auto_start_daemon).changed() {
                        self.settings_manager.save(&self.settings);
                    }
                });

                let alive = self.daemon_alive();
                let via = self.using_daemon();
                let incompatible = self.daemon_incompatible.clone();

                ds_row(ui, |ui| {
                    ds_label(ui, "Status");
                    let (txt, col) = if alive {
                        ("Running", c.success())
                    } else if incompatible.is_some() {
                        ("Incompatible", c.warning())
                    } else {
                        ("Not running", c.error())
                    };
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 4.0, col);
                    ui.add_space(8.0);
                    let resp = ui.label(RichText::new(txt).size(18.0).color(col));
                    if let Some(detail) = &incompatible {
                        let _ = resp.on_hover_text(detail);
                    }
                    ui.add_space(16.0);
                    if alive {
                        if ds_pill_button(ui, &c, "Stop", false).clicked() {
                            self.stop_daemon_process();
                        }
                    } else {
                        let label = if incompatible.is_some() {
                            "Replace"
                        } else {
                            "Start"
                        };
                        if ds_pill_button(ui, &c, label, false).clicked() {
                            self.start_daemon_process();
                        }
                    }
                    ui.add_space(6.0);
                    if ds_pill_button(ui, &c, "Refresh", false).clicked() {
                        self.refresh_daemon_state();
                    }
                });

                ds_row(ui, |ui| {
                    ds_label(ui, "Connection");
                    let (txt, col) = if via {
                        ("Via daemon", c.accent())
                    } else {
                        ("Direct", c.warning())
                    };
                    ui.label(RichText::new(txt).size(18.0).strong().color(col));
                    ui.add_space(16.0);
                    if alive {
                        if via {
                            if ds_pill_button(ui, &c, "Use direct", false).clicked() {
                                self.detach_daemon();
                            }
                        } else if ds_pill_button(ui, &c, "Use daemon", false).clicked() {
                            self.attach_daemon();
                        }
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::gallery_cards;

    #[test]
    fn gallery_svgs_parse_with_the_svg_renderer() {
        for (name, source) in gallery_cards() {
            let egui::ImageSource::Bytes { bytes, .. } = source else {
                panic!("{name}: expected embedded bytes");
            };
            let egui::load::Bytes::Static(bytes) = bytes else {
                panic!("{name}: expected static bytes");
            };
            let image = egui_extras::image::load_svg_bytes(bytes, &Default::default())
                .unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(image.size[0] > 0 && image.size[1] > 0, "{name}");
        }
    }
}
