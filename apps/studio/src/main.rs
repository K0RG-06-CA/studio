#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use studio_shell::StudioShell;

struct App(StudioShell);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Studio",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            studio_shell::theme::apply(&cc.egui_ctx);
            Ok(Box::new(App(StudioShell::default())))
        }),
    )
}
