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
    crate::tests_parity::assert_style_parity(&hosted.ctx, &bare.ctx, crate::tests_parity::App::LightCraft);
}

#[test]
fn t0_photocrafts_whole_window_nests_under_a_host_bar() {
    let (mut hosted, mut bare) = (photocraft(Harness::builder(), demo_photocraft(), true), photocraft(Harness::builder(), demo_photocraft(), false));
    hosted.run_steps(8);
    bare.run_steps(8);
    assert_tree_parity(&hosted, &bare, &[]);
    crate::tests_parity::assert_style_parity(&hosted.ctx, &bare.ctx, crate::tests_parity::App::PhotoCraft);
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
