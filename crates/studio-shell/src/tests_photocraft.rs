//! Milestone 4 spike: can PhotoCraft's editor be drawn piece by piece in Studio's places?
//!
//! (a) its tool bar (left), options bar (top), right dock and one canvas view fit around a dock
//!     tab layout, driven through its `eframe::App` with a test frame;
//! (b) its fonts and LightCraft's live in one context: both replace all fonts, and each needs its
//!     own named families.

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

fn harness() -> Harness<'static, Spike> {
    Harness::builder().with_size([1400.0, 900.0]).build_ui_state(frame, Spike { app: harness_app(), frame: eframe::Frame::_new_kittest(), frames: 0 })
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

#[test]
fn spike_a_photocraft_pieces_draw_in_studio_places() {
    let mut h = harness();
    h.run_steps(6);
    let text = painted_text(h.output());
    for panel in ["Layers", "Properties", "Opacity:"] {
        assert!(text.contains(panel), "{panel}: {text}");
    }
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
    assert!(painted_text(h.output()).contains("My Photos"));
    h.state_mut().photocraft = true;
    h.run_steps(6);
    assert!(painted_text(h.output()).contains("Layers"));
    h.state_mut().photocraft = false;
    h.run_steps(4);
    assert!(painted_text(h.output()).contains("My Photos"));
    // and Develop's panels, which use LightCraft's semibold family
    h.state_mut().shell.run("persona.develop").unwrap();
    h.run_steps(4);
    let text = painted_text(h.output());
    assert!(text.contains("Presets"), "{text}");
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

/// The rectangle of the dock group holding `tab` in Pixel.
fn pixel_leaf(h: &Harness<'_, crate::StudioShell>, tab: &crate::dock::Tab) -> Rect {
    let dock = &h.state().layouts[crate::Persona::Pixel as usize];
    let path = dock.find_tab(tab).unwrap();
    match &dock[path.surface][path.node] {
        egui_dock::Node::Leaf(l) => l.rect,
        _ => panic!("{tab:?} isn't in a leaf"),
    }
}

#[test]
fn pixel_is_photoshops_layout_on_photocraft() {
    let mut h = studio();
    h.run_steps(3);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(4);
    // no document yet: the canvas says how to get one
    assert!(painted_text(h.output()).contains("No document"), "{}", painted_text(h.output()));
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
    // PhotoCraft's options bar, Layers panel (with the document's base layer) and History
    for want in ["Opacity:", "Layers", "Properties", "History", studio_doc::BASE_LAYER] {
        assert!(text.contains(want), "{want}: {text}");
    }
    // its canvas fills Studio's canvas tab, which is titled with the document's name
    let name = h.state().document.as_ref().unwrap().name.clone();
    assert_eq!(h.get_all_by_label(name.as_str()).count(), 2, "the top bar's and the canvas tab's");
    let tab = pixel_leaf(&h, &crate::dock::Tab::Canvas);
    let images: Vec<Rect> = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(m.calc_bounds()),
            _ => None,
        })
        .collect();
    assert!(images.iter().any(|r| tab.contains_rect(*r) && r.width() > 100.0), "the document's image in {tab:?}: {images:?}");
    // Photoshop's tool keys pick from their tool's group: V Move, B a brush, M a marquee, E an eraser
    for (key, group) in [(egui::Key::V, 'V'), (egui::Key::B, 'B'), (egui::Key::M, 'M'), (egui::Key::E, 'E')] {
        h.key_press(key);
        h.run_steps(2);
        let tool = h.state().pixel.as_ref().unwrap().app.ui.tool;
        assert_eq!(tool.key(), group, "{key:?}: {tool:?}");
    }
    // and the theme stays Studio's around it
    assert_eq!(h.ctx.global_style().visuals.panel_fill, crate::theme::TOKENS.chrome);
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
fn pixel_edited(h: &mut Harness<'_, crate::StudioShell>, path: &std::path::Path) {
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
fn press(h: &mut Harness<'_, crate::StudioShell>, modifiers: egui::Modifiers, key: egui::Key) {
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
    let tab = pixel_leaf(&h, &crate::dock::Tab::Canvas);
    let canvas = h.state().pixel.as_ref().unwrap().app.last_canvas_rect;
    assert!(tab.expand(1.0).contains_rect(canvas) && canvas.width() > tab.width() * 0.8, "{canvas:?} in {tab:?}");
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
        let id = h.state().document.as_ref().unwrap().source.photo;
        (lr.app.session.develop_of(id).unwrap().light.exposure, lr.app.session.undo.len())
    };
    let before = develop(&h);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()), [studio_doc::BASE_LAYER], "the Pixel edit is undone");
    assert_eq!(develop(&h), before, "Develop's settings and history are untouched");
    press(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()).len(), 2, "and redone");
}

#[test]
fn files_dropped_on_pixel_dont_open_in_photocraft() {
    let dir = temp_dir("pixel-drop");
    let png = dir.join("dropped.png");
    // a 1×1 PNG
    std::fs::write(
        &png,
        [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01,
            0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ],
    )
    .unwrap();
    // PhotoCraft as Studio sets it up (no import service, its own document area not drawn)
    // neither opens dropped files nor places them as layers (opening a PNG or PSD in Pixel is
    // milestone 5's)
    let mut h = studio();
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    let drop = |h: &mut Harness<'_, crate::StudioShell>| {
        h.input_mut().dropped_files.push(std::sync::Arc::new(crate::tests_lightcraft::Dropped(png.clone())));
        h.run_steps(3);
    };
    drop(&mut h);
    assert!(h.state().pixel.as_ref().unwrap().app.session.documents().is_empty());
    pick_first(&mut h);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run("persona.pixel").unwrap();
    h.run_steps(3);
    drop(&mut h);
    assert_eq!(h.state().pixel.as_ref().unwrap().app.session.documents().len(), 1);
    assert_eq!(layer_names(h.state().document.as_ref().unwrap()), [studio_doc::BASE_LAYER]);
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
    // in Studio's frame: its theme around the dialog stays Studio's
    assert_eq!(h.ctx.global_style().visuals.panel_fill, crate::theme::TOKENS.chrome);
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
    let id = h.state().document.as_ref().unwrap().source.photo.0;
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
    let id = h.state().document.as_ref().unwrap().source.photo;
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
    let tab = pixel_leaf(&h, &crate::dock::Tab::Panel("History".to_owned()));
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
    let c = pixel_leaf(&h, &crate::dock::Tab::Canvas).center();
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
