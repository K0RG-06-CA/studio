//! Milestone 5, task 4: documents without a photo. An image file (PNG, JPEG, TIFF, PSD…) opens
//! straight into Pixel; Develop and Film are off for it; it is never saved over the image; and
//! opening anything asks first when the open document has changes.

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use photocraft_color::{BlendMode, ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Layer, LayerMask};
use photocraft_geom::{Rect, Size};

use crate::tests_lightcraft::{Dropped, settle_base, studio, temp_dir};
use crate::tests_photocraft::{pixel_edited, press};
use crate::{Pending, Persona, StudioShell};

/// An image file as PhotoCraft writes it, at `dir/name`: a PSD (a painted layer, and a group
/// holding a masked Multiply layer) or a flat image of the name's format.
pub(crate) fn image(dir: &Path, name: &str) -> PathBuf {
    let ext = name.rsplit('.').next().unwrap_or_default();
    let mut d = photocraft_doc::Document::new("art", Size { width: 8, height: 6 }, ColorMode::Rgb, SampleType::U8);
    let mut paint = Layer::raster("Paint", PixelFormat::RGBA8);
    paint.surface_mut().unwrap().write_region(Rect::from_xywh(1, 1, 2, 1), &[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
    d.layers.push(paint);
    if ext == "psd" {
        let mut shade = Layer::raster("Shade", PixelFormat::RGBA8);
        shade.surface_mut().unwrap().write_region(Rect::from_xywh(0, 0, 1, 1), &[0.5, 0.5, 0.5, 1.0]);
        shade.blend = BlendMode::Multiply;
        shade.mask = Some(LayerMask::hide_all());
        d.layers.push(Layer::group("Group", vec![shade]));
    }
    let path = dir.join(name);
    std::fs::write(&path, photocraft_io::export(&d, ext, &Default::default()).unwrap().bytes).unwrap();
    path
}

fn open(h: &mut Harness<'_, StudioShell>, path: &Path) -> Result<(), String> {
    h.state_mut().run_with("document.open", &serde_json::json!({"path": path}))
}

fn layers(h: &Harness<'_, StudioShell>) -> Vec<String> {
    h.state().document.as_ref().unwrap().pixel.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect()
}

/// The error modal's title, if one shows.
fn error_title(h: &Harness<'_, StudioShell>) -> Option<String> {
    h.state().error.as_ref().map(|(title, _)| title.clone())
}

#[test]
fn an_image_opens_straight_into_pixel_without_a_photo_and_develop_and_film_are_off() {
    let dir = temp_dir("image-open");
    let png = image(&dir, "flat.png");
    let original = std::fs::read(&png).unwrap();
    let mut h = studio();
    h.run_steps(3);
    open(&mut h, &png).unwrap();
    h.run_steps(3);
    let s = h.state();
    assert_eq!(s.persona, Persona::Pixel, "straight into Pixel");
    let doc = s.document.as_ref().unwrap();
    assert!(doc.source.is_none() && doc.base_layer().is_none() && doc.path.is_none());
    assert_eq!((doc.name.as_str(), s.base.started(), s.base_up_to_date()), ("flat", 0, true), "nothing renders");
    assert_eq!(s.error, None, "no warnings");
    // Develop and Film are off: their commands and keys say why, their buttons are greyed
    for id in ["persona.develop", "persona.film", "view.develop", "tool.crop"] {
        assert!(h.state_mut().run(id).unwrap_err().contains("library photo"), "{id}");
    }
    assert_eq!(h.state().persona, Persona::Pixel);
    for label in ["Develop", "Film"] {
        assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
    }
    assert!(!h.get_by_label("Library").accesskit_node().is_disabled(), "Library stays, to pick a photo");
    // edits and their undo, without a photo
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    assert_eq!(layers(&h).len(), 2);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert_eq!(layers(&h).len(), 1);
    press(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
    assert_eq!(layers(&h).len(), 2);
    // Ctrl+S: it has no file of its own, so the save dialog asks; the image is never written
    let picked = dir.join("flat.studio");
    let to = picked.clone();
    h.state_mut().pick_save_path = Some(Box::new(move |_| Some(to.clone())));
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::S);
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!((doc.path.as_deref(), doc.unsaved()), (Some(picked.as_path()), false));
    assert_eq!(std::fs::read(&png).unwrap(), original, "the PNG is untouched");
    // a save pointed at the image itself (or any non-.studio file) is refused
    let to = png.clone();
    h.state_mut().pick_save_path = Some(Box::new(move |_| Some(to.clone())));
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    assert!(h.state_mut().run("document.saveAs").unwrap_err().contains(".studio"));
    assert_eq!(std::fs::read(&png).unwrap(), original);
    h.state_mut().run("document.dismissError").unwrap();
    // its .studio opened again, from Library: still without a photo, in Pixel
    h.state_mut().run("persona.library").unwrap();
    h.state_mut().run_with("document.open", &serde_json::json!({"path": picked, "discard": true})).unwrap();
    assert_eq!(h.state().persona, Persona::Pixel);
    assert!(h.state().document.as_ref().unwrap().source.is_none());
    assert_eq!(layers(&h).len(), 2);
}

#[test]
fn a_psd_keeps_its_layers_and_what_photocraft_doesnt_model_is_said_at_once() {
    let dir = temp_dir("image-psd");
    let psd = image(&dir, "art.psd");
    let mut h = studio();
    h.run_steps(3);
    open(&mut h, &psd).unwrap();
    for name in ["Paint", "Group", "Shade"] {
        assert!(layers(&h).iter().any(|l| l == name), "{name}: {:?}", layers(&h));
    }
    let doc = &h.state().document.as_ref().unwrap().pixel;
    let shade = doc.walk().into_iter().map(|(_, _, l)| l).find(|l| l.name == "Shade").unwrap();
    assert_eq!(shade.blend, BlendMode::Multiply);
    assert!(shade.mask.is_some());
    // an unknown blend mode (patched in): the warning shows right after opening, before any edit
    let mut odd = std::fs::read(&psd).unwrap();
    let at = odd.windows(8).position(|w| w == b"8BIMmul ").unwrap();
    odd[at + 4..at + 8].copy_from_slice(b"zzzz");
    let odd_path = dir.join("odd.psd");
    std::fs::write(&odd_path, odd).unwrap();
    open(&mut h, &odd_path).unwrap();
    h.run_steps(2);
    assert_eq!(error_title(&h).as_deref(), Some("Opened \u{201c}odd.psd\u{201d} with warnings"));
    assert!(h.query_by_label_contains("blend mode").is_some(), "the modal says what");
    assert!(!h.state().document.as_ref().unwrap().unsaved());
    h.state_mut().run("document.dismissError").unwrap();
    // a file that isn't an image: an error, and the open document stays
    let bad = dir.join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    assert!(open(&mut h, &bad).is_err());
    assert_eq!(error_title(&h).as_deref(), Some("Couldn\u{2019}t open \u{201c}bad.png\u{201d}"));
    assert_eq!(h.state().document.as_ref().unwrap().name, "odd");
}

#[test]
fn opening_anything_with_unsaved_changes_asks_first() {
    let dir = temp_dir("image-prompt");
    let png = image(&dir, "flat.png");
    let mut h = studio();
    pixel_edited(&mut h, &dir.join("trip.studio"));
    let want = Some(Pending::Open(png.clone()));
    // the command
    assert!(open(&mut h, &png).unwrap_err().contains("unsaved"));
    assert_eq!(h.state().prompt, want);
    h.state_mut().run("document.prompt.cancel").unwrap();
    // Ctrl+O, through the open dialog
    let picked = png.clone();
    h.state_mut().pick_open_path = Some(Box::new(move || Some(picked.clone())));
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::O);
    assert_eq!(h.state().prompt, want);
    h.state_mut().run("document.prompt.cancel").unwrap();
    // a file dropped on Pixel
    h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(png.clone())));
    h.run_steps(2);
    assert_eq!(h.state().prompt, want);
    assert_eq!(h.state().pixel.as_ref().unwrap().app.session.documents().len(), 1, "PhotoCraft didn't open it itself");
    // Don't Save: it opens
    h.state_mut().run("document.prompt.discard").unwrap();
    assert!(h.state().document.as_ref().unwrap().source.is_none());
    assert_eq!(h.state().document.as_ref().unwrap().name, "flat");
    // a cancelled dialog does nothing
    h.state_mut().pick_open_path = Some(Box::new(|| None));
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::O);
    assert_eq!((h.state().prompt.as_ref(), h.state().document.as_ref().map(|d| d.name.as_str())), (None, Some("flat")));
}

#[test]
fn pixel_makes_a_document_from_the_picked_photo_and_replaces_one_of_another() {
    let dir = temp_dir("image-pick");
    let mut h = studio();
    h.run_steps(3);
    let photos: Vec<_> = h.state().lightroom.as_ref().unwrap().app.session.catalog.photos().map(|p| p.id).take(2).collect();
    let pick = |h: &mut Harness<'_, StudioShell>, i: usize| {
        h.state_mut().run("persona.library").unwrap();
        h.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = Some(photos[i]);
        h.state_mut().run("persona.pixel").unwrap();
    };
    let photo = |h: &Harness<'_, StudioShell>| h.state().document.as_ref().and_then(|d| d.photo());
    // no document: clicking Pixel makes one of the picked photo
    pick(&mut h, 0);
    assert_eq!(photo(&h), Some(photos[0]));
    settle_base(&mut h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": dir.join("a.studio")})).unwrap();
    // saved, another photo picked: replaced at once (nothing is lost, the file reopens)
    pick(&mut h, 1);
    assert_eq!((photo(&h), h.state().prompt.as_ref()), (Some(photos[1]), None));
    settle_base(&mut h);
    h.state_mut().run("pixel.layer.new.layer").unwrap();
    // unsaved: the prompt asks first
    pick(&mut h, 0);
    assert_eq!((photo(&h), h.state().prompt.as_ref()), (Some(photos[1]), Some(&Pending::New)));
    h.state_mut().run("document.prompt.discard").unwrap();
    assert_eq!(photo(&h), Some(photos[0]));
    // a document opened from an image file is of no photo: the one picked doesn't replace it
    let png = image(&dir, "flat.png");
    h.state_mut().run_with("document.open", &serde_json::json!({"path": png, "discard": true})).unwrap();
    pick(&mut h, 1);
    assert_eq!((h.state().document.as_ref().unwrap().name.as_str(), h.state().prompt.as_ref()), ("flat", None));
}

#[test]
fn a_file_saved_by_milestone_4_opens() {
    let dir = temp_dir("image-m4");
    let path = dir.join("m4.studio");
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../studio-doc/fixtures/m4-saved.studio"), &path).unwrap();
    let mut h = studio();
    h.run_steps(3);
    open(&mut h, &path).unwrap();
    settle_base(&mut h);
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!(layers(&h), [studio_doc::BASE_LAYER, "Over"]);
    let lr = &h.state().lightroom.as_ref().unwrap().app.session;
    assert_eq!(lr.develop_of(doc.photo().unwrap()).unwrap().light.exposure, 0.7);
    assert!(doc.base_layer().unwrap().surface().unwrap().tile_count() > 0, "its base renders");
}

/// Review: the document opened (or reopened at launch) is the photo picked in LightCraft, so a
/// trip to Library and back to Pixel keeps it rather than replacing it with whatever was picked
/// before (and Develop shows its photo).
#[test]
fn an_opened_document_becomes_the_picked_photo_so_pixel_keeps_it() {
    let dir = temp_dir("image-reopen");
    let path = dir.join("a.studio");
    let mut h = studio();
    h.run_steps(3);
    let photos: Vec<_> = h.state().lightroom.as_ref().unwrap().app.session.catalog.photos().map(|p| p.id).take(2).collect();
    let active = |h: &Harness<'_, StudioShell>| h.state().lightroom.as_ref().unwrap().app.session.selection.active;
    h.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = Some(photos[0]);
    h.state_mut().run("document.new").unwrap();
    settle_base(&mut h);
    h.state_mut().run_with("document.saveAs", &serde_json::json!({"path": path})).unwrap();
    h.state_mut().run("document.close").unwrap();
    // another photo picked, then the file opened from Pixel
    h.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = Some(photos[1]);
    h.state_mut().run("persona.pixel").unwrap();
    h.state_mut().run_with("document.open", &serde_json::json!({"path": path, "discard": true})).unwrap();
    assert_eq!(active(&h), Some(photos[0]), "its photo is the one picked");
    h.state_mut().run("persona.library").unwrap();
    h.state_mut().run("persona.pixel").unwrap();
    let doc = h.state().document.as_ref().unwrap();
    assert_eq!((doc.photo(), doc.path.as_deref(), h.state().prompt.as_ref()), (Some(photos[0]), Some(path.as_path()), None));
}
