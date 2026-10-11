//! Studio with PhotoCraft: Pixel is PhotoCraft's own window, the home of the document's pixels.
//!
//! Milestone 4 spike (b), kept: PhotoCraft's fonts and LightCraft's live in one context: both
//! replace all fonts, and each needs its own named families.

use egui::{Rect, Ui, UiBuilder, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer};
use photocraft_geom::Size;
use photocraft_ui_egui::{PhotocraftApp, Services, canvas, panels};

struct Spike {
    app: PhotocraftApp,
    frame: eframe::Frame,
    frames: u32,
}

const RIGHT: Rect = Rect::from_min_max(pos2(1080.0, 40.0), pos2(1400.0, 900.0));

fn frame(ui: &mut Ui, s: &mut Spike) {
    eframe::App::logic(&mut s.app, ui.ctx(), &mut s.frame);
    s.frames += 1;
    // PhotoCraft installs its fonts on the first frame; they are usable from the next
    if s.frames < 3 {
        return;
    }
    let app = &mut s.app;
    panels::options_bar(app, ui);
    panels::toolbar(app, ui);
    let mut right = ui.new_child(UiBuilder::new().max_rect(RIGHT));
    right.set_clip_rect(RIGHT);
    panels::right_dock(app, &mut right);
    egui::CentralPanel::default().show(ui, |ui| {
        let rect = ui.available_rect_before_wrap().with_max_x(RIGHT.min.x);
        if let Some(idx) = app.session.active_index() {
            let view = app.ui.views[idx].clone();
            app.ui.views[idx] = canvas::canvas_view(app, ui, idx, rect, view, true);
        }
    });
}

pub(crate) fn pixel_doc() -> Document {
    let mut doc = Document::new("spike", Size { width: 64, height: 48 }, ColorMode::Rgb, SampleType::U16);
    doc.layers.push(Layer::raster("Develop", PixelFormat::RGBA16));
    doc
}

fn painted_text(output: &egui::FullOutput) -> String {
    output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Studio with LightCraft, and PhotoCraft's pieces drawn over it while `photocraft` is on.
struct Both {
    shell: crate::StudioShell,
    pc: Spike,
    photocraft: bool,
}

#[test]
fn spike_b_photocraft_and_lightcraft_fonts_live_in_one_context() {
    let pc = Spike { app: harness_app(), frame: eframe::Frame::_new_kittest(), frames: 0 };
    let both = Both { shell: crate::tests_lightcraft::demo_shell(), pc, photocraft: false };
    let mut themed = false;
    let mut h = Harness::builder().with_size([1400.0, 900.0]).build_ui_state(
        move |ui, b: &mut Both| {
            if !std::mem::replace(&mut themed, true) {
                crate::theme::apply(ui.ctx());
            }
            if b.photocraft {
                frame(ui, &mut b.pc);
            } else {
                b.shell.ui(ui);
            }
        },
        both,
    );
    // LightCraft first, then PhotoCraft (which replaces every font), then LightCraft again
    h.run_steps(6);
    assert!(painted_text(h.output()).contains("Search Photos"));
    h.state_mut().photocraft = true;
    h.run_steps(6);
    assert!(painted_text(h.output()).contains("Layers"));
    h.state_mut().photocraft = false;
    h.run_steps(4);
    assert!(painted_text(h.output()).contains("Search Photos"));
    // and Develop's panels, which use LightCraft's semibold family
    h.state_mut().shell.run("persona.develop").unwrap();
    h.run_steps(4);
    let text = painted_text(h.output());
    assert!(text.contains("Edit") && text.contains("Exposure"), "{text}");
}

/// L2, menu side: PhotoCraft greys Arrange, Delete and Merge Down by its Background heuristic
/// (named "Background", at the bottom), never by the lock, so on Studio's locked "Develop" layer
/// they stay enabled and the guard in `studio_doc::pixel` takes them back. Fails once upstream
/// greys them for a fully locked layer.
#[test]
fn l2_photocraft_menus_dont_grey_changes_to_a_fully_locked_layer() {
    use photocraft_ui_egui::enable_rules::disabled_for;
    let mut doc = pixel_doc();
    doc.layers[0].locks.all = true;
    doc.layers.push(Layer::raster("Paint", PixelFormat::RGBA16));
    let (base, top) = (&doc.layers[0], &doc.layers[1]);
    for id in ["layer.arrange.bringForward", "layer.arrange.bringToFront", "layer.delete"] {
        assert!(!disabled_for(&doc, Some(base), id), "upstream now greys {id} on a locked layer");
    }
    assert!(!disabled_for(&doc, Some(top), "layer.mergeDown"), "upstream now greys merging into a locked layer");
}

// --- Studio with PhotoCraft as the home of the document's pixels ---

use crate::tests_lightcraft::{pick_first, settle_base, studio, temp_dir};

/// A Pixel edit, as PhotoCraft's panels make one: a new layer on top.
pub(crate) fn add_layer(h: &mut Harness<'_, crate::StudioShell>, name: &str) {
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session
        .edit(name, |d, active| {
            let l = Layer::raster(name, PixelFormat::RGBA16);
            *active = Some(l.id);
            d.layers.push(l);
            Ok(())
        })
        .unwrap();
}

fn layer_names(doc: &studio_doc::Document) -> Vec<String> {
    doc.pixel.layers.iter().map(|l| l.name.clone()).collect()
}

#[test]
fn pixel_edits_live_in_photocraft_and_are_what_saving_writes() {
    let dir = temp_dir("pixel-save");
    let path = dir.join("trip.studio");
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    // the new document's pixels went to PhotoCraft, and the base renders into them there
    settle_base(&mut h);
    let pc = &h.state().pixel.as_ref().unwrap().app.session;
    assert_eq!(pc.documents().len(), 1);
    assert!(pc.active().unwrap().doc.layers[0].surface().unwrap().tile_count() > 0, "the render is in PhotoCraft");
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    // an edit in PhotoCraft, saved without a frame in between
    add_layer(&mut h, "Paint");
    h.state_mut().run("document.save").unwrap();
    assert_eq!(layer_names(&studio_doc::Document::open(&path).unwrap()), ["Develop", "Paint"]);
    // and one the app exits with (no frame, no prompt): recovered from PhotoCraft too
    add_layer(&mut h, "Late");
    assert_eq!(h.state_mut().save_for_exit(&dir.join("Recovered")).unwrap(), Some(path.clone()));
    assert_eq!(layer_names(&studio_doc::Document::open(&path).unwrap()), ["Develop", "Paint", "Late"]);
    // closing the document takes it out of PhotoCraft
    h.state_mut().run("document.close").unwrap();
    assert!(h.state().pixel.as_ref().unwrap().app.session.documents().is_empty());
}

/// The first tile of the document's base layer in PhotoCraft: which render it shows.
fn shown_render(h: &Harness<'_, crate::StudioShell>) -> Option<usize> {
    let doc = &h.state().pixel.as_ref().unwrap().app.session.active().unwrap().doc;
    doc.layers[0].surface().unwrap().tiles().next().map(|(_, t)| std::sync::Arc::as_ptr(t) as usize)
}

#[test]
fn in_pixel_a_new_render_waits_while_a_button_is_held_and_goes_in_on_release() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(2);
    let before = shown_render(&h);
    // a stroke starts (the button goes down), and a develop change renders meanwhile
    let pos = egui::pos2(600.0, 400.0);
    h.event(egui::Event::PointerMoved(pos));
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    let t0 = std::time::Instant::now();
    loop {
        h.run_steps(1);
        let s = h.state();
        if !s.base.rendering() && s.base.started() >= 2 {
            break;
        }
        assert!(t0.elapsed() < std::time::Duration::from_secs(300), "render never finished");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert_eq!(shown_render(&h), before, "held while the button is down");
    assert!(!h.state().base_up_to_date());
    // released: the render goes in
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);
    assert_ne!(shown_render(&h), before);
    assert!(h.state().base_up_to_date());
}

/// Where PhotoCraft's canvas is in Pixel (its own document area says so).
pub(crate) fn canvas(h: &Harness<'_, crate::StudioShell>) -> Rect {
    h.state().pixel.as_ref().unwrap().app.last_canvas_rect
}

/// Where Studio's floating History panel is in Pixel.
fn history_rect(h: &Harness<'_, crate::StudioShell>) -> Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new("studio.history"))).expect("Studio's History shows")
}

/// T3: Pixel is PhotoCraft's own window (its parity with the bare app is `tests_parity`'s):
/// Studio's placeholders are gone, its menus are hidden, "Rendering Develop…" shows over its
/// canvas while the base renders, and its own tool keys pick its tools.
#[test]
fn pixel_is_photocrafts_own_window() {
    let mut h = studio();
    h.run_steps(3);
    // no photo picked, no document: PhotoCraft's own home screen, nothing of Studio's dock
    h.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = None;
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(4);
    assert!(h.state().document.is_none());
    let text = painted_text(h.output());
    assert!(text.contains("Create a new document or open an existing file.") && !text.contains("Filmstrip"), "{text}");
    // its menus are hidden (until Studio hosts them, T3m), and LightCraft's aren't shown here
    for menu in ["File", "Image", "Photo"] {
        assert!(h.query_by_label(menu).is_none(), "{menu} shows");
    }
    h.state_mut().run("persona.library").unwrap();
    h.run_steps(2);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    h.state_mut().run("persona.pixel").unwrap();
    // the frame that starts the render (at 600 px it may be done by the next)
    h.run_steps(1);
    assert!(painted_text(h.output()).contains("Rendering Develop"), "said while the base renders");
    settle_base(&mut h);
    h.run_steps(3);
    let text = painted_text(h.output());
    assert!(!text.contains("Rendering Develop"));
    // its Layers panel shows the document's base layer, and its canvas the document's image
    for want in ["Layers", studio_doc::BASE_LAYER] {
        assert!(text.contains(want), "{want}: {text}");
    }
    let rect = canvas(&h);
    let images: Vec<Rect> = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(m.calc_bounds()),
            _ => None,
        })
        .collect();
    assert!(images.iter().any(|r| rect.contains_rect(*r) && r.width() > 100.0), "the document's image in {rect:?}: {images:?}");
    // Photoshop's tool keys pick from their tool's group: V Move, B a brush, M a marquee, E an eraser
    for (key, group) in [(egui::Key::V, 'V'), (egui::Key::B, 'B'), (egui::Key::M, 'M'), (egui::Key::E, 'E')] {
        h.key_press(key);
        h.run_steps(2);
        let tool = h.state().pixel.as_ref().unwrap().app.ui.tool;
        assert_eq!(tool.key(), group, "{key:?}: {tool:?}");
    }
}

#[test]
fn a_change_to_the_develop_layer_is_taken_back_and_says_why() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    add_layer(&mut h, "Paint");
    h.run_steps(1);
    let rev = h.state().document.as_ref().unwrap().revision;
    // Delete Layer on the Develop layer (PhotoCraft lets it through, see `l2_*`)
    let base = h.state().document.as_ref().unwrap().base_layer().unwrap().id.0;
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session.execute("layer.delete", serde_json::json!({"layer": base})).unwrap();
    h.run_steps(2);
    let text = painted_text(h.output());
    assert!(text.contains("Couldn\u{2019}t change the Develop layer") && text.contains(studio_doc::pixel::LOCKED), "{text}");
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!(layer_names(doc), [studio_doc::BASE_LAYER, "Paint"]);
    assert_eq!(doc.revision, rev, "nothing changed");
    h.state_mut().run("document.dismissError").unwrap();
}

// --- Task 7: commands and shortcuts ---

use crate::tests_lightcraft::{Window, ctrl_q, prompt_shown, run_window};

/// A rendered document saved to `path`, open in Pixel, and a Pixel edit on it (not saved).
pub(crate) fn pixel_edited(h: &mut Harness<'_, crate::StudioShell>, path: &std::path::Path) {
    h.run_steps(3);
    pick_first(h);
    h.state_mut().run("document.new").unwrap();
    settle_base(h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    h.run_steps(1);
    assert!(h.state().document.as_ref().unwrap().unsaved(), "a Pixel edit is unsaved");
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()).len(), 2);
}

/// Press a key with modifiers for the next frame (straight into the input: see `ctrl_q`).
pub(crate) fn press(h: &mut Harness<'_, crate::StudioShell>, modifiers: egui::Modifiers, key: egui::Key) {
    h.input_mut().events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.run_steps(1);
    h.input_mut().events.push(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(1);
}

/// Milestone 5: the document's history reconciles one engine at a time, so only the persona's
/// own engine runs its logic in a frame, and a persona switch takes effect from the next frame.
#[test]
fn one_engine_runs_its_logic_per_frame_across_persona_switches() {
    let mut h = studio();
    h.run_steps(3);
    let counts = |h: &Harness<'_, crate::StudioShell>| (h.state().lightroom.as_ref().unwrap().frames(), h.state().pixel.as_ref().unwrap().frames());
    for persona in ["persona.pixel", "persona.develop", "persona.pixel", "persona.library", "persona.film"] {
        let before = counts(&h);
        h.state_mut().run(persona).unwrap();
        for _ in 0..3 {
            let at = counts(&h);
            h.run_steps(1);
            let after = counts(&h);
            assert!((after.0 - at.0) + (after.1 - at.1) <= 1, "{persona}: two engines ran their logic in one frame");
        }
        let (lr, px) = (counts(&h).0 - before.0, counts(&h).1 - before.1);
        let want = match persona {
            "persona.pixel" => (0, 3),
            "persona.film" => (0, 0),
            _ => (3, 0),
        };
        assert_eq!((lr, px), want, "{persona}: only its own engine, from the next frame on");
    }
}

#[test]
fn pixel_commands_run_photocraft_and_a_change_to_the_develop_layer_says_why() {
    let mut h = studio();
    h.run_steps(3);
    assert!(h.state_mut().run("pixel.layer.new.layer").unwrap_err().contains("no document"));
    h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
    assert_eq!(h.state().pixel.as_ref().unwrap().app.ui.tool, photocraft_ui_egui::state::Tool::Brush);
    assert!(h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "wand of plenty"})).unwrap_err().contains("unknown tool"));
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()).len(), 2, "copied at once");
    let base = h.state().document.as_ref().unwrap().base_layer().unwrap().id.0;
    let e = h.state_mut().run_with("pixel.layer.setProps", &serde_json::json!({"layer": base, "opacity": 0.5})).unwrap_err();
    assert_eq!(e, studio_doc::pixel::LOCKED);
    assert_eq!(h.state().document.as_ref().unwrap().base_layer().unwrap().opacity, 1.0);
    assert!(h.state_mut().run("pixel.no.such.command").is_err());
}

#[test]
fn a_pixel_edit_asks_before_the_document_is_closed_replaced_or_the_window_closes() {
    let dir = temp_dir("pixel-prompt");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    // closing, replacing: the prompt
    assert!(h.state_mut().run("document.close").unwrap_err().contains("unsaved"));
    h.run_steps(2);
    assert!(prompt_shown(&h));
    h.state_mut().run("document.prompt.cancel").unwrap();
    assert!(h.state_mut().run("document.new").unwrap_err().contains("unsaved"));
    h.state_mut().run("document.prompt.cancel").unwrap();
    // the window's close button and Ctrl+Q: stopped, the prompt asks
    assert_eq!(run_window(&mut h, 3, true), Window { closes: 0, cancels: 1, closed: false });
    assert_eq!(h.state().prompt, Some(crate::Pending::Quit));
    h.state_mut().run("document.prompt.cancel").unwrap();
    h.run_steps(2);
    ctrl_q(&mut h);
    // one Close (Studio's Ctrl+Q, not PhotoCraft's), stopped once
    assert_eq!(run_window(&mut h, 6, false), Window { closes: 1, cancels: 1, closed: false });
    assert_eq!(h.state().prompt, Some(crate::Pending::Quit));
    // Save writes the Pixel edit, then closes
    h.state_mut().run("document.prompt.save").unwrap();
    assert_eq!(layer_names(&studio_doc::Document::open(&path).unwrap()).len(), 2);
}

#[test]
fn photoshops_document_keys_act_on_studios_document_in_pixel() {
    let dir = temp_dir("pixel-keys");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    let cmd = egui::Modifiers::COMMAND;
    // Ctrl+S saves the .studio file (PhotoCraft's own save never sees it)
    press(&mut h, cmd, egui::Key::S);
    assert!(!h.state().document.as_ref().unwrap().unsaved());
    assert_eq!(layer_names(&studio_doc::Document::open(&path).unwrap()).len(), 2);
    // Ctrl+O and Shift+Ctrl+S do nothing yet (no dialogs); PhotoCraft still holds one document
    press(&mut h, cmd, egui::Key::O);
    press(&mut h, cmd | egui::Modifiers::SHIFT, egui::Key::S);
    assert_eq!(h.state().pixel.as_ref().unwrap().app.session.documents().len(), 1);
    // Ctrl+W closes the document (saved: no prompt)
    press(&mut h, cmd, egui::Key::W);
    assert!(h.state().document.is_none());
    assert!(h.state().pixel.as_ref().unwrap().app.session.documents().is_empty());
    // Ctrl+1..4 still switch personas from Pixel
    press(&mut h, cmd, egui::Key::Num2);
    assert_eq!(h.state().persona, crate::Persona::Develop);
}

/// Review H1: Studio's document keys match exactly; PhotoCraft's chords that share their letter
/// (Shift+Ctrl+N New Layer, Alt+Ctrl+W Close All, Shift+Ctrl+W, Alt+Ctrl+S Save a Copy) are not
/// taken for them.
#[test]
fn chords_sharing_a_letter_with_studios_document_keys_stay_photocrafts() {
    let dir = temp_dir("pixel-chords");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    h.state_mut().run("document.save").unwrap();
    let (cmd, shift, alt) = (egui::Modifiers::COMMAND, egui::Modifiers::SHIFT, egui::Modifiers::ALT);
    for (mods, key) in [(cmd | shift, egui::Key::N), (cmd | alt, egui::Key::W), (cmd | shift, egui::Key::W), (cmd | alt, egui::Key::N)] {
        press(&mut h, mods, key);
        h.run_steps(2);
        let s = h.state();
        assert!(s.document.as_ref().is_some_and(|d| d.path.as_deref() == Some(path.as_path())), "{mods:?}+{key:?} replaced or closed the document");
        assert!(s.prompt.is_none(), "{mods:?}+{key:?} asked to save");
    }
    // Alt+Ctrl+S isn't Save: a Pixel edit stays unsaved
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    press(&mut h, cmd | alt, egui::Key::S);
    assert!(h.state().document.as_ref().unwrap().unsaved());
}

/// Review H3: Ctrl+C/X/V reach PhotoCraft's pixel commands in Pixel (egui-winit sends them as
/// Copy/Cut/Paste events, which PhotoCraft's raw input hook turns back into key presses).
#[test]
fn copy_cut_and_paste_reach_photocraft_in_pixel() {
    let mut h = studio();
    h.run_steps(3);
    let hooked = |h: &mut Harness<'_, crate::StudioShell>, event: egui::Event| {
        let mut raw = egui::RawInput { events: vec![event], ..Default::default() };
        let ctx = h.ctx.clone();
        h.state_mut().raw_input_hook(&ctx, &mut raw);
        raw.events
    };
    let key = |events: &[egui::Event], want: egui::Key| {
        events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == want && modifiers.command))
    };
    // outside Pixel: as they came
    assert!(matches!(hooked(&mut h, egui::Event::Copy)[..], [egui::Event::Copy]));
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    assert!(key(&hooked(&mut h, egui::Event::Copy), egui::Key::C), "Copy is Ctrl+C for PhotoCraft");
    assert!(key(&hooked(&mut h, egui::Event::Cut), egui::Key::X), "Cut is Ctrl+X");
    assert!(key(&hooked(&mut h, egui::Event::Paste("x".into())), egui::Key::V), "Paste is Ctrl+V");
}

/// Review: PhotoCraft's views follow its documents even on the frame Pixel is clicked in the top
/// bar (its `logic`, which syncs them, didn't run that frame): a document made elsewhere since
/// Pixel was last shown must not be drawn with stale views.
#[test]
fn clicking_pixel_after_a_document_was_made_elsewhere_draws_it() {
    let mut h = studio();
    h.run_steps(3);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(4);
    h.state_mut().run("persona.library").unwrap();
    h.run_steps(2);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.get_by_label("Pixel").click();
    h.run_steps(3);
    assert_eq!(h.state().persona, crate::Persona::Pixel);
    assert!(!painted_text(h.output()).contains("No document"));
}

/// Review: Pixel's canvas does PhotoCraft's document-area bookkeeping: its canvas rectangle (the
/// Navigator, floating panels, zoom and paste use it) and Free Transform's undo steps.
#[test]
fn pixels_canvas_tells_photocraft_where_it_is_and_records_free_transform_steps() {
    let dir = temp_dir("pixel-canvas-rect");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    // PhotoCraft's own document area sets it: under the persona bar, most of the window
    let rect = canvas(&h);
    assert!(rect.min.y > crate::theme::TOKENS.top_bar_h && rect.width() > 700.0 && rect.height() > 500.0, "{rect:?}");
    // Free Transform on a painted layer (an empty one has nothing to transform), nudged with an
    // arrow key: each nudge is a step
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session
        .edit("Paint", |d, active| {
            let mut l = Layer::raster("Painted", PixelFormat::RGBA16);
            l.surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(10, 10, 20, 20), &[1.0, 0.0, 0.0, 1.0].repeat(400));
            *active = Some(l.id);
            d.layers.push(l);
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    let left = |h: &Harness<'_, crate::StudioShell>| h.state().pixel.as_ref().unwrap().app.ui.transform.as_ref().map(|t| t.quad[0][0]);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::T);
    h.run_steps(2);
    let start = left(&h).expect("Free Transform is open");
    // two nudges, a pause after each: two steps; Ctrl+Z takes back only the second
    for _ in 0..2 {
        press(&mut h, egui::Modifiers::NONE, egui::Key::ArrowRight);
        h.run_steps(3);
    }
    assert_eq!(left(&h), Some(start + 2.0));
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert_eq!(left(&h), Some(start + 1.0), "one nudge undone, not the whole transform");
}

/// Review: PhotoCraft holds Studio's document and nothing else. Its own file commands are
/// refused (Studio's are `document.*`); a document it makes some other way (its command palette)
/// is closed; and if it closes Studio's, that is opened again from Studio's copy and said, since
/// its undo history went with it.
#[test]
fn photocraft_holds_studios_document_and_nothing_else() {
    let dir = temp_dir("pixel-files");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let pc =
        |h: &Harness<'_, crate::StudioShell>| h.state().pixel.as_ref().unwrap().app.session.documents().iter().map(|d| d.doc.id).collect::<Vec<_>>();
    let ours = pc(&h);
    for id in ["pixel.file.close", "pixel.file.closeAll", "pixel.file.new"] {
        let e = h.state_mut().run(id).unwrap_err();
        assert!(e.contains("document."), "{id}: {e}");
        h.run_steps(1);
        assert_eq!(pc(&h), ours, "{id}");
    }
    assert!(h.state().pixel.as_ref().unwrap().app.session.active().unwrap().history.can_undo(), "its history is intact");
    // another document in PhotoCraft (its palette's New): closed on the next frame
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session.add_document(pixel_doc(), None);
    h.run_steps(2);
    assert_eq!(pc(&h), ours);
    // PhotoCraft closes Studio's (its palette's Close): opened again from Studio's copy, and said
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session.close(0);
    h.run_steps(2);
    assert_eq!(h.state().pixel.as_ref().unwrap().app.session.documents().len(), 1);
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()).len(), 2, "nothing lost");
    let text = painted_text(h.output());
    assert!(text.contains("undo history"), "{text}");
}

/// Review: work still open in PhotoCraft (a Free Transform not yet applied) counts: closing asks,
/// and saving writes it.
#[test]
fn an_open_free_transform_is_applied_before_closing_asks_or_saving_writes() {
    let dir = temp_dir("pixel-pending");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session
        .edit("Paint", |d, active| {
            let mut l = Layer::raster("Painted", PixelFormat::RGBA16);
            l.surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(10, 10, 20, 20), &[1.0, 0.0, 0.0, 1.0].repeat(400));
            *active = Some(l.id);
            d.layers.push(l);
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    h.state_mut().run("document.save").unwrap();
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::T);
    press(&mut h, egui::Modifiers::NONE, egui::Key::ArrowRight);
    h.run_steps(3);
    assert!(h.state().pixel.as_ref().unwrap().app.ui.transform.is_some());
    // closing: the transform is applied, so the document has changes, and the prompt asks
    assert!(h.state_mut().run("document.close").unwrap_err().contains("unsaved"));
    assert!(h.state().pixel.as_ref().unwrap().app.ui.transform.is_none(), "applied");
    // saving writes it
    h.state_mut().run("document.prompt.save").unwrap();
    let saved = studio_doc::Document::open(&path).unwrap();
    let painted = saved.pixel.layers.iter().find(|l| l.name == "Painted").unwrap();
    let b = photocraft_compose::bounds::content_bounds(painted.surface().unwrap());
    assert_eq!((b.x0, b.y0), (11, 10), "moved one pixel right");
}

#[test]
fn ctrl_z_in_pixel_undoes_pixel_edits_and_leaves_develops_history_alone() {
    let dir = temp_dir("pixel-undo");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    let develop = |h: &Harness<'_, crate::StudioShell>| {
        let lr = h.state().lightroom.as_ref().unwrap();
        let id = h.state().document.as_ref().unwrap().photo().unwrap();
        (lr.app.session.develop_of(id).unwrap().light.exposure, lr.app.session.undo.len())
    };
    let before = develop(&h);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()), [studio_doc::BASE_LAYER], "the Pixel edit is undone");
    assert_eq!(develop(&h), before, "Develop's settings and history are untouched");
    press(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()).len(), 2, "and redone");
}

// --- Task 8: PhotoCraft's overlays ---

#[test]
fn photocrafts_dialogs_show_over_pixel_ctrl_l_opens_levels() {
    let dir = temp_dir("pixel-levels");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    assert!(!painted_text(h.output()).contains("Levels"));
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::L);
    h.run_steps(3);
    let text = painted_text(h.output());
    assert!(text.contains("Levels"), "{text}");
}

fn harness_app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    app.session.add_document(pixel_doc(), None);
    app.sync_views();
    app
}

// --- Milestone 5, task 2: a Develop geometry change is a barrier for Pixel ---

/// A rendered document with a one-pixel "Paint" layer at (300, 100) in PhotoCraft (600 × 400).
fn painted_document(h: &mut Harness<'_, crate::StudioShell>) {
    h.run_steps(3);
    pick_first(h);
    h.state_mut().run("document.new").unwrap();
    settle_base(h);
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session
        .edit("Paint", |d, active| {
            let mut l = Layer::raster("Paint", PixelFormat::RGBA16);
            l.surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(300, 100, 1, 1), &[1.0, 0.0, 0.0, 1.0]);
            *active = Some(l.id);
            d.layers.push(l);
            Ok(())
        })
        .unwrap();
    h.run_steps(1);
}

fn rotate_left(h: &mut Harness<'_, crate::StudioShell>) {
    let id = h.state().document.as_ref().unwrap().photo().unwrap().0;
    let lr = &mut h.state_mut().lightroom.as_mut().unwrap().app.session;
    lr.execute("photo.rotateLeft", &serde_json::json!({"ids": [id]})).unwrap();
}

fn paint_at(doc: &Document) -> (i32, i32) {
    let l = doc.layers.iter().find(|l| l.name == "Paint").unwrap();
    let b = photocraft_compose::bounds::content_bounds(l.surface().unwrap());
    (b.x0, b.y0)
}

#[test]
fn a_save_and_a_close_right_after_a_develop_rotation_write_the_rotated_layers() {
    let dir = temp_dir("follow-save");
    let path = dir.join("turned.studio");
    let mut h = studio();
    painted_document(&mut h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    // rotated in LightCraft, then saved and closed before any frame renders it
    rotate_left(&mut h);
    h.state_mut().run("document.save").unwrap();
    h.state_mut().run("document.close").unwrap();
    let saved = studio_doc::Document::open(&path).unwrap();
    assert_ne!(saved.develop.orientation, Default::default(), "the rotated settings");
    assert_eq!((saved.pixel.size.width, saved.pixel.size.height), (400, 600), "and the rotated canvas");
    assert_eq!(paint_at(&saved.pixel), (100, 299), "and the layer moved with the photo");
}

#[test]
fn pixel_waits_while_develop_renders_a_rotation_and_switching_to_it_lands_the_render_first() {
    let mut h = studio();
    painted_document(&mut h);
    let past = |h: &Harness<'_, crate::StudioShell>| h.state().pixel.as_ref().unwrap().app.session.active().unwrap().history.past_len();
    let before = past(&h);
    rotate_left(&mut h);
    // the frame that sees it starts the render: from now on Pixel waits
    h.run_steps(1);
    assert!(h.state().develop_rendering());
    let e = h.state_mut().run("pixel.layer.new.layer").unwrap_err();
    assert_eq!(e, studio_doc::history::RENDERING, "a Pixel command is refused");
    assert!(h.get_by_label("Develop rendering\u{2026}").rect().width() > 0.0, "the top bar's busy mark");
    // switching to Pixel lands the render on the way: one state, the layer rotated
    h.state_mut().run("persona.pixel").unwrap();
    assert!(!h.state().develop_rendering());
    assert_eq!(past(&h), before + 1);
    let doc = &h.state().pixel.as_ref().unwrap().app.session.active().unwrap().doc;
    assert_eq!(paint_at(doc), (100, 299));
    h.run_steps(2);
    assert!(h.query_by_label("Develop rendering\u{2026}").is_none(), "the mark is gone");
    // now Pixel edits again
    h.state_mut().run("pixel.layer.new.layer").unwrap();
}

#[test]
fn photocrafts_logic_doesnt_run_while_a_develop_rotation_renders() {
    let mut h = studio();
    painted_document(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    // a scripted rotation while Pixel shows (as MCP will do it)
    rotate_left(&mut h);
    let mut waited = 0;
    let t0 = std::time::Instant::now();
    loop {
        let frames = h.state().pixel.as_ref().unwrap().frames();
        h.run_steps(1);
        if !h.state().develop_rendering() {
            break;
        }
        assert_eq!(h.state().pixel.as_ref().unwrap().frames(), frames, "PhotoCraft's logic ran while the rotation rendered");
        waited += 1;
        assert!(t0.elapsed().as_secs() < 300, "the render never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(waited >= 1, "at least the frame that started the render");
    let frames = h.state().pixel.as_ref().unwrap().frames();
    h.run_steps(1);
    assert_eq!(h.state().pixel.as_ref().unwrap().frames(), frames + 1, "and it runs again once it landed");
}

// --- Milestone 5, task 3: one history in the shell ---

/// The document's exposure, as LightCraft has it.
fn exposure(h: &Harness<'_, crate::StudioShell>) -> f64 {
    let id = h.state().document.as_ref().unwrap().photo().unwrap();
    h.state().lightroom.as_ref().unwrap().app.session.develop_of(id).unwrap().light.exposure
}

/// A Develop edit, as LightCraft's panels make one.
fn set_exposure(h: &mut Harness<'_, crate::StudioShell>, value: f64) {
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": value})).unwrap();
    h.run_steps(1);
}

/// A rendered document with a Pixel step ("Paint") and then a Develop step (exposure 1).
fn both_engines_edited(h: &mut Harness<'_, crate::StudioShell>) {
    h.run_steps(3);
    pick_first(h);
    h.state_mut().run("document.new").unwrap();
    settle_base(h);
    add_layer(h, "Paint");
    h.run_steps(1);
    set_exposure(h, 1.0);
    let labels: Vec<_> = h.state().document.as_ref().unwrap().history.entries().into_iter().map(|e| (e.develop, e.pixel)).collect();
    assert_eq!(labels, [(false, false), (false, true), (true, false)], "Original, a Pixel step, a Develop step");
}

fn layer_count(h: &Harness<'_, crate::StudioShell>) -> usize {
    h.state().document.as_ref().unwrap().pixel.layers.len()
}

#[test]
fn ctrl_z_walks_one_history_back_and_forth_from_every_persona() {
    let mut h = studio();
    both_engines_edited(&mut h);
    let (cmd, shift, alt) = (egui::Modifiers::COMMAND, egui::Modifiers::SHIFT, egui::Modifiers::ALT);
    // in Develop: the Develop step, then the Pixel step made before it
    h.state_mut().run("persona.develop").unwrap();
    h.run_steps(2);
    press(&mut h, cmd, egui::Key::Z);
    assert_eq!((exposure(&h), layer_count(&h)), (0.0, 2));
    press(&mut h, cmd, egui::Key::Z);
    assert_eq!((exposure(&h), layer_count(&h)), (0.0, 1), "a Pixel step undone from Develop");
    press(&mut h, cmd | shift, egui::Key::Z);
    press(&mut h, cmd | shift, egui::Key::Z);
    assert_eq!((exposure(&h), layer_count(&h)), (1.0, 2), "both redone");
    // in Pixel: the Develop step, made after the last Pixel edit
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(2);
    press(&mut h, cmd, egui::Key::Z);
    assert_eq!((exposure(&h), layer_count(&h)), (0.0, 2), "a Develop step undone from Pixel");
    // Alt+Ctrl+Z toggles the last state
    press(&mut h, cmd | alt, egui::Key::Z);
    assert_eq!(exposure(&h), 1.0);
    press(&mut h, cmd | alt, egui::Key::Z);
    assert_eq!(exposure(&h), 0.0);
    // and in Film (no engine of its own)
    h.state_mut().run("persona.film").unwrap();
    h.run_steps(1);
    press(&mut h, cmd, egui::Key::Z);
    assert_eq!(layer_count(&h), 1);
    assert_eq!(h.state().document.as_ref().unwrap().history.position(), 0);
}

#[test]
fn the_history_commands_and_photocrafts_own_undo_commands_are_studios() {
    let mut h = studio();
    both_engines_edited(&mut h);
    // PhotoCraft's undo, as a command: Studio's, so the newest step goes, the Develop one
    h.state_mut().run("pixel.edit.undo").unwrap();
    assert_eq!((exposure(&h), layer_count(&h)), (0.0, 2));
    h.state_mut().run("pixel.edit.redo").unwrap();
    assert_eq!(exposure(&h), 1.0);
    h.state_mut().run("pixel.edit.toggleLastState").unwrap();
    assert_eq!(exposure(&h), 0.0);
    h.state_mut().run("edit.undo").unwrap();
    assert_eq!(layer_count(&h), 1);
    h.state_mut().run("edit.redo").unwrap();
    h.state_mut().run("edit.toggleLastState").unwrap();
    assert_eq!((exposure(&h), layer_count(&h)), (1.0, 2), "the toggle redoes while there is a redo");
    h.state_mut().run_with("history.goto", &serde_json::json!({"index": 0})).unwrap();
    assert_eq!((exposure(&h), layer_count(&h)), (0.0, 1));
    h.state_mut().run_with("history.goto", &serde_json::json!({"index": 2})).unwrap();
    assert_eq!((exposure(&h), layer_count(&h)), (1.0, 2));
    assert!(h.state_mut().run_with("history.goto", &serde_json::json!({"index": 3})).is_err());
    assert!(h.state_mut().run("history.goto").is_err(), "needs an index");
    h.state_mut().run_with("document.close", &serde_json::json!({"discard": true})).unwrap();
    assert!(h.state_mut().run("edit.undo").unwrap_err().contains("no document"));
}

#[test]
fn pixels_history_tab_lists_both_engines_steps_and_a_click_jumps() {
    let mut h = studio();
    both_engines_edited(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    // Studio's History tab; PhotoCraft's own History panel is off (its footer did nothing: L4)
    let tab = history_rect(&h);
    assert!(!h.state().pixel.as_ref().unwrap().app.ui.panels.history);
    // a widget labelled `label` in the tab (the Layers panel names "Paint" too)
    let in_tab = |h: &Harness<'_, crate::StudioShell>, label: &str| h.get_all_by_label(label).filter(|n| tab.contains(n.rect().center())).count();
    let develop = h.state().document.as_ref().unwrap().history.entries()[2].label.clone();
    for row in ["Original", "Paint", develop.as_str()] {
        assert_eq!(in_tab(&h, row), 1, "{row}");
    }
    // each row says which persona made it
    assert_eq!((in_tab(&h, "Pixel"), in_tab(&h, "Develop")), (1, 1));
    let click = |h: &mut Harness<'_, crate::StudioShell>, label: &str| {
        h.get_all_by_label(label).find(|n| tab.contains(n.rect().center())).unwrap().click();
        h.run_steps(2);
    };
    click(&mut h, "Paint");
    assert_eq!((exposure(&h), layer_count(&h), h.state().document.as_ref().unwrap().history.position()), (0.0, 2, 1));
    click(&mut h, "Original");
    assert_eq!(layer_count(&h), 1);
    // the undone rows stay, and a click goes forward again
    click(&mut h, &develop);
    assert_eq!((exposure(&h), layer_count(&h)), (1.0, 2));
}

#[test]
fn ctrl_z_in_a_text_field_is_the_fields_own() {
    let mut h = studio();
    both_engines_edited(&mut h);
    h.state_mut().run("persona.library").unwrap();
    h.run_steps(3);
    let field = h.query_all(egui_kittest::kittest::By::new().role(egui::accesskit::Role::TextInput)).next().map(|n| n.rect().center());
    let field = field.expect("a text field in Library");
    h.hover_at(field);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: field, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    assert!(h.ctx.egui_wants_keyboard_input(), "the field has the keyboard");
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!((exposure(&h), layer_count(&h)), (1.0, 2), "the history is untouched");
}

#[test]
fn with_no_document_ctrl_z_in_library_is_lightcrafts_own() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    let rating = |h: &Harness<'_, crate::StudioShell>| {
        let lr = &h.state().lightroom.as_ref().unwrap().app.session;
        lr.catalog.photo(lr.active().unwrap()).unwrap().rating
    };
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("photo.rate", serde_json::json!({"rating": 3})).unwrap();
    h.run_steps(1);
    assert_eq!(rating(&h), 3);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!(rating(&h), 0, "LightCraft undid its rating");
}

#[test]
fn ctrl_z_mid_stroke_commits_the_stroke_then_undoes_it() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
    h.run_steps(2);
    let painted = |h: &Harness<'_, crate::StudioShell>| {
        let doc = &h.state().document.as_ref().unwrap().pixel;
        doc.layers.last().unwrap().surface().unwrap().tile_count() > 0
    };
    assert!(!painted(&h));
    let c = canvas(&h).center();
    h.event(egui::Event::PointerMoved(c));
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    for i in 1..6 {
        h.event(egui::Event::PointerMoved(c + egui::vec2(8.0 * i as f32, 4.0 * i as f32)));
        h.run_steps(1);
    }
    // Ctrl+Z with the button still down: nothing is undone under the stroke
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!(layer_count(&h), 2, "the new layer is still there");
    let end = c + egui::vec2(40.0, 20.0);
    h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(3);
    // the stroke was committed (a step), then undone
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!((layer_count(&h), painted(&h), doc.history.can_redo()), (2, false, true));
    press(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
    assert!(painted(&h), "and it redoes");
}

/// A Pixel edit made in a frame, then a LightCraft command before the next one (as MCP or the
/// smoke script drive it): the steps keep the order they were made in.
#[test]
fn a_pixel_edit_and_a_develop_command_right_after_it_keep_their_order() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    // PhotoCraft's own New Layer key, in its logic, in the last frame
    let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    h.input_mut().events.push(egui::Event::Key { key: egui::Key::N, physical_key: None, pressed: true, repeat: false, modifiers });
    h.run_steps(1);
    assert_eq!(h.state().pixel.as_ref().unwrap().app.session.active().unwrap().doc.layers.len(), 2);
    let lr = h.state_mut().lightroom.as_mut().unwrap();
    lr.app.run("develop.set", serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
    h.run_steps(1);
    let kinds: Vec<_> = h.state().document.as_ref().unwrap().history.entries().into_iter().map(|e| (e.develop, e.pixel)).collect();
    assert_eq!(kinds, [(false, false), (false, true), (true, false)]);
}

/// Review: what PhotoCraft has open is no reason to skip Studio's order. A pen path still open
/// (no keyboard focus, so Ctrl+Z is Studio's) is applied, then the newest step goes: one step,
/// never the Develop step and the Pixel step under it.
#[test]
fn ctrl_z_with_a_pen_path_open_undoes_one_step() {
    let mut h = studio();
    both_engines_edited(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    let app = &mut h.state_mut().pixel.as_mut().unwrap().app;
    app.ui.tool = photocraft_ui_egui::state::Tool::Pen;
    photocraft_ui_egui::vector_ui::pen_down(app, 50.0, 50.0);
    photocraft_ui_egui::vector_ui::pen_up(app);
    assert!(app.ui.pen.is_some(), "a pen path is open");
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    let doc = h.state().document.as_ref().unwrap();
    assert!(layer_names(doc).iter().any(|n| n == "Paint"), "the Pixel step under the Develop one stays");
    assert_eq!(doc.history.entries().len() - 1 - doc.history.position(), 1, "one step undone");
}

/// Review: leaving Pixel applies what PhotoCraft has open (design §2's flush), so a Free
/// Transform left open doesn't take Ctrl+Z in Develop: the Develop edit made there is undone,
/// then the transform, in the order they were made.
#[test]
fn leaving_pixel_applies_a_free_transform_so_ctrl_z_elsewhere_is_the_historys() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    let session = &mut h.state_mut().pixel.as_mut().unwrap().app.session;
    session
        .edit("Paint", |d, active| {
            let mut l = Layer::raster("Painted", PixelFormat::RGBA16);
            l.surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(10, 10, 20, 20), &[1.0, 0.0, 0.0, 1.0].repeat(400));
            *active = Some(l.id);
            d.layers.push(l);
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::T);
    press(&mut h, egui::Modifiers::NONE, egui::Key::ArrowRight);
    h.run_steps(3);
    assert!(h.state().pixel.as_ref().unwrap().app.ui.transform.is_some());
    h.state_mut().run("persona.develop").unwrap();
    assert!(h.state().pixel.as_ref().unwrap().app.ui.transform.is_none(), "applied on the way out");
    h.run_steps(2);
    set_exposure(&mut h, 1.0);
    let left = |h: &Harness<'_, crate::StudioShell>| {
        let doc = &h.state().document.as_ref().unwrap().pixel;
        let l = doc.layers.iter().find(|l| l.name == "Painted").unwrap();
        photocraft_compose::bounds::content_bounds(l.surface().unwrap()).x0
    };
    assert_eq!(left(&h), 11);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!((exposure(&h), left(&h)), (0.0, 11), "the Develop edit, made last");
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!(left(&h), 10, "then the transform");
}

/// Review: every undo key pressed during one stroke counts, in order, once the stroke is in:
/// Ctrl+Z twice undoes the stroke and the step before it; Ctrl+Z then Shift+Ctrl+Z undoes the
/// stroke and redoes it.
#[test]
fn undo_keys_pressed_twice_mid_stroke_each_count_in_order() {
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
    let painted = |h: &Harness<'_, crate::StudioShell>| {
        let doc = &h.state().document.as_ref().unwrap().pixel;
        doc.layers.len() == 2 && doc.layers[1].surface().unwrap().tile_count() > 0
    };
    let c = canvas(&h).center();
    let (cmd, shift) = (egui::Modifiers::COMMAND, egui::Modifiers::SHIFT);
    for (keys, want) in [([cmd, cmd], (1, false)), ([cmd, cmd | shift], (2, true))] {
        h.state_mut().run("pixel.layer.new.layer").unwrap();
        h.run_steps(2);
        h.event(egui::Event::PointerMoved(c));
        h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        for i in 1..6 {
            h.event(egui::Event::PointerMoved(c + egui::vec2(8.0 * i as f32, 4.0 * i as f32)));
            h.run_steps(1);
        }
        for mods in keys {
            press(&mut h, mods, egui::Key::Z);
        }
        let end = c + egui::vec2(40.0, 20.0);
        h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(3);
        assert_eq!((layer_count(&h), painted(&h)), want, "{keys:?}");
        // back to just the Develop layer for the next round
        h.state_mut().run_with("history.goto", &serde_json::json!({"index": 0})).unwrap();
    }
}

// --- Milestone 5, task 5: save and close settle the document first ---

/// A rendered document saved to `path`, open in Pixel with the brush on a new empty layer
/// (saved too).
fn ready_to_paint(h: &mut Harness<'_, crate::StudioShell>, path: &std::path::Path) {
    h.run_steps(3);
    pick_first(h);
    h.state_mut().run("document.new").unwrap();
    settle_base(h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    h.run_steps(2);
}

/// Press the button on the canvas and drag: a stroke in progress (not released).
fn stroke_down(h: &mut Harness<'_, crate::StudioShell>) -> egui::Pos2 {
    let c = canvas(h).center();
    h.event(egui::Event::PointerMoved(c));
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    for i in 1..6 {
        h.event(egui::Event::PointerMoved(c + egui::vec2(8.0 * i as f32, 4.0 * i as f32)));
        h.run_steps(1);
    }
    c + egui::vec2(40.0, 20.0)
}

fn release(h: &mut Harness<'_, crate::StudioShell>, at: egui::Pos2) {
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
}

fn key(h: &mut Harness<'_, crate::StudioShell>, modifiers: egui::Modifiers, key: egui::Key) {
    h.input_mut().events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
}

/// The top layer of the file at `path` has paint on it.
fn saved_painted(path: &std::path::Path) -> bool {
    let saved = studio_doc::Document::open(path).unwrap();
    saved.pixel.layers.last().unwrap().surface().unwrap().tile_count() > 0
}

/// Ctrl+S in the frame a stroke ends, or while it is still being drawn: the file has the stroke
/// (a save never writes half a stroke, nor one without it) and the document is saved.
#[test]
fn ctrl_s_with_a_stroke_ending_or_in_progress_saves_the_stroke() {
    let dir = temp_dir("stroke-save");
    for held in [false, true] {
        let path = dir.join(format!("held-{held}.studio"));
        let mut h = studio();
        ready_to_paint(&mut h, &path);
        let end = stroke_down(&mut h);
        if held {
            key(&mut h, egui::Modifiers::COMMAND, egui::Key::S);
            h.run_steps(1);
            assert!(!saved_painted(&path), "held: nothing is saved under the stroke");
        }
        release(&mut h, end);
        if !held {
            key(&mut h, egui::Modifiers::COMMAND, egui::Key::S);
        }
        h.run_steps(2);
        assert!(saved_painted(&path), "held: {held}: the stroke is in the file");
        assert!(!h.state().document.as_ref().unwrap().unsaved(), "held: {held}: and saved");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The window loses focus mid-stroke (Alt+Tab), so its release is never reported: the stroke
/// ends anyway, and a save pressed during it runs, with it, instead of waiting for a next click.
#[test]
fn a_stroke_cut_short_by_losing_focus_ends_and_the_save_held_for_it_runs() {
    let dir = temp_dir("focus-save");
    let path = dir.join("trip.studio");
    let mut h = studio();
    ready_to_paint(&mut h, &path);
    stroke_down(&mut h);
    key(&mut h, egui::Modifiers::COMMAND, egui::Key::S);
    h.run_steps(2);
    assert!(!saved_painted(&path) && !h.state().held.is_empty(), "waiting for the stroke");
    let mut raw = egui::RawInput { events: vec![egui::Event::WindowFocused(false)], ..Default::default() };
    let ctx = h.ctx.clone();
    h.state_mut().raw_input_hook(&ctx, &mut raw);
    h.input_mut().events.extend(raw.events);
    h.run_steps(2);
    assert!(!h.ctx.input(|i| i.pointer.any_down()), "the button is up");
    assert!(saved_painted(&path), "the stroke ended and the held save wrote it");
    assert!(h.state().held.is_empty() && !h.state().document.as_ref().unwrap().unsaved());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Review: the stroke leaves the window (egui forgets where the pointer is), then the window
/// loses focus: the release that ends it is where the pointer was last seen, so the stroke
/// gets no segment to the window's corner.
#[test]
fn a_stroke_dragged_out_of_the_window_then_cut_short_by_focus_loss_stays_where_it_was_drawn() {
    let dir = temp_dir("focus-gone");
    let path = dir.join("trip.studio");
    let bounds = |h: &Harness<'_, crate::StudioShell>| {
        let doc = &h.state().pixel.as_ref().unwrap().app.session.active().unwrap().doc;
        let b = photocraft_compose::bounds::content_bounds(doc.layers.last().unwrap().surface().unwrap());
        (b.x0, b.y0, b.x1, b.y1)
    };
    // the same stroke, released where it was last drawn
    let mut h = studio();
    ready_to_paint(&mut h, &path);
    let end = stroke_down(&mut h);
    release(&mut h, end);
    h.run_steps(2);
    let drawn = bounds(&h);
    let mut h = studio();
    ready_to_paint(&mut h, &path);
    stroke_down(&mut h);
    h.event(egui::Event::PointerGone);
    h.run_steps(2);
    let mut raw = egui::RawInput { events: vec![egui::Event::WindowFocused(false)], ..Default::default() };
    let ctx = h.ctx.clone();
    h.state_mut().raw_input_hook(&ctx, &mut raw);
    h.input_mut().events.extend(raw.events);
    h.run_steps(2);
    assert!(!h.ctx.input(|i| i.pointer.any_down()), "the button is up");
    let ended = bounds(&h);
    assert_eq!(ended, drawn, "no segment towards the corner");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ctrl+W then Ctrl+Z in one frame in Pixel: the document closes, and the undo queued behind it
/// is dropped without a word (there is nothing left to undo).
#[test]
fn an_undo_key_queued_after_a_close_is_dropped_silently() {
    let dir = temp_dir("close-undo");
    let path = dir.join("trip.studio");
    let mut h = studio();
    ready_to_paint(&mut h, &path);
    key(&mut h, egui::Modifiers::COMMAND, egui::Key::W);
    key(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert!(h.state().document.is_none(), "closed");
    assert_eq!(h.state().error, None, "no error for the undo");
    assert!(h.state().held.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A stroke undone, then saved (the keys in one frame, as typed fast): the file hasn't got it.
#[test]
fn an_undone_stroke_isnt_saved() {
    let dir = temp_dir("undo-save");
    let path = dir.join("trip.studio");
    let mut h = studio();
    ready_to_paint(&mut h, &path);
    let end = stroke_down(&mut h);
    release(&mut h, end);
    h.run_steps(2);
    assert!(h.state().document.as_ref().unwrap().unsaved());
    key(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    key(&mut h, egui::Modifiers::COMMAND, egui::Key::S);
    h.run_steps(2);
    assert!(!saved_painted(&path), "the undone stroke isn't in the file");
    assert!(!h.state().document.as_ref().unwrap().unsaved());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A render still in flight for a Develop edit that is undone and saved: it lands nowhere (the
/// base keeps showing the saved settings, nothing renders again) and the file stays as saved.
#[test]
fn a_late_render_for_an_undone_develop_edit_is_dropped_and_the_file_is_unchanged() {
    let dir = temp_dir("late-render");
    let path = dir.join("trip.studio");
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    let shown = h.state().document.as_ref().unwrap().shown_hash();
    let started = h.state().base.started();
    set_exposure(&mut h, 1.0);
    assert!(h.state().base.rendering() && h.state().base.started() == started + 1, "the edit's render is in flight");
    h.state_mut().run("edit.undo").unwrap();
    h.state_mut().run("document.save").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let t0 = std::time::Instant::now();
    while h.state().base.rendering() {
        assert!(t0.elapsed() < std::time::Duration::from_secs(300), "the render never finished");
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!(doc.shown_hash(), shown, "the base still shows the saved settings");
    assert_eq!(h.state().base.started(), started + 1, "and needs no new render");
    assert!(!doc.unsaved());
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is as saved");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A save over an existing file that can't be completed (the file and its folder read-only:
/// Windows refuses to replace the file, Unix to create the temporary one beside it) leaves the
/// old file as it was, says why, and keeps the document unsaved.
#[test]
fn an_interrupted_save_keeps_the_old_file_and_says_why() {
    let dir = temp_dir("save-interrupted");
    let path = dir.join("trip.studio");
    let mut h = studio();
    pixel_edited(&mut h, &path);
    let old = std::fs::read(&path).unwrap();
    let readonly = |p: &std::path::Path| {
        let before = std::fs::metadata(p).unwrap().permissions();
        let mut ro = before.clone();
        ro.set_readonly(true);
        std::fs::set_permissions(p, ro).unwrap();
        before
    };
    let (file, folder) = (readonly(&path), readonly(&dir));
    let r = h.state_mut().run("document.save");
    h.run_steps(2);
    std::fs::set_permissions(&dir, folder).unwrap();
    std::fs::set_permissions(&path, file).unwrap();
    let why = r.unwrap_err();
    assert_eq!(std::fs::read(&path).unwrap(), old, "the old file is untouched");
    assert!(h.state().document.as_ref().unwrap().unsaved(), "the edit is still unsaved");
    let name = h.state().document.as_ref().unwrap().name.clone();
    h.get_by_label(format!("Couldn\u{2019}t save \u{201c}{name}\u{201d}").as_str());
    h.get_by_label(why.as_str());
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, [std::ffi::OsString::from("trip.studio")], "no temporary file left behind");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Milestone 5, task 6: the round trip end to end ---

/// What a journey step changes: (exposure, layers, the paint's bounds if any, canvas size).
type Snap = (f64, usize, Option<(i32, i32, i32, i32)>, (u32, u32));

fn snap(h: &Harness<'_, crate::StudioShell>) -> Snap {
    let doc = &h.state().document.as_ref().unwrap().pixel;
    let paint = doc.layers.get(1).and_then(|l| l.surface()).filter(|s| s.tile_count() > 0).map(|s| {
        let b = photocraft_compose::bounds::content_bounds(s);
        (b.x0, b.y0, b.x1, b.y1)
    });
    (exposure(h), doc.layers.len(), paint, (doc.size.width, doc.size.height))
}

/// Milestone 5 from start to end: a photo picked, Pixel makes its document; a Develop edit, a new
/// layer and a stroke in Pixel, a rotation in Develop that the paint follows; Ctrl+Z walks the one
/// history back in order and Shift+Ctrl+Z forward again, from Develop and from Pixel; saved,
/// reopened (settings and layers kept, the history starts empty) and closed.
#[test]
fn journey_pick_pixel_develop_paint_rotate_undo_redo_save_reopen_close() {
    let dir = temp_dir("journey");
    let path = dir.join("journey.studio");
    let mut h = studio();
    h.run_steps(3);
    pick_first(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    settle_base(&mut h);
    let mut states = vec![snap(&h)];
    // Develop: an edit
    h.state_mut().run("persona.develop").unwrap();
    set_exposure(&mut h, 1.0);
    settle_base(&mut h);
    states.push(snap(&h));
    // Pixel: a new layer, and a brush stroke on it
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
    h.run_steps(2);
    states.push(snap(&h));
    let end = stroke_down(&mut h);
    release(&mut h, end);
    h.run_steps(2);
    states.push(snap(&h));
    // Develop: rotate left; the paint follows the photo
    h.state_mut().run("persona.develop").unwrap();
    h.run_steps(2);
    rotate_left(&mut h);
    settle_base(&mut h);
    states.push(snap(&h));
    let (_, _, Some((x0, y0, x1, y1)), (w, height)) = states[3] else { panic!("no paint: {:?}", states[3]) };
    assert_eq!(states[4].3, (height, w), "the canvas turned");
    assert_eq!(states[4].2, Some((y0, w as i32 - x1, y1, w as i32 - x0)), "the paint turned with the photo");
    let kinds: Vec<_> = h.state().document.as_ref().unwrap().history.entries().into_iter().map(|e| (e.develop, e.pixel)).collect();
    assert_eq!(kinds, [(false, false), (true, false), (false, true), (false, true), (true, true)]);
    let (cmd, shift) = (egui::Modifiers::COMMAND, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
    for persona in ["persona.develop", "persona.pixel"] {
        h.state_mut().run(persona).unwrap();
        h.run_steps(2);
        for i in (0..4).rev() {
            press(&mut h, cmd, egui::Key::Z);
            assert_eq!(snap(&h), states[i], "{persona}: undo to {i}");
        }
        for (i, state) in states.iter().enumerate().skip(1) {
            press(&mut h, shift, egui::Key::Z);
            assert_eq!(&snap(&h), state, "{persona}: redo to {i}");
        }
    }
    settle_base(&mut h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    h.state_mut().run("document.close").unwrap();
    assert!(h.state().document.is_none());
    h.state_mut().run_with("document.open", &serde_json::json!({"path": path})).unwrap();
    settle_base(&mut h);
    assert_eq!(snap(&h), states[4], "reopened as saved");
    let doc = h.state().document.as_ref().unwrap();
    assert_ne!(doc.develop.orientation, Default::default(), "the rotation kept");
    assert_eq!(doc.history.entries().len(), 1, "the history is per session: only the original");
    h.state_mut().run("document.close").unwrap();
    assert!(h.state().document.is_none());
    assert!(h.state().pixel.as_ref().unwrap().app.session.documents().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Milestone 5, Codex review ---

/// Review (HIGH 2): while a Develop rotation is on its way into the layers, Pixel takes no
/// pointer input either (its canvas, panels and windows), not only no commands: a brush stroke
/// on the canvas then paints nothing, and the rotation is the one state added, the paint turned.
#[test]
fn a_stroke_on_the_canvas_while_a_rotation_renders_paints_nothing() {
    let mut h = studio();
    painted_document(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    h.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
    h.run_steps(2);
    let past = |h: &Harness<'_, crate::StudioShell>| h.state().pixel.as_ref().unwrap().app.session.active().unwrap().history.past_len();
    let before = past(&h);
    // a scripted rotation while Pixel shows (as MCP will do it); the frame that sees it starts
    // the render, and the button held for the stroke keeps it from landing meanwhile
    rotate_left(&mut h);
    h.run_steps(1);
    assert!(h.state().develop_rendering());
    let end = stroke_down(&mut h);
    assert!(h.state().develop_rendering(), "still waiting under the held button");
    release(&mut h, end);
    settle_base(&mut h);
    h.run_steps(2);
    assert!(!h.state().develop_rendering());
    let doc = &h.state().pixel.as_ref().unwrap().app.session.active().unwrap().doc;
    let l = doc.layers.iter().find(|l| l.name == "Paint").unwrap();
    let b = photocraft_compose::bounds::content_bounds(l.surface().unwrap());
    assert_eq!((b.x0, b.y0, b.x1, b.y1), (100, 299, 101, 300), "only the old pixel, turned");
    assert_eq!(past(&h), before + 1, "the rotation is the one new state");
}

// --- T3: Pixel is PhotoCraft's own window ---

/// A pointer click at `pos` (a real click: kittest's `click()` goes past what covers a widget).
fn click_at(h: &mut Harness<'_, crate::StudioShell>, pos: egui::Pos2, button: egui::PointerButton) {
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

/// L15: PhotoCraft's document tab × (and the tab's Close menu) would close its document straight
/// through its engine, past Studio's close (and Studio would only put it back, its undo history
/// gone). Studio covers its tab strip: the × and a right-click do nothing, the history stays.
#[test]
fn l15_photocrafts_document_tab_cant_close_the_document() {
    let dir = temp_dir("l15-tab");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    h.run_steps(2);
    let close = h.get_by_label("Close").rect();
    let strip = h.state().pixel.as_ref().unwrap().tab_strip(&h.ctx).unwrap();
    assert!(strip.contains_rect(close), "the × {close:?} is in the covered strip {strip:?}");
    click_at(&mut h, close.center(), egui::PointerButton::Primary);
    let s = h.state();
    let pc = &s.pixel.as_ref().unwrap().app.session;
    assert_eq!(pc.documents().len(), 1, "PhotoCraft still holds the document");
    assert!(pc.active().unwrap().history.can_undo() && s.error.is_none(), "its undo history stays, and nothing was put back");
    // the tab's right-click menu (Close, Close Others, Close All) doesn't open
    click_at(&mut h, strip.left_center() + egui::vec2(30.0, 0.0), egui::PointerButton::Secondary);
    assert!(h.query_by_label("Close Others").is_none() && h.query_by_label("Close All").is_none());
    assert!(h.state().document.is_some());
}

/// PhotoCraft holds Studio's document and nothing else, with none open too: a document it makes
/// itself (its home screen's New, its palette) is closed, and Studio says how to get one.
#[test]
fn a_document_photocraft_makes_itself_without_studios_is_closed() {
    let mut h = studio();
    h.run_steps(3);
    // no photo picked: Pixel opens with no document
    h.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = None;
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    assert!(h.state().document.is_none());
    h.state_mut().pixel.as_mut().unwrap().app.session.add_document(pixel_doc(), None);
    h.run_steps(1);
    assert!(h.state().pixel.as_ref().unwrap().app.session.documents().is_empty());
    assert!(h.state().document.is_none());
    assert!(h.state().error.as_ref().is_some_and(|(_, why)| why.contains("Ctrl+O")), "{:?}", h.state().error);
}

/// Until Studio hosts PhotoCraft's menus (T3m), the keys of the File items it will hide are
/// dropped in Pixel: each leaves PhotoCraft's session (its documents, the document's state) and
/// Studio's document as they were.
#[test]
fn keys_of_photocraft_file_items_studio_will_hide_do_nothing() {
    let dir = temp_dir("t3-hidden-keys");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let (cmd, alt, shift) = (egui::Modifiers::COMMAND, egui::Modifiers::ALT, egui::Modifiers::SHIFT);
    let state = |h: &Harness<'_, crate::StudioShell>| {
        let pc = &h.state().pixel.as_ref().unwrap().app.session;
        let docs: Vec<_> = pc.documents().iter().map(|d| (d.doc.id, d.revision, d.history.past_len())).collect();
        (docs, layer_names(h.state().document.as_ref().unwrap()), h.state().error.clone(), h.state().prompt.clone())
    };
    let before = state(&h);
    for (mods, k) in [
        (egui::Modifiers::NONE, egui::Key::F12),
        (cmd | alt, egui::Key::W),
        (cmd | alt, egui::Key::P),
        (cmd | alt | shift, egui::Key::O),
        (cmd | alt, egui::Key::S),
    ] {
        press(&mut h, mods, k);
        h.run_steps(2);
        assert_eq!(state(&h), before, "{mods:?}+{k:?}");
        assert!(h.query_by_label("Save a Copy").is_none() && h.query_by_label("Open As").is_none(), "{mods:?}+{k:?} opened a dialog");
    }
}
