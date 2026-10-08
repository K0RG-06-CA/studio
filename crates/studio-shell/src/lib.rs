//! Studio shell: the Affinity-style top bar that never moves, and per-persona areas below it
//! laid out like the Adobe app each persona replaces.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod dock;
pub mod persona;
pub mod theme;

use egui_dock::{DockArea, DockState};

use dock::Tab;
pub use persona::Persona;

#[cfg(test)]
mod tests_ui;

/// Photoshop's tool bar, top to bottom: (shortcut, tool). Placeholders until milestone 4.
const TOOLS: [(&str, &str); 19] = [
    ("V", "Move"),
    ("M", "Rectangular Marquee"),
    ("L", "Lasso"),
    ("W", "Object Selection"),
    ("C", "Crop"),
    ("I", "Eyedropper"),
    ("J", "Spot Healing Brush"),
    ("B", "Brush"),
    ("S", "Clone Stamp"),
    ("Y", "History Brush"),
    ("E", "Eraser"),
    ("G", "Gradient"),
    ("O", "Dodge"),
    ("P", "Pen"),
    ("T", "Type"),
    ("A", "Path Selection"),
    ("U", "Rectangle"),
    ("H", "Hand"),
    ("Z", "Zoom"),
];

/// The shell's state and entry points.
pub struct StudioShell {
    pub persona: Persona,
    /// Each persona's dock layout, indexed by `Persona as usize`; kept while switching away.
    pub layouts: [DockState<Tab>; 7],
}

impl Default for StudioShell {
    fn default() -> Self {
        StudioShell { persona: Persona::default(), layouts: Persona::ALL.map(dock::default_layout) }
    }
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
        self.persona_area(ui);
    }

    /// Everything under the top bar. Fixed chrome (options bar, tool strip, filmstrip) is plain
    /// panels, as in Adobe's apps; the canvas and panel groups are the persona's dock.
    fn persona_area(&mut self, ui: &mut egui::Ui) {
        let t = theme::TOKENS;
        let p = self.persona;
        let chrome = egui::Frame::NONE.fill(t.chrome);
        if p.photoshop_layout() {
            egui::Panel::top("studio.options_bar")
                .exact_size(t.options_bar_h)
                .resizable(false)
                .frame(chrome.inner_margin(egui::Margin::symmetric(8, 0)))
                .show(ui, |ui| ui.horizontal_centered(|ui| ui.label(theme::placeholder("Tool options"))));
        }
        egui::Panel::bottom("studio.filmstrip")
            .exact_size(t.filmstrip_h)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.canvas))
            .show(ui, |ui| ui.centered_and_justified(|ui| ui.label(theme::placeholder("Filmstrip"))));
        if p.photoshop_layout() {
            egui::Panel::left("studio.tools")
                .exact_size(t.tool_strip_w)
                .resizable(false)
                .frame(chrome.inner_margin(egui::Margin::symmetric(3, 6)))
                .show(ui, |ui| {
                    ui.vertical_centered_justified(|ui| {
                        for (key, name) in TOOLS {
                            ui.add_enabled(false, egui::Button::new(key).frame_when_inactive(false))
                                .on_disabled_hover_text(format!("{name} ({key}) arrives in milestone 4"));
                        }
                    });
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.divider)).show(ui, |ui| {
            DockArea::new(&mut self.layouts[p as usize])
                .id(egui::Id::new(("studio.dock", p.id())))
                .style(theme::dock_style(ui.style()))
                .show_close_buttons(false)
                .show_add_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut dock::Viewer { persona: p });
        });
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
