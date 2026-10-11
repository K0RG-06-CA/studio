//! LightCraft (storytold) as the Library and Develop personas: its whole window, drawn under the
//! persona bar in its own style, as it ships. One LightCraft app runs both, so the selection and
//! edits carry over; the persona follows its view (its loupe is Develop, every other view
//! Library).
//!
//! L13: LightCraft has no host hook on its menu commands (`run` has no filter), so a click on its
//! in-window Edit › Undo would be its own undo, which Studio records as a new Develop step
//! (`l13_lightcrafts_edit_undo_menu_item_is_recorded_as_a_new_develop_step`). Its in-window menus
//! stay hidden (`native_menu`) until Studio hosts them in the persona bar row and routes them
//! (T2m). Its shortcuts still run in its `logic`; Studio takes only its own keys first.

use std::sync::Arc;

use egui::{Theme, ThemePreference};
use lightcraft_ui_egui::state::{RightPanel, ViewMode};
use lightcraft_ui_egui::{LightcraftApp, panels};

use crate::Persona;

pub struct Lightroom {
    pub app: LightcraftApp,
    /// Frames its `logic` has run.
    frames: u32,
    /// LightCraft's style, captured once it installed it (it does so once).
    style: Option<Arc<egui::Style>>,
    /// Studio's style and theme preference, while LightCraft's is the global style: put back when
    /// another persona shows.
    studio: Option<(Arc<egui::Style>, ThemePreference)>,
    /// Library's own view (grid, compare…), kept while Develop shows the loupe.
    library_view: ViewMode,
}

impl Lightroom {
    /// LightCraft opens in its loupe; Studio opens Library in the grid. Its in-window menus are
    /// hidden (see L13 above).
    pub fn new(mut app: LightcraftApp) -> Self {
        app.ui.view = ViewMode::PhotoGrid;
        app.native_menu = true;
        Lightroom { app, frames: 0, style: None, studio: None, library_view: ViewMode::PhotoGrid }
    }

    /// The persona changes from `from` to `to`: Library gets its own view back, Develop the loupe
    /// with Edit (or the editing tool already open), and leaving LightCraft saves its changes.
    pub fn switch(&mut self, from: Persona, to: Persona) {
        if from == Persona::Library {
            self.library_view = self.app.ui.view;
        }
        // LightCraft saves from `logic`, which only runs in its personas: commit a slider drag
        // still in progress (its release won't be seen) and save what's pending now
        if from.lightroom() && !to.lightroom() {
            let _ = self.app.session.end_interaction();
            self.app.session.persist_if_dirty();
        }
        match to {
            Persona::Library => self.app.ui.view = self.library_view,
            Persona::Develop => {
                self.app.ui.view = ViewMode::Detail;
                let tool = matches!(
                    self.app.ui.right,
                    RightPanel::Crop | RightPanel::Remove | RightPanel::Masking | RightPanel::RedEye | RightPanel::Profiles
                );
                if !tool {
                    self.app.ui.right = RightPanel::Edit;
                }
            }
            _ => {}
        }
    }

    /// The persona LightCraft's own view asks for now (its keys and clicks change it), if not
    /// `now`: its loupe is Develop, every other view Library. With Develop off (`develop` false)
    /// the persona stays, and its loupe goes back to the grid.
    pub fn follow(&mut self, now: Persona, develop: bool) -> Option<Persona> {
        let loupe = self.app.ui.view == ViewMode::Detail;
        if !loupe {
            self.library_view = self.app.ui.view;
        }
        match (now, loupe) {
            (Persona::Library, true) if develop => Some(Persona::Develop),
            (Persona::Library, true) => {
                self.app.ui.view = ViewMode::PhotoGrid;
                None
            }
            (Persona::Develop, false) => Some(Persona::Library),
            _ => None,
        }
    }

    /// LightCraft's per-frame work (renders, saving, its shortcuts, dropped files), in its own
    /// style. Run it once per frame before drawing, and only while a LightCraft persona is showing.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if self.studio.is_none() {
            self.studio = Some((ctx.global_style(), ctx.options(|o| o.theme_preference)));
            ctx.set_theme(ThemePreference::System);
            match &self.style {
                Some(style) => ctx.set_global_style(style.clone()),
                // its first `logic` styles egui as it comes, as in its own app (it adjusts the
                // style it finds, so Studio's theme would show through)
                None => ctx.options_mut(|o| {
                    o.dark_style = Arc::new(Theme::Dark.default_style());
                    o.light_style = Arc::new(Theme::Light.default_style());
                }),
            }
        }
        self.app.logic(ctx);
        if self.style.is_none() {
            self.style = Some(ctx.global_style());
        }
        self.frames = self.frames.saturating_add(1);
    }

    /// Studio's style back, as another persona shows.
    pub fn give_back_style(&mut self, ctx: &egui::Context) {
        if let Some((style, theme)) = self.studio.take() {
            ctx.set_theme(theme);
            ctx.set_global_style(style);
        }
    }

    /// LightCraft's whole window (panels, overlays, its Quit), in `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.app.ui(ui);
    }

    /// Whether the window may close now. LightCraft asks this in its `logic`, which only runs in its
    /// personas; from another one Studio asks. False: changes couldn't be saved, and LightCraft's
    /// prompt (Try Again / Quit Anyway / Cancel) shows next time Library or Develop is drawn.
    pub fn may_close(&mut self) -> bool {
        panels::notices::may_close(&mut self.app)
    }

    /// Frames its logic has run.
    pub fn frames(&self) -> u32 {
        self.frames
    }
}
