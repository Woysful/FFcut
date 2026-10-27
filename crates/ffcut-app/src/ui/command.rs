//! The final ffmpeg command (last sidebar section) and the Export footer pinned under the sidebar.

use super::side::Section;
use super::widgets::{self, badge, hint, warn};
use crate::app::{open_in_file_manager, App, Status};
use crate::hotkeys::Action;
use crate::theme::{self, radius, size};
use eframe::egui::{self, vec2, Color32, RichText, TextStyle};
use ffcut_core::cmdline;

impl App {
    /// The command, editable and executed exactly as shown. The output file is chosen when exporting.
    pub(super) fn command_section(&mut self, ui: &mut egui::Ui) {
        let sum = if self.cmd.edited { "edited" } else { "auto" };
        self.section(ui, Section::Command, Some(sum.into()), |a, ui| a.command_body(ui));
    }

    fn command_body(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.cmd.edited {
                badge(ui, "EDITED", theme::WARN).on_hover_text("You changed the command by hand; settings no longer update it");
            } else {
                badge(ui, "AUTO", theme::OK).on_hover_text("Built from the settings above");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(self.cmd.text.clone());
                }
                if ui.add_enabled(self.cmd.edited, egui::Button::new("Reset").small()).on_hover_text("Discard your edits, rebuild from the settings").clicked()
                {
                    self.reset_command();
                }
            });
        });
        if self.cmd.stale {
            warn(ui, "Settings changed after you edited the command. Your text is kept; press Reset to apply the new settings.");
        }

        let changed = ui
            .add_enabled_ui(self.job.is_none(), |ui| {
                let stroke = if self.cmd.edited { theme::WARN.gamma_multiply(0.7) } else { theme::LINE };
                egui::Frame::NONE
                    .fill(theme::BG)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(radius::WIDGET)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.cmd.text)
                                .font(TextStyle::Monospace)
                                .frame(egui::Frame::NONE)
                                .desired_rows(6)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    })
                    .inner
            })
            .inner;
        if changed {
            self.cmd.edited = self.cmd.text != self.cmd.last_auto;
            if !self.cmd.edited {
                self.cmd.stale = false;
            }
        }
        if let Err(e) = cmdline::parse(&self.cmd.text) {
            widgets::error(ui, format!("⚠ {e}"));
        }
    }

    /// Always visible under the sidebar: the Export button, or the progress of a running export, and the result.
    pub(super) fn export_footer(&mut self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        match &self.status {
            Some(Status::Done(p)) => {
                let p = p.clone();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✔ Exported").color(theme::OK).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Show in folder").clicked() {
                            open_in_file_manager(&p);
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(p.display().to_string()).size(size::HINT).color(theme::TEXT_DIM)).truncate())
                    .on_hover_text(p.display().to_string());
                ui.add_space(4.0);
            }
            Some(Status::Error(m)) => {
                widgets::error(ui, format!("✖ {m}"));
                ui.add_space(4.0);
            }
            Some(Status::Info(m)) => {
                hint(ui, m.clone());
                ui.add_space(4.0);
            }
            None => {}
        }

        if let Some(run) = &self.job {
            let frac = run.percent.map(|p| p / 100.0);
            let mut text = match run.percent {
                Some(p) => format!("{p:.0}%"),
                None => "working…".into(),
            };
            if let Some(s) = run.speed {
                text.push_str(&format!("   {s:.1}×"));
            }
            if let Some(p) = run.percent.filter(|p| *p > 1.0 && run.speed.is_some()) {
                let el = run.started.elapsed().as_secs_f32();
                text.push_str(&format!("   ~{:.0}s left", el * (100.0 - p) / p));
            }
            ui.horizontal(|ui| {
                let bar_w = (w - 86.0).max(60.0);
                // a darker fill and white text: the text stays readable on both the filled and the empty part
                ui.add(
                    egui::ProgressBar::new(frac.unwrap_or(0.0))
                        .text(RichText::new(text).color(Color32::WHITE))
                        .animate(frac.is_none())
                        .fill(theme::ACCENT.gamma_multiply(0.6))
                        .desired_width(bar_w)
                        .desired_height(32.0)
                        .corner_radius(radius::WIDGET),
                );
                if ui.add_sized(vec2(78.0, 32.0), egui::Button::new("Cancel")).clicked() {
                    run.job.cancel();
                }
            });
            return;
        }

        let command_ok = cmdline::parse(&self.cmd.text).is_ok();
        let why_not = if !self.tools_ok() {
            Some("ffmpeg / ffprobe not found: set them in File → Settings")
        } else if !command_ok {
            Some("Fix the command first")
        } else {
            None
        };
        let label = if self.audio_mode() || self.s.audio_only { "Export audio…" } else { "Export…" };
        let b = widgets::primary_button(label).min_size(vec2(w, 36.0));
        // add_sized centres the label (a plain min_size button keeps it left-aligned in a left-to-right layout)
        let r = ui.add_enabled_ui(why_not.is_none(), |ui| ui.add_sized(vec2(w, 36.0), b)).inner;
        let r = match why_not {
            Some(t) => r.on_disabled_hover_text(t),
            None => r.on_hover_text(self.tip("Choose where to save, then run the command exactly as written", Action::Export)),
        };
        if r.clicked() {
            self.export_clicked();
        }
    }
}
