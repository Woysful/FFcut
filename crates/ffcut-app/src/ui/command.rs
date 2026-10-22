use crate::app::{App, Status};
use crate::theme;
use eframe::egui::{self, vec2, Color32, RichText, TextStyle};
use ffcut_core::cmdline;

impl App {
    /// The heart of the app: the final ffmpeg command, editable, executed exactly as shown.
    /// Lives at the end of the sidebar. The output file is chosen in the dialog the Export button opens.
    pub fn command_block(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("FINAL COMMAND").size(11.0).strong().color(Color32::from_gray(150)));
            if self.cmd_edited {
                ui.label(RichText::new("edited").color(theme::WARN).size(12.0));
            } else {
                ui.label(RichText::new("auto").color(theme::OK).size(12.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(self.cmd_text.clone());
                }
                if ui.add_enabled(self.cmd_edited, egui::Button::new("Reset").small()).on_hover_text("Discard your edits, rebuild from the settings").clicked() {
                    self.reset_command();
                }
            });
        });
        if self.cmd_stale {
            ui.add(egui::Label::new(RichText::new("Settings changed after you edited the command. Your text is kept; press Reset to apply the new settings.").size(12.0).color(theme::WARN)).wrap());
        }

        let before = self.cmd_text.clone();
        let enabled = self.job.is_none();
        ui.add_enabled_ui(enabled, |ui| {
            let stroke = if self.cmd_edited { theme::WARN.gamma_multiply(0.7) } else { Color32::from_gray(60) };
            egui::Frame::NONE.fill(theme::BG).stroke(egui::Stroke::new(1.0, stroke)).corner_radius(6).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.cmd_text)
                        .font(TextStyle::Monospace)
                        .frame(egui::Frame::NONE)
                        .desired_rows(7)
                        .desired_width(f32::INFINITY)
                        .hint_text("Open a video to see the command"),
                );
            });
        });
        if self.cmd_text != before {
            self.cmd_edited = self.cmd_text != self.last_auto;
            if !self.cmd_edited {
                self.cmd_stale = false;
            }
        }

        let parsed = cmdline::parse(&self.cmd_text);
        if let Err(e) = &parsed {
            ui.colored_label(theme::ERR, format!("⚠ {e}"));
        }

        ui.add_space(6.0);
        self.export_controls(ui, parsed.is_ok());

        match &self.status {
            Some(Status::Done(p)) => {
                let p = p.clone();
                ui.add_space(4.0);
                ui.colored_label(theme::OK, "✔ Exported");
                ui.add(egui::Label::new(RichText::new(p.display().to_string()).size(12.0).color(Color32::from_gray(140))).wrap());
                if ui.small_button("Show in folder").clicked() {
                    self.show_in_folder(&p);
                }
            }
            Some(Status::Error(m)) => {
                ui.add_space(4.0);
                ui.colored_label(theme::ERR, format!("✖ {m}"));
            }
            Some(Status::Info(m)) => {
                ui.add_space(4.0);
                ui.weak(m);
            }
            None => {}
        }
    }

    fn export_controls(&mut self, ui: &mut egui::Ui, command_ok: bool) {
        let w = ui.available_width();
        if let Some(run) = &self.job {
            let frac = run.percent.map(|p| p / 100.0);
            let mut text = match run.percent {
                Some(p) => format!("{p:.0}%"),
                None => "working…".into(),
            };
            if let Some(s) = run.speed {
                text.push_str(&format!("   {s:.1}×"));
            }
            if let (Some(p), Some(_)) = (run.percent, run.speed) {
                if p > 1.0 {
                    let el = run.started.elapsed().as_secs_f32();
                    text.push_str(&format!("   ~{:.0}s left", el * (100.0 - p) / p));
                }
            }
            // a darker fill and white text: the text stays readable on both the filled and the empty part
            ui.add(egui::ProgressBar::new(frac.unwrap_or(0.0)).text(RichText::new(text).color(Color32::WHITE).size(13.0)).animate(frac.is_none()).fill(theme::ACCENT.gamma_multiply(0.6)).desired_width(w).corner_radius(6));
            if ui.add_sized(vec2(w, 30.0), egui::Button::new("Cancel")).clicked() {
                run.job.cancel();
            }
        } else {
            let can = self.clip.is_some() && self.tools_ok() && command_ok;
            // add_sized centres the label (a plain min_size button keeps it left-aligned in a left-to-right layout)
            let b = egui::Button::new(RichText::new("Export…").size(15.0).strong().color(Color32::BLACK)).fill(theme::ACCENT);
            if ui.add_enabled_ui(can, |ui| ui.add_sized(vec2(w, 34.0), b)).inner.on_hover_text(self.tip("Choose where to save, then run the command above exactly as written", crate::hotkeys::Action::Export)).clicked() {
                self.export_clicked();
            }
        }
    }
}
