#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::ffi::OsString;
use std::path::PathBuf;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};
use eframe::wgpu::Backends;
use lightcraft_engine::Session;
use lightcraft_ui_egui::panels::library_problem::LibraryProblem;
use lightcraft_ui_egui::{LightcraftApp, Services, UiState};
use studio_shell::StudioShell;
use studio_shell::lightroom::Lightroom;

/// Storage key for the shell state (active persona and every persona's panel layout).
const SHELL_KEY: &str = "studio.shell";
/// Storage key for LightCraft's UI state (panel widths, open sections, settings…).
const LIGHTCRAFT_KEY: &str = "studio.lightcraft";

struct App(StudioShell);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SHELL_KEY, &self.0);
        if let Some(lr) = &self.0.lightroom {
            eframe::set_value(storage, LIGHTCRAFT_KEY, &lr.app.ui);
        }
    }

    fn on_exit(&mut self) {
        // flush the catalog and write its snapshot
        if let Some(lr) = &mut self.0.lightroom
            && let Err(e) = lr.app.session.close_library()
        {
            eprintln!("studio: closing the library failed: {e}");
        }
    }
}

/// Where the photo library lives: `STUDIO_LIBRARY` if set, else `Pictures\Studio Library` in the
/// home folder. Its own library, not LightCraft's: a library is locked by one app at a time.
fn library_dir(env: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    if let Some(p) = env.filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    Some(PathBuf::from(home?).join("Pictures").join("Studio Library"))
}

/// The library session. One that can't be opened leaves an empty in-memory session, and
/// LightCraft's own window says why and offers the ways out.
fn open_library() -> (Session, Option<LibraryProblem>) {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let unopened = || Session::new().with_fs().with_system_clock();
    match library_dir(std::env::var_os("STUDIO_LIBRARY"), home) {
        None => (unopened(), Some(LibraryProblem::new("", "There is no home folder to keep the library in. Choose a folder for it."))),
        Some(dir) => {
            let mut s = unopened();
            match s.open_library(&dir, false) {
                Ok(_) => (s, None),
                Err(e) => (unopened(), Some(LibraryProblem::new(dir.to_string_lossy(), e.to_string()))),
            }
        }
    }
}

/// LightCraft on `session`, with its saved UI state.
fn lightroom((session, problem): (Session, Option<LibraryProblem>), saved: Option<UiState>) -> Lightroom {
    let mut app = LightcraftApp::new(session, services());
    if let Some(ui) = saved {
        app.ui = ui.sanitized();
    }
    app.library_problem = problem;
    Lightroom::new(app)
}

/// What LightCraft needs from the desktop: file and folder pickers, and writing exports.
fn services() -> Services {
    let paths = |p: Option<Vec<PathBuf>>| p.unwrap_or_default().into_iter().map(|p| p.to_string_lossy().to_string()).collect();
    Services {
        pick_files: Some(Box::new(move || paths(rfd::FileDialog::new().set_title("Import Photos").pick_files()))),
        pick_preset_files: Some(Box::new(move || paths(rfd::FileDialog::new().set_title("Import Presets").pick_files()))),
        pick_folder: Some(Box::new(|| rfd::FileDialog::new().set_title("Open Library").pick_folder().map(|p| p.to_string_lossy().to_string()))),
        // atomic (temp file, then rename), as LightCraft writes
        write: Some(Box::new(lightcraft_engine::export::write_file)),
        write_shared: Some(std::sync::Arc::new(lightcraft_engine::export::write_file)),
        ..Default::default()
    }
}

/// Graphics backends for the window. On Windows, DX12 alone: with Vulkan in the set, wgpu loads
/// the Vulkan driver even when it then picks DX12, and on this Windows ARM64 laptop that kills the
/// app at startup (same as LightCraft issue #136). `WGPU_BACKEND` (e.g. `vulkan`) overrides it.
fn window_backends() -> Backends {
    let default = if cfg!(windows) { Backends::DX12 } else { Backends::PRIMARY | Backends::GL };
    Backends::from_env().unwrap_or(default)
}

fn wgpu_options() -> WgpuConfiguration {
    let mut c = WgpuConfiguration::default();
    if let WgpuSetup::CreateNew(n) = &mut c.wgpu_setup {
        n.instance_descriptor.backends = window_backends();
    }
    c
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Studio")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([900.0, 560.0])
            .with_app_id("studio"),
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        "Studio",
        options,
        Box::new(|cc| {
            studio_shell::theme::apply(&cc.egui_ctx);
            // A missing or unreadable saved state falls back to the default layout.
            let saved = cc.storage.and_then(|s| eframe::get_value(s, SHELL_KEY));
            let mut shell = StudioShell::restore(saved);
            shell.lightroom = Some(lightroom(open_library(), cc.storage.and_then(|s| eframe::get_value(s, LIGHTCRAFT_KEY))));
            Ok(Box::new(App(shell)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_lives_in_pictures_unless_studio_library_is_set() {
        let home = Some(OsString::from("C:/Users/me"));
        assert_eq!(library_dir(None, home.clone()), Some(PathBuf::from("C:/Users/me/Pictures/Studio Library")));
        assert_eq!(library_dir(Some("".into()), home.clone()), Some(PathBuf::from("C:/Users/me/Pictures/Studio Library")));
        assert_eq!(library_dir(Some("D:/Photos".into()), home), Some(PathBuf::from("D:/Photos")));
        assert_eq!(library_dir(None, None), None);
    }

    /// eframe's storage, in memory.
    #[derive(Default)]
    struct Memory(std::collections::HashMap<String, String>);

    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn persona_panels_and_lightcraft_settings_come_back_after_a_relaunch() {
        use eframe::App as _;
        let mut app = LightcraftApp::new(Session::with_demo(), Services::default());
        app.ui.right_width = 333.0;
        app.ui.open_sections = vec!["color".into()];
        app.ui.settings.gpu = false;
        let mut shell = StudioShell { lightroom: Some(Lightroom::new(app)), ..Default::default() };
        shell.run("persona.develop").unwrap();
        let mut storage = Memory::default();
        App(shell).save(&mut storage);
        // the next launch
        let shell = StudioShell::restore(eframe::get_value(&storage, SHELL_KEY));
        assert_eq!(shell.persona, studio_shell::Persona::Develop);
        let lr = lightroom((Session::with_demo(), None), eframe::get_value(&storage, LIGHTCRAFT_KEY));
        let ui = &lr.app.ui;
        assert_eq!((ui.right_width, &ui.open_sections, ui.settings.gpu), (333.0, &vec!["color".to_owned()], false));
    }

    #[test]
    fn windows_uses_dx12_only_unless_overridden() {
        if std::env::var_os("WGPU_BACKEND").is_some() {
            return;
        }
        let b = window_backends();
        if cfg!(windows) {
            assert_eq!(b, Backends::DX12);
        } else {
            assert!(b.contains(Backends::VULKAN));
        }
        let WgpuSetup::CreateNew(n) = wgpu_options().wgpu_setup else { panic!("expected CreateNew") };
        assert_eq!(n.instance_descriptor.backends, b);
    }
}
