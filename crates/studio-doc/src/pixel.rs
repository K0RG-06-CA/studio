//! The pixel half of an open document lives in PhotoCraft's session: the Pixel persona edits it
//! there, with PhotoCraft's own undo, and the base layer's renders are written there too. That
//! session is the one place anything writes; [`Document::pixel`] is the copy Studio saves and
//! flattens, refreshed by [`Document::copy_back`] (an edit) and by [`Document::write_base`] (a
//! render, not an edit).

use std::sync::Arc;

use photocraft_doc::{DocId, LayerContent};
use photocraft_engine::{DocState, Session};
use photocraft_geom::Size;
use photocraft_raster::Surface;

use crate::Document;

/// Where an open document's pixels are in PhotoCraft's session.
#[derive(Clone, Debug, Default)]
pub struct Link {
    /// The document's id in the session; none until attached.
    id: Option<DocId>,
    /// PhotoCraft's document as last copied into `Document::pixel` (pointer compare: every edit
    /// there makes a new `Arc`).
    copied: Option<Arc<photocraft_doc::Document>>,
}

impl Document {
    /// Make `session` the home of this document's pixels: whatever it held is closed (it only
    /// ever holds the open document), and this one goes in as last copied.
    pub fn attach(&mut self, session: &mut Session) {
        close_all(session);
        let i = session.add_document(self.pixel.clone(), None);
        let st = session.documents().get(i);
        self.link = Link { id: st.map(|s| s.doc.id), copied: st.map(|s| s.doc.clone()) };
    }

    /// PhotoCraft holds this document (as attached; it may have closed it since).
    pub fn attached(&self, session: &Session) -> bool {
        self.link.id.is_some_and(|id| session.documents().iter().any(|s| s.doc.id == id))
    }

    /// Copy what PhotoCraft changed into `pixel`. Any new document there is an edit (a stroke, a
    /// layer, an undo), so `revision` moves; selecting a layer isn't one. PhotoCraft's own
    /// "unsaved" is kept clear, since saving is Studio's: its close guard and autosave stay quiet.
    /// A document PhotoCraft no longer holds is attached again, as last copied.
    pub fn copy_back(&mut self, session: &mut Session) {
        let Some(st) = self.state(session) else {
            self.attach(session);
            return;
        };
        st.saved_revision = st.revision;
        if self.link.copied.as_ref().is_some_and(|c| Arc::ptr_eq(c, &st.doc)) {
            return;
        }
        let now = st.doc.clone();
        self.pixel = (*now).clone();
        self.link.copied = Some(now);
        self.revision += 1;
    }

    /// Put a render of the base layer (and the document size it brings) into PhotoCraft: no
    /// history step, and not an edit. Its canvas redraws all of it. What PhotoCraft holds,
    /// including edits not yet copied back, is kept, and copied into `pixel` with the render.
    pub(crate) fn write_base(&mut self, session: &mut Session, surface: Surface, size: Size) -> Result<(), String> {
        if !self.attached(session) {
            self.attach(session);
        }
        // an edit made there since the last copy is copied as one, before the render goes in
        self.copy_back(session);
        let st = self.state(session).ok_or("PhotoCraft lost the document")?;
        let mut doc = (*st.doc).clone();
        let base = doc.layers.iter_mut().find(|l| crate::is_base(l)).ok_or("the document has no base layer")?;
        base.content = LayerContent::Raster(surface);
        doc.size = size;
        let doc = Arc::new(doc);
        st.doc = doc.clone();
        st.revision += 1;
        st.saved_revision = st.revision;
        st.last_damage = None;
        self.pixel = (*doc).clone();
        self.link.copied = Some(doc);
        Ok(())
    }

    /// The document's state in PhotoCraft (made the active one: the session only has this one).
    fn state<'s>(&self, session: &'s mut Session) -> Option<&'s mut DocState> {
        let id = self.link.id?;
        let i = session.documents().iter().position(|s| s.doc.id == id)?;
        session.set_active(i);
        session.active_mut()
    }
}

/// Close every document PhotoCraft holds (the open document's pixels go with it: `pixel` keeps
/// the last copy).
pub fn close_all(session: &mut Session) {
    while session.close(0).is_some() {}
}

#[cfg(test)]
mod tests {
    use photocraft_color::PixelFormat;
    use photocraft_doc::Layer;

    use super::*;
    use crate::tests::demo;

    fn attached() -> (Session, Document) {
        let (lc, id) = demo();
        let mut doc = Document::from_photo(&lc, id).unwrap();
        let mut session = Session::new();
        doc.attach(&mut session);
        (session, doc)
    }

    /// A Pixel edit in PhotoCraft: a new layer on top.
    fn add_layer(session: &mut Session, name: &str) {
        session
            .edit(name, |d, active| {
                let l = Layer::raster(name, PixelFormat::RGBA16);
                *active = Some(l.id);
                d.layers.push(l);
                Ok(())
            })
            .unwrap();
    }

    /// A 2×2 opaque render with a red top-left pixel.
    fn render() -> (Surface, Size) {
        let mut s = Surface::new(PixelFormat::RGBA16);
        s.write_region(photocraft_geom::Rect::from_xywh(0, 0, 2, 2), &[1.0, 0.0, 0.0, 1.0].repeat(4));
        (s, Size { width: 2, height: 2 })
    }

    fn names(doc: &photocraft_doc::Document) -> Vec<&str> {
        doc.layers.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn attaching_hands_photocraft_the_document_and_closes_what_it_held() {
        let (mut session, mut doc) = attached();
        assert!(doc.attached(&session));
        assert_eq!(session.documents().len(), 1);
        assert_eq!(names(&session.active().unwrap().doc), [crate::BASE_LAYER]);
        doc.attach(&mut session);
        assert_eq!(session.documents().len(), 1, "attached again: still one");
        close_all(&mut session);
        assert!(!doc.attached(&session));
    }

    #[test]
    fn a_pixel_edit_is_copied_back_and_marks_the_document_unsaved_selecting_doesnt() {
        let (mut session, mut doc) = attached();
        add_layer(&mut session, "Paint");
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER], "not before the copy");
        doc.copy_back(&mut session);
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"]);
        assert!(doc.unsaved());
        assert!(!session.active().unwrap().is_dirty(), "PhotoCraft's own unsaved stays clear");
        let rev = doc.revision;
        doc.copy_back(&mut session);
        assert_eq!(doc.revision, rev, "nothing new");
        let base = doc.base_layer().unwrap().id;
        session.select_layer(base).unwrap();
        doc.copy_back(&mut session);
        assert_eq!(doc.revision, rev, "selecting a layer isn't an edit");
        // an undo is an edit like any other
        assert!(session.undo());
        doc.copy_back(&mut session);
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER]);
        assert_eq!(doc.revision, rev + 1);
    }

    #[test]
    fn a_render_goes_into_photocraft_and_isnt_lost_on_the_next_copy_back() {
        let (mut session, mut doc) = attached();
        let rev = doc.revision;
        let (surface, size) = render();
        doc.write_base(&mut session, surface, size).unwrap();
        // PhotoCraft has it, as a change its canvas redraws in full, without a history step
        let st = session.active().unwrap();
        assert_eq!((st.doc.size, st.last_damage, st.history.past_len()), (size, None, 0));
        assert_eq!(st.doc.layers[0].surface().unwrap().tile_count(), 1);
        doc.copy_back(&mut session);
        assert_eq!(doc.base_layer().unwrap().surface().unwrap().tile_count(), 1, "the render survives the copy-back");
        assert_eq!((doc.pixel.size, doc.revision), (size, rev), "a render isn't an edit");
    }

    #[test]
    fn a_render_keeps_a_pixel_edit_not_yet_copied_back() {
        let (mut session, mut doc) = attached();
        add_layer(&mut session, "Paint");
        let (surface, size) = render();
        doc.write_base(&mut session, surface, size).unwrap();
        doc.copy_back(&mut session);
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"], "the edit survives the render");
        assert_eq!(doc.base_layer().unwrap().surface().unwrap().tile_count(), 1, "and the render the copy");
        assert!(doc.unsaved(), "the edit still counts");
        assert_eq!(names(&session.active().unwrap().doc), [crate::BASE_LAYER, "Paint"]);
    }

    #[test]
    fn a_document_photocraft_lost_is_attached_again_as_last_copied() {
        let (mut session, mut doc) = attached();
        add_layer(&mut session, "Paint");
        doc.copy_back(&mut session);
        close_all(&mut session);
        doc.copy_back(&mut session);
        assert!(doc.attached(&session));
        assert_eq!(names(&session.active().unwrap().doc), [crate::BASE_LAYER, "Paint"]);
    }
}
