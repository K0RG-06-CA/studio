//! UI parity, T0 spike (`docs/ui-parity-plan.md`): what LightCraft and PhotoCraft do when their
//! whole windows are drawn under Studio's persona bar, as they ship.
//!
//! (a) each app's own window nests under a host bar: every accessible widget is the bare app's,
//!     moved down by the bar (so the parity harness can compare accessibility trees);
//! (b) both apps replace all fonts and the last one installed wins: Studio's Library still draws
//!     with PhotoCraft's after a visit to Pixel (T5 swaps them per persona).

use crate::tests_parity::{assert_tree_parity, demo_lightcraft, demo_photocraft, lightcraft, photocraft};
use egui_kittest::Harness;

#[test]
fn t0_lightcrafts_whole_window_nests_under_a_host_bar() {
    let (mut hosted, mut bare) = (lightcraft(Harness::builder(), demo_lightcraft(), true), lightcraft(Harness::builder(), demo_lightcraft(), false));
    hosted.run_steps(8);
    bare.run_steps(8);
    assert_tree_parity(&hosted, &bare, &[]);
    crate::tests_parity::assert_style_parity(&hosted.ctx, &bare.ctx);
}

#[test]
fn t0_photocrafts_whole_window_nests_under_a_host_bar() {
    let (mut hosted, mut bare) = (photocraft(Harness::builder(), demo_photocraft(), true), photocraft(Harness::builder(), demo_photocraft(), false));
    hosted.run_steps(8);
    bare.run_steps(8);
    assert_tree_parity(&hosted, &bare, &[]);
    crate::tests_parity::assert_style_parity(&hosted.ctx, &bare.ctx);
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
