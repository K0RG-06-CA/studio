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
fn top_bar_shows_the_document_name() {
    let h = harness();
    h.get_by_label("Untitled");
}
