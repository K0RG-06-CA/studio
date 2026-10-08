#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};
use eframe::wgpu::Backends;
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
            Ok(Box::new(App(StudioShell::restore(saved))))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
