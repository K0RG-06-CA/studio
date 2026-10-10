//! The `.studio` file: one STORE-mode zip (PhotoCraft's own zip code, so no extra dependency)
//! holding `document.json` (name, source photo, develop and film settings) and `pixel.pcraft`
//! (the layer tree, a plain PhotoCraft bundle). The base layer's pixels are not stored: they are
//! derived from the source photo and the settings, and rendered again when the file is opened.

use std::path::{Path, PathBuf};

use lightcraft_engine::catalog::model::Source as PhotoSource;
use photocraft_color::PixelFormat;
use photocraft_doc::LayerContent;
use photocraft_format::zip::{ZipReader, ZipWriter};
use photocraft_raster::Surface;

use crate::{Document, Header, VERSION};

pub const EXTENSION: &str = "studio";
const HEADER_ENTRY: &str = "document.json";
const PIXEL_ENTRY: &str = "pixel.pcraft";
/// Entry size limits when reading: a header is small; a layer tree is whatever fits in a zip.
const MAX_HEADER: usize = 64 << 20;
const MAX_PIXEL: usize = u32::MAX as usize;

impl Document {
    /// The file's bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let header = serde_json::to_vec_pretty(&self.header()).map_err(|e| e.to_string())?;
        // the base is derived: its tiles stay out of the file
        let mut pixel = self.pixel.clone();
        if let Some(base) = pixel.layers.iter_mut().find(|l| crate::is_base(l)) {
            base.content = LayerContent::Raster(Surface::new(PixelFormat::RGBA16));
        }
        let pixel = photocraft_format::save_to_bytes(&pixel, &photocraft_format::SaveOptions::default()).map_err(|e| e.to_string())?;
        let mut zip = ZipWriter::new();
        zip.add(HEADER_ENTRY, &header).map_err(|e| e.to_string())?;
        zip.add(PIXEL_ENTRY, &pixel).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())
    }

    /// A document from a file's bytes. Its base layer is empty until the first render.
    pub fn from_bytes(bytes: &[u8]) -> Result<Document, String> {
        let zip = ZipReader::new(bytes).map_err(|e| format!("not a .studio file: {e}"))?;
        let header = zip.read_by_name(HEADER_ENTRY, MAX_HEADER).map_err(|e| format!("not a .studio file: {e}"))?;
        let header: Header = serde_json::from_slice(&header).map_err(|e| format!("{HEADER_ENTRY}: {e}"))?;
        if header.version > VERSION {
            return Err(format!("saved by a newer Studio (format {}, this one reads up to {VERSION})", header.version));
        }
        let pixel = zip.read_by_name(PIXEL_ENTRY, MAX_PIXEL).map_err(|e| format!("not a .studio file: {e}"))?;
        let pixel = photocraft_format::load_from_bytes(&pixel).map_err(|e| format!("{PIXEL_ENTRY}: {e}"))?;
        // a document of a library photo has its base layer; one opened from an image file has none
        match (&header.source, pixel.layers.iter().any(crate::is_base)) {
            (Some(_), false) => return Err("the layer tree has no Develop base layer".into()),
            (None, true) => return Err("the layer tree has a Develop base layer but the file names no source photo".into()),
            _ => {}
        }
        Ok(Document {
            name: header.name,
            source: header.source,
            develop: header.develop,
            film: header.film,
            pixel,
            path: None,
            revision: 0,
            saved_revision: 0,
            link: Default::default(),
            history: Default::default(),
            generation: 0,
        })
    }

    pub fn open(path: &Path) -> Result<Document, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut doc = Document::from_bytes(&bytes)?;
        doc.path = Some(path.to_path_buf());
        Ok(doc)
    }

    /// Write the file (atomically) and remember where.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        let bytes = self.to_bytes()?;
        photocraft_format::atomic_write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.path = Some(path.to_path_buf());
        self.saved_revision = self.revision;
        Ok(())
    }

    /// Where the document is saved by default: beside its source file, as `<file>.studio` (like
    /// Lightroom's `-Edit` files, so the photo's document is found without an index; the photo's
    /// extension stays, so a RAW+JPEG pair gets two documents). None for a demo photo, which has
    /// no file, and for a document opened from an image file: it is never saved over that file
    /// (or beside it unasked), so its first save asks where.
    pub fn default_path(&self) -> Option<PathBuf> {
        match &self.source.as_ref()?.source {
            PhotoSource::File { path } => {
                let path = Path::new(path);
                let name = path.file_name()?.to_string_lossy();
                Some(path.with_file_name(format!("{name}.{EXTENSION}")))
            }
            PhotoSource::Demo { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Base;
    use crate::tests::demo;
    use crate::{BASE_LAYER, Film};
    use lightcraft_engine::Session;
    use photocraft_doc::Layer;
    use photocraft_geom::Rect;

    /// A rendered document with a painted layer over the base, and its session.
    fn edited() -> (Session, Document) {
        lightcraft_engine::gpu::set_enabled(false);
        let (mut session, id) = demo();
        let mut doc = Document::from_photo(&session, id).unwrap();
        let (mut base, mut pixel) = (Base::new(), photocraft_engine::Session::new());
        while !base.up_to_date(&doc) {
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut over = Layer::raster("Over", PixelFormat::RGBA16);
        over.surface_mut().unwrap().write_region(Rect::from_xywh(10, 10, 2, 1), &[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.5]);
        doc.pixel.layers.push(over);
        doc.develop.light.exposure = 0.7;
        doc.film = Film::default();
        doc.revision += 1;
        (session, doc)
    }

    fn pixels(doc: &Document, layer: &str) -> Vec<[f32; 4]> {
        let l = doc.pixel.layers.iter().find(|l| l.name == layer).unwrap();
        let mut out = vec![[0.0; 4]; 2];
        l.surface().unwrap().read_rgba_into(Rect::from_xywh(10, 10, 2, 1), &mut out);
        out
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("studio-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn save_and_open_keep_everything_but_the_base_pixels() {
        let (_session, mut doc) = edited();
        assert!(doc.unsaved());
        let path = temp("trip.studio");
        doc.save(&path).unwrap();
        assert_eq!(doc.path.as_deref(), Some(path.as_path()));
        assert!(!doc.unsaved());
        let bytes = std::fs::read(&path).unwrap();
        // two entries; the pixel one is a plain PhotoCraft bundle without the base's tiles
        let zip = ZipReader::new(&bytes).unwrap();
        let mut names: Vec<_> = zip.entries.iter().map(|e| e.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["document.json", "pixel.pcraft"]);
        let pixel = photocraft_format::load_from_bytes(&zip.read_by_name("pixel.pcraft", MAX_PIXEL).unwrap()).unwrap();
        assert_eq!(pixel.layers.iter().find(|l| l.name == BASE_LAYER).unwrap().surface().unwrap().tile_count(), 0);
        assert!(bytes.len() < 64 << 10, "{} bytes: the base's pixels must not be in the file", bytes.len());
        let text = String::from_utf8(zip.read_by_name("document.json", MAX_HEADER).unwrap()).unwrap();
        assert!(text.contains("\"version\": 1") && text.contains("\"film\": {}") && text.contains("\"exposure\": 0.7"), "{text}");

        let back = Document::open(&path).unwrap();
        assert_eq!((back.name.as_str(), &back.source, back.develop.light.exposure, &back.film), (doc.name.as_str(), &doc.source, 0.7, &doc.film));
        assert_eq!(back.develop, doc.develop);
        assert_eq!((back.pixel.size, back.pixel.depth), (doc.pixel.size, doc.pixel.depth));
        assert_eq!(back.pixel.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), [BASE_LAYER, "Over"]);
        assert!(back.base_layer().unwrap().locks.all);
        assert_eq!(back.base_layer().unwrap().surface().unwrap().tile_count(), 0, "re-rendered on open, not stored");
        assert_eq!(pixels(&back, "Over"), pixels(&doc, "Over"));
        assert_eq!((back.path.as_deref(), back.revision, back.unsaved()), (Some(path.as_path()), 0, false));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bad_files_are_errors_not_crashes() {
        let (_session, doc) = edited();
        let bytes = doc.to_bytes().unwrap();
        for (what, bad) in [
            ("garbage", b"garbage".to_vec()),
            ("empty", Vec::new()),
            ("truncated", bytes[..bytes.len() / 2].to_vec()),
            ("one byte short", bytes[..bytes.len() - 1].to_vec()),
        ] {
            let e = Document::from_bytes(&bad).err().unwrap_or_else(|| panic!("{what} loaded"));
            assert!(e.contains("not a .studio file"), "{what}: {e}");
        }
        // a flipped byte inside document.json (after the 30-byte local header and its name)
        let mut flipped = bytes.clone();
        flipped[30 + HEADER_ENTRY.len() + 8] ^= 0xff;
        let e = Document::from_bytes(&flipped).err().unwrap();
        assert!(e.contains("CRC"), "{e}");
        // a header from the future, one without a photo, and a layer tree without the base
        let rebuilt = |header: &str, pixel: Option<&[u8]>| {
            let zip = ZipReader::new(&bytes).unwrap();
            let pcraft = zip.read_by_name(PIXEL_ENTRY, MAX_PIXEL).unwrap();
            let mut w = ZipWriter::new();
            w.add(HEADER_ENTRY, header.as_bytes()).unwrap();
            w.add(PIXEL_ENTRY, pixel.unwrap_or(&pcraft)).unwrap();
            Document::from_bytes(&w.finish().unwrap()).err().unwrap()
        };
        let header = serde_json::to_string(&doc.header()).unwrap();
        assert!(rebuilt(&header.replace("\"version\":1", "\"version\":99"), None).contains("newer Studio"));
        assert!(rebuilt(r#"{"version":1,"name":"x"}"#, None).contains("no source photo"));
        assert!(rebuilt("{", None).contains("document.json"));
        let plain = photocraft_doc::Document::new("plain", doc.pixel.size, doc.pixel.mode, doc.pixel.depth);
        let plain = photocraft_format::save_to_bytes(&plain, &photocraft_format::SaveOptions::default()).unwrap();
        assert!(rebuilt(&header, Some(&plain)).contains("no Develop base layer"));
        assert!(rebuilt(&header, Some(b"nope")).contains("pixel.pcraft"));
        assert!(Document::open(Path::new("Z:/nowhere/x.studio")).err().unwrap().contains("nowhere"));
    }

    #[test]
    fn the_default_path_is_beside_the_source_file() {
        let (_session, mut doc) = edited();
        assert_eq!(doc.default_path(), None, "demo photos have no file");
        doc.source.as_mut().unwrap().source = PhotoSource::File { path: "C:/Photos/trip.rw2".into() };
        assert_eq!(doc.default_path(), Some(PathBuf::from("C:/Photos/trip.rw2.studio")));
        // the JPEG of a RAW+JPEG pair gets its own document
        doc.source.as_mut().unwrap().source = PhotoSource::File { path: "C:/Photos/trip.jpg".into() };
        assert_eq!(doc.default_path(), Some(PathBuf::from("C:/Photos/trip.jpg.studio")));
    }

    /// A file saved by milestone 4's Studio (`fixtures/m4-saved.studio`: written at `fc42139` by
    /// its own `edited()` and `save`, a demo photo at exposure 0.7 with a painted "Over" layer)
    /// still opens, finds its photo and keeps its layers.
    #[test]
    fn a_file_saved_by_milestone_4_still_opens() {
        let doc = Document::from_bytes(include_bytes!("../fixtures/m4-saved.studio")).unwrap();
        let source = doc.source.clone().expect("a library photo's document");
        assert!(matches!(source.source, PhotoSource::Demo { .. }), "{source:?}");
        assert_eq!(doc.develop.light.exposure, 0.7);
        assert_eq!(doc.pixel.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), [BASE_LAYER, "Over"]);
        assert!(doc.base_layer().unwrap().locks.all);
        let over = pixels(&doc, "Over");
        assert_eq!(over[0], [1.0, 0.0, 0.0, 1.0]);
        assert!((over[1][3] - 0.5).abs() < 1e-3 && over[1][1] == 1.0, "{over:?}");
        let (mut session, _) = demo();
        let mut doc = doc;
        doc.open_into(&mut session).unwrap();
        assert_eq!(session.develop_of(doc.photo().unwrap()).unwrap().light.exposure, 0.7);
    }

    /// An image file as PhotoCraft saves it: a PSD with a painted layer and a group holding a
    /// masked Multiply layer, or a flat PNG at `depth`.
    pub(crate) fn image(ext: &str, depth: photocraft_color::SampleType) -> Vec<u8> {
        let size = photocraft_geom::Size { width: 8, height: 6 };
        let mut d = photocraft_doc::Document::new("art", size, photocraft_color::ColorMode::Rgb, depth);
        let format = if depth == photocraft_color::SampleType::U16 { PixelFormat::RGBA16 } else { PixelFormat::RGBA8 };
        let mut paint = Layer::raster("Paint", format);
        paint.surface_mut().unwrap().write_region(Rect::from_xywh(1, 1, 2, 1), &[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
        d.layers.push(paint);
        if ext == "psd" {
            let mut shade = Layer::raster("Shade", format);
            shade.surface_mut().unwrap().write_region(Rect::from_xywh(0, 0, 1, 1), &[0.5, 0.5, 0.5, 1.0]);
            shade.blend = photocraft_doc::BlendMode::Multiply;
            shade.mask = Some(photocraft_doc::LayerMask::hide_all());
            d.layers.push(Layer::group("Group", vec![shade]));
        }
        photocraft_io::export(&d, ext, &Default::default()).unwrap().bytes
    }

    #[test]
    fn an_image_file_opens_as_a_document_without_a_photo() {
        use photocraft_color::SampleType;
        // flat PNGs, 8 and 16 bit: one layer, the depth kept
        for depth in [SampleType::U8, SampleType::U16] {
            let (doc, warnings) = Document::from_image("C:/Art/flat.png", &image("png", depth)).unwrap();
            assert_eq!((doc.name.as_str(), doc.source.as_ref(), doc.photo(), doc.path.as_ref()), ("flat", None, None, None));
            assert_eq!((doc.pixel.size.width, doc.pixel.size.height, doc.pixel.depth), (8, 6, depth), "{depth:?}");
            assert_eq!(doc.pixel.layers.len(), 1);
            assert!(doc.base_layer().is_none() && warnings.is_empty() && !doc.unsaved(), "{warnings:?}");
            assert_eq!(doc.default_path(), None, "never saved over the image, nor beside it unasked");
        }
        // a PSD: its group, mask and blend mode
        let (doc, warnings) = Document::from_image("art.psd", &image("psd", SampleType::U8)).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let names: Vec<_> = doc.pixel.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
        assert!(names.contains(&"Paint".to_owned()) && names.contains(&"Group".to_owned()), "{names:?}");
        let shade = doc.pixel.walk().into_iter().map(|(_, _, l)| l).find(|l| l.name == "Shade").unwrap().clone();
        assert_eq!(shade.blend, photocraft_doc::BlendMode::Multiply);
        assert!(shade.mask.is_some());
        // something PhotoCraft doesn't model is said (an unknown blend mode, patched in)
        let mut odd = image("psd", SampleType::U8);
        let at = odd.windows(8).position(|w| w == b"8BIMmul ").unwrap();
        odd[at + 4..at + 8].copy_from_slice(b"zzzz");
        let (_, warnings) = Document::from_image("odd.psd", &odd).unwrap();
        assert!(warnings.iter().any(|w| w.contains("blend mode")), "{warnings:?}");
        // a file that isn't an image is an error
        assert!(Document::from_image("bad.png", b"not a png").is_err());
        // nothing to render: the base is up to date, no render starts, no Develop step
        let (mut session, _) = demo();
        let (mut doc, _) = Document::from_image("art.psd", &image("psd", SampleType::U8)).unwrap();
        let (mut base, mut pixel) = (Base::new(), photocraft_engine::Session::new());
        doc.attach(&mut pixel);
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert!(base.up_to_date(&doc) && !base.rendering());
        assert_eq!(base.started(), 0);
        doc.open_into(&mut session).unwrap();
        // saved as a .studio and opened again: still without a photo, its layers kept
        let path = temp("art.studio");
        doc.save(&path).unwrap();
        let back = Document::open(&path).unwrap();
        assert_eq!((back.source.as_ref(), back.pixel.walk().len()), (None, doc.pixel.walk().len()));
        let _ = std::fs::remove_file(path);
    }
}
