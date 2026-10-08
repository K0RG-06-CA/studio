//! Personas: the Affinity-style modes in the top bar. Each one looks like the Adobe app it replaces.

/// One mode of the app, in top-bar order (Ctrl+1 … Ctrl+7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Persona {
    /// Lightroom Classic Library.
    #[default]
    Library,
    /// Lightroom Classic Develop.
    Develop,
    /// Film effects, in Lightroom's Develop layout.
    Film,
    /// Photoshop.
    Pixel,
    /// Illustrator-ish, Photoshop layout (v2).
    Vector,
    /// InDesign-ish, Photoshop layout (v3).
    Layout,
    /// Affinity's Export persona (v2).
    Export,
}

impl Persona {
    pub const ALL: [Persona; 7] =
        [Persona::Library, Persona::Develop, Persona::Film, Persona::Pixel, Persona::Vector, Persona::Layout, Persona::Export];

    /// Stable id, used in command ids (`persona.<id>`) and saved layouts.
    pub fn id(self) -> &'static str {
        match self {
            Persona::Library => "library",
            Persona::Develop => "develop",
            Persona::Film => "film",
            Persona::Pixel => "pixel",
            Persona::Vector => "vector",
            Persona::Layout => "layout",
            Persona::Export => "export",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Persona::Library => "Library",
            Persona::Develop => "Develop",
            Persona::Film => "Film",
            Persona::Pixel => "Pixel",
            Persona::Vector => "Vector",
            Persona::Layout => "Layout",
            Persona::Export => "Export",
        }
    }

    pub fn from_id(id: &str) -> Option<Persona> {
        Persona::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The command that switches to this persona.
    pub fn command(self) -> String {
        format!("persona.{}", self.id())
    }

    /// The version this persona ships in; only v1 personas can be opened.
    pub fn version(self) -> u8 {
        match self {
            Persona::Library | Persona::Develop | Persona::Film | Persona::Pixel => 1,
            Persona::Vector | Persona::Export => 2,
            Persona::Layout => 3,
        }
    }

    pub fn enabled(self) -> bool {
        self.version() == 1
    }

    /// Photoshop's layout (tool strip and options bar) rather than Lightroom's side panels.
    pub fn photoshop_layout(self) -> bool {
        matches!(self, Persona::Pixel | Persona::Vector | Persona::Layout)
    }

    /// Ctrl+1 … Ctrl+7 (Cmd on macOS), in top-bar order.
    pub fn shortcut(self) -> egui::KeyboardShortcut {
        const KEYS: [egui::Key; 7] =
            [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7];
        let i = Persona::ALL.iter().position(|p| *p == self).unwrap_or(0);
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, KEYS.get(i).copied().unwrap_or(egui::Key::Num1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_shortcuts_follow_top_bar_order() {
        for (i, p) in Persona::ALL.into_iter().enumerate() {
            assert_eq!(Persona::from_id(p.id()), Some(p));
            assert_eq!(p.shortcut().logical_key.name(), (i + 1).to_string());
        }
        assert_eq!(Persona::from_id("photoshop"), None);
    }

    #[test]
    fn only_v1_personas_are_enabled() {
        let enabled: Vec<_> = Persona::ALL.into_iter().filter(|p| p.enabled()).collect();
        assert_eq!(enabled, [Persona::Library, Persona::Develop, Persona::Film, Persona::Pixel]);
    }
}
