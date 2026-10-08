//! Opening a document into a LightCraft session: the photo it was made from is found in the
//! library (or imported), and the file's develop settings become the photo's, as a History step.
//! From then on the catalog is the settings' home and the document mirrors it ([`crate::base`]).

use lightcraft_engine::Session;
use lightcraft_engine::catalog::model::{PhotoId, Source as PhotoSource};

use crate::Document;
use crate::file::EXTENSION;

impl Document {
    /// Connect the document to `session`'s library. The photo is found by content hash, then by
    /// its path or demo scene (an id alone means nothing in another library), else its file is
    /// imported. If the library's settings differ from the file's, the file wins: they are applied
    /// as the History step "Open <name>.studio", so Develop shows and can undo it.
    pub fn open_into(&mut self, session: &mut Session) -> Result<(), String> {
        let id = match self.find(session) {
            Some(id) => id,
            None => self.import(session)?,
        };
        self.source.photo = id;
        if let Some(p) = session.catalog.photo(id)
            && self.source.content_hash.is_none()
        {
            self.source.content_hash = p.content_hash.clone();
        }
        if session.develop_of(id).is_none_or(|now| *now != self.develop) {
            session.set_develop(id, self.develop.clone(), &format!("Open {}.{EXTENSION}", self.name)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn find(&self, session: &Session) -> Option<PhotoId> {
        let photos = || session.catalog.photos().filter(|p| !p.deleted);
        let by_hash = self.source.content_hash.as_ref().and_then(|h| photos().find(|p| p.content_hash.as_ref() == Some(h)));
        by_hash.or_else(|| photos().find(|p| p.source == self.source.source)).map(|p| p.id)
    }

    fn import(&self, session: &mut Session) -> Result<PhotoId, String> {
        let path = match &self.source.source {
            PhotoSource::File { path } => path,
            PhotoSource::Demo { scene } => return Err(format!("demo photo {scene} isn't in this library")),
        };
        let r = session.execute("library.import", &serde_json::json!({"paths": [path]})).map_err(|e| format!("importing {path}: {e}"))?;
        r["imported"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(serde_json::Value::as_u64)
            .map(PhotoId)
            .ok_or_else(|| format!("{path} couldn't be imported: {r}"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::tests::demo;

    fn steps(session: &Session, id: PhotoId) -> Vec<String> {
        session.catalog.photo(id).unwrap().history.iter().map(|h| h.label.clone()).collect()
    }

    #[test]
    fn the_files_settings_win_as_a_history_step_and_only_when_they_differ() {
        let (mut session, id) = demo();
        let mut doc = Document::from_photo(&session, id).unwrap();
        doc.develop.light.exposure = 0.9;
        doc.open_into(&mut session).unwrap();
        assert_eq!(doc.source.photo, id);
        assert_eq!(session.develop_of(id).unwrap().light.exposure, 0.9);
        let after = steps(&session, id);
        assert_eq!(after.last().map(String::as_str), Some(format!("Open {}.studio", doc.name).as_str()), "{after:?}");
        doc.open_into(&mut session).unwrap();
        assert_eq!(steps(&session, id), after, "same settings: no new step");
        // a wrong id doesn't matter: the photo is found by what it is
        doc.source.photo = PhotoId(u64::MAX);
        doc.open_into(&mut session).unwrap();
        assert_eq!(doc.source.photo, id);
    }

    #[test]
    fn a_demo_photo_missing_from_the_library_is_an_error() {
        let (session, id) = demo();
        let mut doc = Document::from_photo(&session, id).unwrap();
        let mut empty = Session::new();
        let e = doc.open_into(&mut empty).unwrap_err();
        assert!(e.contains("demo photo") && e.contains("isn't in this library"), "{e}");
    }

    /// A 1×1 PNG.
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01,
        0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn library(dir: &PathBuf) -> Session {
        let mut s = Session::new().with_fs().with_system_clock();
        s.open_library(dir, false).unwrap();
        s
    }

    #[test]
    fn a_file_photo_is_found_by_hash_in_one_library_and_imported_into_another() {
        let dir = std::env::temp_dir().join(format!("studio-doc-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("shot.png");
        std::fs::write(&png, PNG).unwrap();
        let mut a = library(&dir.join("a"));
        let r = a.execute("library.import", &serde_json::json!({"paths": [png.to_string_lossy()]})).unwrap();
        let id_a = PhotoId(r["imported"][0].as_u64().unwrap());
        let mut doc = Document::from_photo(&a, id_a).unwrap();
        assert!(doc.source.content_hash.is_some());
        doc.develop.light.exposure = -1.0;
        let file = dir.join("shot.studio");
        doc.save(&file).unwrap();
        // the same library: found by hash, nothing imported twice
        let mut doc = Document::open(&file).unwrap();
        doc.open_into(&mut a).unwrap();
        assert_eq!((doc.source.photo, a.catalog.len()), (id_a, 1));
        assert_eq!(a.develop_of(id_a).unwrap().light.exposure, -1.0);
        a.close_library().unwrap();
        // another, empty library: the photo's file is imported and gets the file's settings
        let mut b = library(&dir.join("b"));
        let mut doc = Document::open(&file).unwrap();
        doc.open_into(&mut b).unwrap();
        let id_b = b.catalog.photos().next().unwrap().id;
        assert_eq!((doc.source.photo, b.catalog.len()), (id_b, 1));
        assert_eq!(b.develop_of(id_b).unwrap().light.exposure, -1.0);
        assert_eq!(steps(&b, id_b).last().map(String::as_str), Some("Open shot.studio"));
        // the file gone: a clear error
        std::fs::remove_file(&png).unwrap();
        let mut c = library(&dir.join("c"));
        let e = doc.open_into(&mut c).unwrap_err();
        assert!(e.contains("shot.png"), "{e}");
        b.close_library().unwrap();
        c.close_library().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
