//! PhotoCraft (storytold) inside the Pixel persona, in Photoshop's places: its tool bar on the
//! left, its options bar on top, its canvas in Studio's canvas tab and its right dock in one dock
//! tab.
//!
//! PhotoCraft's Layers, History and Properties panels are private (`d836e34`), so they can't each
//! get a Studio dock tab: Studio shows PhotoCraft's whole right dock in one. The first example
//! compiles (and so its `compile_fail` doctest fails) once upstream makes the Layers panel public; then each
//! panel can get its own tab. The second one is the same call to a public panel, so the first
//! can't pass on a typo.
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
//!
//! Its Timeline windows (Window › Timeline) are in a private module, so Studio can't show them;
//! the first example compiles (and its doctest fails) once upstream makes it public. The second
//! is the same call to a public module's windows.
//!
//! ```compile_fail
//! fn timeline(app: &mut photocraft_ui_egui::PhotocraftApp, ctx: &egui::Context) {
//!     photocraft_ui_egui::timeline_ui::windows(app, ctx);
//! }
//! ```
//!
//! ```
//! fn workspace(app: &mut photocraft_ui_egui::PhotocraftApp, ctx: &egui::Context) {
//!     photocraft_ui_egui::workspace_ui::windows(app, ctx);
//! }
//! ```

use std::sync::Arc;

use egui::{Rect, Ui, UiBuilder};
use photocraft_ui_egui::theme::{ThemeKind, Tokens};
use photocraft_ui_egui::{PhotocraftApp, canvas, panels};

use crate::theme::TOKENS;

/// The width of PhotoCraft's icon rail beside its right dock, and the dock's narrowest.
const RAIL_W: f32 = 36.0;
const DOCK_MIN_W: f32 = 250.0;

pub struct Pixel {
    /// PhotoCraft's editor; its session holds the open document's pixels (see
    /// `studio_doc::pixel`).
    pub app: PhotocraftApp,
    /// PhotoCraft runs only as an `eframe::App`; its `logic` takes a frame it doesn't use.
    frame: eframe::Frame,
    /// Frames its `logic` has run; its fonts can be used from the second one on.
    frames: u32,
    /// PhotoCraft's style (sizes, spacing) in Studio's colours, for its pieces; the rest of
    /// Studio keeps Studio's theme, which is put back right after its `logic`.
    style: Option<Arc<egui::Style>>,
}

impl Pixel {
    /// PhotoCraft in Photoshop's grammar (its Pro theme: tab strips, flat panels, Properties in
    /// the dock), with Navigator (and Histogram), Properties and Layers in its right dock, as the
    /// PRD's Pixel column; its Color group stays off for Layers' room, and its History is off:
    /// the document's history is Studio's (its own History tab), across personas.
    pub fn new(mut app: PhotocraftApp) -> Self {
        app.ui.theme = ThemeKind::Pro;
        app.ui.panels.color = false;
        app.ui.panels.navigator = true;
        app.ui.panels.layers = true;
        app.ui.panels.properties = true;
        app.ui.panels.history = false;
        Pixel { app, frame: eframe::Frame::_new_kittest(), frames: 0, style: None }
    }

    /// PhotoCraft's per-frame work (its shortcuts, background jobs, views of the documents).
    /// Run it once per frame before drawing, and only while Pixel is showing.
    pub fn logic(&mut self, ctx: &egui::Context) {
        let studio = ctx.global_style();
        eframe::App::logic(&mut self.app, ctx, &mut self.frame);
        let theirs = ctx.global_style();
        if !Arc::ptr_eq(&theirs, &studio) {
            // it set its theme (fonts too, which stay: its panels need its families)
            let mut style = (*theirs).clone();
            style.visuals = studio.visuals.clone();
            self.style = Some(Arc::new(style));
            ctx.set_global_style(studio);
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("photocraft-theme"), tokens()));
        self.frames = self.frames.saturating_add(1);
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

    /// PhotoCraft's fonts are installed, so its pieces can be drawn.
    pub fn ready(&self) -> bool {
        self.frames >= 2
    }

    /// Frames its logic has run.
    #[cfg(test)]
    pub(crate) fn frames(&self) -> u32 {
        self.frames
    }

    /// Draw a piece of PhotoCraft into `ui`, in its style.
    fn show(&mut self, ui: &mut Ui, add: impl FnOnce(&mut PhotocraftApp, &mut Ui)) {
        if !self.ready() {
            return;
        }
        if let Some(style) = &self.style {
            ui.set_style(style.clone());
        }
        add(&mut self.app, ui);
    }

    /// Photoshop's tool bar, on the left.
    pub fn tool_bar(&mut self, ui: &mut Ui) {
        self.show(ui, panels::toolbar);
    }

    /// Photoshop's options bar, under the top bar.
    pub fn options_bar(&mut self, ui: &mut Ui) {
        self.show(ui, panels::options_bar);
    }

    /// The canvas tab: the open document's canvas, with "Rendering…" over it while its base
    /// layer is behind Develop; without a document, how to get one.
    pub fn canvas(&mut self, ui: &mut Ui, rendering: bool) {
        let rect = ui.available_rect_before_wrap();
        let Some(idx) = self.app.session.active_index().filter(|i| *i < self.app.ui.views.len()) else {
            ui.centered_and_justified(|ui| ui.label(crate::theme::placeholder("No document: pick a photo in Library, or open a file (Ctrl+O)")));
            return;
        };
        self.show(ui, |app, ui| {
            // what its own document area does around the canvas: where it is (the Navigator,
            // floating panels, zoom and paste read it) and Free Transform's undo steps
            app.last_canvas_rect = rect;
            photocraft_ui_egui::transform_tool::track_steps(app, ui.ctx());
            let view = app.ui.views[idx].clone();
            app.ui.views[idx] = canvas::canvas_view(app, ui, idx, rect, view, true);
        });
        if rendering {
            let at = rect.left_top() + egui::vec2(8.0, 8.0);
            ui.painter().text(at, egui::Align2::LEFT_TOP, "Rendering Develop\u{2026}", egui::FontId::proportional(TOKENS.font_size), TOKENS.text_dim);
        }
    }

    /// The panels tab: PhotoCraft's right dock (Layers, Properties, and its icon rail)
    /// filling it. Its panels are private, so they come as one (see the module docs). Too narrow
    /// a tab shows the dock's narrowest, clipped; its own resize handle is clipped away (the
    /// Studio dock sets the width).
    pub fn panels(&mut self, ui: &mut Ui) {
        let tab = ui.available_rect_before_wrap();
        let width = (tab.width() - RAIL_W).max(DOCK_MIN_W);
        panels::request_dock_width(ui.ctx(), width);
        let rect = Rect::from_min_max(egui::pos2(tab.max.x - width - RAIL_W, tab.min.y), tab.max);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        let grab = child.style().interaction.resize_grab_radius_side;
        child.set_clip_rect(tab.with_min_x(tab.min.x + grab).intersect(ui.clip_rect()));
        self.show(&mut child, panels::right_dock);
        ui.allocate_rect(tab, egui::Sense::hover());
    }
}

impl Pixel {
    /// What PhotoCraft shows over the whole window (its dialogs: adjustments, filters, Liquify,
    /// Camera Raw…; floating panels, the command palette, notices, job progress), after the
    /// dock is drawn, in its style. Left out: its unsaved-changes prompt and TIFF save options
    /// (saving is Studio's), and the Timeline (its module is private: see the module docs).
    pub fn overlays(&mut self, ctx: &egui::Context) {
        use photocraft_ui_egui::{
            analysis_ui, brush_panel, camera_raw_ui, dialogs, distort_ui, gpu_status, jobs_ui, notices, palette, preset_panels, type_panels_ui,
            wide_angle_ui, workspace_ui,
        };
        if !self.ready() {
            return;
        }
        let studio = ctx.global_style();
        if let Some(style) = &self.style {
            ctx.set_global_style(style.clone());
        }
        let app = &mut self.app;
        brush_panel::window(app, ctx);
        preset_panels::windows(app, ctx);
        type_panels_ui::windows(app, ctx);
        analysis_ui::windows(app, ctx);
        workspace_ui::windows(app, ctx);
        palette::show(app, ctx);
        dialogs::show(app, ctx);
        jobs_ui::dialog(app, ctx);
        distort_ui::show(app, ctx);
        camera_raw_ui::show(app, ctx);
        wide_angle_ui::show(app, ctx);
        canvas::extra_windows(app, ctx);
        notices::show(app, ctx);
        gpu_status::show_fallback(app, ctx);
        ctx.set_global_style(studio);
    }
}

/// PhotoCraft's Pro tokens in Studio's colours.
fn tokens() -> Tokens {
    let t = TOKENS;
    Tokens {
        chrome: t.chrome,
        canvas: t.canvas,
        canvas_dot: t.canvas,
        dock: t.divider,
        card: t.inset,
        card_border: t.divider,
        field: t.field,
        field_border: t.field_border,
        hover: t.hover,
        pressed: t.pressed,
        text: t.text,
        text_dim: t.text_label,
        text_faint: t.text_dim,
        icon: t.text_label,
        accent: t.accent,
        accent_border: t.accent,
        primary_bg: t.accent,
        separator: t.divider,
        tab_strip: t.chrome,
        row_selected: t.active,
        histogram_bg: t.canvas,
        ..Tokens::for_kind(ThemeKind::Pro)
    }
}
