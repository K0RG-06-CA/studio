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
        let source = header.source.ok_or("the file names no source photo")?;
        let pixel = zip.read_by_name(PIXEL_ENTRY, MAX_PIXEL).map_err(|e| format!("not a .studio file: {e}"))?;
        let pixel = photocraft_format::load_from_bytes(&pixel).map_err(|e| format!("{PIXEL_ENTRY}: {e}"))?;
        if !pixel.layers.iter().any(crate::is_base) {
            return Err("the layer tree has no Develop base layer".into());
        }
        Ok(Document { name: header.name, source, develop: header.develop, film: header.film, pixel, path: None, revision: 0, saved_revision: 0 })
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
    /// no file.
    pub fn default_path(&self) -> Option<PathBuf> {
        match &self.source.source {
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
        let mut base = Base::new();
        while !base.up_to_date(&doc) {
            base.sync(&mut session, &mut doc).unwrap();
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
        doc.source.source = PhotoSource::File { path: "C:/Photos/trip.rw2".into() };
        assert_eq!(doc.default_path(), Some(PathBuf::from("C:/Photos/trip.rw2.studio")));
        // the JPEG of a RAW+JPEG pair gets its own document
        doc.source.source = PhotoSource::File { path: "C:/Photos/trip.jpg".into() };
        assert_eq!(doc.default_path(), Some(PathBuf::from("C:/Photos/trip.jpg.studio")));
    }
}
