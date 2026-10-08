//! LightCraft (storytold) inside the Library and Develop personas. One LightCraft app runs them
//! both, so the selection and edits carry over; its panels are drawn one by one into Studio's dock
//! tabs, in Lightroom Classic's places, instead of LightCraft's own Lightroom-desktop layout.

use std::sync::Arc;

use egui::{Rect, Ui, UiBuilder};
use lightcraft_ui_egui::LightcraftApp;
use lightcraft_ui_egui::state::MIN_PHOTO_WIDTH;
use lightcraft_ui_egui::theme::Tokens;

/// Which edge of a dock tab a LightCraft panel hangs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// A side panel anchored on the left (LightCraft's sidebar).
    Left,
    /// A side panel anchored on the right (Edit, Presets, Info…).
    Right,
    /// Content that fills the tab (grid, loupe).
    Fill,
}

pub struct Lightroom {
    pub app: LightcraftApp,
    /// Frames LightCraft's `logic` has run; its fonts can be used from the second one on.
    frames: u32,
    /// LightCraft's own style, captured once it installed it. Its panels draw with it; the rest of
    /// Studio keeps Studio's theme, which is put back right after.
    style: Option<Arc<egui::Style>>,
}

impl Lightroom {
    pub fn new(app: LightcraftApp) -> Self {
        Lightroom { app, frames: 0, style: None }
    }

    /// LightCraft's per-frame work (renders, saving, its shortcuts, dropped files). Run it once per
    /// frame before drawing, and only while a LightCraft persona is showing.
    pub fn logic(&mut self, ctx: &egui::Context) {
        self.app.logic(ctx);
        if self.style.is_none() {
            // LightCraft replaced the global style (and the fonts, which Studio keeps: its panels
            // need its "semibold" family)
            self.style = Some(ctx.global_style());
            crate::theme::apply(ctx);
        }
        self.frames = self.frames.saturating_add(1);
    }

    /// LightCraft's fonts are installed, so its panels can be drawn.
    pub fn ready(&self) -> bool {
        self.frames >= 2
    }

    /// Draw a LightCraft panel into `tab`, in LightCraft's style. Side panels keep room for a photo
    /// beside them; that room is added on the far side and clipped away, so they fill the tab.
    pub fn show(&mut self, ui: &mut Ui, side: Side, add: impl FnOnce(&mut LightcraftApp, &mut Ui)) {
        if !self.ready() {
            return;
        }
        let tab = ui.available_rect_before_wrap();
        let rect = match side {
            Side::Left => tab.with_max_x(tab.max.x + MIN_PHOTO_WIDTH),
            Side::Right => tab.with_min_x(tab.min.x - MIN_PHOTO_WIDTH),
            Side::Fill => tab,
        };
        let app = &mut self.app;
        // Studio's dock decides what is shown; LightCraft's own sidebar and presets column are off
        // so it doesn't keep room for them
        app.ui.left_panel = false;
        app.ui.presets = false;
        match side {
            Side::Left => app.ui.left_width = tab.width(),
            Side::Right => app.ui.right_width = tab.width(),
            Side::Fill => {}
        }
        let tokens = Tokens::get(ui.ctx());
        // the presets column is a fixed-width panel of `panel_w`
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::NULL, Tokens { panel_w: tab.width(), ..tokens }));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        child.set_clip_rect(tab.intersect(ui.clip_rect()));
        if let Some(style) = &self.style {
            child.set_style(style.clone());
        }
        add(app, &mut child);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::NULL, tokens));
        ui.allocate_rect(Rect::from_min_size(tab.min, tab.size()), egui::Sense::hover());
    }

    /// What LightCraft shows over the whole window (dialogs, notices, progress, toasts), after the
    /// panels are drawn.
    pub fn overlays(&mut self, ctx: &egui::Context) {
        use lightcraft_ui_egui::{export_task, import, panels, widgets};
        if !self.ready() {
            return;
        }
        let studio = ctx.global_style();
        if let Some(style) = &self.style {
            ctx.set_global_style(style.clone());
        }
        let app = &mut self.app;
        panels::second::show(app, ctx);
        panels::notices::show(app, ctx);
        panels::dialogs::show(app, ctx);
        panels::library_problem::show(app, ctx);
        import::progress(app, ctx);
        import::scan_progress(app, ctx);
        export_task::poll(app, ctx);
        panels::grid::drag_feedback(app, ctx);
        panels::toast(app, ctx);
        app.widgets = widgets::take_registry(ctx);
        ctx.set_global_style(studio);
    }
}
