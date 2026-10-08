//! Milestone 2 spike: can LightCraft's panels be drawn one by one inside dock tabs?
//!
//! (a) the panels that wrap themselves in an egui `Panel` (left, presets, right) fill a tab;
//! (b) the right panel can't be shown twice in one frame (Edit and Activity): its panel, scroll
//!     area and resize handle ids are fixed, so the two would clash and share state. Develop's
//!     History therefore shares Edit's panel group. If this test starts failing, upstream fixed
//!     it and History can move to the left column, as in Lightroom Classic;
//! (c) the floating overlays (dialogs, notices, toast) run alongside.

use egui::{Rect, Ui, UiBuilder, pos2};
use egui_kittest::Harness;
use lightcraft_ui_egui::state::{MIN_PHOTO_WIDTH, RightPanel};
use lightcraft_ui_egui::theme::Tokens;
use lightcraft_ui_egui::{LightcraftApp, Services, panels};

struct Spike {
    app: LightcraftApp,
    frames: u32,
    twice: bool,
}

const LEFT: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(260.0, 900.0));
const PRESETS: Rect = Rect::from_min_max(pos2(260.0, 0.0), pos2(500.0, 900.0));
const CENTER: Rect = Rect::from_min_max(pos2(500.0, 0.0), pos2(1080.0, 900.0));
const EDIT: Rect = Rect::from_min_max(pos2(1080.0, 0.0), pos2(1400.0, 500.0));
const HISTORY: Rect = Rect::from_min_max(pos2(1080.0, 500.0), pos2(1400.0, 900.0));

/// Draw into `tab`, with LightCraft's photo reserve added on the far side (clipped away), so its
/// side panels take the whole tab.
fn in_tab(ui: &mut Ui, tab: Rect, grow_right: bool, add: impl FnOnce(&mut Ui)) {
    let wide = if grow_right { tab.with_max_x(tab.max.x + MIN_PHOTO_WIDTH) } else { tab.with_min_x(tab.min.x - MIN_PHOTO_WIDTH) };
    let mut child = ui.new_child(UiBuilder::new().max_rect(wide));
    child.set_clip_rect(tab);
    add(&mut child);
}

fn frame(ui: &mut Ui, s: &mut Spike) {
    s.app.logic(ui.ctx());
    s.frames += 1;
    // LightCraft installs its fonts on the first frame; they are usable from the next
    if s.frames < 3 {
        return;
    }
    let app = &mut s.app;
    app.ui.left_panel = false;
    app.ui.presets = false;
    app.ui.left_width = LEFT.width();
    in_tab(ui, LEFT, true, |ui| panels::left::show(app, ui));
    let saved = Tokens::get(ui.ctx());
    let narrow = Tokens { panel_w: PRESETS.width(), ..saved };
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::NULL, narrow));
    in_tab(ui, PRESETS, false, |ui| panels::presets::show(app, ui));
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::NULL, saved));
    in_tab(ui, CENTER, false, |ui| {
        let mut c = ui.new_child(UiBuilder::new().max_rect(CENTER));
        panels::grid::show(app, &mut c);
    });
    app.ui.right = RightPanel::Edit;
    app.ui.right_width = EDIT.width();
    in_tab(ui, EDIT, false, |ui| panels::right::show(app, ui));
    if s.twice {
        app.ui.right = RightPanel::Activity;
        in_tab(ui, HISTORY, false, |ui| panels::right::show(app, ui));
        app.ui.right = RightPanel::Edit;
    }
    let ctx = ui.ctx().clone();
    panels::notices::show(app, &ctx);
    panels::dialogs::show(app, &ctx);
    panels::toast(app, &ctx);
    app.widgets = lightcraft_ui_egui::widgets::take_registry(&ctx);
}

fn harness(twice: bool) -> Harness<'static, Spike> {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    let first = app.session.catalog.photos().next().map(|p| p.id);
    app.session.selection.active = first;
    Harness::builder().with_size([1400.0, 900.0]).build_ui_state(frame, Spike { app, frames: 0, twice })
}

fn widget(s: &Spike, id: &str) -> Rect {
    s.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("no widget {id}: {:?}", s.app.widgets))
}

fn painted_text(h: &Harness<'_, Spike>) -> String {
    h.output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn spike_a_side_panels_fill_their_tabs() {
    let mut h = harness(false);
    h.run_steps(6);
    assert!(h.state().app.session.active().is_some(), "demo session has a photo selected");
    for (id, tab) in [("panel:left_panel", LEFT), ("panel:right_panel", EDIT)] {
        let r = widget(h.state(), id);
        assert!((r.width() - tab.width()).abs() <= 1.0 && (r.min.x - tab.min.x).abs() <= 1.0, "{id}: {r:?} vs {tab:?}");
    }
    let text = painted_text(&h);
    assert!(text.contains("My Photos") && text.contains("Presets"), "{text}");
    assert!(!text.contains("use of"), "{text}");
}

#[test]
fn spike_b_right_panel_twice_in_one_frame_clashes() {
    let mut h = harness(true);
    h.run_steps(6);
    let text = painted_text(&h);
    assert!(text.contains("Second use of Panel ID"), "{text}");
}
