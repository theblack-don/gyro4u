use egui::{Color32, RichText, TextEdit, Ui, vec2};

use crate::app::DS4UApp;
use crate::controller::family_of_kind;
use crate::inputs::ControllerKind;
use crate::profiles::ProfileManager;

use super::widgets::{accent_alpha, ds_label, ds_pill_button, ds_row, ds_section, ds_value_text};

impl DS4UApp {
    pub(crate) fn render_profiles_section(&mut self, ui: &mut Ui) {
        let c = self.theme.colors.clone();
        let kind = self.controller_kind.unwrap_or(ControllerKind::DualSense);
        let family = family_of_kind(kind);

        let cur_name = self
            .current_profile
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Default".to_string());

        let mut do_create = false;
        let mut switch_to: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut convert: Option<String> = None;
        let mut start_rename: Option<String> = None;
        let mut do_rename = false;
        let mut cancel_rename = false;
        let offer = self
            .profile_convert_offer
            .clone()
            .filter(|name| name == self.profile_edit_name.trim());

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ds_section(ui, &c, "Active");
                ds_row(ui, |ui| {
                    ds_label(ui, &cur_name);
                    if ds_pill_button(ui, &c, "Save", false).clicked() {
                        match self.sync_profile_checked() {
                            Ok(()) => {
                                self.status_message = format!("Saved profile '{}'", cur_name);
                                self.error_message.clear();
                            }
                            Err(e) => self.error_message = e,
                        }
                    }
                    ui.add_space(8.0);
                    if ds_pill_button(ui, &c, "Reload", false).clicked() {
                        match self.profile_manager.load_profile(&cur_name) {
                            Ok(profile) => {
                                self.load_profile(&profile);
                                self.status_message = format!("Reloaded '{}'", cur_name);
                            }
                            Err(e) => {
                                self.error_message =
                                    format!("Could not reload profile '{}': {e}", cur_name)
                            }
                        }
                    }
                });

                ds_section(ui, &c, &format!("Create for {}", family.label()));
                ds_row(ui, |ui| {
                    ds_label(ui, "Creating for");
                    ui.label(
                        RichText::new(family.label())
                            .size(18.0)
                            .strong()
                            .color(c.accent()),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(
                            "Profile names are global; collisions show a conversion option",
                        )
                        .size(12.0)
                        .color(c.text_dim()),
                    );
                });
                ds_row(ui, |ui| {
                    ds_label(ui, "Name");
                    let edit = ui.add(
                        TextEdit::singleline(&mut self.profile_edit_name)
                            .hint_text("Profile name")
                            .desired_width(220.0),
                    );
                    if edit.changed() {
                        self.profile_create_error.clear();
                        self.profile_convert_offer = None;
                    }
                    ui.add_space(8.0);
                    if ds_pill_button(ui, &c, "Create", false).clicked() {
                        do_create = true;
                    }
                });
                if !self.profile_create_error.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(crate::ui::widgets::ROW_PAD_X);
                        ui.label(
                            RichText::new(&self.profile_create_error)
                                .size(12.0)
                                .color(c.error()),
                        );
                        if let Some(name) = &offer {
                            ui.add_space(8.0);
                            if ds_pill_button(ui, &c, "Convert to this controller", false).clicked()
                            {
                                convert = Some(name.clone());
                            }
                        }
                    });
                }

                if let Some(from) = self.profile_rename_from.clone() {
                    ds_section(ui, &c, "Rename Profile");
                    ds_row(ui, |ui| {
                        ds_label(ui, &format!("Renaming '{}'", from));
                        ui.add(
                            TextEdit::singleline(&mut self.profile_rename_name)
                                .hint_text("New profile name")
                                .desired_width(220.0),
                        );
                        ui.add_space(8.0);
                        if ds_pill_button(ui, &c, "Rename", false).clicked() {
                            do_rename = true;
                        }
                        ui.add_space(8.0);
                        if ds_pill_button(ui, &c, "Cancel", false).clicked() {
                            cancel_rename = true;
                        }
                    });
                }

                ds_section(ui, &c, "All Profiles");
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(crate::ui::widgets::ROW_PAD_X);
                    ui.label(
                        RichText::new(
                            "Converting a profile across controller families keeps its mappings; \
                             gyro center calibration is reset and must be recalibrated on the new \
                             controller. Family-specific buttons, pads, and features may become \
                             inert.",
                        )
                        .size(12.0)
                        .italics()
                        .color(c.text_dim()),
                    );
                });
                for profile in self.profile_manager.list_profiles() {
                    let selected = profile.name == cur_name;
                    let same_family = profile.matches_device(kind);
                    let name = profile.name.clone();
                    let badge = match profile.device.family() {
                        crate::inputs::ControllerFamily::DualSense => "DualSense",
                        crate::inputs::ControllerFamily::Steam => "Steam",
                        crate::inputs::ControllerFamily::EightBitDo => "8BitDo",
                    };
                    let can_delete = ProfileManager::default_profile_name(profile.device)
                        != profile.name.as_str();
                    ds_row(ui, |ui| {
                        let lbl = if selected {
                            RichText::new(&name).size(20.0).color(c.accent()).strong()
                        } else {
                            RichText::new(&name)
                                .size(20.0)
                                .color(Color32::from_rgba_unmultiplied(255, 255, 255, 204))
                        };
                        ui.add_sized(
                            vec2(
                                crate::ui::widgets::LBL_WIDTH,
                                crate::ui::widgets::ROW_HEIGHT,
                            ),
                            egui::Label::new(lbl).selectable(false),
                        );
                        ui.label(
                            RichText::new(badge)
                                .size(12.0)
                                .color(c.text_dim())
                                .background_color(accent_alpha(&c, 28)),
                        );
                        ui.add_space(10.0);

                        if same_family {
                            if !selected && ds_pill_button(ui, &c, "Activate", false).clicked() {
                                switch_to = Some(name.clone());
                            }
                        } else if ds_pill_button(
                            ui,
                            &c,
                            &format!("Convert to {}", family.label()),
                            false,
                        )
                        .clicked()
                        {
                            convert = Some(name.clone());
                        }
                        if can_delete {
                            ui.add_space(8.0);
                            if ds_pill_button(ui, &c, "Rename", false).clicked() {
                                start_rename = Some(name.clone());
                            }
                            ui.add_space(8.0);
                            if ds_pill_button(ui, &c, "Delete", false).clicked() {
                                self.profile_delete_confirm = Some(name.clone());
                            }
                        }
                        let tag = if selected {
                            "Active"
                        } else if same_family {
                            "Saved"
                        } else {
                            "Other family"
                        };
                        ds_value_text(ui, tag);
                    });
                }
            });

        if let Some(name) = self.profile_delete_confirm.clone() {
            let deletable = match self.profile_manager.load_profile(&name) {
                Ok(profile) => {
                    let canonical = ProfileManager::default_profile_name(profile.device);
                    canonical != name.as_str() && canonical != profile.name.as_str()
                }
                Err(_) => !ProfileManager::is_default_profile_name(&name),
            };
            if !deletable {
                self.profile_delete_confirm = None;
            } else {
                let mut confirm = false;
                let mut cancel = false;
                let frame = egui::Frame::popup(ui.style())
                    .fill(c.window_bg())
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        crate::ui::widgets::sep_color(&c),
                    ))
                    .inner_margin(24.0)
                    .corner_radius(8.0);
                let response = egui::Modal::new(egui::Id::new("gyro4u_delete_profile"))
                    .frame(frame)
                    .show(ui.ctx(), |ui| {
                        ui.set_min_width(360.0);
                        ui.label(
                            RichText::new(format!("Delete profile '{}'?", name))
                                .size(22.0)
                                .strong()
                                .color(c.text()),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("This removes the profile file and cannot be undone.")
                                .size(14.0)
                                .color(c.text_dim()),
                        );
                        if name == cur_name {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(
                                    "This is the active profile; the controller will switch \
                                     to the default profile.",
                                )
                                .size(14.0)
                                .color(c.error()),
                            );
                        }
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            if ds_pill_button(ui, &c, "Delete", false).clicked() {
                                confirm = true;
                            }
                            ui.add_space(8.0);
                            if ds_pill_button(ui, &c, "Cancel", false).clicked() {
                                cancel = true;
                            }
                        });
                    });
                if cancel || response.should_close() {
                    self.profile_delete_confirm = None;
                }
                if confirm {
                    self.profile_delete_confirm = None;
                    delete = Some(name);
                }
            }
        }

        if do_create {
            let name = self.profile_edit_name.trim().to_string();
            match self.create_profile(&name) {
                Ok(()) => {
                    self.status_message = format!("Created profile '{}'", name);
                    self.profile_edit_name.clear();
                    self.error_message.clear();
                }
                Err(e) => {
                    self.profile_create_error = e;
                }
            }
        }
        if let Some(name) = start_rename {
            self.profile_rename_from = Some(name.clone());
            self.profile_rename_name = name;
            self.profile_create_error.clear();
        }
        if cancel_rename {
            self.profile_rename_from = None;
            self.profile_rename_name.clear();
        }
        if do_rename && let Some(old) = self.profile_rename_from.clone() {
            let new = self.profile_rename_name.trim().to_string();
            match self.rename_profile(&old, &new) {
                Ok(()) => {
                    self.profile_rename_from = None;
                    self.profile_rename_name.clear();
                    self.status_message = format!("Renamed '{}' to '{}'", old, new);
                    self.error_message.clear();
                }
                Err(e) => self.error_message = e,
            }
        }
        if let Some(name) = switch_to {
            match self.profile_manager.load_profile(&name) {
                Ok(profile) => {
                    self.load_profile(&profile);
                    self.status_message = format!("Activated '{}'", name);
                }
                Err(e) => self.error_message = format!("Could not activate '{}': {e}", name),
            }
        }
        if let Some(name) = convert {
            match self.convert_profile(&name) {
                Ok(()) => {
                    self.status_message = format!("Converted '{}' to {}", name, family.label());
                    self.profile_create_error.clear();
                    self.profile_convert_offer = None;
                }
                Err(e) => self.error_message = e,
            }
        }
        if let Some(name) = delete {
            match self.delete_profile(&name) {
                Ok(()) => {
                    self.status_message = format!("Deleted '{}'", name);
                    if self.profile_convert_offer.as_deref() == Some(name.as_str()) {
                        self.profile_convert_offer = None;
                    }
                    if self.profile_rename_from.as_deref() == Some(name.as_str()) {
                        self.profile_rename_from = None;
                        self.profile_rename_name.clear();
                    }
                }
                Err(e) => self.error_message = e,
            }
        }
    }
}
