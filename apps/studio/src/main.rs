#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use studio_shell::StudioShell;

/// Storage key for the shell state (active persona and every persona's panel layout).
const SHELL_KEY: &str = "studio.shell";

struct App(StudioShell);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SHELL_KEY, &self.0);
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Studio",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            studio_shell::theme::apply(&cc.egui_ctx);
            // A missing or unreadable saved state falls back to the default layout.
            let saved = cc.storage.and_then(|s| eframe::get_value(s, SHELL_KEY));
            Ok(Box::new(App(StudioShell::restore(saved))))
        }),
    )
}
