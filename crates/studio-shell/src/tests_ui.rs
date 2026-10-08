//! Headless UI tests of the shell (egui_kittest): clicks and shortcuts, as a user would.

use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::dock::{self, Tab};
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

/// The dock leaf (surface, node) a panel tab sits in.
fn leaf_of(shell: &StudioShell, p: Persona, panel: &str) -> String {
    let path = shell.layouts[p as usize].find_tab(&Tab::Panel(panel.to_owned())).unwrap();
    format!("{:?}/{:?}", path.surface, path.node)
}

/// Drag with the mouse in small steps, like a user, so egui sees a drag rather than a click.
fn mouse_drag(h: &mut Harness<'_, StudioShell>, from: Pos2, to: Pos2) {
    h.hover_at(from);
    h.step();
    h.drag_at(from);
    h.step();
    for i in 1..=12 {
        h.hover_at(from + (to - from) * (i as f32 / 12.0));
        h.step();
    }
    h.step();
    h.drop_at(to);
    h.run();
}

#[test]
fn dragging_a_panel_tab_regroups_it_and_the_layout_survives_switching() {
    let mut h = harness();
    h.get_by_label("Pixel").click();
    h.run();
    let before = leaf_of(h.state(), Persona::Pixel, "History");
    assert_ne!(before, leaf_of(h.state(), Persona::Pixel, "Layers"));
    // Drop History's tab in the middle of the Layers group: it becomes a tab of that group.
    let from = h.get_by_label("History").rect().center();
    let to = h.get_by_label("Layers").rect().center() + egui::vec2(60.0, 120.0);
    mouse_drag(&mut h, from, to);
    assert_eq!(leaf_of(h.state(), Persona::Pixel, "History"), leaf_of(h.state(), Persona::Pixel, "Layers"));
    // Away and back: the dragged layout is still there.
    h.get_by_label("Library").click();
    h.run();
    h.get_by_label("Pixel").click();
    h.run();
    assert_eq!(leaf_of(h.state(), Persona::Pixel, "History"), leaf_of(h.state(), Persona::Pixel, "Layers"));
}

#[test]
fn every_panel_tab_of_every_persona_has_an_accessible_name() {
    let mut h = harness();
    for p in Persona::ALL.into_iter().filter(|p| p.enabled()) {
        h.state_mut().run(&p.command()).unwrap();
        h.run();
        let panels: Vec<String> = h.state().layouts[p as usize]
            .iter_all_tabs()
            .filter_map(|(_, t)| match t {
                Tab::Panel(n) => Some(n.clone()),
                Tab::Canvas => None,
            })
            .collect();
        for name in panels {
            assert!(h.query_by_label(&name).is_some(), "{p:?}: no accessible tab named {name}");
        }
    }
}

#[test]
fn no_persona_panics_at_minimum_or_tiny_window_sizes() {
    for size in [[900.0, 560.0], [320.0, 200.0], [40.0, 30.0]] {
        let mut h = Harness::builder().with_size(size).build_ui_state(
            |ui, shell: &mut StudioShell| {
                crate::theme::apply(ui.ctx());
                shell.ui(ui);
            },
            StudioShell::default(),
        );
        for p in Persona::ALL.into_iter().filter(|p| p.enabled()) {
            h.state_mut().run(&p.command()).unwrap();
            h.run();
        }
    }
}

/// Saved states that still parse but are broken in ways a corrupt or hand-edited settings file
/// could be. Every edit hits Library's layout (the first one, and the persona shown at start).
/// (name, RON of a whole `StudioShell`, whether restore must reset Library's layout to default).
fn corrupt_saves() -> Vec<(&'static str, String, bool)> {
    let good = ron::to_string(&StudioShell::default()).unwrap();
    // Replace the whole `Leaf((…))` node holding the Navigator tab.
    let tab = good.find(r#"tabs:[Panel("Folders")]"#).unwrap();
    let start = good[..tab].rfind("Leaf((").unwrap();
    let end = tab + good[tab..].find("tab_bar_hidden:false))").unwrap() + "tab_bar_hidden:false))".len();
    let leaf_to_empty = format!("{}Empty{}", &good[..start], &good[end..]);
    // Make the first stored rectangle coordinate NaN.
    let x = good.find("rect:(min:(x:").unwrap() + "rect:(min:(x:".len();
    let x_end = x + good[x..].find(',').unwrap();
    let nan_rect = format!("{}NaN{}", &good[..x], &good[x_end..]);
    vec![
        ("NaN split fraction", good.replacen("fraction:0.18", "fraction:NaN", 1), true),
        ("infinite split fraction", good.replacen("fraction:0.18", "fraction:inf", 1), true),
        ("negative split fraction", good.replacen("fraction:0.18", "fraction:-1.0", 1), true),
        ("split fraction above 1", good.replacen("fraction:0.18", "fraction:7.0", 1), true),
        ("NaN rectangle", nan_rect, true),
        ("canvas missing", good.replacen("tabs:[Canvas]", r#"tabs:[Panel("Folders")]"#, 1), true),
        // These parse into layouts egui_dock copes with, so they are kept as they are.
        ("active tab out of range", good.replacen("active:(0)", "active:(9)", 1), false),
        ("leaf with no tabs", good.replacen(r#"tabs:[Panel("Folders")]"#, "tabs:[]", 1), false),
        ("split child missing", leaf_to_empty, false),
    ]
}

/// A corrupt settings file must never crash Studio (it would crash at every launch, since the
/// layout is reloaded each time). Regression: a NaN split fraction panicked in egui.
#[test]
fn corrupt_saved_layouts_never_crash_and_unsafe_ones_reset_to_default() {
    let default_library = ron::to_string(&dock::default_layout(Persona::Library)).unwrap();
    for (name, ron_text, must_reset) in corrupt_saves() {
        // A save that doesn't parse at all is never restored: the app starts from defaults.
        let Ok(saved) = ron::from_str::<StudioShell>(&ron_text) else { continue };
        let shell = StudioShell::restore(Some(saved));
        let library = ron::to_string(&shell.layouts[Persona::Library as usize]).unwrap();
        assert_eq!(library == default_library, must_reset, "{name}: reset expected = {must_reset}");
        let mut h = Harness::builder().with_size([1400.0, 900.0]).build_ui_state(|ui, s: &mut StudioShell| s.ui(ui), shell);
        h.run();
    }
}

#[test]
fn right_click_reset_workspace_puts_the_panels_back() {
    let mut h = harness();
    h.get_by_label("Pixel").click();
    h.run();
    let from = h.get_by_label("History").rect().center();
    let to = h.get_by_label("Layers").rect().center() + egui::vec2(60.0, 120.0);
    mouse_drag(&mut h, from, to);
    assert_eq!(leaf_of(h.state(), Persona::Pixel, "History"), leaf_of(h.state(), Persona::Pixel, "Layers"));
    h.get_by_label("Layers").click_secondary();
    h.run();
    h.get_by_label("Reset Workspace").click();
    h.run();
    assert_ne!(leaf_of(h.state(), Persona::Pixel, "History"), leaf_of(h.state(), Persona::Pixel, "Layers"));
}

/// Renders each v1 persona to `target/renders/<persona>.png` for a visual check. Needs a GPU, so
/// it is opt-in: `cargo test -p studio-shell -- --ignored render_personas` (on Windows ARM64 set
/// `WGPU_BACKEND=dx12`, as for the app).
#[test]
#[ignore]
fn render_personas() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/renders");
    std::fs::create_dir_all(&dir).unwrap();
    // Library and Develop on LightCraft's demo photos, one picked for Develop
    let mut h = crate::tests_lightcraft::studio_with(Harness::builder().with_size([1400.0, 900.0]).wgpu());
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.session.selection.active = lr.app.session.catalog.photos().nth(3).map(|p| p.id);
    for p in Persona::ALL.into_iter().filter(|p| p.enabled()) {
        h.state_mut().run(&p.command()).unwrap();
        settle(&mut h);
        h.render().unwrap().save(dir.join(format!("{}.png", p.id()))).unwrap();
    }
}

/// Run frames until LightCraft's background renders (thumbnails, the loupe) are done.
fn settle(h: &mut Harness<'_, StudioShell>) {
    let t0 = std::time::Instant::now();
    let mut quiet = 0;
    while quiet < 12 && t0.elapsed().as_secs() < 30 {
        h.run_steps(1);
        let busy = h.state().lightroom.as_ref().is_some_and(|lr| lr.app.renderer.in_flight() > 0);
        quiet = if busy { 0 } else { quiet + 1 };
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn top_bar_shows_the_document_name() {
    let h = harness();
    h.get_by_label("Untitled");
}

// --- Milestone 1's former hand checks ---

fn tooltip(h: &mut Harness<'_, StudioShell>, at: Pos2, text: &str) -> bool {
    h.hover_at(at);
    // egui shows tooltips after a short delay
    for _ in 0..90 {
        h.step();
        // tooltips are painted but not in the accessibility tree
        let painted = h.output().shapes.iter().any(|c| matches!(&c.shape, egui::Shape::Text(t) if t.galley.text().contains(text)));
        if painted {
            return true;
        }
    }
    false
}

#[test]
fn greyed_out_personas_and_tools_say_when_they_arrive() {
    let mut h = harness();
    for (label, text) in [("Vector", "Vector arrives in v2"), ("Layout", "Layout arrives in v3"), ("Export", "Export arrives in v2")] {
        let at = h.get_by_label(label).rect().center();
        assert!(tooltip(&mut h, at, text), "{label}: no tooltip `{text}`");
    }
    h.get_by_label("Pixel").click();
    h.run();
    for (key, name) in [("B", "Brush"), ("V", "Move"), ("Z", "Zoom")] {
        let at = h.get_by_label(key).rect().center();
        let text = format!("{name} ({key}) arrives in milestone 4");
        assert!(tooltip(&mut h, at, &text), "tool {key}: no tooltip `{text}`");
    }
}

#[test]
fn ctrl_1_to_4_switch_personas_and_ctrl_5_to_7_do_nothing() {
    let mut h = harness();
    for (n, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
        h.key_press_modifiers(Modifiers::COMMAND, key);
        h.run();
        assert_eq!(h.state().persona, Persona::ALL[n]);
    }
    for key in [Key::Num5, Key::Num6, Key::Num7] {
        h.key_press_modifiers(Modifiers::COMMAND, key);
        h.run();
        assert_eq!(h.state().persona, Persona::Pixel, "{key:?}");
    }
}

fn leaves(shell: &StudioShell, p: Persona) -> usize {
    shell.layouts[p as usize].iter_surfaces().flat_map(|s| s.iter_nodes()).filter(|n| n.is_leaf()).count()
}

fn windows(shell: &StudioShell, p: Persona) -> usize {
    shell.layouts[p as usize].iter_surfaces().filter(|s| matches!(s, egui_dock::Surface::Window(..))).count()
}

#[test]
fn dropping_a_tab_on_a_groups_edge_splits_it() {
    let mut h = harness();
    h.get_by_label("Pixel").click();
    h.run();
    let before = leaves(h.state(), Persona::Pixel);
    // History onto the bottom edge of the Layers group's body
    let from = h.get_by_label("History").rect().center();
    let layers = h.get_by_label("Layers").rect();
    let to = egui::pos2(layers.center().x + 60.0, layers.bottom() + 200.0);
    mouse_drag(&mut h, from, to);
    assert_eq!(leaves(h.state(), Persona::Pixel), before + 1, "a new group");
    assert_ne!(leaf_of(h.state(), Persona::Pixel, "History"), leaf_of(h.state(), Persona::Pixel, "Layers"));
}

#[test]
fn a_panel_dropped_away_from_the_split_buttons_floats_but_the_canvas_never_does() {
    let mut h = harness();
    h.get_by_label("Pixel").click();
    h.run();
    // onto the canvas, away from its split buttons (in its middle)
    let canvas = leaf_rect(h.state(), Persona::Pixel, "Layers");
    let out = egui::pos2(canvas.left() - 500.0, 200.0);
    let from = h.get_by_label("History").rect().center();
    mouse_drag(&mut h, from, out);
    assert_eq!(windows(h.state(), Persona::Pixel), 1, "History floats");
    // the canvas tab's title (the top bar's document name is "Untitled" too)
    let from = h.get_all_by_label("Untitled").map(|n| n.rect()).find(|r| r.top() > 40.0).unwrap().center();
    mouse_drag(&mut h, from, egui::pos2(out.x, 600.0));
    assert_eq!(windows(h.state(), Persona::Pixel), 1, "the canvas stays docked");
    assert!(dock::layout_is_usable(&h.state().layouts[Persona::Pixel as usize]));
}

/// The rectangle egui_dock laid out for the group holding `panel` (its tab bar and body).
fn leaf_rect(shell: &StudioShell, p: Persona, panel: &str) -> egui::Rect {
    let dock = &shell.layouts[p as usize];
    let path = dock.find_tab(&Tab::Panel(panel.to_owned())).unwrap();
    match &dock[path.surface][path.node] {
        egui_dock::Node::Leaf(l) => l.rect,
        _ => panic!("{panel} isn't in a leaf"),
    }
}

fn click_at(h: &mut Harness<'_, StudioShell>, at: Pos2) {
    h.hover_at(at);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run();
}

#[test]
fn the_collapse_button_folds_a_panel_group() {
    let mut h = harness();
    h.get_by_label("Pixel").click();
    h.run();
    let collapsed = |s: &StudioShell| {
        let dock = &s.layouts[Persona::Pixel as usize];
        let path = dock.find_tab(&Tab::Panel("Layers".to_owned())).unwrap();
        matches!(&dock[path.surface][path.node], egui_dock::Node::Leaf(l) if l.collapsed)
    };
    assert!(!collapsed(h.state()));
    // the ▼ button is the left end of the group's tab bar
    let group = leaf_rect(h.state(), Persona::Pixel, "Layers");
    let layers = h.get_by_label("Layers").rect();
    click_at(&mut h, egui::pos2(group.left() + 6.0, layers.center().y));
    assert!(collapsed(h.state()), "Layers' group is folded");
}

#[test]
fn dragging_a_column_separator_resizes_it() {
    let mut h = harness();
    h.get_by_label("Pixel").click();
    h.run();
    let column = |s: &StudioShell| leaf_rect(s, Persona::Pixel, "Layers").width();
    let w = column(h.state());
    // the separator runs just left of the right column
    let x = leaf_rect(h.state(), Persona::Pixel, "Layers").left() - 1.0;
    mouse_drag(&mut h, egui::pos2(x, 450.0), egui::pos2(x - 120.0, 450.0));
    assert!(column(h.state()) > w + 60.0, "{w} → {}", column(h.state()));
}
