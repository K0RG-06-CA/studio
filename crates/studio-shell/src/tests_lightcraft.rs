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
use egui_kittest::kittest::{NodeT, Queryable};
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

// --- Studio with LightCraft in the shell ---

use crate::lightroom::Lightroom;
use crate::{Persona, StudioShell};

/// The shell running LightCraft on its in-memory demo library (no disk, no GPU work).
pub(crate) fn demo_shell() -> StudioShell {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    StudioShell { lightroom: Some(Lightroom::new(app)), ..Default::default() }
}

/// Studio's theme applied once, at the start, as the app does.
pub(crate) fn studio() -> Harness<'static, StudioShell> {
    studio_with(Harness::builder().with_size([1400.0, 900.0]))
}

pub(crate) fn studio_with(builder: egui_kittest::HarnessBuilder<StudioShell>) -> Harness<'static, StudioShell> {
    let shell = demo_shell();
    let mut themed = false;
    builder.build_ui_state(
        move |ui, shell: &mut StudioShell| {
            if !std::mem::replace(&mut themed, true) {
                crate::theme::apply(ui.ctx());
            }
            shell.ui(ui);
        },
        shell,
    )
}

fn assert_studio_style(h: &Harness<'_, StudioShell>, when: &str) {
    let s = h.ctx.global_style();
    let body = s.text_styles.get(&egui::TextStyle::Body).map(|f| f.size);
    assert_eq!(body, Some(crate::theme::TOKENS.font_size), "{when}: body text size");
    assert_eq!(s.visuals.override_text_color, None, "{when}: LightCraft's text colour leaked");
    assert_eq!(s.visuals.panel_fill, crate::theme::TOKENS.chrome, "{when}: panel fill");
}

#[test]
fn studio_theme_survives_lightcraft_frames_and_persona_switches() {
    let mut h = studio();
    for n in [1, 2, 5, 10] {
        h.run_steps(1);
        assert_studio_style(&h, &format!("Library, {n} frames"));
        h.run_steps(n - 1);
    }
    for (p, steps) in [("pixel", 5), ("develop", 5), ("film", 3), ("library", 5)] {
        h.state_mut().run(&format!("persona.{p}")).unwrap();
        h.run_steps(steps);
        assert_studio_style(&h, &format!("after switching to {p}"));
    }
    assert!(h.state().lightroom.as_ref().is_some_and(Lightroom::ready));
    assert_eq!(h.state().persona, Persona::Library);
}

fn lc_widget(h: &Harness<'_, StudioShell>, id: &str) -> Option<Rect> {
    let lr = h.state().lightroom.as_ref()?;
    lr.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r)
}

pub(crate) fn click_at(h: &mut Harness<'_, StudioShell>, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

#[test]
fn library_shows_lightcraft_in_lightroom_classic_places() {
    let mut h = studio();
    h.run_steps(10);
    for tab in ["Folders", "Grid", "Keywording", "Metadata"] {
        h.get_by_label(tab);
    }
    // sidebar in the left column, photo grid in the middle, keywords on the right
    let left = lc_widget(&h, "panel:left_panel").expect("sidebar");
    let right = lc_widget(&h, "panel:right_panel").expect("right panel");
    let lr = h.state().lightroom.as_ref().unwrap();
    let ids: Vec<_> = lr.app.session.catalog.photos().map(|p| p.id).collect();
    let thumb = ids.iter().find_map(|id| lc_widget(&h, &format!("thumb:{}", id.0)).map(|r| (*id, r))).expect("a thumbnail");
    assert!(left.right() <= thumb.1.left() && thumb.1.right() <= right.left(), "{left:?} {thumb:?} {right:?}");
    // flush with their tabs, inside the dock's 8 pt gutter
    assert!(left.left() <= 8.0 && right.right() >= 1392.0, "{left:?} {right:?}");
    assert_eq!(h.state().lightroom.as_ref().unwrap().app.ui.right, RightPanel::Keywords);
    // clicking a thumbnail selects that photo
    let other = ids.iter().rev().find_map(|id| lc_widget(&h, &format!("thumb:{}", id.0)).map(|r| (*id, r))).unwrap();
    click_at(&mut h, other.1.center());
    assert_eq!(h.state().lightroom.as_ref().unwrap().app.session.active(), Some(other.0));
    // the Metadata tab shows LightCraft's Info panel
    h.get_by_label("Metadata").click();
    h.run_steps(3);
    assert_eq!(h.state().lightroom.as_ref().unwrap().app.ui.right, RightPanel::Info);
}

#[test]
fn develop_edits_the_photo_picked_in_library() {
    let mut h = studio();
    h.run_steps(10);
    let lr = h.state().lightroom.as_ref().unwrap();
    let last = lr.app.session.catalog.photos().filter_map(|p| lc_widget(&h, &format!("thumb:{}", p.id.0)).map(|r| (p.id, r))).last();
    let (id, thumb) = last.expect("a thumbnail");
    click_at(&mut h, thumb.center());
    h.get_by_label("Develop").click();
    h.run_steps(5);
    for tab in ["Presets", "Loupe", "Edit", "History"] {
        h.get_by_label(tab);
    }
    let lr = h.state().lightroom.as_ref().unwrap();
    assert_eq!(lr.app.session.active(), Some(id), "Develop shows the photo picked in Library");
    assert_eq!(lr.app.ui.right, RightPanel::Edit);
    // the loupe in the middle, Edit on the right
    let image = lc_widget(&h, "canvas:image").expect("loupe");
    let edit = lc_widget(&h, "panel:right_panel").expect("Edit panel");
    assert!(image.right() <= edit.left(), "{image:?} {edit:?}");
    // a slider command edits that photo
    let develop = |h: &Harness<'_, StudioShell>| h.state().lightroom.as_ref().unwrap().app.session.develop_of(id);
    let before = develop(&h);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 0.7})).unwrap();
    h.run_steps(2);
    assert_ne!(before, develop(&h));
    // History shows LightCraft's activity
    h.get_by_label("History").click();
    h.run_steps(3);
    assert_eq!(h.state().lightroom.as_ref().unwrap().app.ui.right, RightPanel::Activity);
    // back in Library: the grid it was showing
    h.get_by_label("Library").click();
    h.run_steps(3);
    h.get_by_label("Grid");
}

fn lc<'h>(h: &'h Harness<'_, StudioShell>) -> &'h LightcraftApp {
    &h.state().lightroom.as_ref().unwrap().app
}

fn press(h: &mut Harness<'_, StudioShell>, key: egui::Key) {
    h.key_press(key);
    h.run_steps(3);
}

#[test]
fn g_e_d_r_switch_modules_like_lightroom_classic() {
    use lightcraft_ui_egui::state::{BeforeAfter, ViewMode};
    let mut h = studio();
    h.run_steps(10);
    press(&mut h, egui::Key::D);
    assert_eq!((h.state().persona, lc(&h).ui.view, lc(&h).ui.right), (Persona::Develop, ViewMode::Detail, RightPanel::Edit));
    press(&mut h, egui::Key::G);
    assert_eq!((h.state().persona, lc(&h).ui.view), (Persona::Library, ViewMode::PhotoGrid));
    h.get_by_label("Grid");
    press(&mut h, egui::Key::E);
    assert_eq!((h.state().persona, lc(&h).ui.view), (Persona::Library, ViewMode::Detail));
    h.get_by_label("Loupe");
    // Library keeps its own view while Develop shows the loupe
    press(&mut h, egui::Key::G);
    press(&mut h, egui::Key::D);
    h.get_by_label("Library").click();
    h.run_steps(3);
    assert_eq!(lc(&h).ui.view, ViewMode::PhotoGrid);
    // R: crop in Develop, in the Edit tab even when History was showing
    press(&mut h, egui::Key::D);
    h.get_by_label("History").click();
    h.run_steps(3);
    press(&mut h, egui::Key::R);
    assert_eq!((h.state().persona, lc(&h).ui.right), (Persona::Develop, RightPanel::Crop));
    assert!(lc_widget(&h, "panel:right_panel").is_some(), "crop panel shows");
    press(&mut h, egui::Key::R);
    assert_eq!(lc(&h).ui.right, RightPanel::Crop, "R again keeps crop open");
    // \ is LightCraft's: before/after
    press(&mut h, egui::Key::Backslash);
    assert_eq!(lc(&h).ui.before_after, BeforeAfter::Original);
}

#[test]
fn ctrl_numbers_only_switch_personas() {
    let mut h = studio();
    h.run_steps(10);
    press(&mut h, egui::Key::D);
    let sections = lc(&h).ui.open_sections.clone();
    for n in [egui::Key::Num2, egui::Key::Num1, egui::Key::Num2] {
        h.key_press_modifiers(egui::Modifiers::COMMAND, n);
        h.run_steps(3);
    }
    assert_eq!(h.state().persona, Persona::Develop);
    assert_eq!(lc(&h).ui.open_sections, sections, "LightCraft's Cmd+1/2 (Edit sections) didn't fire");
}

#[test]
fn lightroom_keys_do_nothing_in_pixel() {
    let mut h = studio();
    h.run_steps(10);
    let marks =
        |h: &Harness<'_, StudioShell>| -> Vec<String> { lc(h).session.catalog.photos().map(|p| format!("{} {:?}", p.rating, p.flag)).collect() };
    let before = marks(&h);
    h.get_by_label("Pixel").click();
    h.run_steps(3);
    for key in [egui::Key::G, egui::Key::D, egui::Key::E, egui::Key::R, egui::Key::P, egui::Key::X, egui::Key::Num3, egui::Key::Num0] {
        press(&mut h, key);
    }
    assert_eq!(h.state().persona, Persona::Pixel);
    assert_eq!(marks(&h), before, "no photo was flagged or rated from Pixel");
}

#[test]
fn lightroom_keys_type_into_text_fields() {
    let shell = demo_shell();
    let mut text = String::new();
    let mut h = Harness::builder().with_size([1400.0, 900.0]).build_ui_state(
        move |ui, shell: &mut StudioShell| {
            egui::Window::new("Find").show(ui.ctx(), |ui| {
                let label = ui.label("Test field");
                ui.text_edit_singleline(&mut text).labelled_by(label.id);
            });
            shell.ui(ui);
        },
        shell,
    );
    h.run_steps(10);
    let field = h.get_all_by_role(egui::accesskit::Role::TextInput).find(|n| n.accesskit_node().label().as_deref() == Some("Test field"));
    field.expect("the test field").focus();
    h.run_steps(2);
    for key in [egui::Key::D, egui::Key::E, egui::Key::R, egui::Key::G] {
        press(&mut h, key);
        let at = (h.state().persona, lc(&h).ui.view);
        assert_eq!(at, (Persona::Library, lightcraft_ui_egui::state::ViewMode::PhotoGrid), "{key:?} while typing");
    }
    // and with the field unfocused, D works again
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    press(&mut h, egui::Key::D);
    assert_eq!(h.state().persona, Persona::Develop);
}

#[test]
fn leaving_develop_saves_its_edits() {
    let dir = std::env::temp_dir().join(format!("studio-leave-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut session = lightcraft_engine::Session::new().with_fs();
    session.open_library(&dir, true).unwrap();
    let mut app = LightcraftApp::new(session, Services::default());
    app.ui.settings.gpu = false;
    let first = app.session.catalog.photos().next().map(|p| p.id);
    app.session.selection.active = first;
    let mut shell = StudioShell { lightroom: Some(Lightroom::new(app)), ..Default::default() };
    shell.run("persona.develop").unwrap();
    let log = dir.join("catalog.log");
    let size = || std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    let lr = shell.lightroom.as_mut().unwrap();
    lr.app.session.persist_if_dirty();
    let before = size();
    // a slider drag still in progress (LightCraft commits it on release, which the Edit panel
    // never sees once Pixel shows), then straight to Pixel
    lr.app.session.begin_interaction("Exposure").unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 0.7})).unwrap();
    assert_eq!(size(), before, "not saved yet");
    shell.run("persona.pixel").unwrap();
    assert!(size() > before, "the edit is in the library's log on disk");
    let lr = shell.lightroom.as_mut().unwrap();
    lr.app.session.close_library().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

fn mouse_drag(h: &mut Harness<'_, StudioShell>, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    for i in 1..=12 {
        h.hover_at(from + (to - from) * (i as f32 / 12.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(3);
}

/// Screen x of the dock's two column separators (left column | canvas | right column).
fn separators(h: &Harness<'_, StudioShell>, p: Persona) -> [f32; 2] {
    let f: Vec<f32> = h.state().layouts[p as usize]
        .main_surface()
        .iter()
        .filter_map(|n| match n {
            egui_dock::Node::Horizontal(s) => Some(s.fraction),
            _ => None,
        })
        .collect();
    // inside the dock's 8 pt gutter; the right split divides what the left column leaves
    let left = 8.0 + 1384.0 * f[0];
    [left, left + (1392.0 - left) * f[1]]
}

#[test]
fn dock_columns_resize_by_their_separators_around_lightcraft_panels() {
    let mut h = studio();
    h.run_steps(10);
    for p in [Persona::Library, Persona::Develop] {
        h.state_mut().run(&p.command()).unwrap();
        h.run_steps(5);
        // on the separator and just inside the side panel next to it (just inside the canvas is the
        // grid's scroll bar, as in Lightroom)
        for (edge, into_panel) in [(0, -2.0), (1, 2.0)] {
            for dx in [0.0, into_panel] {
                let x = separators(&h, p)[edge];
                mouse_drag(&mut h, egui::pos2(x + dx, 450.0), egui::pos2(x + dx + 60.0, 450.0));
                let moved = separators(&h, p)[edge] - x;
                assert!(moved > 30.0, "{p:?}: separator {edge} grabbed at {dx:+}: moved {moved}");
                h.state_mut().run("window.resetWorkspace").unwrap();
                h.run_steps(3);
            }
        }
    }
}
