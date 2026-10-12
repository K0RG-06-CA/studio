//! T3m: PhotoCraft's menus in the persona bar row in Pixel, routed by `pixel_menu`: its File items
//! are Studio's document commands (prompt, never over the source), its undo is Studio's history,
//! Window › History is Studio's panel, what has no meaning with one document is hidden or off, and
//! the keys of those items go the same way.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};

use crate::tests_lightcraft::{Window, click_at, history, pick_first, run_window, settle_base, studio, temp_dir};
use crate::tests_photocraft::{pixel_edited, press};
use crate::{Pending, StudioShell};

/// Close any open menu with a click on the persona bar right of its persona buttons.
fn close_menus(h: &mut Harness<'_, StudioShell>) {
    let r = h.get_by_label("Layout").rect();
    click_at(h, egui::pos2(r.max.x + 24.0, r.center().y));
}

/// Whether the menu row `l` (its label, then its shortcut) is `label`.
fn item_is(l: &str, label: &str) -> bool {
    let l = l.trim_start_matches(['\u{2714}', ' ']);
    l == label || l.starts_with(&format!("{label} "))
}

/// Every labelled node: its id, label, enabled and rect.
fn nodes(h: &Harness<'_, StudioShell>) -> Vec<(String, String, bool, egui::Rect)> {
    h.query_all(by().predicate(|n| n.label().is_some()))
        .map(|n| {
            let a = n.accesskit_node();
            (format!("{:?}", a.id()), a.label().unwrap_or_default(), !a.is_disabled(), n.rect())
        })
        .collect()
}

/// A title of the persona bar's menus (in the bar's row: PhotoCraft's window has its own labels).
fn title(h: &Harness<'_, StudioShell>, title: &str) -> (bool, egui::Rect) {
    let n = h.query_all_by_label(title).find(|n| n.rect().max.y < 50.0).unwrap_or_else(|| panic!("no menu {title} in the bar"));
    (!n.accesskit_node().is_disabled(), n.rect())
}

/// Open the persona bar's menu `title` (closing any other first): its rows, which are what showed
/// with it (label, enabled, rect).
fn open_menu(h: &mut Harness<'_, StudioShell>, menu: &str) -> Vec<(String, bool, egui::Rect)> {
    close_menus(h);
    let before: Vec<_> = nodes(h).into_iter().map(|n| n.0).collect();
    let (_, r) = title(h, menu);
    click_at(h, r.center());
    nodes(h).into_iter().filter(|n| !before.contains(&n.0)).map(|(_, l, on, r)| (l, on, r)).collect()
}

/// The row `label` of the menu `title`: whether it is enabled (none: not in the menu).
fn row(h: &mut Harness<'_, StudioShell>, menu: &str, label: &str) -> Option<bool> {
    open_menu(h, menu).into_iter().find(|(l, ..)| item_is(l, label)).map(|(_, on, _)| on)
}

/// Click the row `label` of the menu `title`: it runs in the release's frame, the last one run.
fn click_row_now(h: &mut Harness<'_, StudioShell>, menu: &str, label: &str) {
    let rows = open_menu(h, menu);
    let pos = rows.iter().find(|(l, ..)| item_is(l, label)).unwrap_or_else(|| panic!("no {label} in {menu}: {rows:?}")).2.center();
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
}

/// Click the row `label` of the menu `title`, and run it.
fn click_row(h: &mut Harness<'_, StudioShell>, menu: &str, label: &str) {
    click_row_now(h, menu, label);
    h.run_steps(4);
}

fn layers(h: &Harness<'_, StudioShell>) -> usize {
    h.state().document.as_ref().unwrap().pixel.layers.len()
}

/// The T0 undo test, flipped: PhotoCraft's Edit › Undo undid its own Pixel step, which Studio read
/// as a jump to before it, so a newer Develop step went too. Studio's menu Undo takes only the
/// newest step (the Develop one), adds none, and Redo brings it back.
#[test]
fn photocrafts_edit_undo_menu_item_takes_only_the_newest_step() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    crate::tests_photocraft::add_layer(&mut h, "Paint");
    h.run_steps(1);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    h.run_steps(1);
    let steps = vec![(false, false), (false, true), (true, false)];
    assert_eq!(history(&h), (steps.clone(), 2));
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    let exposure = |h: &Harness<'_, StudioShell>| {
        let id = h.state().document.as_ref().unwrap().photo().unwrap();
        h.state().lightroom.as_ref().unwrap().app.session.develop_of(id).unwrap().light.exposure
    };
    click_row(&mut h, "Edit", "Undo");
    assert_eq!((layers(&h), exposure(&h), history(&h)), (2, 0.0, (steps.clone(), 1)), "only the Develop step undone");
    click_row(&mut h, "Edit", "Redo");
    assert_eq!((layers(&h), exposure(&h), history(&h)), (2, 1.0, (steps, 2)));
}

/// PhotoCraft's menu titles sit left of the persona buttons, which all stay in the window (egui's
/// menu bar would take the whole row) and switch personas.
#[test]
fn photocrafts_menus_leave_room_for_the_persona_buttons() {
    let mut h = studio();
    h.run_steps(3);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    let (_, help) = title(&h, "Help");
    let (library, export) = (h.get_by_label("Library").rect(), h.get_by_label("Export").rect());
    assert!(help.max.x < library.min.x && export.max.x < 1400.0, "Help {help:?}, Library {library:?}, Export {export:?}");
    h.get_by_label("Library").click();
    h.run_steps(1);
    assert_eq!(h.state().persona, crate::Persona::Library);
}

/// Review: an undo key the user binds in PhotoCraft's Keyboard Shortcuts is Studio's undo too,
/// as Ctrl+Z is: PhotoCraft's own undo would take two steps at once (the T0 bug).
#[test]
fn a_rebound_undo_key_is_studios_undo() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    crate::tests_photocraft::add_layer(&mut h, "Paint");
    h.run_steps(1);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    h.run_steps(1);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    let px = h.state_mut().pixel.as_mut().unwrap();
    px.app.session.edit_prefs(|p| p.shortcuts.insert("edit.undo".into(), "Cmd+U".into()));
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::U);
    h.run_steps(2);
    assert_eq!((layers(&h), history(&h).1), (2, 1), "only the Develop step undone");
}

/// A PhotoCraft menu item that edits the document runs as PhotoCraft runs it: one Pixel step.
#[test]
fn a_photocraft_menu_edit_is_one_pixel_step() {
    let dir = temp_dir("t3m-pass");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let (_, at) = history(&h);
    let visible = |h: &Harness<'_, StudioShell>| h.state().document.as_ref().unwrap().pixel.layers.iter().map(|l| l.visible).collect::<Vec<_>>();
    assert_eq!(visible(&h), [true, true]);
    click_row(&mut h, "Layer", "Hide Layers");
    assert_eq!(visible(&h), [true, false], "the new layer hidden");
    let (rows, now) = history(&h);
    assert_eq!((now, rows.last()), (at + 1, Some(&(false, true))));
}

/// Its File items go through Studio's document commands: Save writes the `.studio` file (and only
/// it), Close and Exit ask about unsaved changes, Open and Save As use Studio's dialogs, and Save a
/// Copy writes a copy, leaving the document on its own file with its changes unsaved.
#[test]
fn photocrafts_file_items_are_studios_document_commands() {
    let dir = temp_dir("t3m-file");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    let picked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let (asked, copy) = (picked.clone(), dir.join("copy.studio"));
    h.state_mut().pick_save_path = Some(Box::new(move |name| {
        asked.borrow_mut().push(name.to_owned());
        Some(copy.clone())
    }));
    let opened = std::rc::Rc::new(std::cell::Cell::new(0));
    let o = opened.clone();
    h.state_mut().pick_open_path = Some(Box::new(move || {
        o.set(o.get() + 1);
        None
    }));
    // Save a Copy: a copy, the document as it was
    click_row(&mut h, "File", "Save a Copy…");
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!((doc.path.as_deref(), doc.unsaved()), (Some(path.as_path()), true));
    assert_eq!(studio_doc::Document::open(&dir.join("copy.studio")).unwrap().pixel.layers.len(), 2, "the copy has the edit");
    assert_eq!(studio_doc::Document::open(&path).unwrap().pixel.layers.len(), 1, "its own file doesn't");
    assert_eq!(picked.borrow().as_slice(), [format!("{} copy.studio", doc.name)]);
    // Close and Exit ask first
    click_row(&mut h, "File", "Close");
    assert_eq!(h.state().prompt, Some(Pending::Close));
    h.state_mut().run("document.prompt.cancel").unwrap();
    // Exit: the click's frame asks to close, and the next stops it
    click_row_now(&mut h, "File", "Exit");
    let w = run_window(&mut h, 4, false);
    assert_eq!(w, Window { closes: 1, cancels: 1, closed: false });
    assert_eq!(h.state().prompt, Some(Pending::Quit));
    h.state_mut().run("document.prompt.cancel").unwrap();
    // Open: Studio's open dialog (cancelled here)
    click_row(&mut h, "File", "Open…");
    assert_eq!(opened.get(), 1);
    // Save: its .studio file, nothing else
    click_row(&mut h, "File", "Save");
    assert!(!h.state().document.as_ref().unwrap().unsaved());
    assert_eq!(studio_doc::Document::open(&path).unwrap().pixel.layers.len(), 2);
    // Save As: Studio's save dialog
    click_row(&mut h, "File", "Save As…");
    assert_eq!(h.state().document.as_ref().unwrap().path.as_deref(), Some(dir.join("copy.studio").as_path()));
    assert_eq!(picked.borrow().len(), 2);
}

/// Revert opens the document's own file again, through Studio's prompt: off with nothing to go
/// back to (no changes since the save), and with changes, Don't Save takes them back.
#[test]
fn photocrafts_revert_reopens_the_saved_file_through_the_prompt() {
    let dir = temp_dir("t3m-revert");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    click_row(&mut h, "File", "Revert");
    assert_eq!(h.state().prompt, Some(Pending::Open(dir.join("trip.studio"))));
    h.state_mut().run("document.prompt.discard").unwrap();
    h.run_steps(2);
    assert_eq!(layers(&h), 1, "back to the saved layers");
    assert!(!h.state().document.as_ref().unwrap().unsaved());
    assert_eq!(row(&mut h, "File", "Revert"), Some(false), "nothing to revert");
    // nor does its key or command reopen it (that would only lose the history)
    let generation = h.state().document.as_ref().unwrap() as *const _;
    assert!(h.state_mut().run("document.revert").is_err());
    assert!(std::ptr::eq(generation, h.state().document.as_ref().unwrap()), "the same document");
}

/// What has no meaning with Studio's one document isn't in its menus (New, Open Recent, Close
/// All, Close Others, Purge); what would make a second document is off (Duplicate), and so is the
/// palette (L19); Help's links say they go upstream.
#[test]
fn photocrafts_menus_hide_what_one_document_cant_use() {
    let dir = temp_dir("t3m-hidden");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let file = open_menu(&mut h, "File");
    assert!(file.iter().any(|(l, ..)| item_is(l, "Save a Copy…")), "{file:?}");
    for gone in ["New…", "Open Recent", "Close All", "Close Others", "New from Clipboard"] {
        assert!(!file.iter().any(|(l, ..)| item_is(l, gone)), "{gone} in {file:?}");
    }
    let edit = open_menu(&mut h, "Edit");
    assert!(!edit.iter().any(|(l, ..)| item_is(l, "Purge")), "{edit:?}");
    assert_eq!(edit.iter().find(|(l, ..)| item_is(l, "Search…")).map(|r| r.1), Some(false), "{edit:?}");
    assert_eq!(row(&mut h, "Image", "Duplicate…"), Some(false));
    let help = open_menu(&mut h, "Help");
    for link in ["PhotoCraft upstream website", "PhotoCraft upstream on GitHub", "Report an Issue to PhotoCraft upstream…"] {
        assert!(help.iter().any(|(l, ..)| item_is(l, link)), "{link} not in {help:?}");
    }
    assert!(!help.iter().any(|(l, ..)| item_is(l, "PhotoCraft Website")));
}

/// Window › History is Studio's panel (checked while it shows); PhotoCraft's own History is never
/// drawn, nor turned on by a workspace that has it.
#[test]
fn window_history_is_studios_history_panel() {
    let dir = temp_dir("t3m-history");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let shown = |h: &Harness<'_, StudioShell>| h.query_by_label("Original").is_some();
    assert!(shown(&h));
    click_row(&mut h, "Window", "History");
    assert!(!shown(&h) && h.state().history_hidden);
    assert!(open_menu(&mut h, "Window").iter().any(|(l, ..)| l.starts_with("   History") || l.starts_with("  History")), "unchecked");
    click_row(&mut h, "Window", "History");
    assert!(shown(&h));
    assert!(open_menu(&mut h, "Window").iter().any(|(l, ..)| l.starts_with("\u{2714} History")), "checked");
    assert!(!h.state().pixel.as_ref().unwrap().app.ui.panels.history);
    // a workspace with History in it (Photography) leaves PhotoCraft's off
    let ctx = h.ctx.clone();
    let px = h.state_mut().pixel.as_mut().unwrap();
    photocraft_ui_egui::menus::invoke(&mut px.app, &ctx, "window.workspace.photography", serde_json::json!({})).unwrap();
    assert!(px.app.ui.panels.history, "PhotoCraft's Photography workspace has its History");
    px.app.ui.panels.history = false;
    h.state_mut().pixel_menu_item(&ctx, "window.workspace.photography");
    assert!(!h.state().pixel.as_ref().unwrap().app.ui.panels.history);
}

/// The keys of the items Studio routes run its commands (Revert F12, Open As Ctrl+Alt+Shift+O,
/// Save a Copy Ctrl+Alt+S); those of items it hides do nothing (Close All, Close Others).
#[test]
fn keys_of_photocrafts_items_go_where_the_items_route() {
    let dir = temp_dir("t3m-keys");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let (cmd, alt, shift) = (egui::Modifiers::COMMAND, egui::Modifiers::ALT, egui::Modifiers::SHIFT);
    let copy = dir.join("copy.studio");
    let to = copy.clone();
    h.state_mut().pick_save_path = Some(Box::new(move |_| Some(to.clone())));
    let opened = std::rc::Rc::new(std::cell::Cell::new(0));
    let o = opened.clone();
    h.state_mut().pick_open_path = Some(Box::new(move || {
        o.set(o.get() + 1);
        None
    }));
    let state = |h: &Harness<'_, StudioShell>| {
        let pc = &h.state().pixel.as_ref().unwrap().app;
        let docs: Vec<_> = pc.session.documents().iter().map(|d| (d.doc.id, d.revision, d.history.past_len())).collect();
        (docs, layers(h), h.state().error.clone(), h.state().prompt.clone(), pc.ui.palette_open)
    };
    let before = state(&h);
    for (mods, k) in [(cmd | alt, egui::Key::W), (cmd | alt, egui::Key::P)] {
        press(&mut h, mods, k);
        h.run_steps(2);
        assert_eq!(state(&h), before, "{mods:?}+{k:?}");
    }
    press(&mut h, cmd | alt, egui::Key::S);
    h.run_steps(2);
    assert!(copy.exists() && h.state().document.as_ref().unwrap().unsaved(), "Ctrl+Alt+S saved a copy");
    press(&mut h, cmd | alt | shift, egui::Key::O);
    h.run_steps(2);
    assert_eq!(opened.get(), 1, "Ctrl+Alt+Shift+O is Studio's open");
    press(&mut h, egui::Modifiers::NONE, egui::Key::F12);
    h.run_steps(2);
    assert_eq!(h.state().prompt, Some(Pending::Open(dir.join("trip.studio"))), "F12 reverts through the prompt");
}

/// L19: PhotoCraft's command palette runs commands past Studio's routing: opened by its title
/// bar's icon, Studio closes it after the frame.
#[test]
fn photocrafts_palette_never_stays_open() {
    let dir = temp_dir("t3m-palette");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    h.state_mut().pixel.as_mut().unwrap().app.ui.palette_open = true;
    h.run_steps(1);
    assert!(!h.state().pixel.as_ref().unwrap().app.ui.palette_open);
}

/// Nothing of PhotoCraft's menus runs while a Develop geometry change is on its way into the
/// layers (as its own `logic` doesn't): the bar's menus are off.
#[test]
fn photocrafts_menus_are_off_while_develop_renders_a_rotation() {
    let dir = temp_dir("t3m-busy");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let id = h.state().document.as_ref().unwrap().photo().unwrap().0;
    let lr = &mut h.state_mut().lightroom.as_mut().unwrap().app.session;
    lr.execute("photo.rotateLeft", &serde_json::json!({"ids": [id]})).unwrap();
    h.run_steps(1);
    assert!(h.state().develop_rendering());
    let before = (layers(&h), history(&h));
    assert!(!title(&h, "Layer").0, "its titles are off");
    let ctx = h.ctx.clone();
    h.state_mut().pixel_menu_item(&ctx, "layer.hideLayers");
    assert_eq!((layers(&h), history(&h)), before);
}
