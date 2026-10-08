//! Studio's shared document: the one file every persona edits (PRD › Architecture). LightCraft's
//! develop settings are the recipe of a live base layer, and PhotoCraft's layer tree holds
//! everything above it. Both engines' types are used as they are; nothing upstream is patched.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod base;
pub mod file;

use std::path::PathBuf;
use std::sync::Arc;

use lightcraft_engine::Session;
use lightcraft_engine::catalog::model::{PhotoId, Source as PhotoSource};
use lightcraft_engine::develop::DevelopSettings;
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::Layer;
use photocraft_geom::Size;
use serde::{Deserialize, Serialize};

/// Version of `document.json`; a file from a newer Studio is refused rather than misread.
pub const VERSION: u32 = 1;

/// The library photo a document is made from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// The photo's id in the library the document was made in (ids are per library).
    pub photo: PhotoId,
    /// Hash of the file's bytes, LightCraft's: finds the same photo in any library. Demo scenes
    /// have none.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Where its pixels come from: a file on disk, or one of LightCraft's demo scenes.
    pub source: PhotoSource,
}

/// Settings of the Film persona. Reserved: nothing is built yet, so every document carries an
/// empty `film` and future fields read as their defaults. They follow `PRD-halation.md`'s
/// pipeline order: chromatic-aberration correction, highlight boost, Lazy Blur, emulsion scatter,
/// halation, Saturation RGB (with the profile and tone curve), Lazy Bleach, grain.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Film {}

/// The document. `develop` and `film` are the base layer's recipe; `pixel` is PhotoCraft's layer
/// tree, whose bottom layer is the base. `revision` counts every change (one undo history later
/// snapshots by it, as PhotoCraft's does), `saved_revision` the one on disk.
pub struct Document {
    pub name: String,
    pub source: Source,
    pub develop: DevelopSettings,
    pub film: Film,
    pub pixel: photocraft_doc::Document,
    /// Where it was last saved or opened from.
    pub path: Option<PathBuf>,
    pub revision: u64,
    pub saved_revision: u64,
}

/// `document.json`: everything but the layer tree (which is a `.pcraft` bundle next to it).
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Header {
    pub version: u32,
    pub name: String,
    pub source: Option<Source>,
    pub develop: DevelopSettings,
    pub film: Film,
}

impl Default for Header {
    fn default() -> Self {
        Header { version: VERSION, name: String::new(), source: None, develop: DevelopSettings::default(), film: Film::default() }
    }
}

/// Name of the base layer, as the Layers panel shows it.
pub const BASE_LAYER: &str = "Develop";
/// Marks the base layer: an opaque block PhotoCraft keeps through `.pcraft` and PSD. The base is
/// found by it, not by its position or id (which a load regenerates).
const BASE_MARK: [u8; 4] = *b"stdv";

impl Document {
    /// A new document from the library photo `id`: its develop settings as they are now, and a
    /// 16-bit RGB layer tree holding only the base layer (empty until the first render).
    pub fn from_photo(session: &Session, id: PhotoId) -> Result<Document, String> {
        let photo = session.catalog.photo(id).ok_or_else(|| format!("no photo {}", id.0))?;
        let develop = session.develop_of(id).ok_or_else(|| format!("no photo {}", id.0))?;
        let name = photo.file_name.rsplit_once('.').map_or(photo.file_name.as_str(), |(stem, _)| stem).to_owned();
        let size = Size { width: photo.width.max(1), height: photo.height.max(1) };
        let mut pixel = photocraft_doc::Document::new(name.clone(), size, ColorMode::Rgb, SampleType::U16);
        let mut base = Layer::raster(BASE_LAYER, PixelFormat::RGBA16);
        base.locks.all = true;
        base.psd_blocks.push((BASE_MARK, Arc::new(Vec::new())));
        pixel.layers.push(base);
        Ok(Document {
            name,
            source: Source { photo: id, content_hash: photo.content_hash.clone(), source: photo.source.clone() },
            develop: (*develop).clone(),
            film: Film::default(),
            pixel,
            path: None,
            revision: 0,
            saved_revision: 0,
        })
    }

    /// The base layer: the photo as Develop renders it, at the bottom of the tree.
    pub fn base_layer(&self) -> Option<&Layer> {
        self.pixel.layers.iter().find(|l| is_base(l))
    }

    pub fn base_layer_mut(&mut self) -> Option<&mut Layer> {
        self.pixel.layers.iter_mut().find(|l| is_base(l))
    }

    pub fn unsaved(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub fn header(&self) -> Header {
        Header {
            version: VERSION,
            name: self.name.clone(),
            source: Some(self.source.clone()),
            develop: self.develop.clone(),
            film: self.film.clone(),
        }
    }
}

pub(crate) fn is_base(l: &Layer) -> bool {
    l.psd_blocks.iter().any(|(key, _)| *key == BASE_MARK)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LightCraft's in-memory demo library, and its first photo.
    pub(crate) fn demo() -> (Session, PhotoId) {
        let session = Session::with_demo();
        let first = session.catalog.photos().next().map(|p| p.id).unwrap();
        (session, first)
    }

    #[test]
    fn from_photo_puts_a_locked_marked_16_bit_base_layer_at_the_bottom() {
        let (session, id) = demo();
        let doc = Document::from_photo(&session, id).unwrap();
        let photo = session.catalog.photo(id).unwrap();
        assert_eq!((doc.source.photo, &doc.source.source), (id, &photo.source));
        assert_eq!(doc.develop, *session.develop_of(id).unwrap());
        assert_eq!((doc.pixel.mode, doc.pixel.depth), (ColorMode::Rgb, SampleType::U16));
        assert_eq!((doc.pixel.size.width, doc.pixel.size.height), (photo.width, photo.height));
        assert_eq!(doc.pixel.layers.len(), 1);
        let base = doc.base_layer().unwrap();
        assert!(std::ptr::eq(base, &doc.pixel.layers[0]));
        assert_eq!(base.name, BASE_LAYER);
        assert!(base.locks.all);
        assert_eq!(base.surface().map(|s| s.format()), Some(PixelFormat::RGBA16));
        assert!(!doc.name.contains('.'), "{}", doc.name);
        assert!(!doc.unsaved());
        assert!(Document::from_photo(&session, PhotoId(u64::MAX)).err().unwrap().contains("no photo"));
    }

    #[test]
    fn the_base_is_found_by_its_mark_wherever_it_sits() {
        let (session, id) = demo();
        let mut doc = Document::from_photo(&session, id).unwrap();
        doc.pixel.layers.insert(0, Layer::raster("Under", PixelFormat::RGBA16));
        doc.pixel.layers.push(Layer::raster("Over", PixelFormat::RGBA16));
        assert_eq!(doc.base_layer().map(|l| l.name.as_str()), Some(BASE_LAYER));
        doc.base_layer_mut().unwrap().name = "Renamed".into();
        assert_eq!(doc.base_layer().map(|l| l.name.as_str()), Some("Renamed"));
        doc.pixel.layers.retain(|l| !is_base(l));
        assert!(doc.base_layer().is_none());
    }

    #[test]
    fn header_round_trips_and_reads_tolerantly() {
        let mut develop = DevelopSettings::default();
        develop.light.exposure = 1.5;
        let h = Header {
            version: VERSION,
            name: "trip".into(),
            source: Some(Source { photo: PhotoId(7), content_hash: Some("abc".into()), source: PhotoSource::File { path: "C:/p/trip.rw2".into() } }),
            develop,
            film: Film::default(),
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"film\":{}"), "{json}");
        let back: Header = serde_json::from_str(&json).unwrap();
        assert_eq!((back.version, back.name.as_str(), back.develop.light.exposure), (VERSION, "trip", 1.5));
        assert_eq!(back.source, h.source);
        // unknown keys (a future field) are ignored, missing ones take defaults
        let back: Header = serde_json::from_str(r#"{"version":1,"name":"x","future":{"a":1},"film":{"halation":{"amount":1}}}"#).unwrap();
        assert_eq!((back.name.as_str(), back.source, back.film), ("x", None, Film::default()));
        assert!(serde_json::from_str::<Header>("{").is_err());
    }
}
