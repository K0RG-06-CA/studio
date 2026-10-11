//! The pixel half of an open document lives in PhotoCraft's session: the Pixel persona edits it
//! there, with PhotoCraft's own undo, and the base layer's renders are written there too. That
//! session is the one place anything writes; [`Document::pixel`] is the copy Studio saves and
//! flattens, refreshed by [`Document::copy_back`] (an edit) and by [`Document::write_base`] (a
//! render, not an edit).
//!
//! The base layer is derived from the develop settings, never part of PhotoCraft's undo, which
//! restores whole documents: a state from before a render comes back with that render's pixels
//! and size. Every render written is kept in a register (weakly: an entry goes once no state
//! holds its pixels), so a base showing an earlier one is known to come from history, and the
//! current render goes back in. Pixel layers stay on the same photo pixels when Develop changes
//! the frame: each render has an [`Anchor`] (the settings it was made with), and a Develop
//! geometry change moves the layers with the photo as one recorded state paired with the Develop
//! step ([`crate::follow`]). A render that lands outside such a step (an exposure, a render of
//! undone settings) only ever shifts them by whole pixels, never resamples.

use std::sync::{Arc, Weak};

use lightcraft_engine::develop::{DevelopSettings, EmbeddedLens};
use photocraft_doc::{DocId, Layer, LayerContent};
use photocraft_engine::{DocState, Session};
use photocraft_geom::Size;
use photocraft_raster::{Surface, Tile};

use crate::Document;
use crate::follow::{Map, map};

/// Where a render puts the photo, which Pixel layers are anchored to: everything the frame
/// mapping needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    /// The settings the render was made with.
    pub settings: Arc<DevelopSettings>,
    /// `settings.hash64()`: which settings the render shows.
    pub hash: u64,
    /// The source's (EXIF-oriented) size in pixels.
    pub src: (usize, usize),
    /// The source's embedded lens corrections, as the render applied them.
    pub lens: Option<EmbeddedLens>,
}

impl Anchor {
    pub fn new(settings: Arc<DevelopSettings>, src: (usize, usize), lens: Option<EmbeddedLens>) -> Anchor {
        let hash = settings.hash64();
        Anchor { settings, hash, src, lens }
    }

    /// The two renders frame the photo the same way (only what isn't geometry differs).
    pub fn same_frame(&self, other: &Anchor) -> bool {
        self.src == other.src && self.lens == other.lens && framing(&self.settings) == framing(&other.settings)
    }
}

/// Everything in `s` that frames the photo: orientation, crop (rectangle, straighten, flips),
/// lens and perspective corrections. Two settings with the same framing render the same frame.
pub fn framing(s: &DevelopSettings) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let text = serde_json::to_string(&(&s.orientation, &s.crop, &s.optics, &s.geometry)).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// What a render that changes the frame outside a Develop step does to Pixel layers.
pub const LAYERS_STAY: &str =
    "Pixel layers didn\u{2019}t follow a change of the Develop frame made outside a Develop step: they stayed where they were";

/// What happened when PhotoCraft closed the open document by its own means.
pub const REOPENED: &str = "PhotoCraft closed the document: it was opened again as it was, without its Pixel undo history";

/// Why an edit of the base layer (or of the canvas size, which follows its render) was taken back.
pub const LOCKED: &str = "The Develop layer is locked: change it in Develop";

/// Where an open document's pixels are in PhotoCraft's session.
#[derive(Clone, Debug, Default)]
pub struct Link {
    /// The document's id in the session; none until attached.
    id: Option<DocId>,
    /// PhotoCraft's document as last copied into `Document::pixel` (pointer compare: every edit
    /// there makes a new `Arc`).
    copied: Option<Arc<photocraft_doc::Document>>,
    /// The newest render, with its size and anchor: what an undone-to state gets back.
    current: Option<(Surface, Size, Option<Anchor>)>,
    /// Every render written that some state may still show (the base's pixels at attach first).
    written: Vec<Written>,
    /// Something to tell whoever shows the document (e.g. layers that couldn't follow a crop).
    notice: Option<String>,
}

/// A render as written: its tiles (weakly, so the register doesn't keep them alive) and anchor.
#[derive(Clone, Debug)]
struct Written {
    tiles: Vec<Weak<Tile>>,
    anchor: Option<Anchor>,
}

impl Written {
    fn of(surface: &Surface, anchor: Option<Anchor>) -> Self {
        Written { tiles: surface.tiles().map(|(_, t)| Arc::downgrade(t)).collect(), anchor }
    }

    /// `surface` holds exactly these tiles. The weak pointers keep their allocations, so an
    /// address can't be reused by other pixels while the entry exists.
    fn shows(&self, surface: &Surface) -> bool {
        self.tiles.len() == surface.tile_count() && self.tiles.iter().zip(surface.tiles()).all(|(w, (_, t))| std::ptr::eq(w.as_ptr(), Arc::as_ptr(t)))
    }

    /// No state can show it any more (an empty render always can: the base before the first).
    fn gone(&self) -> bool {
        self.tiles.iter().any(|t| t.strong_count() == 0)
    }
}

impl Document {
    /// Make `session` the home of this document's pixels: whatever it held is closed (it only
    /// ever holds the open document), and this one goes in as last copied.
    pub fn attach(&mut self, session: &mut Session) {
        close_all(session);
        self.generation += 1;
        let i = session.add_document(self.pixel.clone(), None);
        let st = session.documents().get(i);
        let written = self.base_layer().and_then(Layer::surface).map(|s| Written::of(s, None)).into_iter().collect();
        self.link = Link { id: st.map(|s| s.doc.id), copied: st.map(|s| s.doc.clone()), written, ..Link::default() };
    }

    /// PhotoCraft holds this document (as attached; it may have closed it since).
    pub fn attached(&self, session: &Session) -> bool {
        self.link.id.is_some_and(|id| session.documents().iter().any(|s| s.doc.id == id))
    }

    /// Copy what PhotoCraft changed into `pixel`. Any new document there is an edit (a stroke, a
    /// layer, an undo), so `revision` moves; selecting a layer isn't one. A state from history
    /// that shows an earlier render gets the current one back first (and its size; the layers
    /// move with the crop), without a history step. PhotoCraft's own "unsaved" is kept clear,
    /// since saving is Studio's: its close guard and autosave stay quiet. PhotoCraft holds this
    /// document only: another one (made by its palette, say) is closed; if it closed this one, it
    /// is attached again as last copied, and the notice says its undo history went.
    pub fn copy_back(&mut self, session: &mut Session) {
        if let Some(id) = self.link.id {
            while let Some(i) = session.documents().iter().position(|s| s.doc.id != id) {
                session.close(i);
            }
        }
        if self.state(session).is_none() {
            let (was, current, written) = (self.link.id.is_some(), self.link.current.take(), std::mem::take(&mut self.link.written));
            self.attach(session);
            if was {
                // the pixels are the ones it last had: the renders it showed still stand
                self.link.current = current;
                self.link.written.extend(written);
                self.link.notice = Some(REOPENED.into());
            }
            return;
        }
        // the history sees PhotoCraft's document as it left it (an undo to a state showing an
        // old render is one before the copy puts the current render back), and then the copy
        self.reconcile_pixel(session);
        self.copy_changed(session);
        self.reconcile_pixel(session);
    }

    /// The copy itself, once PhotoCraft holds the document.
    fn copy_changed(&mut self, session: &mut Session) {
        let Some(st) = self.state(session) else { return };
        st.saved_revision = st.revision;
        if self.link.copied.as_ref().is_some_and(|c| Arc::ptr_eq(c, &st.doc)) {
            return;
        }
        if let Some(why) = self.link.refusal(&st.doc) {
            self.refuse(session, why);
        }
        let Some(st) = self.state(session) else { return };
        if self.link.copied.as_ref().is_some_and(|c| Arc::ptr_eq(c, &st.doc)) {
            return;
        }
        let unmarked = self.link.unmarked(&st.doc);
        let doc = unmarked.as_ref().unwrap_or(&st.doc);
        // a state from history showing the render of the settings now current (undone to the
        // other side of a geometry change) is right as it is: its render is the current one
        self.link.adopt(doc, self.develop.hash64());
        if let Some(doc) = self.link.restored(doc).or(unmarked) {
            st.doc = Arc::new(doc);
            st.revision += 1;
            st.saved_revision = st.revision;
            st.last_damage = None;
        }
        let now = st.doc.clone();
        self.pixel = (*now).clone();
        self.link.copied = Some(now);
        self.link.written.retain(|w| !w.gone());
        self.revision += 1;
    }

    /// Put a render of the base layer (and the document size it brings) into PhotoCraft: no
    /// history step, and not an edit. Its canvas redraws all of it. What PhotoCraft holds,
    /// including edits not yet copied back, is kept, and copied into `pixel` with the render;
    /// Pixel layers move with the crop ([`Anchor`]).
    pub(crate) fn write_base(&mut self, session: &mut Session, surface: Surface, size: Size, anchor: Option<Anchor>) -> Result<(), String> {
        // an edit made there since the last copy is copied as one, before the render goes in (and
        // a document PhotoCraft doesn't hold yet is attached)
        self.copy_back(session);
        let st = self.state(session).ok_or("PhotoCraft lost the document")?;
        let mut doc = (*st.doc).clone();
        let shown = doc.layers.iter().find(|l| crate::is_base(l)).and_then(Layer::surface).ok_or("the document has no base layer")?;
        let from = self.link.anchor_of(shown);
        self.link.place(&mut doc, from.as_ref(), anchor.as_ref(), surface.clone(), size)?;
        self.link.written.push(Written::of(&surface, anchor.clone()));
        self.link.current = Some((surface, size, anchor));
        let doc = Arc::new(doc);
        st.doc = doc.clone();
        st.revision += 1;
        st.saved_revision = st.revision;
        st.last_damage = None;
        self.pixel = (*doc).clone();
        self.link.copied = Some(doc);
        self.link.written.retain(|w| !w.gone());
        self.reconcile_pixel(session);
        Ok(())
    }

    /// What to tell the user about the document, once (layers that couldn't follow a Develop
    /// change; a history that had to start over).
    pub fn take_notice(&mut self) -> Option<String> {
        self.link.notice.take().or_else(|| self.history.take_notice())
    }

    /// Which settings the base layer's pixels show (none before the first render).
    pub fn shown_hash(&self) -> Option<u64> {
        self.link.current.as_ref()?.2.as_ref().map(|a| a.hash)
    }

    pub(crate) fn set_notice(&mut self, why: String) {
        self.link.notice = Some(why);
    }

    /// Take back the edit that made PhotoCraft's document break the lock (`why`): PhotoCraft's own
    /// undo, its redo cleared, so its history is as before; failing that (nothing to undo, a
    /// background job running, or the step undone to still breaks it), the document as last
    /// copied goes back in. Its notice says why.
    fn refuse(&mut self, session: &mut Session, why: &'static str) {
        // back to the first state that stands: a good edit made on the bad one goes too, so no
        // step is left that undoes to nothing (the history is finite, so is this)
        let mut undone = false;
        while session.undo() {
            undone = true;
            if self.state(session).is_none_or(|st| self.link.refusal(&st.doc).is_none()) {
                break;
            }
        }
        if let Some(st) = self.state(session) {
            if undone {
                st.history.clear_redo();
            }
            if (!undone || self.link.refusal(&st.doc).is_some())
                && let Some(good) = self.link.copied.clone()
            {
                st.doc = good;
                st.revision += 1;
                st.last_damage = None;
            }
            st.saved_revision = st.revision;
        }
        self.link.notice = Some(why.into());
    }

    /// The document's state in PhotoCraft (made the active one: the session only has this one).
    pub(crate) fn state<'s>(&self, session: &'s mut Session) -> Option<&'s mut DocState> {
        state_of(session, self.link.id?)
    }
}

/// The state of document `id` in `session`, made the active one.
pub(crate) fn state_of(session: &mut Session, id: DocId) -> Option<&mut DocState> {
    let i = session.documents().iter().position(|s| s.doc.id == id)?;
    session.set_active(i);
    session.active_mut()
}

impl Link {
    /// The document's id in PhotoCraft's session, once attached.
    pub(crate) fn id(&self) -> Option<DocId> {
        self.id
    }

    /// Drop the renders no state shows any more (after the history let go of states).
    pub(crate) fn forget_gone(&mut self) {
        self.written.retain(|w| !w.gone());
    }

    /// Renders some state may still show.
    #[cfg(test)]
    pub(crate) fn written_len(&self) -> usize {
        self.written.len()
    }

    /// `doc` without the base's mark (and lock) on any layer but the base: a Duplicate Layer of
    /// it is an ordinary layer, as Photoshop's `"Background copy`". None when there is none.
    fn unmarked(&self, doc: &photocraft_doc::Document) -> Option<photocraft_doc::Document> {
        let base = self.copied.as_ref()?.layers.iter().find(|l| crate::is_base(l))?.id;
        let copies: Vec<_> = doc.walk().into_iter().filter(|(_, _, l)| crate::is_base(l) && l.id != base).map(|(_, _, l)| l.id).collect();
        if copies.is_empty() {
            return None;
        }
        let mut out = doc.clone();
        for id in copies {
            if let Some(l) = out.layer_mut(id) {
                crate::unmark(l);
            }
        }
        Some(out)
    }

    /// Why `doc` (new in PhotoCraft) can't stand: an edit changed the base layer or the canvas.
    /// The base must be the one marked layer, at the bottom of the root; keep everything as it
    /// was but its visibility (hiding it is fine, as hiding a locked layer is in Photoshop); show
    /// pixels Studio wrote; and the canvas keeps the size of the render it shows. PhotoCraft
    /// enforces the lock on painting, moving and filters itself, not on deleting, arranging,
    /// merging, flattening or layer properties (`d836e34`; see `tests::l2_*`).
    fn refusal(&self, doc: &photocraft_doc::Document) -> Option<&'static str> {
        let good = self.copied.as_ref()?;
        let good_base = good.layers.iter().find(|l| crate::is_base(l))?;
        // by its id: a duplicate of it carries its mark too, and is an ordinary layer (see `unmarked`)
        let Some(base) = doc.layers.first().filter(|l| l.id == good_base.id && crate::is_base(l)) else { return Some(LOCKED) };
        // everything but visibility and the pixels
        let plain =
            |l: &Layer| Layer { visible: true, content: LayerContent::Raster(Surface::new(photocraft_color::PixelFormat::RGBA16)), ..l.clone() };
        if plain(base) != plain(good_base) {
            return Some(LOCKED);
        }
        let (Some(shown), Some(good_shown)) = (base.surface(), good_base.surface()) else { return Some(LOCKED) };
        let unchanged = Written::of(good_shown, None).shows(shown);
        if (unchanged && doc.size != good.size) || (!unchanged && !self.written.iter().any(|w| w.shows(shown))) {
            return Some(LOCKED);
        }
        None
    }

    /// The anchor of the render `surface` shows: a written one, or none (unknown, or the empty
    /// base before the first render).
    pub(crate) fn anchor_of(&self, surface: &Surface) -> Option<Anchor> {
        self.written.iter().rev().find(|w| w.shows(surface)).and_then(|w| w.anchor.clone())
    }

    /// A render went into PhotoCraft's document `doc` (a follow's recorded state): the register
    /// and the last copy take it.
    pub(crate) fn landed(&mut self, doc: Arc<photocraft_doc::Document>, surface: Surface, size: Size, anchor: Anchor) {
        self.written.push(Written::of(&surface, Some(anchor.clone())));
        self.current = Some((surface, size, Some(anchor)));
        self.copied = Some(doc);
        self.written.retain(|w| !w.gone());
    }

    /// `doc` (PhotoCraft's current document) shows a written render of the settings `want`: it
    /// becomes the current render, so nothing is put back and nothing renders (an undo or redo
    /// landed on the other side of a geometry change). One of other settings is put right by
    /// `restored` (the current render back in, the layers shifted by whole pixels for a crop),
    /// unless that would take more than a shift (the current render is in another frame: the
    /// state is from the other side of a geometry change, whose render was replaced since), or a
    /// shift to a render that isn't of `want` either (a crop redone over a later render: the
    /// state is in the frame `want` puts it, and the stale render in another). Then
    /// the document stays as it is and has no current render: the next render of `want` lands
    /// into it, in the state's own frame, and nothing is resampled.
    fn adopt(&mut self, doc: &photocraft_doc::Document, want: u64) {
        let Some(shown) = doc.layers.iter().find(|l| crate::is_base(l)).and_then(Layer::surface) else { return };
        let Some(anchor) = self.written.iter().rev().find(|w| w.shows(shown)).and_then(|w| w.anchor.clone()) else { return };
        if anchor.hash == want {
            self.current = Some((shown.clone(), doc.size, Some(anchor)));
            return;
        }
        // the current render goes back in when it is in the state's frame, or a crop away from it
        // and of the settings wanted (the layers then shift into its frame); a stale one a crop
        // away would shift them out of the frame the settings wanted put them in (a crop redone
        // over a later render)
        let fits = |(_, size, to): &(Surface, Size, Option<Anchor>)| {
            to.as_ref().is_none_or(|to| match map(&anchor, doc.size, to, *size) {
                Map::Same => true,
                Map::Shift(..) => to.hash == want,
                _ => false,
            })
        };
        if self.current.as_ref().is_some_and(|c| !fits(c)) {
            self.current = None;
        }
    }

    /// `doc` from history showing an earlier render, with the current one put back: none when it
    /// shows the current one already (or something Studio didn't write, which the lock guard
    /// deals with).
    fn restored(&mut self, doc: &photocraft_doc::Document) -> Option<photocraft_doc::Document> {
        let (surface, size, anchor) = self.current.clone()?;
        let shown = doc.layers.iter().find(|l| crate::is_base(l)).and_then(Layer::surface)?;
        if Written::of(&surface, None).shows(shown) {
            return None;
        }
        let from = self.written.iter().rev().find(|w| w.shows(shown))?.anchor.clone();
        let mut out = doc.clone();
        self.place(&mut out, from.as_ref(), anchor.as_ref(), surface, size).ok()?;
        Some(out)
    }

    /// Put a render into `doc` (base pixels and size), moving the other layers from where `from`
    /// framed the photo to where `to` does: by whole pixels only (a crop), exactly. A frame change
    /// that needs more is a Develop step's follow ([`crate::follow`]), never a plain render; one
    /// that arrives here anyway leaves the layers where they are, and the notice says so.
    fn place(
        &mut self,
        doc: &mut photocraft_doc::Document,
        from: Option<&Anchor>,
        to: Option<&Anchor>,
        surface: Surface,
        size: Size,
    ) -> Result<(), String> {
        let others = top_level_others(doc);
        if let (Some(from), Some(to), false) = (from, to, others.is_empty()) {
            match map(from, doc.size, to, size) {
                Map::Same => {}
                Map::Shift(dx, dy) => {
                    let targets = photocraft_engine::layer_multi_cmds::move_targets(doc, &others);
                    // a layer the user locked in place stops the move: all of them stay, and say so
                    match photocraft_engine::layer_multi_cmds::moved(doc, &targets, dx, dy) {
                        Ok(m) => *doc = m,
                        Err(e) => self.notice = Some(format!("Pixel layers didn\u{2019}t follow the Develop crop: {e}")),
                    }
                }
                Map::Affine(_) | Map::Warped(_) => self.notice = Some(LAYERS_STAY.into()),
            }
        }
        put_base(doc, surface, size)
    }
}

/// The layers above the base, at the root (groups move as one).
pub(crate) fn top_level_others(doc: &photocraft_doc::Document) -> Vec<photocraft_doc::LayerId> {
    doc.layers.iter().filter(|l| !crate::is_base(l)).map(|l| l.id).collect()
}

/// The render's pixels into the base layer, and its size onto the canvas.
pub(crate) fn put_base(doc: &mut photocraft_doc::Document, surface: Surface, size: Size) -> Result<(), String> {
    let base = doc.layers.iter_mut().find(|l| crate::is_base(l)).ok_or("the document has no base layer")?;
    base.content = LayerContent::Raster(surface);
    doc.size = size;
    Ok(())
}

/// Close every document PhotoCraft holds (the open document's pixels go with it: `pixel` keeps
/// the last copy).
pub fn close_all(session: &mut Session) {
    while session.close(0).is_some() {}
}

#[cfg(test)]
pub(crate) mod tests {
    use photocraft_color::PixelFormat;
    use photocraft_doc::Layer;

    use super::*;
    use crate::tests::demo;

    pub(crate) fn attached() -> (Session, Document) {
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

    /// Bytes of one tile (every surface in these tests fits in one).
    pub(crate) const TILE: usize = 256 * 256 * 8;

    pub(crate) fn names(doc: &photocraft_doc::Document) -> Vec<&str> {
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
        doc.write_base(&mut session, surface, size, None).unwrap();
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
        doc.write_base(&mut session, surface, size, None).unwrap();
        doc.copy_back(&mut session);
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"], "the edit survives the render");
        assert_eq!(doc.base_layer().unwrap().surface().unwrap().tile_count(), 1, "and the render the copy");
        assert!(doc.unsaved(), "the edit still counts");
        assert_eq!(names(&session.active().unwrap().doc), [crate::BASE_LAYER, "Paint"]);
    }

    /// A new render of `w × h` (fresh tiles every call).
    pub(crate) fn render_sized(w: u32, h: u32) -> (Surface, Size) {
        let mut s = Surface::new(PixelFormat::RGBA16);
        s.write_region(photocraft_geom::Rect::from_xywh(0, 0, w, h), &[0.5, 0.5, 0.5, 1.0].repeat((w * h) as usize));
        (s, Size { width: w, height: h })
    }

    /// An anchor for a 64 × 48 source: `frame` 1 upright, 2 rotated left; the crop's top-left at
    /// (`x`, `y`) source pixels.
    pub(crate) fn anchor(frame: u64, x: f64, y: f64) -> Option<Anchor> {
        let mut s = DevelopSettings::default();
        if frame == 2 {
            s.orientation = s.orientation.rotated(false);
        }
        s.crop.geometry.rect.x0 = x / 64.0;
        s.crop.geometry.rect.y0 = y / 48.0;
        Some(Anchor::new(Arc::new(s), (64, 48), None))
    }

    /// A Pixel edit: a layer with one painted pixel at (x, y).
    pub(crate) fn paint(session: &mut Session, name: &str, x: i32, y: i32) {
        session
            .edit(name, |d, active| {
                let mut l = Layer::raster(name, PixelFormat::RGBA16);
                l.surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(x, y, 1, 1), &[1.0, 0.0, 0.0, 1.0]);
                *active = Some(l.id);
                d.layers.push(l);
                Ok(())
            })
            .unwrap();
    }

    /// Where the painted pixel of layer `name` is.
    fn at(doc: &photocraft_doc::Document, name: &str) -> (i32, i32) {
        let l = doc.layers.iter().find(|l| l.name == name).unwrap();
        let b = photocraft_compose::bounds::content_bounds(l.surface().unwrap());
        (b.x0, b.y0)
    }

    /// The base shows the newest render, in Studio's copy and in PhotoCraft.
    fn shows_current(doc: &Document, session: &Session) -> bool {
        let (surface, size, _) = doc.link.current.as_ref().unwrap();
        let current = Written::of(surface, None);
        let pc = &session.active().unwrap().doc;
        let base = |d: &photocraft_doc::Document| d.layers.iter().find(|l| crate::is_base(l)).unwrap().surface().unwrap().clone();
        current.shows(&base(&doc.pixel)) && current.shows(&base(pc)) && doc.pixel.size == *size && pc.size == *size
    }

    fn undo(session: &mut Session, doc: &mut Document) -> bool {
        let undone = session.undo();
        doc.copy_back(session);
        undone
    }

    #[test]
    fn undoing_to_a_state_from_before_a_render_gets_the_current_render_back() {
        let (mut session, mut doc) = attached();
        let (r1, s1) = render_sized(64, 48);
        doc.write_base(&mut session, r1, s1, anchor(1, 0.0, 0.0)).unwrap();
        paint(&mut session, "Paint", 10, 10);
        doc.copy_back(&mut session);
        let (r2, s2) = render_sized(64, 48);
        doc.write_base(&mut session, r2, s2, anchor(1, 0.0, 0.0)).unwrap();
        paint(&mut session, "More", 20, 20);
        doc.copy_back(&mut session);
        assert!(undo(&mut session, &mut doc) && shows_current(&doc, &session), "undo to a state written after the render");
        assert!(undo(&mut session, &mut doc), "undo to the state from before it");
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER]);
        assert!(shows_current(&doc, &session), "the current render came back, not the first");
        let st = session.active().unwrap();
        assert_eq!((st.last_damage, st.is_dirty()), (None, false));
        // redo too: the redone state shows the current render
        assert!(session.redo());
        doc.copy_back(&mut session);
        assert!(shows_current(&doc, &session));
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"]);
    }

    #[test]
    fn with_photocrafts_history_cap_hit_undo_still_gets_the_current_render_back() {
        let (mut session, mut doc) = attached();
        // the cap is Studio's (milestone 5): room for the current document (a render and five
        // paints, a tile each), two older renders and Studio's own few bytes
        doc.history.bytes_cap = 9 * TILE;
        for i in 0..5 {
            paint(&mut session, &format!("Paint {i}"), i, i);
            doc.copy_back(&mut session);
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        assert_eq!(session.active().unwrap().history.past_len(), 2, "older steps were dropped");
        let mut undone = 0;
        while undo(&mut session, &mut doc) {
            undone += 1;
            assert!(shows_current(&doc, &session), "after {undone} undos");
        }
        assert_eq!(undone, 2);
        assert_eq!(names(&doc.pixel).len(), 4, "the base and the three paints the cap kept");
    }

    #[test]
    fn the_register_forgets_renders_no_state_holds() {
        let (mut session, mut doc) = attached();
        for _ in 0..10 {
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        // the empty base from before the first render (always showable) and the current one
        assert_eq!(doc.link.written.len(), 2);
        // a render under each edit stays known: undo can bring each state back
        for i in 0..3 {
            paint(&mut session, &format!("Paint {i}"), i, i);
            doc.copy_back(&mut session);
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        assert_eq!(doc.link.written.len(), 5);
        // once the history lets go of them, so does the register
        session.active_mut().unwrap().history.clear();
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        assert_eq!(doc.link.written.len(), 2);
    }

    #[test]
    fn pixel_layers_follow_a_crop_even_through_undo() {
        let (mut session, mut doc) = attached();
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        paint(&mut session, "Paint", 10, 10);
        paint(&mut session, "Later", 30, 30);
        doc.copy_back(&mut session);
        // Develop crops 4 px off the left, 2 off the top: the layers stay on the same photo pixels
        let rev = doc.revision;
        let (r, cropped) = render_sized(60, 46);
        let crop = anchor(1, 4.0, 2.0);
        // the render is of the document's settings, as `Base` makes it (an undo to a state showing
        // a render of the current settings would adopt it instead)
        doc.develop = (*crop.as_ref().unwrap().settings).clone();
        doc.write_base(&mut session, r, cropped, crop).unwrap();
        assert_eq!((at(&doc.pixel, "Paint"), at(&doc.pixel, "Later")), ((6, 8), (26, 28)));
        assert_eq!(at(&session.active().unwrap().doc, "Paint"), (6, 8), "in PhotoCraft too");
        doc.copy_back(&mut session);
        assert_eq!(doc.revision, rev, "the move is the render's, not an edit");
        assert_eq!(session.active().unwrap().history.past_len(), 2, "and no history step");
        // undo a Pixel edit made before the crop: the state comes back in the old frame, and
        // is moved like the render
        assert!(undo(&mut session, &mut doc));
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"]);
        assert_eq!(at(&doc.pixel, "Paint"), (6, 8));
        assert!(shows_current(&doc, &session));
        assert_eq!(doc.pixel.size, cropped);
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn a_frame_change_written_outside_a_develop_step_leaves_layers_and_says_so() {
        let (mut session, mut doc) = attached();
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        paint(&mut session, "Paint", 10, 10);
        doc.copy_back(&mut session);
        // another frame (a rotation): the sides swap, the layer stays where it was
        let (r, turned) = render_sized(48, 64);
        doc.write_base(&mut session, r, turned, anchor(2, 0.0, 0.0)).unwrap();
        assert_eq!((at(&doc.pixel, "Paint"), doc.pixel.size), ((10, 10), turned));
        assert_eq!(doc.take_notice().as_deref(), Some(LAYERS_STAY));
        assert_eq!(doc.take_notice(), None, "said once");
        // without Pixel layers there is nothing to say
        let (mut session, mut doc) = attached();
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        let (r, s) = render_sized(48, 64);
        doc.write_base(&mut session, r, s, anchor(2, 0.0, 0.0)).unwrap();
        assert_eq!(doc.take_notice(), None);
    }

    /// A rendered document with a Pixel layer "Paint" over the base, both copied back.
    fn painted() -> (Session, Document) {
        let (mut session, mut doc) = attached();
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        paint(&mut session, "Paint", 10, 10);
        doc.copy_back(&mut session);
        (session, doc)
    }

    fn base_id(doc: &Document) -> u64 {
        doc.base_layer().unwrap().id.0
    }

    fn paint_id(doc: &Document) -> u64 {
        doc.pixel.layers.iter().find(|l| l.name == "Paint").unwrap().id.0
    }

    /// Run a PhotoCraft command, then copy back (as the next frame does): was it taken back?
    fn refused(session: &mut Session, doc: &mut Document, id: &str, params: serde_json::Value) -> bool {
        let before = (names(&doc.pixel).iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(), session.active().unwrap().history.entries());
        session.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
        doc.copy_back(session);
        let after = (names(&doc.pixel).iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(), session.active().unwrap().history.entries());
        match doc.take_notice() {
            Some(n) => {
                assert_eq!(n, LOCKED, "{id}");
                assert_eq!(before, after, "{id}: layers and history as before");
                let base = doc.base_layer().unwrap();
                assert!(std::ptr::eq(base, &doc.pixel.layers[0]) && base.locks.all && base.visible, "{id}");
                assert!(!session.active().unwrap().history.can_redo(), "{id}: nothing to redo into");
                true
            }
            None => false,
        }
    }

    #[test]
    fn the_develop_layer_can_be_hidden_but_nothing_else_changes() {
        let (mut session, mut doc) = painted();
        let base = base_id(&doc);
        // hiding it is fine, and undoable
        assert!(!refused(&mut session, &mut doc, "layer.setProps", serde_json::json!({"layer": base, "visible": false})));
        assert!(!doc.base_layer().unwrap().visible);
        assert!(undo(&mut session, &mut doc) && doc.base_layer().unwrap().visible);
        // hiding it while clearing its lock is refused as a whole: it stays shown and locked
        let unlock = serde_json::json!({"layer": base, "visible": false, "locks": {"all": false}});
        assert!(refused(&mut session, &mut doc, "layer.setProps", unlock));
        for change in [
            serde_json::json!({"layer": base, "name": "Mine now"}),
            serde_json::json!({"layer": base, "opacity": 0.5}),
            serde_json::json!({"layer": base, "blend": "Multiply"}),
            serde_json::json!({"layer": base, "locked": false}),
        ] {
            assert!(refused(&mut session, &mut doc, "layer.setProps", change.clone()), "{change}");
        }
        // the same on another layer is an ordinary edit
        let paint = paint_id(&doc);
        assert!(!refused(&mut session, &mut doc, "layer.setProps", serde_json::json!({"layer": paint, "opacity": 0.5})));
    }

    #[test]
    fn deleting_moving_merging_or_flattening_the_develop_layer_is_refused() {
        let (mut session, mut doc) = painted();
        let (base, paint) = (base_id(&doc), paint_id(&doc));
        assert!(refused(&mut session, &mut doc, "layer.delete", serde_json::json!({"layer": base})));
        assert!(refused(&mut session, &mut doc, "layer.arrange.bringForward", serde_json::json!({"layer": base})));
        assert!(refused(&mut session, &mut doc, "layer.arrange.sendToBack", serde_json::json!({"layer": paint})));
        assert!(refused(&mut session, &mut doc, "layer.moveTo", serde_json::json!({"layer": paint, "target": base, "position": "below"})));
        assert!(refused(&mut session, &mut doc, "layer.mergeDown", serde_json::json!({"layer": paint})));
        assert!(refused(&mut session, &mut doc, "layer.flattenImage", serde_json::json!({})));
        // ordinary edits still go through
        assert!(!refused(&mut session, &mut doc, "layer.delete", serde_json::json!({"layer": paint})));
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER]);
    }

    /// Review H2: Duplicate Layer on the Develop layer makes an ordinary layer (Photoshop's
    /// "Background copy"), not a second, locked base.
    #[test]
    fn duplicating_the_develop_layer_makes_an_ordinary_layer() {
        let (mut session, mut doc) = painted();
        let base = base_id(&doc);
        assert!(!refused(&mut session, &mut doc, "layer.duplicate", serde_json::json!({"layer": base})));
        assert_eq!(doc.pixel.layers.len(), 3);
        let marked: Vec<_> = doc.pixel.walk().into_iter().filter(|(_, _, l)| crate::is_base(l)).map(|(_, _, l)| l.id.0).collect();
        assert_eq!(marked, [base], "only the Develop layer is the base");
        let copy = doc.pixel.layers.iter().find(|l| l.id.0 != base && l.name != "Paint").unwrap();
        assert!(!copy.locks.all, "the copy can be edited");
        let copy = copy.id;
        assert!(doc.unsaved());
        // in PhotoCraft too, and the next render goes into the Develop layer, not the copy
        assert_eq!(session.active().unwrap().doc.walk().into_iter().filter(|(_, _, l)| crate::is_base(l)).count(), 1);
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        assert!(shows_current(&doc, &session) && base_id(&doc) == base);
        // and painting into the copy is an ordinary edit
        session
            .edit("Paint copy", |d, _| {
                d.layer_mut(copy).unwrap().surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(0, 0, 1, 1), &[1.0; 4]);
                Ok(())
            })
            .unwrap();
        doc.copy_back(&mut session);
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn painting_into_the_develop_layer_or_resizing_the_canvas_by_any_path_is_refused() {
        let (mut session, mut doc) = painted();
        // what PhotoCraft's tools refuse on a locked layer, done by a path that doesn't check
        session
            .edit("Sneaky", |d, _| {
                let base = d.layers.iter_mut().find(|l| crate::is_base(l)).unwrap();
                base.surface_mut().unwrap().write_region(photocraft_geom::Rect::from_xywh(0, 0, 1, 1), &[0.0, 0.0, 1.0, 1.0]);
                Ok(())
            })
            .unwrap();
        doc.copy_back(&mut session);
        assert_eq!(doc.take_notice().as_deref(), Some(LOCKED));
        assert!(shows_current(&doc, &session), "the render, untouched");
        session
            .edit("Canvas Size", |d, _| {
                d.size = Size { width: 100, height: 100 };
                Ok(())
            })
            .unwrap();
        doc.copy_back(&mut session);
        assert_eq!(doc.take_notice().as_deref(), Some(LOCKED));
        assert_eq!(doc.pixel.size, Size { width: 64, height: 48 });
    }

    #[test]
    fn a_bad_edit_after_a_good_one_in_the_same_frame_keeps_the_good_one() {
        let (mut session, mut doc) = painted();
        paint(&mut session, "Good", 5, 5);
        session.execute("layer.delete", serde_json::json!({"layer": base_id(&doc)})).unwrap();
        doc.copy_back(&mut session);
        assert_eq!(doc.take_notice().as_deref(), Some(LOCKED));
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint", "Good"]);
        assert!(undo(&mut session, &mut doc), "the good one undoes as usual");
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"]);
    }

    /// Review: a bad edit followed by a good one in the same frame (built on the bad state, so it
    /// goes too) leaves the history as it was before both: no step that undoes to nothing.
    #[test]
    fn a_good_edit_after_a_bad_one_in_the_same_frame_goes_with_it_and_leaves_no_step() {
        let (mut session, mut doc) = painted();
        let entries = session.active().unwrap().history.entries();
        let base = base_id(&doc);
        session.execute("layer.setProps", serde_json::json!({"layer": base, "opacity": 0.5})).unwrap();
        paint(&mut session, "Built on it", 5, 5);
        doc.copy_back(&mut session);
        assert_eq!(doc.take_notice().as_deref(), Some(LOCKED));
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER, "Paint"]);
        assert_eq!(doc.base_layer().unwrap().opacity, 1.0);
        assert_eq!(session.active().unwrap().history.entries(), entries, "the history as before both");
        // so the next undo takes back the edit before them
        assert!(undo(&mut session, &mut doc));
        assert_eq!(names(&doc.pixel), [crate::BASE_LAYER]);
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn with_the_history_cap_hit_a_bad_edit_is_still_refused() {
        let (mut session, mut doc) = painted();
        doc.history.bytes_cap = 9 * TILE;
        for i in 0..5 {
            paint(&mut session, &format!("Paint {i}"), i, i);
            doc.copy_back(&mut session);
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut session, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        while undo(&mut session, &mut doc) {}
        assert_eq!(doc.take_notice(), None, "nothing undone to was taken for a bad edit");
        assert!(shows_current(&doc, &session));
        // merging the layer right above the base into it
        let above = doc.pixel.layers[1].id.0;
        assert!(refused(&mut session, &mut doc, "layer.mergeDown", serde_json::json!({"layer": above})));
    }

    // --- PhotoCraft limitations (`d836e34`): each test fails once upstream fixes it ---

    /// L2: PhotoCraft's engine lets these through on a fully locked layer (it refuses painting,
    /// moving and filters on one). Once it refuses one of them, its test fails: the guard above
    /// can stay (it also covers paths that never ask), but this list shrinks.
    fn locked_doc() -> (Session, u64, u64) {
        let mut d =
            photocraft_doc::Document::new("l2", Size { width: 8, height: 8 }, photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U16);
        let mut locked = Layer::raster("Locked", PixelFormat::RGBA16);
        locked.locks.all = true;
        let free = Layer::raster("Free", PixelFormat::RGBA16);
        let (l, f) = (locked.id.0, free.id.0);
        d.layers.push(locked);
        d.layers.push(free);
        let mut session = Session::new();
        session.add_document(d, None);
        (session, l, f)
    }

    fn l2_runs(id: &str, params: impl Fn(u64, u64) -> serde_json::Value) {
        let (mut session, locked, free) = locked_doc();
        let before = session.active().unwrap().doc.clone();
        session.execute(id, params(locked, free)).unwrap_or_else(|e| panic!("upstream now refuses {id} on a locked layer: {e}"));
        assert!(!Arc::ptr_eq(&before, &session.active().unwrap().doc), "{id} changed nothing");
    }

    #[test]
    fn l2_photocraft_deletes_a_locked_layer() {
        l2_runs("layer.delete", |l, _| serde_json::json!({"layer": l}));
    }

    #[test]
    fn l2_photocraft_arranges_a_locked_layer() {
        l2_runs("layer.arrange.bringForward", |l, _| serde_json::json!({"layer": l}));
    }

    #[test]
    fn l2_photocraft_moves_a_layer_below_a_locked_bottom_layer() {
        l2_runs("layer.moveTo", |l, f| serde_json::json!({"layer": f, "target": l, "position": "below"}));
    }

    #[test]
    fn l2_photocraft_changes_a_locked_layers_properties_and_its_lock() {
        l2_runs("layer.setProps", |l, _| serde_json::json!({"layer": l, "opacity": 0.5, "locks": {"all": false}}));
    }

    #[test]
    fn l2_photocraft_merges_down_into_a_locked_layer() {
        l2_runs("layer.mergeDown", |_, f| serde_json::json!({"layer": f}));
    }

    #[test]
    fn l2_photocraft_flattens_a_locked_layer() {
        l2_runs("layer.flattenImage", |_, _| serde_json::json!({}));
    }

    /// L3: PhotoCraft's undo restores the whole document, a layer the host wrote outside its
    /// history included. Fails once it can leave a layer out (the register above can go then).
    #[test]
    fn l3_photocraft_undo_brings_back_pixels_written_outside_its_history() {
        let (mut session, locked, _) = locked_doc();
        let before = session.active().unwrap().doc.clone();
        session.execute("layer.setProps", serde_json::json!({"layer": locked, "visible": false})).unwrap();
        // the host writes the locked layer's pixels, no history step
        let st = session.active_mut().unwrap();
        let mut d = (*st.doc).clone();
        d.layer_mut(photocraft_doc::LayerId(locked))
            .unwrap()
            .surface_mut()
            .unwrap()
            .write_region(photocraft_geom::Rect::from_xywh(0, 0, 1, 1), &[1.0; 4]);
        d.size = Size { width: 4, height: 4 };
        st.doc = Arc::new(d);
        assert!(session.undo());
        let now = &session.active().unwrap().doc;
        assert!(Arc::ptr_eq(&before, now), "undo brought back the whole earlier document, pixels and size");
        assert_eq!(now.layer(photocraft_doc::LayerId(locked)).unwrap().surface().unwrap().tile_count(), 0);
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
