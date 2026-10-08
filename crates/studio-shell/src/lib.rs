//! Studio shell: the Affinity-style top bar that never moves, and per-persona areas below it
//! laid out like the Adobe app each persona replaces.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod persona;
pub mod theme;

pub use persona::Persona;

#[cfg(test)]
mod tests_ui;

/// The shell's state and entry points.
#[derive(Default)]
pub struct StudioShell {
    pub persona: Persona,
}

impl StudioShell {
    /// Run a command by id. Every user action goes through here so the CLI and MCP can drive it.
    ///
    /// - `persona.<id>`: switch persona (v2/v3 personas are disabled and return an error).
    pub fn run(&mut self, id: &str) -> Result<(), String> {
        if let Some(p) = id.strip_prefix("persona.") {
            let p = Persona::from_id(p).ok_or_else(|| format!("unknown persona `{p}`"))?;
            if !p.enabled() {
                return Err(format!("{} arrives in v{}", p.label(), p.version()));
            }
            self.persona = p;
            return Ok(());
        }
        Err(format!("unknown command `{id}`"))
    }

    /// Draw the whole shell into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.shortcuts(ui);
        self.top_bar(ui);
        egui::CentralPanel::default().show(ui, |ui| ui.label(self.persona.label()));
    }

    /// Affinity's top bar: persona buttons on the left, the document name on the right. It never
    /// changes with the persona.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let t = theme::TOKENS;
        let frame = egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(8, 0));
        egui::Panel::top("studio.top_bar").exact_size(t.top_bar_h).resizable(false).frame(frame).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for p in Persona::ALL {
                    let on = p == self.persona;
                    let colour = if on { t.text } else { t.text_label };
                    let button = egui::Button::new(egui::RichText::new(p.label()).color(colour))
                        .fill(if on { t.active } else { egui::Color32::TRANSPARENT })
                        .frame_when_inactive(on)
                        .min_size(egui::vec2(64.0, t.top_bar_h - 10.0));
                    let r = ui
                        .add_enabled(p.enabled(), button)
                        .on_hover_text(format!("{} ({})", p.label(), shortcut_text(ui, p)))
                        .on_disabled_hover_text(format!("{} arrives in v{}", p.label(), p.version()));
                    if r.clicked() {
                        let _ = self.run(&p.command());
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // ponytail: fixed name until the shared document exists (milestone 3)
                    ui.label(egui::RichText::new("Untitled").color(t.text_dim));
                });
            });
        });
    }

    fn shortcuts(&mut self, ui: &mut egui::Ui) {
        for p in Persona::ALL {
            if ui.input_mut(|i| i.consume_shortcut(&p.shortcut())) {
                // A disabled persona's shortcut does nothing, like its greyed-out button.
                let _ = self.run(&p.command());
            }
        }
    }
}

fn shortcut_text(ui: &egui::Ui, p: Persona) -> String {
    ui.ctx().format_shortcut(&p.shortcut())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_commands_switch_enabled_personas_only() {
        let mut s = StudioShell::default();
        assert_eq!(s.persona, Persona::Library);
        s.run("persona.develop").unwrap();
        assert_eq!(s.persona, Persona::Develop);
        s.run("persona.pixel").unwrap();
        assert_eq!(s.persona, Persona::Pixel);
        assert!(s.run("persona.vector").unwrap_err().contains("v2"));
        assert!(s.run("persona.photoshop").is_err());
        assert!(s.run("persona.").is_err());
        assert!(s.run("file.open").is_err());
        assert_eq!(s.persona, Persona::Pixel, "failed commands change nothing");
    }
}
