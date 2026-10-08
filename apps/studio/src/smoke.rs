//! `studio --smoke`: the real app, in a real window on the real GPU (DX12 on Windows), driven
//! through every persona by a script, then closed by itself. Its data folder and library are
//! throwaway ones (`STUDIO_SMOKE_DIR`, else `studio-smoke` in the temp folder; the library is seeded
//! with LightCraft's demo photos, so Develop renders). Any panic on any thread, any error logged
//! (wgpu, egui-wgpu, LightCraft) or a LightCraft GPU failure fails it: exit code 1.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use lightcraft_engine::gpu;
use studio_shell::StudioShell;

/// The script: one command every [`STEP`] seconds. Ends in Develop, so the next run starts there.
/// The document is saved as `smoke.studio` in the data folder, closed and opened again; the next
/// run opens it at launch.
const SCRIPT: &[&str] = &[
    "persona.develop",
    "window.resetWorkspace",
    "document.new",
    "persona.film",
    "window.resetWorkspace",
    "persona.pixel",
    "window.resetWorkspace",
    "document.saveAs",
    "document.close",
    "document.open",
    "persona.library",
    "window.resetWorkspace",
    "view.loupe",
    "view.grid",
    "tool.crop",
    "view.develop",
];
const STEP: f64 = 0.4;
/// The document file the script saves and opens.
const DOCUMENT: &str = "smoke.studio";

/// A script command's parameters.
fn params(id: &str) -> serde_json::Value {
    match id {
        "document.saveAs" | "document.open" => serde_json::json!({"path": dir().join(DOCUMENT)}),
        _ => serde_json::Value::Null,
    }
}
/// How long the script waits at the end for LightCraft's renders and GPU before giving up.
const SETTLE: f64 = 20.0;

static FAILURES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static FINISHED: AtomicBool = AtomicBool::new(false);

fn fail(why: impl Into<String>) {
    let why = why.into();
    eprintln!("smoke: FAIL {why}");
    if let Ok(mut f) = FAILURES.lock() {
        f.push(why);
    }
}

/// The throwaway folder for the app's saved state (`app.ron`) and library (`library/`).
pub fn dir() -> PathBuf {
    std::env::var_os("STUDIO_SMOKE_DIR").filter(|d| !d.is_empty()).map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("studio-smoke"))
}

/// Count panics on any thread and error logs as failures (and print warnings and errors).
pub fn install() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        fail(format!("panic: {info}"));
        default(info);
    }));
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

struct Logger;
static LOGGER: Logger = Logger;

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }

    fn log(&self, r: &log::Record) {
        if r.level() == log::Level::Error {
            fail(format!("error log [{}]: {}", r.target(), r.args()));
        } else if self.enabled(r.metadata()) {
            eprintln!("smoke: [{} {}] {}", r.level(), r.target(), r.args());
        }
    }

    fn flush(&self) {}
}

pub struct Script {
    start: Option<f64>,
    next: usize,
    closing: bool,
}

impl Script {
    pub fn new(shell: &StudioShell) -> Self {
        // the persona and document the last run saved (the relaunch check)
        println!("smoke: restored persona {}", shell.persona.id());
        let doc = shell.document_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        println!("smoke: restored document {}", doc.unwrap_or_else(|| "none".into()));
        Script { start: None, next: 0, closing: false }
    }

    /// Run after each frame of the shell.
    pub fn tick(&mut self, ctx: &egui::Context, shell: &mut StudioShell) {
        if self.closing {
            return;
        }
        ctx.request_repaint();
        let now = ctx.input(|i| i.time);
        let start = *self.start.get_or_insert(now);
        let Some(lr) = &mut shell.lightroom else {
            fail("LightCraft isn't running");
            return self.close(ctx);
        };
        // a photo for Develop
        if lr.app.session.active().is_none() {
            lr.app.session.selection.active = lr.app.session.catalog.photos().next().map(|p| p.id);
        }
        while self.next < SCRIPT.len() && now - start >= STEP * (self.next + 1) as f64 {
            let id = SCRIPT[self.next];
            println!("smoke: {id}");
            if let Err(e) = shell.run_with(id, &params(id)) {
                fail(format!("{id}: {e}"));
            }
            self.next += 1;
        }
        if self.next < SCRIPT.len() {
            return;
        }
        let Some(lr) = &shell.lightroom else { return self.close(ctx) };
        let gpu_pending = gpu::enabled() && !gpu::ready() && gpu::unavailable_reason().is_none();
        let rendered = matches!(lr.app.loupe_shown, Some((_, "render")));
        let base_rendered = shell.base_up_to_date();
        let busy = lr.app.renderer.in_flight() > 0 || gpu_pending || !rendered || !base_rendered;
        let end = start + STEP * (SCRIPT.len() + 1) as f64;
        if busy && now < end + SETTLE {
            return;
        }
        if !rendered {
            fail(format!("Develop's loupe never showed a render (showed {:?})", lr.app.loupe_shown.map(|l| l.1)));
        }
        if !base_rendered {
            fail(format!("the document's base layer never rendered ({:?})", shell.base_error));
        }
        if let Some(e) = &shell.base_error {
            fail(format!("document base: {e}"));
        }
        if let Some(r) = gpu::unavailable_reason() {
            fail(format!("LightCraft GPU: {r}"));
        }
        if let Some(r) = gpu::last_fallback() {
            fail(format!("LightCraft fell back to the CPU: {r}"));
        }
        println!("smoke: script done ({:.1} s)", now - start);
        FINISHED.store(true, Ordering::SeqCst);
        self.close(ctx);
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.closing = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

/// The exit code: success only if the script ran to the end with nothing failing.
pub fn finish(result: eframe::Result) -> ExitCode {
    if let Err(e) = result {
        fail(format!("eframe: {e}"));
    }
    if !FINISHED.load(Ordering::SeqCst) {
        fail("the window closed before the script finished");
    }
    let failed = FAILURES.lock().map(|f| !f.is_empty()).unwrap_or(true);
    if failed {
        return ExitCode::FAILURE;
    }
    println!("smoke: ok");
    ExitCode::SUCCESS
}
