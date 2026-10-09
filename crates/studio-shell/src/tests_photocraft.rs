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

fn harness_app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    app.session.add_document(pixel_doc(), None);
    app.sync_views();
    app
}
