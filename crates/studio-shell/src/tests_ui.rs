//! Headless UI tests of the shell (egui_kittest): clicks and shortcuts, as a user would.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::{Persona, StudioShell};

fn harness() -> Harness<'static, StudioShell> {
    Harness::builder().with_size([1400.0, 900.0]).build_ui_state(
        |ui, shell: &mut StudioShell| {
            crate::theme::apply(ui.ctx());
            shell.ui(ui);
        },
        StudioShell::default(),
    )
}

#[test]
fn clicking_a_persona_in_the_top_bar_switches_to_it() {
    let mut h = harness();
    h.get_by_label("Develop").click();
    h.run();
    assert_eq!(h.state().persona, Persona::Develop);
    h.get_by_label("Pixel").click();
    h.run();
    assert_eq!(h.state().persona, Persona::Pixel);
}

#[test]
fn disabled_personas_are_greyed_out_and_ignore_clicks() {
    let mut h = harness();
    for label in ["Vector", "Layout", "Export"] {
        let node = h.get_by_label(label);
        assert!(node.accesskit_node().is_disabled(), "{label} should be disabled");
        node.click();
        h.run();
        assert_eq!(h.state().persona, Persona::Library);
    }
}

#[test]
fn ctrl_number_jumps_between_personas() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num4);
    h.run();
    assert_eq!(h.state().persona, Persona::Pixel);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num2);
    h.run();
    assert_eq!(h.state().persona, Persona::Develop);
    // Ctrl+5 is Vector, which is disabled: nothing happens.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num5);
    h.run();
    assert_eq!(h.state().persona, Persona::Develop);
}

#[test]
fn each_persona_shows_its_adobe_layout() {
    let mut h = harness();
    // Library: Lightroom side panels, no Photoshop chrome.
    h.get_by_label("Folders");
    h.get_by_label("Filmstrip");
    assert!(h.query_by_label("Tool options").is_none());
    // Develop: slider groups as collapsible sections.
    h.get_by_label("Develop").click();
    h.run();
    h.get_by_label("Basic");
    h.get_by_label("Tone Curve");
    // Film: effects in pipeline order.
    h.get_by_label("Film").click();
    h.run();
    h.get_by_label("Halation");
    // Pixel: tools left, options bar on top, Layers and Properties on the right.
    h.get_by_label("Pixel").click();
    h.run();
    h.get_by_label("Tool options");
    h.get_by_label("Layers");
    h.get_by_label("Properties");
    h.get_by_label("B");
    assert!(h.query_by_label("Folders").is_none());
}

/// Renders each v1 persona to `target/renders/<persona>.png` for a visual check. Needs a GPU, so
/// it is opt-in: `cargo test -p studio-shell -- --ignored render_personas` (on Windows ARM64 set
/// `WGPU_BACKEND=dx12`, as for the app).
#[test]
#[ignore]
fn render_personas() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/renders");
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::builder().with_size([1400.0, 900.0]).wgpu().build_ui_state(
        |ui, shell: &mut StudioShell| {
            crate::theme::apply(ui.ctx());
            shell.ui(ui);
        },
        StudioShell::default(),
    );
    for p in Persona::ALL.into_iter().filter(|p| p.enabled()) {
        h.state_mut().run(&p.command()).unwrap();
        h.run();
        h.render().unwrap().save(dir.join(format!("{}.png", p.id()))).unwrap();
    }
}

#[test]
fn top_bar_shows_the_document_name() {
    let h = harness();
    h.get_by_label("Untitled");
}
