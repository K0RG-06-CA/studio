//! Studio's shared document: the one file every persona edits (PRD › Architecture). LightCraft's
//! develop settings are the recipe of a live base layer, and PhotoCraft's layer tree holds
//! everything above it. Both engines' types are used as they are; nothing upstream is patched.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::PathBuf;

use lightcraft_engine::catalog::model::{PhotoId, Source as PhotoSource};
use lightcraft_engine::develop::DevelopSettings;
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

impl Document {
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

#[cfg(test)]
mod tests {
    use super::*;

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
