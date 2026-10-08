//! Studio shell: the Affinity-style top bar that never moves, and per-persona areas below it
//! laid out like the Adobe app each persona replaces.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod dock;
pub mod lightroom;
pub mod persona;
pub mod theme;

use egui_dock::{DockArea, DockState};

use dock::Tab;
pub use persona::Persona;

#[cfg(test)]
mod tests_ui;

#[cfg(test)]
mod tests_lightcraft;

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

/// Lightroom Classic's module keys: (key, command, persona, what LightCraft shows).
const LIGHTROOM_KEYS: [(egui::Key, &str, Persona, lightroom::Go); 4] = [
    (egui::Key::G, "view.grid", Persona::Library, lightroom::Go::Grid),
    (egui::Key::E, "view.loupe", Persona::Library, lightroom::Go::Loupe),
    (egui::Key::D, "view.develop", Persona::Develop, lightroom::Go::Edit),
    (egui::Key::R, "tool.crop", Persona::Develop, lightroom::Go::Crop),
];

/// The shell's state and entry points. Serialisable: the app saves it between launches.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StudioShell {
    pub persona: Persona,
    /// Each persona's dock layout, indexed by `Persona as usize`; kept while switching away.
    pub layouts: [DockState<Tab>; 7],
    /// LightCraft, which runs Library and Develop. Set by the app after restoring the shell; none
    /// in tests of the shell alone, which then shows placeholders.
    #[serde(skip)]
    pub lightroom: Option<lightroom::Lightroom>,
}

impl Default for StudioShell {
    fn default() -> Self {
        StudioShell { persona: Persona::default(), layouts: Persona::ALL.map(dock::default_layout), lightroom: None }
    }
}

impl StudioShell {
    /// The shell from a saved state, or the default one if there is none or it can't be used
    /// (e.g. saved by a version where a now-disabled persona was active). Each persona's layout is
    /// checked on its own: one that isn't safe to show goes back to the default.
    pub fn restore(saved: Option<StudioShell>) -> StudioShell {
        let Some(mut shell) = saved.filter(|s| s.persona.enabled()) else { return StudioShell::default() };
        for p in Persona::ALL {
            let layout = &mut shell.layouts[p as usize];
            if !dock::layout_is_usable(layout) {
                *layout = dock::default_layout(p);
            }
        }
        shell
    }

    /// Run a command by id. Every user action goes through here so the CLI and MCP can drive it.
    ///
    /// - `persona.<id>`: switch persona (v2/v3 personas are disabled and return an error).
    /// - `window.resetWorkspace`: put the current persona's panels back where they started.
    /// - Lightroom Classic's module keys: `view.grid` (G) and `view.loupe` (E) in Library,
    ///   `view.develop` (D), `tool.crop` (R) in Develop.
    pub fn run(&mut self, id: &str) -> Result<(), String> {
        if let Some(&(_, _, persona, view)) = LIGHTROOM_KEYS.iter().find(|k| k.1 == id) {
            self.run(&persona.command())?;
            if let Some(lr) = &mut self.lightroom {
                lr.go(view);
            }
            if persona == Persona::Develop {
                // the tab LightCraft's Edit and Crop panels show in
                let dock = &mut self.layouts[Persona::Develop as usize];
                if let Some(path) = dock.find_tab(&Tab::Panel("Edit".to_owned())) {
                    let _ = dock.set_active_tab(path);
                }
            }
            return Ok(());
        }
        if id == "window.resetWorkspace" {
            self.layouts[self.persona as usize] = dock::default_layout(self.persona);
            return Ok(());
        }
        if let Some(p) = id.strip_prefix("persona.") {
            let p = Persona::from_id(p).ok_or_else(|| format!("unknown persona `{p}`"))?;
            if !p.enabled() {
                return Err(format!("{} arrives in v{}", p.label(), p.version()));
            }
            if let Some(lr) = &mut self.lightroom {
                lr.switch(self.persona, p);
            }
            self.persona = p;
            return Ok(());
        }
        Err(format!("unknown command `{id}`"))
    }

    /// Draw the whole shell into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.shortcuts(ui);
        if let Some(lr) = self.lightroom.as_mut().filter(|_| self.persona.lightroom()) {
            lr.logic(ui.ctx());
        }
        self.top_bar(ui);
        self.persona_area(ui);
        if let Some(lr) = self.lightroom.as_mut().filter(|_| self.persona.lightroom()) {
            lr.overlays(ui.ctx());
        }
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
        let lightroom = self.lightroom.as_mut().filter(|_| p.lightroom());
        // LightCraft draws its own filmstrip under the loupe
        if lightroom.is_none() {
            egui::Panel::bottom("studio.filmstrip")
                .exact_size(t.filmstrip_h)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(t.canvas))
                .show(ui, |ui| ui.centered_and_justified(|ui| ui.label(theme::placeholder("Filmstrip"))));
        }
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
        let mut viewer = dock::Viewer { persona: p, lightroom, reset_requested: false };
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.divider)).show(ui, |ui| {
            DockArea::new(&mut self.layouts[p as usize])
                .id(egui::Id::new(("studio.dock", p.id())))
                .style(theme::dock_style(ui.style()))
                .show_close_buttons(false)
                .show_add_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        if viewer.reset_requested {
            let _ = self.run("window.resetWorkspace");
        }
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
        // Consumed here, so LightCraft's own Cmd+1..5 (its Edit sections) never see them.
        for p in Persona::ALL {
            if ui.input_mut(|i| i.consume_shortcut(&p.shortcut())) {
                // A disabled persona's shortcut does nothing, like its greyed-out button.
                let _ = self.run(&p.command());
            }
        }
        // Lightroom's single keys belong to the Lightroom personas (in Pixel, G and E are Photoshop
        // tools), never while typing, and only bare: Shift+E and the like stay LightCraft's.
        let lightroom = matches!(self.persona, Persona::Library | Persona::Develop | Persona::Film);
        if !lightroom || ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        for (key, id, _, _) in LIGHTROOM_KEYS {
            if ui.input_mut(|i| i.modifiers.is_none() && i.consume_key(egui::Modifiers::NONE, key)) {
                let _ = self.run(id);
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

    fn has_panel(s: &StudioShell, p: Persona, name: &str) -> bool {
        s.layouts[p as usize].find_tab(&Tab::Panel(name.to_owned())).is_some()
    }

    /// Close a panel the way a user rearranging the dock would change the layout.
    fn remove_panel(s: &mut StudioShell, p: Persona, name: &str) {
        let dock = &mut s.layouts[p as usize];
        let path = dock.find_tab(&Tab::Panel(name.to_owned())).unwrap();
        dock.remove_tab(path).unwrap();
    }

    #[test]
    fn each_persona_keeps_its_own_layout_and_reset_restores_only_the_current_one() {
        let mut s = StudioShell::default();
        remove_panel(&mut s, Persona::Pixel, "History");
        remove_panel(&mut s, Persona::Library, "Folders");
        s.run("persona.pixel").unwrap();
        s.run("persona.library").unwrap();
        s.run("persona.pixel").unwrap();
        assert!(!has_panel(&s, Persona::Pixel, "History"), "layout survives switching personas");
        s.run("window.resetWorkspace").unwrap();
        assert!(has_panel(&s, Persona::Pixel, "History"));
        assert!(!has_panel(&s, Persona::Library, "Folders"), "other personas are untouched");
    }

    #[test]
    fn shell_state_survives_save_and_restore() {
        let mut s = StudioShell::default();
        s.run("persona.develop").unwrap();
        remove_panel(&mut s, Persona::Develop, "Presets");
        let saved = ron::to_string(&s).unwrap();
        let back = StudioShell::restore(Some(ron::from_str(&saved).unwrap()));
        assert_eq!(back.persona, Persona::Develop);
        assert!(!has_panel(&back, Persona::Develop, "Presets"));
        assert!(has_panel(&back, Persona::Develop, "History"));
    }

    #[test]
    fn restore_falls_back_to_defaults_for_unusable_state() {
        assert_eq!(StudioShell::restore(None).persona, Persona::Library);
        // Only reachable through a hand-edited or future save.
        let s = StudioShell { persona: Persona::Vector, ..Default::default() };
        assert_eq!(StudioShell::restore(Some(s)).persona, Persona::Library);
        // A save with the wrong number of layouts doesn't parse, so the app falls back to defaults.
        assert!(ron::from_str::<StudioShell>("(persona: pixel, layouts: [])").is_err());
    }
}
