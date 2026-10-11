//! UI parity, T0 spike (`docs/ui-parity-plan.md`): what LightCraft and PhotoCraft do when their
//! whole windows are drawn under Studio's persona bar, as they ship.
//!
//! (a) each app's own window nests under a host bar: every accessible widget is the bare app's,
//!     moved down by the bar (so the parity harness can compare accessibility trees);
//! (b) both apps replace all fonts and the last one installed wins: Studio's Library still draws
//!     with PhotoCraft's after a visit to Pixel (T5 swaps them per persona).

use egui::Ui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
use lightcraft_ui_egui::{LightcraftApp, Services};
use photocraft_ui_egui::PhotocraftApp;

/// Studio's persona bar height (points).
const BAR: f32 = 34.0;
const SIZE: [f32; 2] = [1400.0, 900.0];

/// The host's bar: an empty panel of [`BAR`], as Studio's top bar takes its place.
fn bar(ui: &mut Ui) {
    egui::Panel::top("spike.bar").exact_size(BAR).resizable(false).frame(egui::Frame::NONE).show(ui, |_| {});
}

/// Every labelled accessible node: role, label and rect (whole points, moved up by `dy`), sorted.
fn tree<S>(h: &Harness<'_, S>, dy: f32) -> Vec<String> {
    let mut nodes: Vec<String> = h
        .query_all(by().predicate(|n| n.label().is_some_and(|l| !l.is_empty())))
        .map(|n| {
            let r = n.rect().translate(egui::vec2(0.0, -dy));
            let label = n.accesskit_node().label().unwrap_or_default();
            format!("{:?} {label:?} [{:.0} {:.0} {:.0} {:.0}]", n.accesskit_node().role(), r.min.x, r.min.y, r.max.x, r.max.y)
        })
        .collect();
    nodes.sort();
    nodes
}

fn lightcraft() -> LightcraftApp {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    app.session.selection.active = app.session.catalog.photos().next().map(|p| p.id);
    app
}

fn lightcraft_harness(host: bool) -> Harness<'static, LightcraftApp> {
    let size = if host { SIZE } else { [SIZE[0], SIZE[1] - BAR] };
    Harness::builder().with_size(size).build_ui_state(
        move |ui, app: &mut LightcraftApp| {
            if host {
                bar(ui);
            }
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        lightcraft(),
    )
}

#[test]
fn t0_lightcrafts_whole_window_nests_under_a_host_bar() {
    let (mut hosted, mut bare) = (lightcraft_harness(true), lightcraft_harness(false));
    hosted.run_steps(8);
    bare.run_steps(8);
    let (hosted, bare) = (tree(&hosted, BAR), tree(&bare, 0.0));
    assert!(bare.len() > 10, "{bare:#?}");
    assert_eq!(hosted, bare);
}

struct Pc {
    app: PhotocraftApp,
    frame: eframe::Frame,
}

fn photocraft_harness(host: bool) -> Harness<'static, Pc> {
    let size = if host { SIZE } else { [SIZE[0], SIZE[1] - BAR] };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
    app.session.add_document(crate::tests_photocraft::pixel_doc(), None);
    app.sync_views();
    Harness::builder().with_size(size).build_ui_state(
        move |ui, pc: &mut Pc| {
            if host {
                bar(ui);
            }
            let ctx = ui.ctx().clone();
            eframe::App::logic(&mut pc.app, &ctx, &mut pc.frame);
            eframe::App::ui(&mut pc.app, ui, &mut pc.frame);
        },
        Pc { app, frame: eframe::Frame::_new_kittest() },
    )
}

#[test]
fn t0_photocrafts_whole_window_nests_under_a_host_bar() {
    let (mut hosted, mut bare) = (photocraft_harness(true), photocraft_harness(false));
    hosted.run_steps(8);
    bare.run_steps(8);
    let (hosted, bare) = (tree(&hosted, BAR), tree(&bare, 0.0));
    assert!(bare.len() > 10, "{bare:#?}");
    assert_eq!(hosted, bare);
}

/// PhotoCraft's own faces: LightCraft has no JetBrains Mono.
fn photocraft_fonts(h: &Harness<'_, crate::StudioShell>) -> bool {
    h.ctx.fonts(|f| f.definitions().font_data.contains_key("JetBrainsMono"))
}

#[test]
fn t0_after_a_visit_to_pixel_library_draws_with_photocrafts_fonts() {
    let mut h = crate::tests_lightcraft::studio();
    h.run_steps(4);
    assert!(!photocraft_fonts(&h), "Library starts with LightCraft's fonts");
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(4);
    assert!(photocraft_fonts(&h));
    h.state_mut().run("persona.library").unwrap();
    h.run_steps(4);
    // LightCraft installs its fonts once (again only when the language changes)
    assert!(photocraft_fonts(&h), "fixed: LightCraft put its fonts back");
}

/// Studio's history: (made in Develop, made in Pixel) per row, and the row it is at.
fn history(h: &Harness<'_, crate::StudioShell>) -> (Vec<(bool, bool)>, usize) {
    let d = h.state().document.as_ref().unwrap();
    (d.history.entries().into_iter().map(|e| (e.develop, e.pixel)).collect(), d.history.position())
}

#[test]
fn t0_lightcrafts_edit_undo_menu_item_is_recorded_as_a_new_develop_step() {
    let mut h = crate::tests_lightcraft::studio();
    h.run_steps(3);
    crate::tests_lightcraft::pick_first(&mut h);
    h.state_mut().run("persona.develop").unwrap();
    h.state_mut().run("document.new").unwrap();
    crate::tests_lightcraft::settle_base(&mut h);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    h.run_steps(1);
    assert_eq!(history(&h), (vec![(false, false), (true, false)], 1));
    // what a click on LightCraft's Edit › Undo runs (its in-window menu bar)
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lightcraft_ui_egui::menubar::run_item(&mut lr.app, "edit.undo", serde_json::Value::Null).unwrap();
    h.run_steps(1);
    assert_eq!(history(&h), (vec![(false, false), (true, false), (true, false)], 2), "fixed: Studio took it as its undo");
}

#[test]
fn t0_photocrafts_edit_undo_menu_item_undoes_two_steps_at_once() {
    let mut h = crate::tests_lightcraft::studio();
    h.run_steps(3);
    crate::tests_lightcraft::pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    crate::tests_lightcraft::settle_base(&mut h);
    crate::tests_photocraft::add_layer(&mut h, "Paint");
    h.run_steps(1);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    h.run_steps(1);
    assert_eq!(history(&h), (vec![(false, false), (false, true), (true, false)], 2));
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    // what a click on PhotoCraft's Edit › Undo runs (its in-window menu bar)
    let ctx = h.ctx.clone();
    let px = h.state_mut().pixel.as_mut().unwrap();
    photocraft_ui_egui::menus::invoke(&mut px.app, &ctx, "edit.undo", serde_json::Value::Null).unwrap();
    h.run_steps(2);
    let layers = h.state().document.as_ref().unwrap().pixel.layers.len();
    let id = h.state().document.as_ref().unwrap().photo().unwrap();
    let exposure = h.state().lightroom.as_ref().unwrap().app.session.develop_of(id).unwrap().light.exposure;
    // Studio reads PhotoCraft's undo of its own Pixel step as a jump to before it: the newer
    // Develop step goes too, in one click (Studio's undo takes only the newest step)
    assert_eq!((layers, exposure, history(&h)), (1, 0.0, (vec![(false, false), (false, true), (true, false)], 0)), "fixed: one step undone");
}
