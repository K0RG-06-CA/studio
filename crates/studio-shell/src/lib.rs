//! Studio shell: the Affinity-style top bar that never moves, and per-persona areas below it
//! laid out like the Adobe app each persona replaces.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod dock;
mod history_panel;
pub mod lightcraft_menu;
pub mod lightroom;
pub mod persona;
pub mod pixel;
pub mod pixel_menu;
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

#[cfg(test)]
mod tests_images;

#[cfg(test)]
mod tests_parity;
#[cfg(test)]
mod tests_parity_spike;
#[cfg(test)]
mod tests_pixel_menu;

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
    /// The desktop's open dialog (a `.studio` document or an image file; none: cancelled). Set by
    /// the app; tests put a fake here.
    #[serde(skip)]
    pub pick_open_path: Option<PickOpenPath>,
    /// A document action that failed where no caller sees the error (a save, what follows the
    /// prompt's answer): (title, why), shown until dismissed (`document.dismissError`).
    #[serde(skip)]
    pub error: Option<(String, String)>,
    /// Studio's keys pressed in Pixel (document and undo keys), in order: each runs at the end of
    /// the frame once no button is down, so a stroke has ended and its edit is in (PhotoCraft's
    /// gesture is its own until release).
    #[serde(skip)]
    pub held: Vec<&'static str>,
    /// Where the pointer was last seen over the window (egui forgets it once the pointer leaves).
    #[serde(skip)]
    pub pointer: Option<egui::Pos2>,
    /// Studio's History panel in Pixel is closed (PhotoCraft's Window › History: `window.history`).
    #[serde(skip)]
    pub history_hidden: bool,
}

/// What a save returns when its dialog was cancelled: not a failure, nothing to show.
const SAVE_CANCELLED: &str = "the save was cancelled";

/// The top bar's busy mark while a Develop geometry change renders on its way into Pixel.
const BUSY: &str = "Develop rendering\u{2026}";

/// Why a document PhotoCraft opened itself was closed.
const STRAY_DOCUMENT: &str =
    "Studio opens documents: pick a photo in Library, or open a file with Ctrl+O (PhotoCraft\u{2019}s own New and Open don\u{2019}t make one).";

/// Studio's floating History panel in Pixel: its width, and its rows' height before they scroll.
const HISTORY_W: f32 = 220.0;
const HISTORY_H: f32 = 240.0;

/// See [`StudioShell::pick_save_path`].
pub type PickSavePath = Box<dyn FnMut(&str) -> Option<PathBuf>>;
/// See [`StudioShell::pick_open_path`].
pub type PickOpenPath = Box<dyn FnMut() -> Option<PathBuf>>;

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
            pick_open_path: None,
            error: None,
            held: Vec::new(),
            pointer: None,
            history_hidden: false,
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

    /// LightCraft, set by the app once the shell is restored. It shows the restored persona's
    /// view (Develop: the loupe), or the persona would follow its grid back to Library.
    pub fn set_lightroom(&mut self, mut lr: lightroom::Lightroom) {
        lr.switch(Persona::Library, self.persona);
        self.lightroom = Some(lr);
    }

    /// Run a command by id, without parameters. See [`Self::run_with`].
    pub fn run(&mut self, id: &str) -> Result<(), String> {
        self.run_with(id, &Value::Null)
    }

    /// Run a command by id. Every user action goes through here so the CLI and MCP can drive it.
    ///
    /// - `persona.<id>`: switch persona (v2/v3 personas are disabled and return an error).
    /// - `window.resetWorkspace`: put the current persona's panels back where they started.
    /// - `window.history`: show or hide Studio's History panel in Pixel (PhotoCraft's Window ›
    ///   History).
    /// - `app.quit` (Ctrl+Q): close the window; an unsaved document asks first.
    /// - The document: `document.new` (from the photo picked in Library), `document.open {path?}`
    ///   (a `.studio` file, or an image file PhotoCraft opens: PNG, JPEG, TIFF, PSD…, which
    ///   becomes a document without a photo, in Pixel; no path: the open dialog),
    ///   `document.save` (to its path, else beside its source file, else where the save dialog
    ///   says), `document.saveAs {path?}` (no path: the save dialog); only ever to a `.studio`
    ///   file, so an opened image is never written over,
    ///   `document.saveACopy {path?}` (a `.studio` copy, by the save dialog without a path; the
    ///   document keeps its file and its unsaved changes), `document.revert` (open its own file
    ///   again: back to how it was saved, through the prompt),
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
        if id == "app.quit" {
            self.close_window = true;
            return Ok(());
        }
        if id == "window.history" {
            self.history_hidden = !self.history_hidden;
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
            if let Some(why) = self.persona_off(p) {
                return Err(why);
            }
            // a Develop geometry change lands (Pixel layers follow) before any persona shows it;
            // one that can't render was undone, and its notice shows next frame
            let _ = self.flush();
            if p == Persona::Pixel {
                self.pixel_document();
            }
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
            // no path: the open dialog says which (cancelled: nothing happens)
            "open" if path("path").is_none() => {
                let pick = self.pick_open_path.as_mut().ok_or("document.open needs a `path`")?;
                let Some(picked) = pick() else { return Ok(()) };
                self.run_with("document.open", &serde_json::json!({"path": picked, "discard": discard}))
            }
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
                            // from PhotoCraft too, as `document.close` does (else it holds a
                            // document Studio has none of, which `settle` closes and reports)
                            if let Some(px) = &mut self.pixel {
                                studio_doc::pixel::close_all(&mut px.app.session);
                            }
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
                let r = self.open(&path);
                // every way an open fails is shown (unsaved changes aren't a failure: it asks)
                if let Err(why) = &r {
                    let name = path.file_name().map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy());
                    self.error = Some((format!("Couldn\u{2019}t open \u{201c}{name}\u{201d}"), why.clone()));
                }
                r
            }
            "revert" => {
                let doc = self.document.as_ref().ok_or("no document is open")?;
                let path = doc.path.clone().ok_or("this document has no file to go back to: it was never saved")?;
                // as PhotoCraft's Revert is off: nothing to go back to (a reopen would only lose the history)
                if !unsaved {
                    return Err("nothing to revert: no changes since it was saved".into());
                }
                self.run_with("document.open", &serde_json::json!({"path": path, "discard": discard}))
            }
            "save" | "saveAs" | "saveACopy" => {
                let r = if cmd == "saveACopy" { self.save_copy(path("path")) } else { self.save(cmd == "saveAs", path("path")) };
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
            true => to,
            false => doc.path.clone().or_else(|| doc.default_path()),
        };
        let file = format!("{}.{}", doc.name, studio_doc::file::EXTENSION);
        let path = match (known, self.pick_save_path.as_mut()) {
            (Some(p), _) => p,
            // no file yet: ask where (the dialog may be cancelled)
            (None, Some(pick)) => pick(&file).ok_or(SAVE_CANCELLED)?,
            (None, None) if save_as => return Err("document.saveAs needs a `path`".into()),
            (None, None) => return Err("this document has no file yet: use document.saveAs".into()),
        };
        let path = studio_file(path)?;
        self.document.as_mut().ok_or("no document is open")?.save(&path)?;
        self.document_path = Some(path);
        Ok(())
    }

    /// `document.saveACopy {path}` (`to`): the document as it is now, written to a `.studio` file
    /// (by the save dialog without a path), leaving it on its own file with its changes unsaved.
    fn save_copy(&mut self, to: Option<PathBuf>) -> Result<(), String> {
        let doc = self.document.as_mut().ok_or("no document is open")?;
        if let Some(lr) = &self.lightroom {
            self.base.mirror(&lr.app.session, doc);
        }
        let file = format!("{} copy.{}", doc.name, studio_doc::file::EXTENSION);
        let path = match (to, self.pick_save_path.as_mut()) {
            (Some(p), _) => p,
            (None, Some(pick)) => pick(&file).ok_or(SAVE_CANCELLED)?,
            (None, None) => return Err("document.saveACopy needs a `path`".into()),
        };
        doc.save_copy(&studio_file(path)?)
    }

    /// `document.open`: a `.studio` file (its photo found in the library), or an image file
    /// PhotoCraft can open, as a document without a photo; what its import approximated or
    /// dropped is shown at once, before anything is edited or saved.
    fn open(&mut self, path: &std::path::Path) -> Result<(), String> {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(studio_doc::file::EXTENSION)) {
            let lr = self.lightroom.as_mut().ok_or("LightCraft isn't running")?;
            let mut doc = Document::open(path)?;
            doc.open_into(&mut lr.app.session)?;
            return self.replace(doc);
        }
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_name().map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy()).into_owned();
        let (doc, warnings) = Document::from_image(&name, &bytes)?;
        self.replace(doc)?;
        if !warnings.is_empty() {
            self.error = Some((format!("Opened \u{201c}{name}\u{201d} with warnings"), warnings.join("\n")));
        }
        Ok(())
    }

    fn replace(&mut self, mut doc: Document) -> Result<(), String> {
        self.document_path = doc.path.clone();
        // its pixels' home from now on (whatever PhotoCraft held goes)
        if let Some(px) = &mut self.pixel {
            doc.attach(&mut px.app.session);
        }
        // its photo is the one picked: Develop shows it, and clicking Pixel again keeps the
        // document instead of replacing it with whatever was picked before it was opened
        if let (Some(id), Some(lr)) = (doc.photo(), &mut self.lightroom)
            && lr.app.session.selection.active != Some(id)
        {
            lr.app.session.selection.ids = vec![id];
            lr.app.session.selection.active = Some(id);
        }
        let photo = doc.source.is_some();
        self.document = Some(doc);
        self.base = self.base.renewed();
        self.base_error = None;
        // a document without a photo is Pixel's: it opens there (Develop and Film are off)
        if !photo {
            self.dispatch("persona.pixel", &Value::Null)?;
        }
        Ok(())
    }

    /// Why persona `p` can't show the open document: Develop and Film edit a library photo, and
    /// a document opened from an image file has none (their buttons are greyed with this).
    fn persona_off(&self, p: Persona) -> Option<String> {
        let photoless = self.document.as_ref().is_some_and(|d| d.source.is_none());
        (photoless && matches!(p, Persona::Develop | Persona::Film))
            .then(|| format!("{} needs a library photo: this document was opened from an image file", p.label()))
    }

    /// Pixel shows the photo picked in Library: a document is made from it when none is open, and
    /// one of another photo is replaced (at once if it is saved, else through the "save
    /// changes?" prompt). A document opened from an image file is of no photo, so the photo
    /// picked meanwhile doesn't replace it.
    fn pixel_document(&mut self) {
        let Some(picked) = self.lightroom.as_ref().and_then(|lr| lr.app.session.active()) else { return };
        if self.document.as_ref().is_none_or(|d| d.photo().is_some_and(|p| p != picked)) {
            // a failure shows itself (the prompt; a photo that can't be read: its error)
            if let Err(e) = self.run("document.new")
                && self.prompt.is_none()
            {
                self.error = Some(("Couldn\u{2019}t make a new document".to_owned(), e));
            }
        }
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
        // the app reports it (at launch, a modal for a file that went away would only be noise)
        if r.is_err() {
            self.error = None;
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
        self.keep_document_photo();
        // PhotoCraft holds Studio's document and nothing else (`Document::copy_back`): with none
        // open, what it opened itself (its home screen's New, its palette) is closed too, and said
        if let Some(px) = self.pixel.as_mut().filter(|px| self.document.is_none() && !px.app.session.documents().is_empty()) {
            studio_doc::pixel::close_all(&mut px.app.session);
            self.error = Some(("Pixel".to_owned(), STRAY_DOCUMENT.to_owned()));
        }
        let (Some(doc), Some(lr), Some(px)) = (&mut self.document, &mut self.lightroom, &mut self.pixel) else { return };
        doc.note_develop(&lr.app.session);
        if let Err(e) = doc.reconcile(&mut lr.app.session, &mut px.app.session) {
            self.error = Some(("History".to_owned(), e));
        }
    }

    /// The document's photo stays in the library while the document is open: whatever moved it to
    /// Recently Deleted past Studio's menu (LightCraft's context-menu Delete, its delete
    /// confirmation, Remove Folder from Library) is taken back at once and said why, so Delete
    /// Permanently and Empty Recently Deleted never reach it.
    fn keep_document_photo(&mut self) {
        let (Some(photo), Some(lr)) = (self.document.as_ref().and_then(Document::photo), self.lightroom.as_mut()) else { return };
        if !lr.app.session.catalog.photo(photo).is_some_and(|p| p.deleted) {
            return;
        }
        // committed directly: LightCraft's Restore wants the photo selected, and a delete clears that
        let op = lightcraft_engine::catalog::Op::SetDeleted { id: photo, deleted: false };
        let why = match lr.app.session.commit("Restore", op) {
            Ok(()) => "It stays in the library while its document is open: close the document first.".to_owned(),
            Err(e) => e.to_string(),
        };
        self.error = Some(("Couldn\u{2019}t delete the document\u{2019}s photo".to_owned(), why));
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
        if self.persona == Persona::Pixel {
            release_on_focus_loss(ctx, raw, self.pointer);
        }
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
        if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
            self.pointer = Some(pos);
        }
        self.document_tick(ui.ctx());
        // egui repaints only on input: keep polling for the render while one is in flight (one
        // held for a gesture goes in on the input that ends it)
        if self.base.rendering() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.shortcuts(ui);
        // a file dropped on Pixel opens as the document (asking first if the open one has
        // changes); taken here, so PhotoCraft never opens it in its own session
        if self.persona == Persona::Pixel {
            let dropped = ui.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
            if let Some(file) = dropped.first() {
                // failures show themselves (the open's error, the prompt)
                let _ = self.run_with("document.open", &serde_json::json!({"path": file.path()}));
            }
        }
        // Closing from another persona with library changes that couldn't be saved: stop, and go
        // to Library, where LightCraft asks what to do (in its personas it checks this itself).
        if !self.persona.lightroom() && ui.input(|i| i.viewport().close_requested()) && self.lightroom.as_mut().is_some_and(|lr| !lr.may_close()) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let _ = self.run(&Persona::Library.command());
        }
        self.engine_logic(ui.ctx());
        // LightCraft's keys may have changed its view: the bar shows the persona it asks for
        self.follow_lightcraft();
        self.top_bar(ui);
        self.persona_area(ui);
        // and its clicks
        self.follow_lightcraft();
        self.settle();
        // Studio's keys pressed in Pixel, in order, once no button is down (checked every frame):
        // a stroke that ended this frame is in first, so a save writes it and an undo takes it
        if !ui.input(|i| i.pointer.any_down()) {
            for id in std::mem::take(&mut self.held) {
                self.key_command(id);
            }
        }
        self.document_prompt(ui.ctx());
        self.error_notice(ui.ctx());
    }

    /// One engine's logic per frame, by construction: the persona's own (LightCraft's in Library,
    /// Develop and Film; PhotoCraft's in Pixel), so the document's history only ever sees one
    /// engine change between two looks (milestone 5). There is no second call site.
    fn engine_logic(&mut self, ctx: &egui::Context) {
        // each app's style is the global one only while its personas show: Studio's comes back
        // first, so the app whose persona shows now takes over from Studio's, not the other app's
        if let Some(lr) = self.lightroom.as_mut().filter(|_| !self.persona.lightroom()) {
            lr.give_back_style(ctx);
        }
        if let Some(px) = self.pixel.as_mut().filter(|_| self.persona != Persona::Pixel) {
            px.give_back_style(ctx);
        }
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

    /// The persona follows LightCraft's view, which its own keys and clicks change (D2): its loupe
    /// is Develop, every other view Library. Develop off (a document without a photo): its loupe
    /// goes back to the grid.
    fn follow_lightcraft(&mut self) {
        if !self.persona.lightroom() {
            return;
        }
        let develop = self.persona_off(Persona::Develop).is_none();
        if let Some(p) = self.lightroom.as_mut().and_then(|lr| lr.follow(self.persona, develop)) {
            self.persona = p;
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

    /// Everything under the top bar. Library and Develop: LightCraft's whole window. Pixel:
    /// PhotoCraft's (see [`Self::pixel_area`]). Film, until it is built: a placeholder dock.
    fn persona_area(&mut self, ui: &mut egui::Ui) {
        let t = theme::TOKENS;
        let p = self.persona;
        if let Some(lr) = self.lightroom.as_mut().filter(|_| p.lightroom()) {
            lr.ui(ui, self.document.is_some());
            return;
        }
        if p == Persona::Pixel {
            return self.pixel_area(ui);
        }
        let chrome = egui::Frame::NONE.fill(t.chrome);
        if p.photoshop_layout() {
            egui::Panel::top("studio.options_bar").exact_size(t.options_bar_h).resizable(false).frame(chrome).show(ui, |_| {});
        }
        egui::Panel::bottom("studio.filmstrip")
            .exact_size(t.filmstrip_h)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.canvas))
            .show(ui, |ui| ui.centered_and_justified(|ui| ui.label(theme::placeholder("Filmstrip"))));
        if p.photoshop_layout() {
            egui::Panel::left("studio.tools").exact_size(t.tool_strip_w).resizable(false).frame(chrome).show(ui, |_| {});
        }
        let mut viewer = dock::Viewer { persona: p, reset_requested: false };
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

    /// Pixel: PhotoCraft's whole window, as it ships. Studio adds only what the document needs:
    /// - nothing at all while a Develop geometry change is on its way into the layers (its
    ///   `logic` doesn't run then, so neither does its `ui`: its canvas, panels and windows would
    ///   edit the layers the render is about to turn);
    /// - its document tab strip covered (L15: its × and Close would close the document past
    ///   Studio's close; Ctrl+W and Studio's commands close it);
    /// - "Rendering Develop…" over its canvas while the base layer is behind Develop;
    /// - Studio's History, floating at the left edge of its dock (D1; until T4 restyles it),
    ///   unless its Window › History closed it.
    fn pixel_area(&mut self, ui: &mut egui::Ui) {
        let Some(px) = self.pixel.as_mut() else { return };
        // its views follow its documents first: a document Studio opened since its `logic`
        // (which syncs them) ran, and its panels index them unchecked
        px.app.sync_views();
        px.ui(ui);
        if let Some(strip) = px.tab_strip(ui.ctx()) {
            ui.interact(strip, egui::Id::new("studio.pixel.tab_strip"), egui::Sense::click());
        }
        let Some(doc) = self.document.as_ref() else { return };
        let canvas = px.app.last_canvas_rect;
        let t = photocraft_ui_egui::theme::Tokens::get(ui.ctx());
        if !self.base.up_to_date(doc) {
            let at = canvas.left_top() + egui::vec2(8.0, 8.0);
            ui.painter().text(at, egui::Align2::LEFT_TOP, "Rendering Develop\u{2026}", egui::FontId::proportional(12.0), t.text_dim);
        }
        if self.history_hidden {
            return;
        }
        let (rows, at) = (doc.history.entries(), doc.history.position());
        let pos = egui::pos2(canvas.max.x - HISTORY_W - 8.0, canvas.min.y + 8.0);
        let goto = egui::Area::new(egui::Id::new("studio.history"))
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .show(ui, |ui| {
                        ui.set_width(HISTORY_W);
                        ui.set_max_height(HISTORY_H);
                        ui.strong("History");
                        history_panel::show(ui, &rows, at)
                    })
                    .inner
            })
            .inner;
        if let Some(index) = goto
            && let Err(e) = self.run_with("history.goto", &serde_json::json!({"index": index}))
        {
            self.error = Some(("History".to_owned(), e));
        }
    }

    /// Affinity's top bar: persona buttons on the left, the document name on the right. The
    /// persona's app menus come first (D4): LightCraft's in Library and Develop, routed by
    /// [`lightcraft_menu`], and PhotoCraft's in Pixel, routed by [`pixel_menu`] (off while a
    /// Develop geometry change is on its way into its layers).
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let t = theme::TOKENS;
        let frame = egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(8, 0));
        let taken: Vec<String> = DOCUMENT_KEYS.iter().map(|(m, k, _)| lightcraft_menu::shortcut_name(*m, *k)).collect();
        let menus =
            self.lightroom.as_ref().filter(|_| self.persona.lightroom()).map(|lr| lightcraft_menu::bar(&lr.app, self.document.as_ref(), &taken));
        let pixel = self.pixel.as_ref().filter(|_| self.persona == Persona::Pixel);
        // PhotoCraft's items whose key is a persona's show no shortcut (Actual Pixels' Ctrl+1)
        let personas = Persona::ALL.map(|p| p.shortcut());
        let (mut clicked, mut pixel_clicked, mut switch) = (None, None, None);
        egui::Panel::top("studio.top_bar").exact_size(t.top_bar_h).resizable(false).frame(frame).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(menus) = &menus {
                    clicked = lightcraft_menu::show(ui, menus);
                    ui.add_space(16.0);
                }
                if let Some(px) = pixel {
                    let (doc, shown) = (self.document.as_ref(), !self.history_hidden);
                    ui.add_enabled_ui(!self.develop_rendering(), |ui| {
                        pixel_clicked = pixel_menu::show(ui, || pixel_menu::items(&px.app, doc, shown, &personas));
                    });
                    ui.add_space(16.0);
                }
                ui.spacing_mut().item_spacing.x = 2.0;
                for p in Persona::ALL {
                    let on = p == self.persona;
                    let colour = if on { t.text } else { t.text_label };
                    let button = egui::Button::new(egui::RichText::new(p.label()).color(colour))
                        .fill(if on { t.active } else { egui::Color32::TRANSPARENT })
                        .frame_when_inactive(on)
                        .min_size(egui::vec2(64.0, t.top_bar_h - 10.0));
                    let off = self.persona_off(p);
                    let r = ui
                        .add_enabled(p.enabled() && off.is_none(), button)
                        .on_hover_text(format!("{} ({})", p.label(), shortcut_text(ui, p)))
                        .on_disabled_hover_text(off.unwrap_or_else(|| format!("{} arrives in v{}", p.label(), p.version())));
                    if r.clicked() {
                        switch = Some(p);
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
        if let Some(p) = switch {
            let _ = self.run(&p.command());
        }
        if let Some((id, params)) = clicked {
            self.lightcraft_menu_item(ui.ctx(), &id, params);
        }
        if let Some(id) = pixel_clicked {
            self.pixel_menu_item(ui.ctx(), &id);
        }
    }

    /// A PhotoCraft menu item clicked, as [`pixel_menu::route`] says (checked again here: only
    /// what the menu allows runs, and nothing while a Develop geometry change is on its way into
    /// the layers).
    fn pixel_menu_item(&mut self, ctx: &egui::Context, id: &str) {
        if self.develop_rendering() || !pixel_menu::allowed(id, self.document.as_ref()) {
            return;
        }
        match pixel_menu::route(id) {
            Some(pixel_menu::Route::Document(cmd)) => self.key_command(cmd),
            Some(pixel_menu::Route::History) => self.key_command(id),
            Some(pixel_menu::Route::HistoryPanel) => {
                let _ = self.run("window.history");
            }
            Some(pixel_menu::Route::Quit) => {
                let _ = self.run("app.quit");
            }
            Some(pixel_menu::Route::Pass) => {
                let Some(px) = self.pixel.as_mut() else { return };
                // what fails says why in its status bar, as in its own menus
                if let Err(e) = photocraft_ui_egui::menus::invoke(&mut px.app, ctx, id, serde_json::json!({})) {
                    px.app.ui.status = e;
                }
                // its own History shows its states only (D1): a workspace that turns it on doesn't
                px.app.ui.panels.history = false;
            }
            Some(pixel_menu::Route::Off | pixel_menu::Route::Hidden) | None => {}
        }
    }

    /// A LightCraft menu item clicked, as [`lightcraft_menu::route`] says (checked again here:
    /// only what the menu allows runs).
    fn lightcraft_menu_item(&mut self, ctx: &egui::Context, id: &str, params: Value) {
        let Some(lr) = self.lightroom.as_ref() else { return };
        if !lightcraft_menu::allowed(id, &lr.app, self.document.as_ref()) {
            return;
        }
        match lightcraft_menu::route(id) {
            Some(lightcraft_menu::Route::History) if self.document.is_some() => self.key_command(id),
            Some(lightcraft_menu::Route::Quit) => {
                let _ = self.run("app.quit");
            }
            _ => {
                let Some(lr) = self.lightroom.as_mut() else { return };
                // an export that can't start (e.g. no folder) says why, as in LightCraft's menus
                if let Err(e) = lightcraft_ui_egui::menubar::run_item(&mut lr.app, id, params)
                    && matches!(id, "app.export" | "app.exportPrevious")
                {
                    lr.app.toast(ctx, e);
                }
            }
        }
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
        // The document's one history: Photoshop's undo keys in every persona while a document is
        // open (without one, LightCraft's own in its personas), never while typing (a text field's
        // own undo). One document, one save: Photoshop's keys for Studio's document commands in
        // every persona (Open and Save As through the desktop's dialogs), not the engines' own
        // (PhotoCraft's act on its session; LightCraft's Ctrl+S writes an XMP sidecar, its Ctrl+N
        // and Shift+Ctrl+S are New Album and Create Version). Taken here, so neither engine sees
        // them.
        let mut keys = Vec::from(DOCUMENT_KEYS);
        let typing = ui.ctx().egui_wants_keyboard_input();
        if self.document.is_some() && !typing {
            keys.extend(UNDO_KEYS);
        }
        // LightCraft's Delete key, as its Photo › Delete: never on the document's photo
        if self.persona.lightroom()
            && !typing
            && self.lightroom.as_ref().is_some_and(|lr| lightcraft_menu::delete_key_takes_document_photo(&lr.app, self.document.as_ref()))
        {
            ui.input_mut(|i| take_keys(i, &DELETE_KEYS));
        }
        let mut pressed = ui.input_mut(|i| take_keys(i, &keys));
        // then PhotoCraft's keys, as its menu items route: a routed item's runs Studio's command
        // (Revert, Open As, Save a Copy…), one Studio keeps off or hides is dropped (they would act
        // on PhotoCraft's own session, not on the document)
        if let Some(px) = self.pixel.as_ref().filter(|_| self.persona == Persona::Pixel)
            && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. })))
        {
            let routed = pixel_menu::keys(&px.app, typing);
            pressed.extend(ui.input_mut(|i| pixel_menu::take_keys(i, &routed)));
        }
        if self.persona == Persona::Pixel {
            // PhotoCraft commits a stroke at release, later in the frame: they run in the order
            // pressed at the end of the frame, once no button is down (see `ui`)
            self.held.extend(pressed);
        } else {
            for id in pressed {
                self.key_command(id);
            }
        }
    }

    /// A Studio key's command; what stops it is shown (a document command's failure shows itself:
    /// a save's or an open's error, the prompt).
    fn key_command(&mut self, id: &str) {
        // an undo key queued behind a close in Pixel has no document left: nothing to undo
        let document = id.starts_with("document.");
        if !document && self.document.is_none() {
            return;
        }
        if let Err(e) = self.run(id)
            && !document
        {
            self.error = Some(("History".to_owned(), e));
        }
    }
}

/// A button let go while the window has no focus (Alt+Tab mid-stroke) is never reported, and
/// egui keeps it down until the next click: the window losing focus releases every button egui
/// has down, so a stroke ends (Photoshop's way) and the keys held for it run. The release is
/// where the pointer was last seen (`last`, once egui has forgotten), so a stroke dragged out of
/// the window gets no segment to the corner.
fn release_on_focus_loss(ctx: &egui::Context, raw: &mut egui::RawInput, last: Option<egui::Pos2>) {
    if !raw.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))) {
        return;
    }
    use egui::PointerButton::{Extra1, Extra2, Middle, Primary, Secondary};
    let (pos, down) = ctx.input(|i| {
        let pos = i.pointer.interact_pos().or(i.pointer.latest_pos()).or(last);
        (pos, [Primary, Secondary, Middle, Extra1, Extra2].into_iter().filter(|&b| i.pointer.button_down(b)).collect::<Vec<_>>())
    });
    // no position ever seen: no button went down over the window
    let Some(pos) = pos else { return };
    for button in down {
        raw.events.push(egui::Event::PointerButton { pos, button, pressed: false, modifiers: egui::Modifiers::NONE });
    }
}

const CMD: egui::Modifiers = egui::Modifiers::COMMAND;

/// LightCraft's Delete key (Delete or Backspace, as it reads it), taken and dropped.
const DELETE_KEYS: [(egui::Modifiers, egui::Key, &str); 2] =
    [(egui::Modifiers::NONE, egui::Key::Delete, "photo.delete"), (egui::Modifiers::NONE, egui::Key::Backspace, "photo.delete")];

/// Studio's document keys, in every persona.
const DOCUMENT_KEYS: [(egui::Modifiers, egui::Key, &str); 5] = [
    (CMD.plus(egui::Modifiers::SHIFT), egui::Key::S, "document.saveAs"),
    (CMD, egui::Key::S, "document.save"),
    (CMD, egui::Key::N, "document.new"),
    (CMD, egui::Key::W, "document.close"),
    (CMD, egui::Key::O, "document.open"),
];

/// The document's history keys (Photoshop's).
const UNDO_KEYS: [(egui::Modifiers, egui::Key, &str); 3] = [
    (CMD, egui::Key::Z, "edit.undo"),
    (CMD.plus(egui::Modifiers::SHIFT), egui::Key::Z, "edit.redo"),
    (CMD.plus(egui::Modifiers::ALT), egui::Key::Z, "edit.toggleLastState"),
];

/// Where a document is saved: only ever a `.studio` file, so a document opened from an image is
/// never written over it (a name typed without an extension, which some save dialogs pass on as
/// typed, gets it).
fn studio_file(path: PathBuf) -> Result<PathBuf, String> {
    let ext = studio_doc::file::EXTENSION;
    match path.extension() {
        None => Ok(path.with_extension(ext)),
        Some(e) if e.eq_ignore_ascii_case(ext) => Ok(path),
        Some(_) => Err(format!("Studio saves .{ext} documents, not {}", path.display())),
    }
}

/// Take the presses of `keys`, each with exactly its modifiers (egui's `consume_shortcut`
/// ignores extra Shift and Alt, so Ctrl+N would take PhotoCraft's Shift+Ctrl+N New Layer): their
/// commands, in the order pressed.
fn take_keys(i: &mut egui::InputState, keys: &[(egui::Modifiers, egui::Key, &'static str)]) -> Vec<&'static str> {
    let mut ids = Vec::new();
    i.events.retain(|e| {
        let id = match e {
            egui::Event::Key { key, pressed: true, modifiers, .. } => keys.iter().find(|(m, k, _)| k == key && modifiers.matches_exact(*m)),
            _ => None,
        };
        ids.extend(id.map(|k| k.2));
        id.is_none()
    });
    ids
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
