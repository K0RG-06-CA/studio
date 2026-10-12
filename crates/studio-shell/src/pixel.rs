//! PhotoCraft (storytold) as the Pixel persona: its whole window, drawn under the persona bar in
//! its own style (its default theme), as it ships. Its session holds the open document's pixels
//! (see `studio_doc::pixel`).
//!
//! Its in-window menus are hidden: its menu bar goes to a native-menu backend that draws nothing
//! and sends no clicks, as on a Mac its menus would be the host's. Studio draws and routes them in
//! the persona bar row instead (`pixel_menu`). Its shortcuts still run in its `logic`; Studio
//! takes its own keys first, and those of the menu items it routes, keeps off or hides.
//!
//! L15: its document tab's × and the tab's Close menu close its document straight through its
//! engine, past Studio's close (its tab strip is private). Studio covers the strip instead (see
//! [`Pixel::tab_strip`]).
//!
//! L18: its home screen (no document) has New document… and Open… buttons that open its own
//! dialogs straight from its private start screen, past any command Studio could route, and they
//! can't be covered as the tab strip is: they sit in a card centred on its central panel, whose
//! rect nothing public gives while the home screen shows. Studio closes what they (or its
//! palette) make while it has no document open, and says how to get one (`settle`); with a
//! document open, `Document::copy_back` already keeps PhotoCraft to it. The first example
//! compiles (and its `compile_fail` doctest fails) once upstream makes the start screen public,
//! so a host can place or replace it; the second is its public neighbour, so a typo can't pass.
//!
//! ```compile_fail
//! fn home(app: &mut photocraft_ui_egui::PhotocraftApp, ui: &mut egui::Ui) {
//!     photocraft_ui_egui::canvas::start_screen(app, ui);
//! }
//! ```
//!
//! ```
//! fn area(app: &mut photocraft_ui_egui::PhotocraftApp, ui: &mut egui::Ui) {
//!     photocraft_ui_egui::canvas::document_area(app, ui);
//! }
//! ```
//!
//! L1: its dock panels (Layers, History, Properties…) are private, so Studio's History can't sit
//! in its dock; it floats beside it (D1). The first example compiles (and so its `compile_fail`
//! doctest fails) once upstream makes the Layers panel public (T4 re-aims this at its dock
//! bodies). The second one is the same call to a public panel, so the first can't pass on a typo.
//!
//! ```compile_fail
//! fn layers(app: &mut photocraft_ui_egui::PhotocraftApp, ui: &mut egui::Ui) {
//!     photocraft_ui_egui::panels::layers(app, ui);
//! }
//! ```
//!
//! ```
//! fn toolbar(app: &mut photocraft_ui_egui::PhotocraftApp, ui: &mut egui::Ui) {
//!     photocraft_ui_egui::panels::toolbar(app, ui);
//! }
//! ```

use std::sync::Arc;

use egui::{Rect, Theme, ThemePreference};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::native_menu::{Backend, Event, MenuBar, NativeMenu};

pub struct Pixel {
    /// PhotoCraft's editor; its session holds the open document's pixels.
    pub app: PhotocraftApp,
    /// PhotoCraft runs only as an `eframe::App` (L5): its `logic` and `ui` take a frame they don't
    /// use.
    frame: eframe::Frame,
    /// Frames its `logic` has run.
    frames: u32,
    /// Its `logic` ran this frame, so its `ui` may (eframe runs them in pairs).
    ran: bool,
    /// PhotoCraft's style, as it last left it.
    style: Option<Arc<egui::Style>>,
    /// Studio's style and theme preference, while PhotoCraft's is the global style: put back when
    /// another persona shows.
    studio: Option<(Arc<egui::Style>, ThemePreference)>,
}

/// A native menu bar that shows nothing and is never clicked: PhotoCraft's menus, hidden.
struct NoMenu;

impl Backend for NoMenu {
    fn sync(&mut self, _bar: &MenuBar) {}
    fn drain(&mut self) -> Vec<Event> {
        Vec::new()
    }
}

/// PhotoCraft with its in-window menus hidden (see the module docs).
pub fn hide_menus(app: &mut PhotocraftApp) {
    app.services.native_menu = Some(NativeMenu::new(Box::new(NoMenu)));
}

impl Pixel {
    /// PhotoCraft as it ships, but for two things: its History panel is off (the document's
    /// history is Studio's, across personas: D1) and its menus are hidden.
    pub fn new(mut app: PhotocraftApp) -> Self {
        app.ui.panels.history = false;
        hide_menus(&mut app);
        Pixel { app, frame: eframe::Frame::_new_kittest(), frames: 0, ran: false, style: None, studio: None }
    }

    /// PhotoCraft's per-frame work (its shortcuts, background jobs, views of the documents), in
    /// its own style. Run it once per frame before drawing, and only while Pixel is showing.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if self.studio.is_none() {
            self.studio = Some((ctx.global_style(), ctx.options(|o| o.theme_preference)));
            match &self.style {
                Some(style) => ctx.set_global_style(style.clone()),
                // its first `logic` sets itself up on egui as it comes (as LightCraft's: Studio's
                // theme would show through what it doesn't set)
                None => {
                    ctx.set_theme(ThemePreference::System);
                    ctx.options_mut(|o| {
                        o.dark_style = Arc::new(Theme::Dark.default_style());
                        o.light_style = Arc::new(Theme::Light.default_style());
                    });
                }
            }
        }
        eframe::App::logic(&mut self.app, ctx, &mut self.frame);
        // its theme follows its preferences
        self.style = Some(ctx.global_style());
        self.frames = self.frames.saturating_add(1);
        self.ran = true;
    }

    /// Studio's style back, as another persona shows (every frame it does): a `logic` that ran
    /// before Pixel was left by a click no longer counts for its `ui`.
    pub fn give_back_style(&mut self, ctx: &egui::Context) {
        self.ran = false;
        if let Some((style, theme)) = self.studio.take() {
            ctx.set_theme(theme);
            ctx.set_global_style(style);
        }
    }

    /// PhotoCraft's whole window (title bar, options bar, tool bar, dock, document area, its
    /// dialogs and windows), in `ui`, if its `logic` ran this frame (not on the frame Pixel was
    /// picked in the persona bar: the next one draws it). Its command palette is closed after it
    /// (L19, see `pixel_menu`): opened by its title bar's icon, it shows for that frame only. Its
    /// own History stays off before and after (D1: its rows would undo past Studio's history),
    /// however it was turned on: a workspace (its key, a saved one), its rail button (whose click
    /// lands after its dock is drawn, so it never shows).
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if std::mem::take(&mut self.ran) {
            self.app.ui.panels.history = false;
            eframe::App::ui(&mut self.app, ui, &mut self.frame);
        }
        self.app.ui.palette_open = false;
        self.app.ui.panels.history = false;
    }

    /// Where its document tab strip is (above its canvas, while it shows documents), for Studio
    /// to cover (L15).
    pub fn tab_strip(&self, ctx: &egui::Context) -> Option<Rect> {
        let shown = !self.app.session.documents().is_empty() && !self.app.ui.view.hides_tabs();
        // its Pro themes' strip is 26 pt; the others' 26 pt row in a 6 + 4 pt frame
        let h = if photocraft_ui_egui::theme::Tokens::get(ctx).pro { 26.0 } else { 36.0 };
        let canvas = self.app.last_canvas_rect;
        shown.then(|| Rect::from_min_max(egui::pos2(canvas.min.x, canvas.min.y - h), egui::pos2(canvas.max.x, canvas.min.y)))
    }

    /// Apply what is still open in PhotoCraft (a Free Transform, type being edited, a pen path),
    /// as picking another tool would: before anything asks whether the document has changes, so
    /// they count, and are saved.
    pub fn apply_pending(&mut self) {
        use photocraft_ui_egui::{transform_tool, type_tool, vector_ui};
        let app = &mut self.app;
        if app.ui.transform.is_some() {
            transform_tool::commit(app);
        }
        if app.ui.text_edit.is_some() {
            type_tool::commit(app);
        }
        if app.ui.pen.is_some() {
            vector_ui::pen_commit(app, false);
        }
    }

    /// Frames its logic has run.
    #[cfg(test)]
    pub(crate) fn frames(&self) -> u32 {
        self.frames
    }
}
