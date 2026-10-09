//! LightCraft (storytold) inside the Library and Develop personas. One LightCraft app runs them
//! both, so the selection and edits carry over; its panels are drawn one by one into Studio's dock
//! tabs, in Lightroom Classic's places, instead of LightCraft's own Lightroom-desktop layout.

use std::sync::Arc;

use egui::{Rect, Ui, UiBuilder};
use lightcraft_ui_egui::state::{MIN_PHOTO_WIDTH, RightPanel, ViewMode};
use lightcraft_ui_egui::theme::Tokens;
use lightcraft_ui_egui::{LightcraftApp, panels};

use crate::Persona;
use crate::dock::Tab;

/// Where a Lightroom Classic module key takes LightCraft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    Grid,
    Loupe,
    /// Develop's Edit tab.
    Edit,
    Crop,
}

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
    /// LightCraft's right panel (Edit, History, Keywording, Metadata…) was drawn this frame. It can
    /// only be drawn once per frame (fixed ids), so those tabs share one panel group.
    right_shown: bool,
    /// Library's own view (grid, loupe, compare…), kept while Develop shows the loupe, as
    /// Lightroom Classic's modules each keep theirs.
    library_view: ViewMode,
}

impl Lightroom {
    /// LightCraft opens in its loupe; Studio opens Library in the grid, as Lightroom Classic does.
    pub fn new(mut app: LightcraftApp) -> Self {
        app.ui.view = ViewMode::PhotoGrid;
        Lightroom { app, frames: 0, style: None, right_shown: false, library_view: ViewMode::PhotoGrid }
    }

    /// The persona changes from `from` to `to`: Library gets its own view back, Develop the loupe,
    /// and leaving LightCraft saves its changes.
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
            Persona::Develop => self.app.ui.view = ViewMode::Detail,
            _ => {}
        }
    }

    /// LightCraft's per-frame work (renders, saving, its shortcuts, dropped files). Run it once per
    /// frame before drawing, and only while a LightCraft persona is showing.
    pub fn logic(&mut self, ctx: &egui::Context) {
        self.app.logic(ctx);
        self.right_shown = false;
        // LightCraft's Quit (Ctrl+Q, its quit prompt, its library-problem window): its own `ui`,
        // which Studio doesn't call, closes the window for it
        if std::mem::take(&mut self.app.ui.quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // as its `ui` does: the preset or profile under the pointer sets it again this frame
        self.app.hover_preview = None;
        if self.style.is_none() {
            // LightCraft replaced the global style (and the fonts, which Studio keeps: its panels
            // need its "semibold" family)
            self.style = Some(ctx.global_style());
            crate::theme::apply(ctx);
        }
        self.frames = self.frames.saturating_add(1);
    }

    /// Whether the window may close now. LightCraft asks this in its `logic`, which only runs in its
    /// personas; from another one Studio asks. False: changes couldn't be saved, and LightCraft's
    /// prompt (Try Again / Quit Anyway / Cancel) shows next time Library or Develop is drawn.
    pub fn may_close(&mut self) -> bool {
        panels::notices::may_close(&mut self.app)
    }

    /// LightCraft's fonts are installed, so its panels can be drawn.
    pub fn ready(&self) -> bool {
        self.frames >= 2
    }

    /// Frames its logic has run.
    #[cfg(test)]
    pub(crate) fn frames(&self) -> u32 {
        self.frames
    }

    /// Show what a module key asks for (after the persona switch).
    pub fn go(&mut self, to: Go) {
        match to {
            Go::Grid => self.app.ui.view = ViewMode::PhotoGrid,
            Go::Loupe => self.app.ui.view = ViewMode::Detail,
            // the Edit tab shows Edit, or the editing tool already open
            Go::Edit => {}
            // LightCraft's own command; it toggles, so only when crop isn't open yet
            Go::Crop => {
                if self.app.ui.right != RightPanel::Crop {
                    let _ = self.app.run("panel.crop", serde_json::json!({}));
                }
            }
        }
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
        if let Some(style) = &self.style {
            child.set_style(style.clone());
        }
        // A side panel's own resize handle sits on its inner edge, over the dock's separator, and
        // would take its drags (the dock sets the width): clip the handle away, a few points of
        // border with it.
        let grab = child.style().interaction.resize_grab_radius_side;
        let clip = match side {
            Side::Left => tab.with_max_x(tab.max.x - grab),
            Side::Right => tab.with_min_x(tab.min.x + grab),
            Side::Fill => tab,
        };
        child.set_clip_rect(clip.intersect(ui.clip_rect()));
        add(app, &mut child);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::NULL, tokens));
        ui.allocate_rect(Rect::from_min_size(tab.min, tab.size()), egui::Sense::hover());
    }

    /// Draw a LightCraft persona's dock tab. False when LightCraft has nothing for it (Studio then
    /// shows its placeholder).
    pub fn tab(&mut self, ui: &mut Ui, persona: Persona, tab: &Tab) -> bool {
        let name = match tab {
            Tab::Canvas => {
                self.show(ui, Side::Fill, |app, ui| canvas(app, ui, persona));
                return true;
            }
            Tab::Panel(name) => name.as_str(),
        };
        match (persona, name) {
            // Lightroom Classic's Catalog, Folders and Collections: LightCraft's sidebar has them all
            (Persona::Library, "Folders") => self.show(ui, Side::Left, panels::left::show),
            (Persona::Library, "Keywording") => self.right(ui, RightPanel::Keywords),
            (Persona::Library, "Metadata") => self.right(ui, RightPanel::Info),
            (Persona::Develop, "Presets") => self.show(ui, Side::Right, panels::presets::show),
            (Persona::Develop, "Edit") => {
                // the Edit tab also shows the editing tools LightCraft opens in its right panel
                let tool = matches!(
                    self.app.ui.right,
                    RightPanel::Crop | RightPanel::Remove | RightPanel::Masking | RightPanel::RedEye | RightPanel::Profiles
                );
                self.right(ui, if tool { self.app.ui.right } else { RightPanel::Edit });
            }
            (Persona::Develop, "History") => self.right(ui, RightPanel::Activity),
            _ => return false,
        }
        true
    }

    /// The canvas tab's title.
    pub fn canvas_title(&self, persona: Persona) -> &'static str {
        match (persona, self.app.ui.view) {
            (Persona::Library, ViewMode::PhotoGrid | ViewMode::SquareGrid) => "Grid",
            (Persona::Library, ViewMode::Compare) => "Compare",
            (Persona::Library, ViewMode::Survey) => "Survey",
            (Persona::Library, ViewMode::Reference) => "Reference",
            (Persona::Library, ViewMode::People) => "People",
            _ => "Loupe",
        }
    }

    /// LightCraft's right panel showing `mode`, or a hint if another tab already shows it.
    fn right(&mut self, ui: &mut Ui, mode: RightPanel) {
        if std::mem::replace(&mut self.right_shown, true) {
            ui.label(crate::theme::placeholder("Shown one at a time: keep these panels in one group"));
            return;
        }
        self.app.ui.right = mode;
        self.show(ui, Side::Right, panels::right::show);
    }

    /// What LightCraft shows over the whole window (dialogs, notices, progress, toasts), after the
    /// panels are drawn.
    pub fn overlays(&mut self, ctx: &egui::Context) {
        use lightcraft_ui_egui::{export_task, import, widgets};
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

/// The photo area: LightCraft's toolbar (Lightroom Classic's, under the photo) and the current view.
fn canvas(app: &mut LightcraftApp, ui: &mut Ui, persona: Persona) {
    if persona == Persona::Develop {
        app.ui.view = ViewMode::Detail;
    }
    panels::bottombar::show(app, ui);
    let t = Tokens::get(ui.ctx());
    let bg = if matches!(app.ui.view, ViewMode::PhotoGrid | ViewMode::SquareGrid | ViewMode::People) { t.grid_bg } else { t.canvas };
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ui, |ui| match app.ui.view {
        ViewMode::PhotoGrid | ViewMode::SquareGrid => panels::grid::show(app, ui),
        ViewMode::Detail => panels::detail::show(app, ui),
        ViewMode::Compare => panels::compare::show_compare(app, ui),
        ViewMode::Survey => panels::compare::show_survey(app, ui),
        ViewMode::Reference => panels::compare::show_reference(app, ui),
        ViewMode::People => panels::people::show(app, ui),
    });
}
