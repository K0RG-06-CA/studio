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
    let mut shell = StudioShell { lightroom: Some(Lightroom::new(app)), ..Default::default() };
    // the demo photos are 24 MP: a document's base renders at 600 px here (CI's runners take a
    // minute or more for the full size on the CPU)
    shell.base.max_edge = Some(600);
    shell
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
    let id = lr.app.session.active().unwrap();
    let edited = lr.app.session.develop_of(id);
    lr.app.session.close_library().unwrap();
    // quit (the library's lock goes with the session) and relaunch: the edit is there
    drop(shell);
    let mut again = lightcraft_engine::Session::new().with_fs();
    again.open_library(&dir, false).unwrap();
    assert_eq!(again.develop_of(id), edited);
    again.close_library().unwrap();
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

/// Run `steps` frames; whether any of them asked the window to close.
fn closes(h: &mut Harness<'_, StudioShell>, steps: usize) -> bool {
    (0..steps).any(|_| {
        h.run_steps(1);
        h.output().viewport_output.values().any(|v| v.commands.contains(&egui::ViewportCommand::Close))
    })
}

#[test]
fn lightcraft_quit_closes_the_window() {
    // LightCraft's Quit buttons (library-problem window, quit prompt) set its quit flag, which
    // only its own `ui` acts on. (Ctrl+Q is egui's own quit shortcut.)
    let mut h = studio();
    h.state_mut().lightroom.as_mut().unwrap().app.library_problem =
        Some(lightcraft_ui_egui::panels::library_problem::LibraryProblem::new("D:/gone", "The drive isn't connected"));
    h.run_steps(10);
    assert!(!closes(&mut h, 3));
    let quit = lc_widget(&h, "button:libraryQuit").expect("the library-problem window's Quit button");
    h.hover_at(quit.center());
    h.run_steps(1);
    for pressed in [true, false] {
        let button = egui::PointerButton::Primary;
        h.event(egui::Event::PointerButton { pos: quit.center(), button, pressed, modifiers: egui::Modifiers::NONE });
    }
    assert!(closes(&mut h, 4), "Quit closes Studio");
}

/// A library store whose writes fail while `fail` is set (an unplugged drive).
#[derive(Clone, Default)]
struct Unplugged {
    files: lightcraft_catalog::MemStore,
    fail: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Unplugged {
    fn check(&self) -> std::io::Result<()> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) { Err(std::io::Error::other("drive unplugged")) } else { Ok(()) }
    }
}

impl lightcraft_catalog::Store for Unplugged {
    fn read(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        self.files.read(name)
    }
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        self.check()?;
        self.files.write_atomic(name, data)
    }
    fn append(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        self.check()?;
        self.files.append(name, data)
    }
    fn truncate(&mut self, name: &str, len: u64) -> std::io::Result<()> {
        self.files.truncate(name, len)
    }
    fn describe(&self) -> String {
        "unplugged".into()
    }
}

#[test]
fn closing_from_pixel_with_unsaved_changes_asks_first() {
    let store = Unplugged::default();
    let mut session = lightcraft_engine::Session::new();
    let catalog = Box::new(store.clone());
    let files = Box::new(lightcraft_catalog::MemStore::new());
    let stores = lightcraft_engine::library::LibraryStores { dir: "unplugged".into(), catalog, files, on_disk: false };
    session.open_library_in(stores, false).unwrap();
    let mut app = LightcraftApp::new(session, Services::default());
    app.ui.settings.gpu = false;
    let shell = StudioShell { lightroom: Some(Lightroom::new(app)), ..Default::default() };
    let mut h = Harness::builder().with_size([1400.0, 900.0]).build_ui_state(|ui, shell: &mut StudioShell| shell.ui(ui), shell);
    h.run_steps(5);
    // a change that can't be written, then off to Pixel and close the window
    store.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    assert!(lr.app.run("album.create", serde_json::json!({"name": "Kept"})).is_err());
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(2);
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    h.run_steps(1);
    let cancelled = h.output().viewport_output.values().any(|v| v.commands.contains(&egui::ViewportCommand::CancelClose));
    assert!(cancelled, "the close is stopped");
    assert_eq!(h.state().persona, Persona::Library, "where LightCraft's prompt shows");
    assert!(lc(&h).quit_prompt.as_deref().is_some_and(|p| p.contains("drive unplugged")));
    // nothing unsaved: closing from Pixel goes ahead
    store.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.quit_prompt = None;
    let _ = lr.app.session.persist();
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(2);
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    h.run_steps(1);
    assert_eq!(h.state().persona, Persona::Pixel);
}

#[test]
fn hovering_a_preset_previews_it_only_while_hovered() {
    let mut h = studio();
    h.run_steps(10);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.session.selection.active = lr.app.session.catalog.photos().next().map(|p| p.id);
    h.state_mut().run("view.develop").unwrap();
    h.run_steps(5);
    let preset = lc(&h).widgets.iter().find(|(w, _)| w.starts_with("preset:")).map(|(_, r)| *r).expect("a preset row");
    h.hover_at(preset.center());
    h.run_steps(3);
    assert!(lc(&h).hover_preview.is_some(), "the loupe previews the hovered preset");
    h.hover_at(egui::pos2(700.0, 400.0));
    h.run_steps(3);
    assert!(lc(&h).hover_preview.is_none(), "and stops once the pointer leaves it");
}

/// A 1ÃƒÆ’Ã¢â‚¬â€1 PNG.
const PNG: [u8; 67] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// A file dropped on the window (what the windowing integration hands egui).
#[derive(Debug)]
struct Dropped(std::path::PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn files_dropped_on_library_are_imported() {
    let dir = std::env::temp_dir().join(format!("studio-drop-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let photo = dir.join("dropped.png");
    std::fs::write(&photo, PNG).unwrap();
    let mut session = lightcraft_engine::Session::new().with_fs();
    session.open_library(dir.join("lib"), false).unwrap();
    let mut app = LightcraftApp::new(session, Services::default());
    app.ui.settings.gpu = false;
    let shell = StudioShell { lightroom: Some(Lightroom::new(app)), ..Default::default() };
    let mut h = Harness::builder().with_size([1400.0, 900.0]).build_ui_state(|ui, shell: &mut StudioShell| shell.ui(ui), shell);
    h.run_steps(5);
    assert!(lc(&h).session.catalog.is_empty());
    h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(photo.clone())));
    let t0 = std::time::Instant::now();
    while lc(&h).session.catalog.is_empty() && t0.elapsed().as_secs() < 20 {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(lc(&h).session.catalog.len(), 1, "the dropped photo is in the library");
    // and in the grid
    h.run_steps(5);
    let id = lc(&h).session.catalog.photos().next().unwrap().id;
    assert!(lc_widget(&h, &format!("thumb:{}", id.0)).is_some());
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.session.close_library().unwrap();
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_lists_develop_steps() {
    let mut h = studio();
    h.run_steps(10);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.session.selection.active = lr.app.session.catalog.photos().nth(1).map(|p| p.id);
    h.state_mut().run("view.develop").unwrap();
    h.run_steps(3);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.contrast", "value": 25})).unwrap();
    h.get_by_label("History").click();
    h.run_steps(5);
    let painted: Vec<String> = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    assert!(painted.iter().any(|t| t.contains("Contrast")), "{painted:?}");
}

// --- Milestone 3: the shared document in the shell ---

/// Pick the first demo photo (Library's selection) so `document.new` has one.
fn pick_first(h: &mut Harness<'_, StudioShell>) {
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.session.selection.active = lr.app.session.catalog.photos().next().map(|p| p.id);
}

/// Run frames until the document's base layer is rendered for its current settings (one frame
/// first: the document mirrors the catalog during a frame).
fn settle_base(h: &mut Harness<'_, StudioShell>) {
    let t0 = std::time::Instant::now();
    h.run_steps(1);
    while !h.state().base_up_to_date() {
        assert!(t0.elapsed().as_secs() < 60, "the base never rendered in {:?}: {:?}", t0.elapsed(), h.state().base_error);
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Mean brightness of the document as PhotoCraft composites it.
fn base_brightness(h: &Harness<'_, StudioShell>) -> f32 {
    let b = photocraft_compose::flatten(&h.state().document.as_ref().unwrap().pixel);
    b.px.iter().map(|p| (p[0] + p[1] + p[2]) / 3.0).sum::<f32>() / b.px.len() as f32
}

#[test]
fn document_new_needs_a_picked_photo_and_puts_its_name_in_the_top_bar() {
    let mut h = studio();
    h.run_steps(3);
    h.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = None;
    assert!(h.state_mut().run("document.new").unwrap_err().contains("no photo"));
    h.get_by_label("Untitled");
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    let doc = h.state().document.as_ref().unwrap();
    let (name, size) = (doc.name.clone(), doc.pixel.size);
    assert!(!doc.unsaved() && size.width > 0, "rendered; a render isn't an edit");
    h.run_steps(1);
    h.get_by_label(name.as_str());
    assert!(h.state().base_error.is_none());
    // an edit marks it unsaved
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.contrast", "value": 20})).unwrap();
    h.run_steps(1);
    assert!(h.state().document.as_ref().unwrap().unsaved());
    h.get_by_label(format!("{name}*").as_str());
    // a second document while this one is unsaved
    assert!(h.state_mut().run("document.new").unwrap_err().contains("unsaved"));
    assert!(h.state_mut().run("document.close").unwrap_err().contains("unsaved"));
    h.state_mut().run_with("document.close", &serde_json::json!({"discard": true})).unwrap();
    h.run_steps(1);
    h.get_by_label("Untitled");
    assert!(h.state_mut().run("document.save").unwrap_err().contains("no document"));
}

#[test]
fn editing_in_develop_re_renders_the_documents_base_layer() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    let before = base_brightness(&h);
    h.state_mut().run("view.develop").unwrap();
    h.run_steps(3);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 2.0})).unwrap();
    h.run_steps(1);
    assert_eq!(h.state().document.as_ref().unwrap().develop.light.exposure, 2.0, "the document mirrors the catalog");
    assert!(!h.state().base_up_to_date(), "a render is due");
    settle_base(&mut h);
    let after = base_brightness(&h);
    assert!(after > before + 0.05, "{before} ÃƒÂ¢Ã¢â‚¬Â Ã¢â‚¬â„¢ {after}");
    // in Pixel too: the base keeps following (sync runs in every persona)
    h.state_mut().run("persona.pixel").unwrap();
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    let id = lr.app.session.active().unwrap();
    let mut s = (*lr.app.session.develop_of(id).unwrap()).clone();
    s.light.exposure = -2.0;
    lr.app.session.set_develop(id, s, "Exposure").unwrap();
    settle_base(&mut h);
    assert!(base_brightness(&h) < before, "{before} ÃƒÂ¢Ã¢â‚¬Â Ã¢â‚¬â„¢ {}", base_brightness(&h));
}

#[test]
fn a_saved_document_comes_back_after_a_relaunch() {
    let dir = std::env::temp_dir().join(format!("studio-doc-relaunch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    let name = h.state().document.as_ref().unwrap().name.clone();
    // a demo photo has no file to save beside
    assert!(h.state_mut().run("document.save").unwrap_err().contains("saveAs"));
    let path = dir.join("trip.studio");
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    assert!(path.is_file());
    assert_eq!(h.state().document_path.as_deref(), Some(path.as_path()));
    h.run_steps(1);
    h.get_by_label(name.as_str());
    // a further edit, then plain save goes to the same file
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    settle_base(&mut h);
    assert!(h.state().document.as_ref().unwrap().unsaved());
    h.state_mut().run("document.save").unwrap();
    assert!(!h.state().document.as_ref().unwrap().unsaved());
    // relaunch: the shell state is restored, LightCraft set, the document reopened
    let saved = ron::to_string(h.state()).unwrap();
    drop(h);
    let mut shell = StudioShell::restore(Some(ron::from_str(&saved).unwrap()));
    assert!(shell.document.is_none() && shell.document_path.is_some());
    shell.lightroom = demo_shell().lightroom;
    shell.base.max_edge = Some(600);
    shell.reopen().unwrap();
    let doc = shell.document.as_ref().unwrap();
    assert_eq!((doc.name.as_str(), doc.develop.light.exposure, doc.path.as_deref()), (name.as_str(), 1.0, Some(path.as_path())));
    // and the file's settings reached the (fresh) library
    let lr = shell.lightroom.as_ref().unwrap();
    assert_eq!(lr.app.session.develop_of(doc.source.photo).unwrap().light.exposure, 1.0);
    let mut h = studio_with(Harness::builder().with_size([1400.0, 900.0]));
    *h.state_mut() = shell;
    settle_base(&mut h);
    h.get_by_label(name.as_str());
    // a file that can't be opened right now (here: corrupt) is reported and kept for next time
    h.state_mut().run_with("document.close", &serde_json::json!({"discard": true})).unwrap();
    h.state_mut().document_path = Some(path.clone());
    std::fs::write(&path, b"not a document").unwrap();
    assert!(h.state_mut().reopen().unwrap_err().contains("not a .studio file"));
    assert_eq!(h.state().document_path.as_deref(), Some(path.as_path()), "kept: the file is there");
    // one that is gone is forgotten
    std::fs::remove_file(&path).unwrap();
    assert!(h.state_mut().reopen().unwrap_err().contains("trip.studio"));
    assert_eq!(h.state().document_path, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_base_render_in_flight_keeps_the_app_repainting() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    // Pixel: LightCraft's own logic (and its repaint requests) doesn't run here
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(1);
    assert!(h.state().base.rendering(), "the first render is in flight");
    assert!(h.ctx.has_requested_repaint(), "the shell polls for the render");
    settle_base(&mut h);
    h.run_steps(3);
    h.step();
    assert!(!h.ctx.has_requested_repaint(), "idle once the base is rendered");
}

/// An edit in Develop's settings for the document's photo (the catalog is their home).
fn edit(h: &mut Harness<'_, StudioShell>, exposure: f64) {
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": exposure})).unwrap();
    h.run_steps(1);
    assert!(h.state().document.as_ref().unwrap().unsaved());
}

/// A new, rendered document on the first demo photo, saved to `path`, then edited.
fn edited_document(h: &mut Harness<'_, StudioShell>, path: &std::path::Path) {
    h.run_steps(3);
    pick_first(h);
    h.state_mut().run("document.new").unwrap();
    settle_base(h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    edit(h, 1.5);
}

fn prompt_shown(h: &Harness<'_, StudioShell>) -> bool {
    h.query_by_label("Don't Save").is_some()
}

/// Click a button of the prompt (or any button) by its label.
fn click_button(h: &mut Harness<'_, StudioShell>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("studio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn replacing_an_unsaved_document_asks_save_dont_save_or_cancel() {
    let dir = temp_dir("prompt-replace");
    let path = dir.join("trip.studio");
    let mut h = studio();
    edited_document(&mut h, &path);
    // Cancel: nothing happens
    assert!(h.state_mut().run("document.new").unwrap_err().contains("unsaved"));
    h.run_steps(1);
    assert!(prompt_shown(&h), "new asks");
    h.get_by_label(format!("Save changes to \u{201c}{}\u{201d} before closing?", h.state().document.as_ref().unwrap().name).as_str());
    click_button(&mut h, "Cancel");
    assert!(!prompt_shown(&h) && h.state().prompt.is_none());
    assert!(h.state().document.as_ref().unwrap().unsaved(), "kept, edit and all");
    // Save: the edit is written, then the new document replaces it
    assert!(h.state_mut().run("document.new").is_err());
    h.run_steps(1);
    click_button(&mut h, "Save");
    assert_eq!(studio_doc::Document::open(&path).unwrap().develop.light.exposure, 1.5, "saved first");
    let doc = h.state().document.as_ref().unwrap();
    assert!(doc.path.is_none() && !doc.unsaved(), "then replaced by the new one");
    // open: Don't Save drops the edit and opens the file
    settle_base(&mut h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    edit(&mut h, -1.0);
    assert!(h.state_mut().run_with("document.open", &serde_json::json!({"path": path})).is_err());
    assert_eq!(h.state().prompt, Some(crate::Pending::Open(path.clone())));
    h.run_steps(1);
    click_button(&mut h, "Don't Save");
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!((doc.develop.light.exposure, doc.unsaved()), (1.5, false), "the file as saved");
    // close: Don't Save closes it; a clean document closes without asking
    edit(&mut h, 0.5);
    assert!(h.state_mut().run("document.close").is_err());
    h.run_steps(1);
    click_button(&mut h, "Don't Save");
    assert!(h.state().document.is_none() && !prompt_shown(&h));
    h.state_mut().run_with("document.open", &serde_json::json!({"path": path})).unwrap();
    h.state_mut().run("document.close").unwrap();
    h.run_steps(1);
    assert!(!prompt_shown(&h) && h.state().prompt.is_none());
    assert!(h.state_mut().run("document.prompt.save").unwrap_err().contains("nothing is waiting"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// What happened while running frames as a window does: a `Close` command makes the next frame's
/// close request, which closes the window unless that frame sends `CancelClose`.
#[derive(Debug, Default, PartialEq)]
struct Window {
    /// `Close` commands sent.
    closes: usize,
    /// Close requests stopped.
    cancels: usize,
    closed: bool,
}

/// Press Ctrl+Q (Cmd+Q on macOS) for the next frame. Straight into the input, not kittest's
/// queue: that runs one pass per event and keeps only the last pass's output, so the `Close` sent
/// on the press would never be seen.
fn ctrl_q(h: &mut Harness<'_, StudioShell>) {
    let (key, modifiers) = (egui::Key::Q, egui::Modifiers::COMMAND);
    h.input_mut().events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
}

fn run_window(h: &mut Harness<'_, StudioShell>, frames: usize, mut requested: bool) -> Window {
    let mut w = Window::default();
    for _ in 0..frames {
        if requested {
            h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
        }
        h.run_steps(1);
        let sent = |c: &egui::ViewportCommand| h.output().viewport_output.values().any(|v| v.commands.contains(c));
        let (close, cancel) = (sent(&egui::ViewportCommand::Close), sent(&egui::ViewportCommand::CancelClose));
        if requested && !cancel {
            w.closed = true;
            return w;
        }
        w.cancels += usize::from(requested && cancel);
        w.closes += usize::from(close);
        requested = close;
    }
    w
}

/// Click a prompt button; its effect shows from the next frame.
fn answer(h: &mut Harness<'_, StudioShell>, label: &str) {
    h.get_by_label(label).click();
}

#[test]
fn closing_the_window_with_an_unsaved_document_asks_first_in_every_persona() {
    let dir = temp_dir("prompt-close");
    let path = dir.join("trip.studio");
    for p in [Persona::Develop, Persona::Pixel] {
        let mut h = studio();
        edited_document(&mut h, &path);
        h.state_mut().run(&p.command()).unwrap();
        h.run_steps(2);
        // the window's close button: stopped, and the prompt shows
        assert_eq!(run_window(&mut h, 3, true), Window { closes: 0, cancels: 1, closed: false }, "{p:?}");
        assert_eq!(h.state().prompt, Some(crate::Pending::Quit));
        answer(&mut h, "Cancel");
        assert_eq!(run_window(&mut h, 5, false), Window::default(), "{p:?}: Cancel keeps the window");
        assert!(h.state().document.as_ref().unwrap().unsaved(), "and the edit");
        // Don't Save: the window closes, and the next launch opens the file as saved
        // the prompt's first frame is its sizing pass: it takes clicks from the next
        run_window(&mut h, 2, true);
        answer(&mut h, "Don't Save");
        let w = run_window(&mut h, 5, false);
        assert!(w.closed && w.closes == 1, "{p:?}: {w:?}");
        assert!(h.state().document.is_none());
        assert_eq!(h.state().document_path.as_deref(), Some(path.as_path()));
    }
    // Save: written, then closed
    let mut h = studio();
    edited_document(&mut h, &path);
    run_window(&mut h, 2, true);
    answer(&mut h, "Save");
    assert!(run_window(&mut h, 5, false).closed);
    assert_eq!(studio_doc::Document::open(&path).unwrap().develop.light.exposure, 1.5);
    // nothing unsaved: closes straight away
    let mut h = studio();
    edited_document(&mut h, &path);
    h.state_mut().run("document.save").unwrap();
    assert!(run_window(&mut h, 2, true).closed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saving_a_document_without_a_file_asks_where_and_a_cancelled_dialog_keeps_everything() {
    let dir = temp_dir("prompt-pick");
    let path = dir.join("picked.studio");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut h = studio();
    // a demo photo's document: no file, nowhere beside a source file
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    edit(&mut h, 1.5);
    let name = h.state().document.as_ref().unwrap().name.clone();
    // the dialog is cancelled: the prompt stays, nothing closes
    let log = asked.clone();
    h.state_mut().pick_save_path = Some(Box::new(move |file| {
        log.lock().unwrap().push(file.to_owned());
        None
    }));
    run_window(&mut h, 2, true);
    answer(&mut h, "Save");
    assert_eq!(run_window(&mut h, 4, false), Window::default());
    assert_eq!(*asked.lock().unwrap(), [format!("{name}.studio")], "suggests <name>.studio");
    assert_eq!(h.state().prompt, Some(crate::Pending::Quit), "still asking");
    assert!(h.state().document.as_ref().unwrap().unsaved());
    // a file is picked: saved there, then closed
    let picked = path.clone();
    h.state_mut().pick_save_path = Some(Box::new(move |_| Some(picked.clone())));
    answer(&mut h, "Save");
    assert!(run_window(&mut h, 4, false).closed);
    assert_eq!(studio_doc::Document::open(&path).unwrap().develop.light.exposure, 1.5);
    assert_eq!(h.state().document_path.as_deref(), Some(path.as_path()), "opened again next launch");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_save_says_why_and_keeps_the_document_and_the_window() {
    let dir = temp_dir("save-error");
    let mut h = studio();
    edited_document(&mut h, &dir.join("trip.studio"));
    // its folder is now a file: every save fails
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"").unwrap();
    h.state_mut().document.as_mut().unwrap().path = Some(blocker.join("trip.studio"));
    let name = h.state().document.as_ref().unwrap().name.clone();
    run_window(&mut h, 2, true);
    answer(&mut h, "Save");
    assert_eq!(run_window(&mut h, 4, false), Window::default(), "the window stays");
    h.get_by_label(format!("Couldn\u{2019}t save \u{201c}{name}\u{201d}").as_str());
    let why = h.state().save_error.clone().unwrap();
    assert!(why.contains("blocker"), "{why}");
    h.get_by_label(why.as_str());
    assert_eq!(h.state().prompt, Some(crate::Pending::Quit), "still asking");
    assert!(h.state().document.as_ref().unwrap().unsaved(), "nothing lost");
    answer(&mut h, "OK");
    h.run_steps(2);
    assert!(h.state().save_error.is_none() && h.query_by_label(why.as_str()).is_none());
    assert!(prompt_shown(&h), "back to the prompt");
    // a save from anywhere else shows it too
    answer(&mut h, "Cancel");
    h.run_steps(2);
    assert!(h.state_mut().run("document.save").is_err());
    h.run_steps(2);
    h.get_by_label(why.as_str());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exiting_with_unsaved_changes_anyway_saves_them_never_over_an_earlier_recovery() {
    let dir = temp_dir("exit-save");
    let recovered = dir.join("Recovered");
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    assert_eq!(h.state_mut().save_for_exit(&recovered).unwrap(), None, "nothing unsaved");
    // a demo photo's document has no file: it goes into Recovered, never through a dialog
    h.state_mut().pick_save_path = Some(Box::new(|_| panic!("no dialog at exit")));
    edit(&mut h, 1.5);
    let name = h.state().document.as_ref().unwrap().name.clone();
    let first = h.state_mut().save_for_exit(&recovered).unwrap().unwrap();
    assert_eq!(first, recovered.join(format!("{name}.studio")));
    assert_eq!(studio_doc::Document::open(&first).unwrap().develop.light.exposure, 1.5);
    // the next one doesn't replace it
    h.state_mut().document.as_mut().unwrap().path = None;
    edit(&mut h, -1.0);
    let second = h.state_mut().save_for_exit(&recovered).unwrap().unwrap();
    assert_eq!(second, recovered.join(format!("{name} 2.studio")));
    assert_eq!(studio_doc::Document::open(&first).unwrap().develop.light.exposure, 1.5, "kept");
    // one with a file is saved to it
    edit(&mut h, 0.5);
    assert_eq!(h.state_mut().save_for_exit(&recovered).unwrap(), Some(second.clone()));
    assert_eq!(studio_doc::Document::open(&second).unwrap().develop.light.exposure, 0.5);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ctrl_q_asks_exactly_once_in_every_persona() {
    let dir = temp_dir("prompt-quit");
    let path = dir.join("trip.studio");
    let mut h = studio();
    edited_document(&mut h, &path);
    for p in [Persona::Library, Persona::Develop, Persona::Film, Persona::Pixel] {
        h.state_mut().run(&p.command()).unwrap();
        h.run_steps(3);
        ctrl_q(&mut h);
        // one Close, stopped once; one prompt (a second "Don't Save" would fail the lookup)
        assert_eq!(run_window(&mut h, 6, false), Window { closes: 1, cancels: 1, closed: false }, "{p:?}");
        assert_eq!(h.state().prompt, Some(crate::Pending::Quit));
        h.get_by_label("Don't Save");
        assert!(lc(&h).quit_prompt.is_none(), "{p:?}: LightCraft's own prompt stays away (its library is saved)");
        answer(&mut h, "Cancel");
        assert_eq!(run_window(&mut h, 5, false), Window::default(), "{p:?}: nothing more");
    }
    // nothing unsaved: Ctrl+Q quits
    h.state_mut().run("document.save").unwrap();
    ctrl_q(&mut h);
    assert!(run_window(&mut h, 6, false).closed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saving_right_after_an_edit_writes_the_edit() {
    let dir = std::env::temp_dir().join(format!("studio-doc-save-now-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    let path = dir.join("now.studio");
    // the edit and the save in the same frame, as a panel's slider release followed by Ctrl+S
    // (or quitting) does: no sync in between
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.25})).unwrap();
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    let saved = studio_doc::Document::open(&path).unwrap();
    assert_eq!(saved.develop.light.exposure, 1.25);
    assert_eq!(h.state().document.as_ref().unwrap().develop.light.exposure, 1.25);
    let _ = std::fs::remove_dir_all(&dir);
}
