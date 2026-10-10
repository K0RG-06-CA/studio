//! Studio shell: the Affinity-style top bar that never moves, and per-persona areas below it
//! laid out like the Adobe app each persona replaces.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod dock;
mod history_panel;
pub mod lightroom;
pub mod persona;
pub mod pixel;
pub mod theme;

use std::path::PathBuf;

use egui_dock::{DockArea, DockState};
use serde_json::Value;
use studio_doc::Document;
use studio_doc::base::Base;

use dock::Tab;
pub use persona::Persona;

#[cfg(test)]
mod tests_ui;

#[cfg(test)]
mod tests_lightcraft;

#[cfg(test)]
mod tests_photocraft;

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
    /// PhotoCraft, which runs Pixel and is the home of the open document's pixels (its layers,
    /// their undo, the base layer's renders). Set by the app like `lightroom`.
    #[serde(skip)]
    pub pixel: Option<pixel::Pixel>,
    /// The open document (milestone 3): one library photo's develop settings as a live base layer
    /// under PhotoCraft's layer tree. Reopened at launch from `document_path`.
    #[serde(skip)]
    pub document: Option<Document>,
    /// Keeps the document's base layer rendered from its settings.
    #[serde(skip)]
    pub base: Base,
    /// The last error of keeping the base rendered (e.g. its photo left the library); the base
    /// keeps its last pixels.
    #[serde(skip)]
    pub base_error: Option<String>,
    /// Where the open document was last saved, so the next launch opens it again.
    #[serde(default)]
    pub document_path: Option<PathBuf>,
    /// What waits on the "save changes?" prompt, which shows while this is set.
    #[serde(skip)]
    pub prompt: Option<Pending>,
    /// Close the window on the next frame (`app.quit`, or the prompt's answer to a close).
    #[serde(skip)]
    pub close_window: bool,
    /// The desktop's save dialog, given a file name to suggest (`<name>.studio`): where to save a
    /// document that has no file yet (none: cancelled). Set by the app; tests put a fake here.
    #[serde(skip)]
    pub pick_save_path: Option<PickSavePath>,
    /// A document action that failed where no caller sees the error (a save, what follows the
    /// prompt's answer): (title, why), shown until dismissed (`document.dismissError`).
    #[serde(skip)]
    pub error: Option<(String, String)>,
    /// Undo keys pressed during a stroke or drag in Pixel, in order: each runs once the gesture
    /// has ended and its edit is in (PhotoCraft's gesture is its own until release).
    #[serde(skip)]
    pub held: Vec<&'static str>,
}

/// What a save returns when its dialog was cancelled: not a failure, nothing to show.
const SAVE_CANCELLED: &str = "the save was cancelled";

/// The top bar's busy mark while a Develop geometry change renders on its way into Pixel.
const BUSY: &str = "Develop rendering\u{2026}";

/// See [`StudioShell::pick_save_path`].
pub type PickSavePath = Box<dyn FnMut(&str) -> Option<PathBuf>>;

/// An action that would drop the open document's unsaved changes, held while the prompt asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Closing the window (its close button, Ctrl+Q, LightCraft's Quit).
    Quit,
    New,
    Open(PathBuf),
    Close,
}

impl Default for StudioShell {
    fn default() -> Self {
        StudioShell {
            persona: Persona::default(),
            layouts: Persona::ALL.map(dock::default_layout),
            lightroom: None,
            pixel: None,
            document: None,
            base: Base::new(),
            base_error: None,
            document_path: None,
            prompt: None,
            close_window: false,
            pick_save_path: None,
            error: None,
            held: Vec::new(),
        }
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
            if !dock::layout_is_usable(layout) || !dock::tabs_known(p, layout) {
                *layout = dock::default_layout(p);
            }
        }
        shell
    }

    /// Run a command by id, without parameters. See [`Self::run_with`].
    pub fn run(&mut self, id: &str) -> Result<(), String> {
        self.run_with(id, &Value::Null)
    }

    /// Run a command by id. Every user action goes through here so the CLI and MCP can drive it.
    ///
    /// - `persona.<id>`: switch persona (v2/v3 personas are disabled and return an error).
    /// - `window.resetWorkspace`: put the current persona's panels back where they started.
    /// - `app.quit` (Ctrl+Q): close the window; an unsaved document asks first.
    /// - Lightroom Classic's module keys: `view.grid` (G) and `view.loupe` (E) in Library,
    ///   `view.develop` (D), `tool.crop` (R) in Develop.
    /// - The document: `document.new` (from the photo picked in Library), `document.open {path}`,
    ///   `document.save` (to its path, else beside its source file, else where the save dialog
    ///   says), `document.saveAs {path}`,
    ///   `document.close {discard?}`. Unsaved changes stop `new`, `open` and `close` unless
    ///   `discard` is true: they return an error and show the "save changes?" prompt, which
    ///   `document.prompt.save` (save, then go ahead), `document.prompt.discard` (go ahead) and
    ///   `document.prompt.cancel` answer. A save that fails shows why until
    ///   `document.dismissError`.
    /// - Pixel (PhotoCraft): `pixel.tool {name}`, and `pixel.<PhotoCraft command id>` with its
    ///   params (see [`Self::run_pixel`]).
    /// - The document's one history, across personas (see [`Self::run_history`]): `edit.undo`
    ///   (Ctrl+Z), `edit.redo` (Shift+Ctrl+Z), `edit.toggleLastState` (Alt+Ctrl+Z),
    ///   `history.goto {index}` (a row of Pixel's History tab, 0 = the original).
    ///
    /// What a command changed in either engine is a history step when it returns, so a step
    /// made right after it (a script's next command) comes after it.
    pub fn run_with(&mut self, id: &str, params: &Value) -> Result<(), String> {
        let r = self.dispatch(id, params);
        self.settle();
        r
    }

    fn dispatch(&mut self, id: &str, params: &Value) -> Result<(), String> {
        if matches!(id, "edit.undo" | "edit.redo" | "edit.toggleLastState" | "history.goto") {
            return self.run_history(id, params);
        }
        if let Some(cmd) = id.strip_prefix("document.") {
            return self.run_document(cmd, params);
        }
        if let Some(cmd) = id.strip_prefix("pixel.") {
            return self.run_pixel(cmd, params);
        }
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
        if id == "app.quit" {
            self.close_window = true;
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
            // a Develop geometry change lands (Pixel layers follow) before any persona shows it;
            // one that can't render was undone, and its notice shows next frame
            let _ = self.flush();
            if let Some(lr) = &mut self.lightroom {
                lr.switch(self.persona, p);
            }
            self.persona = p;
            return Ok(());
        }
        Err(format!("unknown command `{id}`"))
    }

    fn run_document(&mut self, cmd: &str, params: &Value) -> Result<(), String> {
        let path = |key: &str| params.get(key).and_then(Value::as_str).map(PathBuf::from);
        let discard = params.get("discard").and_then(Value::as_bool).unwrap_or(false);
        let unsaved = self.unsaved();
        match cmd {
            "new" | "open" | "close" if unsaved && !discard => {
                self.prompt = Some(match cmd {
                    "new" => Pending::New,
                    "open" => Pending::Open(path("path").ok_or("document.open needs a `path`")?),
                    _ => Pending::Close,
                });
                Err("the open document has unsaved changes: save it, or close it with discard".into())
            }
            "prompt.save" | "prompt.discard" => {
                let pending = self.prompt.clone().ok_or("nothing is waiting on the prompt")?;
                if cmd == "prompt.save" {
                    // a save that fails keeps the prompt (and the document)
                    self.run("document.save")?;
                }
                self.prompt = None;
                let discard = serde_json::json!({"discard": true});
                match pending {
                    Pending::Quit => {
                        // discarded: the next launch opens the file as it was saved, if it has one
                        if let Some(doc) = self.document.take_if(|d| d.unsaved()) {
                            self.document_path = doc.path;
                        }
                        self.close_window = true;
                        Ok(())
                    }
                    Pending::New => self.run_with("document.new", &discard),
                    Pending::Open(path) => self.run_with("document.open", &serde_json::json!({"path": path, "discard": true})),
                    Pending::Close => self.run_with("document.close", &discard),
                }
            }
            "prompt.cancel" => {
                self.prompt = None;
                Ok(())
            }
            "dismissError" => {
                self.error = None;
                Ok(())
            }
            "new" => {
                let lr = self.lightroom.as_ref().ok_or("LightCraft isn't running")?;
                let id = lr.app.session.active().ok_or("no photo is picked in Library")?;
                self.replace(Document::from_photo(&lr.app.session, id)?)
            }
            "open" => {
                let path = path("path").ok_or("document.open needs a `path`")?;
                let lr = self.lightroom.as_mut().ok_or("LightCraft isn't running")?;
                let mut doc = Document::open(&path)?;
                doc.open_into(&mut lr.app.session)?;
                self.replace(doc)
            }
            "save" | "saveAs" => {
                let r = self.save(cmd == "saveAs", path("path"));
                // every way a save of an open document fails is shown; a cancelled dialog isn't one
                if let (Err(why), Some(doc)) = (&r, &self.document)
                    && why != SAVE_CANCELLED
                {
                    self.error = Some((format!("Couldn\u{2019}t save \u{201c}{}\u{201d}", doc.name), why.clone()));
                }
                r
            }
            "close" => {
                self.document.take().ok_or("no document is open")?;
                if let Some(px) = &mut self.pixel {
                    studio_doc::pixel::close_all(&mut px.app.session);
                }
                self.document_path = None;
                Ok(())
            }
            other => Err(format!("unknown command `document.{other}`")),
        }
    }

    /// `edit.undo`, `edit.redo`, `edit.toggleLastState` (redo if there is one, else undo, as
    /// PhotoCraft's) and `history.goto {index}` on the open document's one history
    /// (`studio_doc::history`), whichever engine made the steps. While PhotoCraft has a Free
    /// Transform open, undo, redo and the toggle are its own (they step the box, which its
    /// `run` intercepts); anything else open (type, a pen path) is applied first, as before a
    /// jump, so it is the newest step (PhotoCraft's plain undo would reach past a Develop step).
    fn run_history(&mut self, id: &str, params: &Value) -> Result<(), String> {
        let index = params.get("index").and_then(Value::as_u64);
        let (Some(doc), Some(lr), Some(px)) = (&mut self.document, &mut self.lightroom, &mut self.pixel) else {
            return Err("no document is open".into());
        };
        if id != "history.goto" && px.app.ui.transform.is_some() {
            return px.app.run(id, serde_json::json!({})).map(|_| ());
        }
        px.apply_pending();
        let (lr, pc, base) = (&mut lr.app.session, &mut px.app.session, &mut self.base);
        let redo = id == "edit.redo" || (id == "edit.toggleLastState" && doc.history.can_redo());
        match id {
            "history.goto" => doc.goto(lr, pc, base, index.ok_or("history.goto needs an `index`")? as usize),
            _ if redo => doc.redo(lr, pc, base).map(|_| ()),
            _ => doc.undo(lr, pc, base).map(|_| ()),
        }
    }

    /// `pixel.tool {name}` (a Photoshop tool: "brush", "move", "rectMarquee"…), and any PhotoCraft
    /// command as `pixel.<its id>` with its params (`pixel.layer.new.layer`, `pixel.edit.undo`…)
    /// on the open document. It's copied back at once, so a change to the locked Develop layer is
    /// taken back now and the command returns why.
    fn run_pixel(&mut self, cmd: &str, params: &Value) -> Result<(), String> {
        // PhotoCraft's own undo is the document's one history
        if matches!(cmd, "edit.undo" | "edit.redo" | "edit.toggleLastState") {
            return self.run_history(cmd, params);
        }
        let px = self.pixel.as_mut().ok_or("PhotoCraft isn't running")?;
        // its documents are Studio's to open, save and close
        if cmd.starts_with("file.") {
            return Err(format!("PhotoCraft's {cmd} isn't used in Studio: use the document.* commands"));
        }
        if cmd == "tool" {
            let name = params.get("name").and_then(Value::as_str).ok_or("pixel.tool needs a `name`")?;
            px.app.ui.tool = photocraft_ui_egui::state::Tool::from_name(name).ok_or_else(|| format!("unknown tool `{name}`"))?;
            return Ok(());
        }
        let doc = self.document.as_mut().ok_or("no document is open")?;
        // a Develop geometry change is on its way into the layers: nothing may edit them meanwhile
        if doc.history.pending_follow() {
            return Err(studio_doc::history::RENDERING.into());
        }
        let params = if params.is_null() { serde_json::json!({}) } else { params.clone() };
        px.app.run(cmd, params)?;
        doc.copy_back(&mut px.app.session);
        doc.take_notice().map_or(Ok(()), Err)
    }

    /// `document.save` / `document.saveAs {path}` (`to`).
    fn save(&mut self, save_as: bool, to: Option<PathBuf>) -> Result<(), String> {
        let doc = self.document.as_mut().ok_or("no document is open")?;
        // an edit made since this frame's sync (LightCraft's panels run after it; PhotoCraft's
        // were copied by `unsaved`, which every document command asks first)
        if let Some(lr) = &self.lightroom {
            self.base.mirror(&lr.app.session, doc);
        }
        let known = match save_as {
            true => Some(to.ok_or("document.saveAs needs a `path`")?),
            false => doc.path.clone().or_else(|| doc.default_path()),
        };
        let file = format!("{}.{}", doc.name, studio_doc::file::EXTENSION);
        let path = match (known, self.pick_save_path.as_mut()) {
            (Some(p), _) => p,
            // no file yet: ask where (the dialog may be cancelled)
            (None, Some(pick)) => pick(&file).ok_or(SAVE_CANCELLED)?,
            (None, None) => return Err("this document has no file yet: use document.saveAs".into()),
        };
        self.document.as_mut().ok_or("no document is open")?.save(&path)?;
        self.document_path = Some(path);
        Ok(())
    }

    fn replace(&mut self, mut doc: Document) -> Result<(), String> {
        self.document_path = doc.path.clone();
        // its pixels' home from now on (whatever PhotoCraft held goes)
        if let Some(px) = &mut self.pixel {
            doc.attach(&mut px.app.session);
        }
        self.document = Some(doc);
        self.base = self.base.renewed();
        self.base_error = None;
        Ok(())
    }

    /// Open the document of the last launch again (`document_path`), once LightCraft is set. A
    /// file that is gone is forgotten; one that exists but can't be opened now (a drive offline,
    /// a library problem) is kept for the next launch.
    pub fn reopen(&mut self) -> Result<(), String> {
        let Some(path) = self.document_path.clone() else { return Ok(()) };
        let r = self.run_with("document.open", &serde_json::json!({"path": path}));
        if r.is_err() && !path.exists() {
            self.document_path = None;
        }
        r
    }

    /// The document's base layer follows its photo's develop settings; once per frame.
    fn document_tick(&mut self, ctx: &egui::Context) {
        // a Pixel gesture in progress (a stroke or drag: a button held; Free Transform; typing):
        // a new render waits until it ends
        self.base.hold = self.persona == Persona::Pixel
            && (ctx.input(|i| i.pointer.any_down())
                || self.pixel.as_ref().is_some_and(|px| px.app.ui.transform.is_some() || px.app.ui.text_edit.is_some()));
        if let (Some(doc), Some(lr), Some(px)) = (&mut self.document, &mut self.lightroom, &mut self.pixel) {
            if let Err(e) = self.base.sync(&mut lr.app.session, &mut px.app.session, doc) {
                self.base_error = Some(e);
            }
            doc.copy_back(&mut px.app.session);
            // what the copy said (a change to the locked base taken back, layers that stayed)
            if let Some(why) = doc.take_notice() {
                let title = if why == studio_doc::pixel::LOCKED { "Couldn\u{2019}t change the Develop layer" } else { "Pixel" };
                self.error = Some((title.to_owned(), why));
            }
        }
    }

    /// The open document has unsaved changes, counting an edit LightCraft or PhotoCraft made since
    /// the last sync (their panels edit after it, later in the frame): copied first, so a close or
    /// a replace right after an edit still asks.
    fn unsaved(&mut self) -> bool {
        if let (Some(doc), Some(lr)) = (&mut self.document, &self.lightroom) {
            self.base.mirror(&lr.app.session, doc);
        }
        // every document command and the quit prompt ask this first: a Develop geometry change
        // lands before (one that can't render was undone, and says so)
        let _ = self.flush();
        if let (Some(doc), Some(px)) = (&mut self.document, &mut self.pixel) {
            doc.copy_back(&mut px.app.session);
        }
        self.document.as_ref().is_some_and(Document::unsaved)
    }

    /// Settle the document before a decision on it ([`Document::flush`]): a drag ends, a Develop
    /// geometry change waiting for its render lands as the step's Pixel half (see
    /// `studio_doc::follow`), PhotoCraft's edits are reconciled, and what it has open (a Free
    /// Transform, type, a pen path) is applied, as picking another tool would: leaving Pixel
    /// leaves nothing open for an undo key elsewhere to reach.
    fn flush(&mut self) -> Result<(), String> {
        let (Some(doc), Some(lr), Some(px)) = (&mut self.document, &mut self.lightroom, &mut self.pixel) else { return Ok(()) };
        let r = doc.flush(&mut lr.app.session, &mut px.app.session, &mut self.base);
        px.apply_pending();
        if let Err(e) = &r {
            self.base_error = Some(e.clone());
        }
        r
    }

    /// What either engine changed since the last look becomes history steps, in the order it was
    /// made: at the end of every frame (PhotoCraft's logic, panels and dialogs edit during it) and
    /// after every command. A jump PhotoCraft began itself is finished here (see
    /// [`Document::reconcile`]).
    fn settle(&mut self) {
        let (Some(doc), Some(lr), Some(px)) = (&mut self.document, &mut self.lightroom, &mut self.pixel) else { return };
        doc.note_develop(&lr.app.session);
        if let Err(e) = doc.reconcile(&mut lr.app.session, &mut px.app.session) {
            self.error = Some(("History".to_owned(), e));
        }
    }

    /// A Develop geometry change waits for its render: PhotoCraft is barred until it lands.
    pub fn develop_rendering(&self) -> bool {
        self.document.as_ref().is_some_and(|d| d.history.pending_follow())
    }

    /// The base layer shows the document's current settings (nothing stale, nothing in flight).
    pub fn base_up_to_date(&self) -> bool {
        self.document.as_ref().is_some_and(|d| self.base.up_to_date(d))
    }

    /// The top bar's document name: `name*` while unsaved, "Untitled" without a document.
    pub fn document_title(&self) -> String {
        match &self.document {
            Some(d) if d.unsaved() => format!("{}*", d.name),
            Some(d) => d.name.clone(),
            None => "Untitled".to_owned(),
        }
    }

    /// The app's raw input, before the frame (eframe's `raw_input_hook`). In Pixel, PhotoCraft's:
    /// egui-winit turns Ctrl+C/X/V into Copy/Cut/Paste events (and drops Ctrl+V when the
    /// clipboard holds an image), which PhotoCraft gives back to its pixel commands as key presses.
    pub fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(px) = self.pixel.as_mut().filter(|_| self.persona == Persona::Pixel) {
            eframe::App::raw_input_hook(&mut px.app, ctx, raw);
        }
    }

    /// Draw the whole shell into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if std::mem::take(&mut self.close_window) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Every way of closing the window (its close button, Ctrl+Q, LightCraft's Quit) arrives
        // here: an unsaved document stops it and asks.
        if ui.input(|i| i.viewport().close_requested()) && self.unsaved() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.prompt = Some(Pending::Quit);
        }
        self.document_tick(ui.ctx());
        // egui repaints only on input: keep polling for the render while one is in flight (one
        // held for a gesture goes in on the input that ends it)
        if self.base.rendering() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.shortcuts(ui);
        // Closing from another persona with library changes that couldn't be saved: stop, and go
        // to Library, where LightCraft asks what to do (in its personas it checks this itself).
        if !self.persona.lightroom() && ui.input(|i| i.viewport().close_requested()) && self.lightroom.as_mut().is_some_and(|lr| !lr.may_close()) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let _ = self.run(&Persona::Library.command());
        }
        self.engine_logic(ui.ctx());
        self.top_bar(ui);
        self.persona_area(ui);
        if let Some(lr) = self.lightroom.as_mut().filter(|_| self.persona.lightroom()) {
            lr.overlays(ui.ctx());
        }
        if let Some(px) = self.pixel.as_mut().filter(|_| self.persona == Persona::Pixel) {
            px.overlays(ui.ctx());
        }
        self.settle();
        // undo keys held for a gesture in Pixel: the gesture ended this frame (its edit is in)
        if !ui.input(|i| i.pointer.any_down()) {
            for id in std::mem::take(&mut self.held) {
                self.undo_key(id);
            }
        }
        self.document_prompt(ui.ctx());
        self.error_notice(ui.ctx());
    }

    /// One engine's logic per frame, by construction: the persona's own (LightCraft's in Library,
    /// Develop and Film; PhotoCraft's in Pixel), so the document's history only ever sees one
    /// engine change between two looks (milestone 5). There is no second call site.
    fn engine_logic(&mut self, ctx: &egui::Context) {
        match self.persona {
            // not while a Develop geometry change is on its way into the layers (its shortcuts and
            // tools would edit them meanwhile)
            Persona::Pixel if self.develop_rendering() => {}
            Persona::Pixel => {
                if let Some(px) = self.pixel.as_mut() {
                    px.logic(ctx);
                }
            }
            p if p.lightroom() => {
                if let Some(lr) = self.lightroom.as_mut() {
                    lr.logic(ctx);
                }
            }
            _ => {}
        }
    }

    /// What failed and why, over everything (and over the prompt it may have come from).
    fn error_notice(&mut self, ctx: &egui::Context) {
        let Some((title, why)) = &self.error else { return };
        let mut ok = false;
        let r = egui::Modal::new(egui::Id::new("studio.document_error")).show(ctx, |ui| {
            ui.strong(title.as_str());
            ui.label(why.as_str());
            ui.add_space(8.0);
            ok = ui.button("OK").clicked();
        });
        if ok || r.should_close() {
            let _ = self.run("document.dismissError");
        }
    }

    /// Last resort when the app exits with unsaved changes anyway (the prompt stops every close it
    /// sees): save to the document's file, else beside its source file; if there is neither, or
    /// writing there fails (a drive gone), into `recovered`. Never through a dialog. Where it went.
    pub fn save_for_exit(&mut self, recovered: &std::path::Path) -> Result<Option<PathBuf>, String> {
        if !self.unsaved() {
            return Ok(None);
        }
        let own = self.document.as_ref().and_then(|d| d.path.clone().or_else(|| d.default_path()));
        if let Some(path) = own
            && self.run_with("document.saveAs", &serde_json::json!({"path": path})).is_ok()
        {
            return Ok(Some(path));
        }
        let name = self.document.as_ref().map_or_else(String::new, |d| d.name.clone());
        std::fs::create_dir_all(recovered).map_err(|e| format!("{}: {e}", recovered.display()))?;
        // never over an earlier recovery
        let ext = studio_doc::file::EXTENSION;
        let path = (1..)
            .map(|n| recovered.join(if n == 1 { format!("{name}.{ext}") } else { format!("{name} {n}.{ext}") }))
            .find(|p| !p.exists())
            .ok_or("no free file name to recover the document under")?;
        self.run_with("document.saveAs", &serde_json::json!({"path": path}))?;
        Ok(Some(path))
    }

    /// Photoshop's "save changes?" prompt, over everything. Each button is a command; Escape
    /// cancels.
    fn document_prompt(&mut self, ctx: &egui::Context) {
        // the document it asks about was closed meanwhile (another caller, e.g. MCP): nothing to ask
        if self.document.is_none() {
            self.prompt = None;
        }
        let Some(name) = self.prompt.as_ref().and(self.document.as_ref()).map(|d| d.name.as_str()) else { return };
        let text = format!("Save changes to \u{201c}{name}\u{201d} before closing?");
        let mut answer = None;
        let r = egui::Modal::new(egui::Id::new("studio.document_prompt")).show(ctx, |ui| {
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (label, cmd) in
                    [("Save", "document.prompt.save"), ("Don't Save", "document.prompt.discard"), ("Cancel", "document.prompt.cancel")]
                {
                    if ui.button(label).clicked() {
                        answer = Some(cmd);
                    }
                }
            });
        });
        if r.should_close() && answer.is_none() {
            answer = Some("document.prompt.cancel");
        }
        let Some(cmd) = answer else { return };
        let pending = self.prompt.clone();
        // answered (the prompt is gone) but what it let through failed: say so. A failed save
        // keeps the prompt and has said why; a cancelled save dialog keeps it, silently.
        if let Err(why) = self.run(cmd)
            && self.prompt.is_none()
            && self.error.is_none()
        {
            let title = match pending {
                Some(Pending::Open(path)) => {
                    format!(
                        "Couldn\u{2019}t open \u{201c}{}\u{201d}",
                        path.file_name().map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
                    )
                }
                Some(Pending::New) => "Couldn\u{2019}t make a new document".to_owned(),
                _ => "That didn\u{2019}t work".to_owned(),
            };
            self.error = Some((title, why));
        }
    }

    /// Everything under the top bar. Fixed chrome (options bar, tool strip, filmstrip) is plain
    /// panels, as in Adobe's apps; the canvas and panel groups are the persona's dock.
    fn persona_area(&mut self, ui: &mut egui::Ui) {
        let t = theme::TOKENS;
        let p = self.persona;
        let chrome = egui::Frame::NONE.fill(t.chrome);
        // Pixel: PhotoCraft's own options bar and tool bar (Photoshop's), once its fonts are in.
        // Its views follow its documents first: its `logic` (which syncs them) didn't run on the
        // frame Pixel was clicked in the top bar, and its panels index them unchecked.
        if let Some(px) = self.pixel.as_mut().filter(|_| p == Persona::Pixel) {
            px.app.sync_views();
        }
        let photocraft = self.pixel.as_ref().is_some_and(|px| p == Persona::Pixel && px.ready());
        if let Some(px) = self.pixel.as_mut().filter(|_| photocraft) {
            px.options_bar(ui);
        } else if p.photoshop_layout() {
            egui::Panel::top("studio.options_bar").exact_size(t.options_bar_h).resizable(false).frame(chrome).show(ui, |_| {});
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
        if let Some(px) = self.pixel.as_mut().filter(|_| photocraft) {
            px.tool_bar(ui);
        } else if p.photoshop_layout() {
            egui::Panel::left("studio.tools").exact_size(t.tool_strip_w).resizable(false).frame(chrome).show(ui, |_| {});
        }
        let pixel = self.pixel.as_mut().filter(|_| p == Persona::Pixel);
        let rendering = self.document.as_ref().is_some_and(|d| !self.base.up_to_date(d));
        let document = self.document.as_ref().map(|d| d.name.clone());
        let history = self.document.as_ref().filter(|_| p == Persona::Pixel).map(|d| (d.history.entries(), d.history.position()));
        let mut viewer = dock::Viewer { persona: p, lightroom, pixel, rendering, document, reset_requested: false, history, goto: None };
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.divider)).show(ui, |ui| {
            DockArea::new(&mut self.layouts[p as usize])
                .id(egui::Id::new(("studio.dock", p.id())))
                .style(theme::dock_style(ui.style()))
                .show_close_buttons(false)
                .show_add_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        let (reset, goto) = (viewer.reset_requested, viewer.goto);
        if reset {
            let _ = self.run("window.resetWorkspace");
        }
        if let Some(index) = goto
            && let Err(e) = self.run_with("history.goto", &serde_json::json!({"index": index}))
        {
            self.error = Some(("History".to_owned(), e));
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
                let title = self.document_title();
                let busy = self.develop_rendering();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(title).color(t.text_dim));
                    // the busy mark: Pixel waits for a Develop geometry change to land
                    if busy {
                        ui.label(egui::RichText::new(BUSY).color(t.text_dim)).on_hover_text(studio_doc::history::RENDERING);
                        ui.spinner();
                    }
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
        // Quit: LightCraft's own Cmd+Q in its personas, Studio's in the others
        let quit = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Q);
        if !self.persona.lightroom() && ui.input_mut(|i| i.consume_shortcut(&quit)) {
            let _ = self.run("app.quit");
        }
        // Pixel: PhotoCraft's own file shortcuts would act on its session, not on the document.
        // Photoshop's keys for Studio's document commands instead; Open and Save As have no
        // dialog yet, so they do nothing.
        if self.persona == Persona::Pixel {
            let (cmd, shift) = (egui::Modifiers::COMMAND, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
            let keys = [
                (shift, egui::Key::S, None),
                (cmd, egui::Key::S, Some("document.save")),
                (cmd, egui::Key::N, Some("document.new")),
                (cmd, egui::Key::W, Some("document.close")),
                (cmd, egui::Key::O, None),
            ];
            for (mods, key, id) in keys {
                if ui.input_mut(|i| consume_exact(i, mods, key))
                    && let Some(id) = id
                {
                    // failures show themselves (a save's error, the prompt)
                    let _ = self.run(id);
                }
            }
        }
        // The document's one history: Photoshop's undo keys in every persona while a document is
        // open (without one, LightCraft's own in its personas), never while typing (a text field's
        // own undo). Taken here, so neither engine's own undo sees them.
        if self.document.is_some() && !ui.ctx().egui_wants_keyboard_input() {
            let (cmd, shift, alt) = (egui::Modifiers::COMMAND, egui::Modifiers::SHIFT, egui::Modifiers::ALT);
            for (mods, id) in [(cmd, "edit.undo"), (cmd | shift, "edit.redo"), (cmd | alt, "edit.toggleLastState")] {
                if !ui.input_mut(|i| consume_exact(i, mods, egui::Key::Z)) {
                    continue;
                }
                // mid-stroke in Pixel: PhotoCraft commits the stroke at release, then these run, in
                // order (a key in the release frame, before the commit, waits behind them too)
                if self.persona == Persona::Pixel && (ui.input(|i| i.pointer.any_down()) || !self.held.is_empty()) {
                    self.held.push(id);
                } else {
                    self.undo_key(id);
                }
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

    /// An undo key's command; what stops it is shown.
    fn undo_key(&mut self, id: &str) {
        if let Err(e) = self.run(id) {
            self.error = Some(("History".to_owned(), e));
        }
    }
}

/// Take a press of `key` with exactly `mods` (egui's `consume_shortcut` ignores extra Shift and
/// Alt, so Ctrl+N would take PhotoCraft's Shift+Ctrl+N New Layer).
fn consume_exact(i: &mut egui::InputState, mods: egui::Modifiers, key: egui::Key) -> bool {
    let before = i.events.len();
    i.events.retain(|e| !matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. } if *k == key && m.matches_exact(mods)));
    i.events.len() != before
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
        remove_panel(&mut s, Persona::Film, "History");
        remove_panel(&mut s, Persona::Library, "Folders");
        s.run("persona.film").unwrap();
        s.run("persona.library").unwrap();
        s.run("persona.film").unwrap();
        assert!(!has_panel(&s, Persona::Film, "History"), "layout survives switching personas");
        s.run("window.resetWorkspace").unwrap();
        assert!(has_panel(&s, Persona::Film, "History"));
        assert!(!has_panel(&s, Persona::Library, "Folders"), "other personas are untouched");
    }

    /// Move a panel into a group of its own on the right, as a user dragging its tab would.
    fn move_panel(s: &mut StudioShell, p: Persona, name: &str) {
        remove_panel(s, p, name);
        let dock = &mut s.layouts[p as usize];
        dock.main_surface_mut().split_right(egui_dock::NodeIndex::root(), 0.9, vec![Tab::Panel(name.to_owned())]);
    }

    fn where_is(s: &StudioShell, p: Persona, name: &str) -> Option<egui_dock::TabPath> {
        s.layouts[p as usize].find_tab(&Tab::Panel(name.to_owned()))
    }

    #[test]
    fn shell_state_survives_save_and_restore() {
        let mut s = StudioShell::default();
        s.run("persona.develop").unwrap();
        move_panel(&mut s, Persona::Develop, "Presets");
        let moved = where_is(&s, Persona::Develop, "Presets");
        assert_ne!(moved, where_is(&StudioShell::default(), Persona::Develop, "Presets"));
        let saved = ron::to_string(&s).unwrap();
        let back = StudioShell::restore(Some(ron::from_str(&saved).unwrap()));
        assert_eq!(back.persona, Persona::Develop);
        assert_eq!(where_is(&back, Persona::Develop, "Presets"), moved);
        assert!(has_panel(&back, Persona::Develop, "History"));
    }

    #[test]
    fn restore_falls_back_to_defaults_for_unusable_state() {
        assert_eq!(StudioShell::restore(None).persona, Persona::Library);
        // Only reachable through a hand-edited or future save.
        let s = StudioShell { persona: Persona::Vector, ..Default::default() };
        assert_eq!(StudioShell::restore(Some(s)).persona, Persona::Library);
        // A layout saved before Pixel ran PhotoCraft (its placeholder panels) goes back to default;
        // the other personas' layouts are kept.
        let mut old = StudioShell::default();
        old.layouts[Persona::Pixel as usize] = dock::default_layout(Persona::Vector);
        move_panel(&mut old, Persona::Library, "Folders");
        let moved = where_is(&old, Persona::Library, "Folders");
        let back = StudioShell::restore(Some(old));
        assert!(has_panel(&back, Persona::Pixel, "Layers") && !has_panel(&back, Persona::Pixel, "Histogram"));
        assert_eq!(where_is(&back, Persona::Library, "Folders"), moved);
        // one saved before Pixel had Studio's History tab (milestone 4: PhotoCraft's dock alone)
        // goes back to default too
        let mut m4 = StudioShell::default();
        let mut pixel = DockState::new(vec![Tab::Canvas]);
        pixel.main_surface_mut().split_right(egui_dock::NodeIndex::root(), 0.76, vec![Tab::Panel("Layers".to_owned())]);
        m4.layouts[Persona::Pixel as usize] = pixel;
        assert!(has_panel(&StudioShell::restore(Some(m4)), Persona::Pixel, "History"));
        // A save with the wrong number of layouts doesn't parse, so the app falls back to defaults.
        assert!(ron::from_str::<StudioShell>("(persona: pixel, layouts: [])").is_err());
    }
}
