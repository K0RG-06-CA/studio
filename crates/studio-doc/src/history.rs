//! One undo history per document, across personas (PRD › milestone 5; `docs/m5-plan.md`).
//! Studio owns the history; the engines keep the states.
//!
//! A **Develop step** is a change of this photo's develop settings, recorded by Studio as the
//! settings (and the photo's History list) before and after. It is undone by committing the
//! `before` pair to LightCraft (`session.commit`, so it is journaled and survives a restart; the
//! same ops LightCraft's own undo applies, so its History panel shows the step gone). Ratings,
//! imports, "Open x.studio", other photos' edits and anything else on LightCraft's stack are not
//! steps and are never touched. LightCraft's own stack grows with "Undo X" entries; Studio never
//! reads it.
//!
//! A **Pixel step** is a PhotoCraft history state, known by identity (the state's `Arc`, kept
//! weakly so an address is never reused while Studio holds it), never by counts: after every
//! copy-back the past states are diffed against those last seen. States appended are new steps;
//! a changed current document with the same states is a host replacement (a render, a restored
//! state) or PhotoCraft's own coalescing, not an edit; states removed with PhotoCraft's redo grown
//! are an undo PhotoCraft did itself (accepted as Studio's when the steps are the newest and all
//! Pixel; otherwise put back and done as a Studio jump, Develop steps included); states removed
//! with no redo are a fold or a refused edit; a lost prefix is PhotoCraft's cap. Anything the diff
//! can't name clears the history, with a notice: never a guess.
//!
//! Memory: one budget ([`HISTORY_BYTES`], [`HISTORY_STEPS`]) for PhotoCraft's states and Studio's
//! own steps. Studio owns PhotoCraft's `max_states`/`max_bytes` (its prefs would re-apply 50 / 8 GB)
//! and trims after every write; redo states count and are never pruned (an edit clears them); the
//! newest undo state is always kept (upstream `trim`), so retained bytes ≤ cap + one state. Studio
//! prunes Develop-only steps from the oldest end itself; a step that owns a Pixel state can only
//! go when PhotoCraft drops that state, which it does only inside `record` (`max_states`) or `trim`
//! (bytes), never on demand (L12), so the step cap can lag by one Pixel state.

use std::collections::HashSet;
use std::sync::{Arc, Weak};

use lightcraft_engine::Session;
use lightcraft_engine::catalog::model::PhotoId;
use lightcraft_engine::catalog::{HistoryStep, Op};
use lightcraft_engine::develop::DevelopSettings;
use photocraft_doc::DocId;
use photocraft_engine::{DocState, Session as PixelSession};

use crate::Document;
use crate::base::Base;
use crate::pixel::{framing, state_of};

/// Pixel memory the history may hold (PhotoCraft's states and Studio's own steps), by default.
pub const HISTORY_BYTES: usize = 1 << 30;
/// Steps kept, at most.
pub const HISTORY_STEPS: usize = 1000;

/// What the history says when it had to start over.
pub const RESET_PIXEL: &str = "History reset: PhotoCraft changed it in a way Studio can\u{2019}t follow";
pub const RESET_LIBRARY: &str = "History reset: the library changed";

/// Why PhotoCraft and the history wait while a Develop geometry change renders.
pub const RENDERING: &str = "Develop is rendering: Pixel follows once it has landed";

/// A PhotoCraft state, by identity. Weak: the register never keeps pixels alive, but the
/// allocation stays, so no other state can get its address while Studio remembers it.
type Pointer = Weak<photocraft_doc::Document>;

/// A change of this photo's develop settings: the settings and History list to commit to go
/// back, and to go forward again. Neighbouring steps share one list (`after` of one is `before`
/// of the next).
#[derive(Clone)]
struct Develop {
    before: Arc<DevelopSettings>,
    after: Arc<DevelopSettings>,
    history_before: Arc<Vec<HistoryStep>>,
    history_after: Arc<Vec<HistoryStep>>,
}

/// One undoable step: a Develop change, PhotoCraft states, or (a geometry change, milestone 5
/// task 2) both.
pub struct Step {
    pub label: String,
    develop: Option<Develop>,
    /// The PhotoCraft states this step made, oldest first (each is the document before its edit).
    pixel: Vec<Pointer>,
}

impl Step {
    fn pixel_only(&self) -> bool {
        self.develop.is_none()
    }

    /// Bytes Studio itself holds for the step (struct sizes, no deep walk); a History list counts
    /// once, for the first step holding it.
    fn bytes(&self, seen: &mut HashSet<usize>) -> usize {
        let Some(d) = &self.develop else { return self.label.len() };
        let mut list = |h: &Arc<Vec<HistoryStep>>| if seen.insert(Arc::as_ptr(h) as usize) { h.len() * size_of::<HistoryStep>() } else { 0 };
        self.label.len() + 2 * size_of::<DevelopSettings>() + list(&d.history_before) + list(&d.history_after)
    }
}

/// A row of the History panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub develop: bool,
    pub pixel: bool,
}

/// A jump PhotoCraft began by itself (its own undo or redo past a Develop step), put back, and
/// done as a Studio jump by [`Document::reconcile`]: the state to reach.
enum Jump {
    Undo(Pointer),
    Redo(Pointer),
}

pub struct History {
    steps: Vec<Step>,
    redo: Vec<Step>,
    /// Pixel memory the history may hold: [`HISTORY_BYTES`] unless a test lowers it.
    pub bytes_cap: usize,
    /// The settings and History list at the last step boundary: what the next Develop step is
    /// a change from.
    develop_base: Option<(Arc<DevelopSettings>, Arc<Vec<HistoryStep>>)>,
    /// Which library loaded (LightCraft gives each load its own revision range).
    library: Option<u64>,
    /// PhotoCraft's past states as last seen, its current document, and the documents on its
    /// redo stack (newest last), all by identity.
    seen: Vec<Pointer>,
    current: Option<Pointer>,
    redo_docs: Vec<Pointer>,
    /// PhotoCraft's retained pixel bytes as last measured.
    pixel_bytes: usize,
    /// Studio's redo was cleared (a Develop edit, a reset) while PhotoCraft's still stands: it
    /// goes at the next copy-back.
    clear_pixel_redo: bool,
    requested: Option<Jump>,
    notice: Option<String>,
    /// The newest step is a Develop step that changed the frame, and waits for its render to land
    /// as its Pixel half (the follow, [`crate::follow`]): the hash of the settings it waits for.
    /// Until then PhotoCraft is barred (the shell skips its logic and refuses its commands), and
    /// every further Develop change joins the step.
    pending: Option<u64>,
    /// Tests: each Pixel move of an undo or redo takes one entry; `true` makes it fail.
    #[cfg(test)]
    pub(crate) fail_moves: std::collections::VecDeque<bool>,
}

impl Default for History {
    fn default() -> Self {
        History {
            steps: Vec::new(),
            redo: Vec::new(),
            bytes_cap: HISTORY_BYTES,
            develop_base: None,
            library: None,
            seen: Vec::new(),
            current: None,
            redo_docs: Vec::new(),
            pixel_bytes: 0,
            clear_pixel_redo: false,
            requested: None,
            notice: None,
            pending: None,
            #[cfg(test)]
            fail_moves: Default::default(),
        }
    }
}

fn same(a: &Pointer, b: &Pointer) -> bool {
    Weak::ptr_eq(a, b)
}

fn starts_with(list: &[Pointer], prefix: &[Pointer]) -> bool {
    list.len() >= prefix.len() && list.iter().zip(prefix).all(|(a, b)| same(a, b))
}

/// `op` sets photo `id`'s develop settings or History somewhere inside.
fn touches(op: &Op, id: PhotoId) -> bool {
    match op {
        Op::SetDevelop { id: x, .. } | Op::SetHistory { id: x, .. } | Op::PushHistory { id: x, .. } => *x == id,
        Op::Batch { ops } => ops.iter().any(|o| touches(o, id)),
        _ => false,
    }
}

impl History {
    /// The panel's rows: the original document, every step, then what can be redone (greyed, the
    /// next redo first). [`History::position`] is the row the document is at.
    pub fn entries(&self) -> Vec<Entry> {
        let row = |s: &Step| Entry { label: s.label.clone(), develop: s.develop.is_some(), pixel: !s.pixel.is_empty() };
        std::iter::once(Entry { label: "Original".into(), develop: false, pixel: false })
            .chain(self.steps.iter().map(row))
            .chain(self.redo.iter().rev().map(row))
            .collect()
    }

    /// The row the document is at: the number of steps done.
    pub fn position(&self) -> usize {
        self.steps.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Bytes Studio's own steps hold (settings pairs and History lists; PhotoCraft's states are
    /// its own, see `pixel_bytes`).
    pub fn studio_bytes(&self) -> usize {
        let mut seen = HashSet::new();
        self.steps.iter().chain(&self.redo).map(|s| s.bytes(&mut seen)).sum()
    }

    /// Forget every step, with `why` as the notice. PhotoCraft's redo goes with it at the next
    /// copy-back (nothing of Studio's leads there any more).
    pub fn clear(&mut self, why: &str) {
        self.steps.clear();
        self.redo.clear();
        self.redo_docs.clear();
        self.clear_pixel_redo = true;
        self.requested = None;
        self.pending = None;
        self.notice = Some(why.to_owned());
    }

    pub(crate) fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// A Develop step waits for its render to land as its Pixel half (see `pending`).
    pub fn pending_follow(&self) -> bool {
        self.pending.is_some()
    }

    /// The label of the step waiting for its follow.
    pub(crate) fn pending_label(&self) -> Option<&str> {
        self.pending.and_then(|_| self.steps.last()).map(|s| s.label.as_str())
    }

    /// The follow landed and was reconciled: the diff joined its state to the Develop step waiting
    /// for it (one step, Develop and Pixel). If it didn't, the history is one Studio can't name.
    pub(crate) fn absorb_follow(&mut self) {
        if self.pending.take().is_some() {
            self.clear(RESET_PIXEL);
        }
    }

    // ---------------------------------------------------------------- Develop

    /// See the catalog's settings for `photo`: a change since the last boundary (and no drag open
    /// on it) is one Develop step, labelled as LightCraft's top undo entry if that entry touches
    /// the photo (its own commit), else "Develop". Returns the settings when they changed since
    /// last seen, for the document to mirror.
    fn note_develop(&mut self, lr: &Session, photo: PhotoId) -> Option<Arc<DevelopSettings>> {
        let library = lr.catalog.revision >> 32;
        if self.library.is_some_and(|l| l != library) {
            self.clear(RESET_LIBRARY);
            self.develop_base = None;
        }
        self.library = Some(library);
        let p = lr.catalog.photo(photo)?;
        let now = p.develop.clone();
        let Some((base, list)) = self.develop_base.clone() else {
            self.develop_base = Some((now.clone(), Arc::new(p.history.clone())));
            return Some(now);
        };
        if Arc::ptr_eq(&base, &now) {
            return None;
        }
        if *base == *now {
            // a drag that ended where it began, say: the same settings in a new allocation
            self.develop_base = Some((now, list));
            return None;
        }
        if lr.interaction.as_ref().is_some_and(|i| i.photo == photo) {
            // a drag: the live base follows, the step is recorded at release
            return Some(now);
        }
        let history_after = Arc::new(p.history.clone());
        if self.pending.is_some() {
            // while the step waits for its render, every Develop change joins it: `after` moves,
            // `before` stays the checkpoint; one render lands, one state is recorded
            let Some(d) = self.steps.last_mut().and_then(|s| s.develop.as_mut()) else {
                self.clear(RESET_PIXEL);
                return Some(now);
            };
            d.after = now.clone();
            d.history_after = history_after.clone();
            self.pending = (framing(&d.before) != framing(&now)).then(|| now.hash64());
            self.develop_base = Some((now.clone(), history_after));
            return Some(now);
        }
        let label = lr.undo.last().filter(|e| touches(&e.op, photo)).map_or("Develop", |e| e.label.as_str()).to_owned();
        let pending = (framing(&base) != framing(&now)).then(|| now.hash64());
        let develop = Develop { before: base, after: now.clone(), history_before: list, history_after: history_after.clone() };
        self.steps.push(Step { label, develop: Some(develop), pixel: Vec::new() });
        self.redo.clear();
        // PhotoCraft's redo follows at the next copy-back
        self.redo_docs.clear();
        self.clear_pixel_redo = true;
        self.develop_base = Some((now.clone(), history_after));
        self.pending = pending;
        self.prune(None);
        Some(now)
    }

    // ---------------------------------------------------------------- Pixel

    /// Take over PhotoCraft's caps for the document (its prefs re-apply 50 / 8 GB otherwise).
    fn own_caps(&self, st: &mut DocState) {
        st.history.max_states = usize::MAX / 2;
        st.history.max_bytes = self.bytes_cap.saturating_sub(self.studio_bytes()).max(1);
    }

    /// Diff PhotoCraft's history for the document `id` against what was last seen (see the module
    /// doc), and keep the budget. Runs after every copy-back and render write.
    fn reconcile(&mut self, px: &mut PixelSession, id: DocId) {
        let Some(st) = state_of(px, id) else { return };
        self.own_caps(st);
        if std::mem::take(&mut self.clear_pixel_redo) {
            st.history.clear_redo();
        }
        let unchanged = |st: &DocState| {
            let now = states(st);
            now.len() == self.seen.len() && starts_with(&now, &self.seen) && self.current.as_ref().is_some_and(|c| same(c, &Arc::downgrade(&st.doc)))
        };
        if unchanged(st) && st.history.redo_labels().count() == self.redo_docs.len() {
            return;
        }
        st.history.trim(&st.doc);
        let (now, cur, redo_count) = (states(st), Arc::downgrade(&st.doc), st.history.redo_labels().count());
        let old = std::mem::take(&mut self.seen);
        let before = self.current.take();
        let ok = self.diff(px, id, &old, before.as_ref(), &now, &cur, redo_count);
        let Some(st) = state_of(px, id) else { return };
        if !ok {
            self.steps.clear();
            self.redo.clear();
            self.redo_docs.clear();
            self.requested = None;
            st.history.clear_redo();
            self.notice = Some(RESET_PIXEL.into());
        }
        self.seen = states(st);
        self.current = Some(Arc::downgrade(&st.doc));
        self.pixel_bytes = st.history.pixel_bytes(&st.doc);
        self.prune(Some(st));
    }

    /// The diff itself: false when it can't name what happened.
    #[allow(clippy::too_many_arguments)]
    fn diff(
        &mut self,
        px: &mut PixelSession,
        id: DocId,
        old: &[Pointer],
        before: Option<&Pointer>,
        now: &[Pointer],
        cur: &Pointer,
        redo_count: usize,
    ) -> bool {
        // a lost prefix is PhotoCraft dropping its oldest states (its cap or a trim): states go
        // from the front nowhere else. Every one of them may go, when an edit's own `record`
        // and trim leave only the states it just made: the first of those is the document that
        // was current.
        let lost = match now.first() {
            None => 0,
            Some(first) => match old.iter().position(|p| same(p, first)) {
                Some(s) => s,
                None if old.is_empty() => 0,
                None if before.is_some_and(|b| same(b, first)) => old.len(),
                None => return false,
            },
        };
        if lost > 0 {
            // every step older than the oldest surviving state goes, Develop steps included: a
            // contiguous prefix of the whole history, never a hole. A surviving state no step owns
            // yet is new (every old one went): all steps are older, but the one waiting for it
            let waiting = usize::from(self.pending.is_some());
            let keep = now
                .first()
                .and_then(|first| self.steps.iter().position(|s| s.pixel.iter().any(|p| same(p, first))))
                .unwrap_or(self.steps.len().saturating_sub(waiting));
            self.steps.drain(..keep);
        }
        let old = &old[lost..];
        if starts_with(now, old) {
            let added = &now[old.len()..];
            if added.is_empty() {
                // the same states, another current document: a host replacement or PhotoCraft's own
                // coalescing of the current step, not an edit; redo stays. Unless PhotoCraft let
                // its redo go (its own `clear_redo`): then Studio's redo leads nowhere and goes too
                if redo_count < self.redo_docs.len() {
                    self.redo.clear();
                    self.redo_docs.clear();
                } else if redo_count > self.redo_docs.len() {
                    return false;
                }
                if before.is_none_or(|b| !same(b, cur))
                    && let Some(label) = state_of(px, id).and_then(|st| st.history.undo_label().map(str::to_owned))
                    && let Some(s) =
                        self.steps.last_mut().filter(|s| s.pixel_only() && s.pixel.last().is_some_and(|p| now.last().is_some_and(|n| same(p, n))))
                {
                    s.label = label;
                }
                return true;
            }
            let k = added.len();
            let target = self.redo_docs.len().checked_sub(k).map(|i| self.redo_docs[i].clone());
            if let Some(target) = target.filter(|t| same(t, cur) && redo_count + k == self.redo_docs.len()) {
                // PhotoCraft redid k states itself: the next Studio redo steps, if all Pixel. The
                // states it pushed are the documents that were current (a host may have replaced
                // one since the step recorded it), so the steps take their pointers afresh.
                let (mut n, mut got, mut develop) = (0, 0, false);
                for s in self.redo.iter().rev() {
                    if got >= k {
                        break;
                    }
                    n += 1;
                    got += s.pixel.len();
                    develop |= !s.pixel_only();
                }
                if got == k && !develop {
                    let at = self.redo.len() - n;
                    let mut back: Vec<_> = self.redo.drain(at..).rev().collect();
                    let mut fresh = added.iter();
                    for p in back.iter_mut().flat_map(|s| s.pixel.iter_mut()) {
                        if let Some(q) = fresh.next() {
                            *p = q.clone();
                        }
                    }
                    self.steps.extend(back);
                    self.redo_docs.truncate(self.redo_docs.len() - k);
                    return true;
                }
                // past a Develop step: put back, and jump as Studio does (Develop steps included)
                if !(0..k).all(|_| px.undo()) {
                    return false;
                }
                self.requested = Some(Jump::Redo(target));
                return true;
            }
            if redo_count != 0 {
                return false;
            }
            // the follow a Develop step waits for (PhotoCraft is barred meanwhile, so the one
            // state appended is it): it is that step's Pixel half, joined here, before anything
            // (the budget's prune) could take the step for a Develop-only one
            if self.pending.is_some()
                && let [follow] = added
                && let Some(s) = self.steps.last_mut().filter(|s| s.develop.is_some() && s.pixel.is_empty())
            {
                s.pixel = vec![follow.clone()];
                self.pending = None;
                self.redo.clear();
                self.redo_docs.clear();
                return true;
            }
            // new edits: one step per state, labelled by the step that made the next state
            let labels = state_of(px, id).map(|st| st.history.entries()).unwrap_or_default();
            for (i, p) in added.iter().enumerate() {
                let label = labels.get(old.len() + i + 1).cloned().unwrap_or_default();
                self.steps.push(Step { label, develop: None, pixel: vec![p.clone()] });
            }
            self.redo.clear();
            self.redo_docs.clear();
            return true;
        }
        if !starts_with(old, now) {
            return false;
        }
        let removed = &old[now.len()..];
        let k = removed.len();
        let Some(n) = suffix_of(&self.steps, removed) else { return false };
        if redo_count == self.redo_docs.len() + k {
            // PhotoCraft undid k states itself: the current document is the oldest removed state
            if !same(cur, &removed[0]) {
                return false;
            }
            if let Some(n) = n {
                let at = self.steps.len() - n;
                let undone: Vec<_> = self.steps.drain(at..).rev().collect();
                self.redo.extend(undone);
                // what became redoable: the document before, then each removed state but the oldest
                self.redo_docs.extend(before.into_iter().cloned().chain(removed[1..].iter().rev().cloned()));
                return true;
            }
            if !(0..k).all(|_| px.redo()) {
                return false;
            }
            self.requested = Some(Jump::Undo(removed[0].clone()));
            return true;
        }
        if redo_count != 0 {
            return false;
        }
        // a fold (`purge_last` + relabel) or a refused edit (M4 `refuse`): the steps go; a fold's
        // label lands on the step it folded into, which must be a Pixel step too
        let Some(n) = n else { return false };
        let folded = before.is_some_and(|b| same(b, cur));
        let at = self.steps.len() - n;
        self.steps.truncate(at);
        if folded && !now.is_empty() {
            let Some(s) = self.steps.last_mut().filter(|s| s.pixel_only()) else { return false };
            if let Some(label) = state_of(px, id).and_then(|st| st.history.undo_label().map(str::to_owned)) {
                s.label = label;
            }
        }
        self.redo.clear();
        self.redo_docs.clear();
        true
    }

    /// Keep the budget: Develop-only steps go from the oldest end at once; a step owning a Pixel
    /// state goes when PhotoCraft drops the state (its next `record`, cap lowered here).
    fn prune(&mut self, st: Option<&mut DocState>) {
        let over = |h: &History| h.steps.len() > HISTORY_STEPS || h.studio_bytes().saturating_add(h.pixel_bytes) > h.bytes_cap;
        // a step waiting for its follow (always the newest) owns no state yet, but isn't Develop-only
        let waiting = usize::from(self.pending.is_some());
        while over(self) && self.steps.len() > waiting && self.steps.first().is_some_and(|s| s.pixel.is_empty()) {
            self.steps.remove(0);
        }
        if let Some(st) = st {
            self.own_caps(st);
            if self.steps.len() > HISTORY_STEPS {
                st.history.max_states = st.history.past_len().saturating_sub(1).max(1);
            }
        }
    }
}

/// How many of the newest `steps` own exactly the states `wanted` (oldest first): `Some(Some(n))`
/// when they are all Pixel-only, `Some(None)` when a Develop step is among them, `None` when the
/// states aren't theirs.
fn suffix_of(steps: &[Step], wanted: &[Pointer]) -> Option<Option<usize>> {
    let (mut owned, mut n, mut develop) = (Vec::new(), 0, false);
    for s in steps.iter().rev() {
        if owned.len() >= wanted.len() {
            break;
        }
        n += 1;
        develop |= !s.pixel_only();
        owned.splice(0..0, s.pixel.iter().cloned());
    }
    (owned.len() == wanted.len() && starts_with(&owned, wanted)).then_some((!develop).then_some(n))
}

/// PhotoCraft's past states, oldest first.
fn states(st: &DocState) -> Vec<Pointer> {
    (0..st.history.past_len()).filter_map(|i| st.history.state(i)).map(|d| Arc::downgrade(&d)).collect()
}

/// Both halves of `step` can move now: no background job holds PhotoCraft's document, no slider
/// drag is open on the photo.
fn ready(lr: &Session, px: &PixelSession, photo: PhotoId, step: &Step) -> Result<(), String> {
    if step.develop.is_some() && lr.interaction.as_ref().is_some_and(|i| i.photo == photo) {
        return Err("a Develop slider is still being dragged: nothing was undone".into());
    }
    match px.active_job() {
        Some(job) if !step.pixel.is_empty() => Err(format!("PhotoCraft is busy ({}): nothing was undone", job.label)),
        _ => Ok(()),
    }
}

impl Document {
    /// Mirror the catalog's settings for the document's photo (the base follows them), noting a
    /// change as a Develop step. Once per frame, and before anything decides on the history.
    pub fn note_develop(&mut self, lr: &Session) {
        if let Some(now) = self.history.note_develop(lr, self.source.photo)
            && *now != self.develop
        {
            self.develop = (*now).clone();
            self.revision += 1;
        }
    }

    pub(crate) fn reconcile_pixel(&mut self, px: &mut PixelSession) {
        if let Some(id) = self.link.id() {
            self.history.reconcile(px, id);
            self.link.forget_gone();
        }
    }

    /// Copy back what PhotoCraft changed, and finish a jump it began itself (its own undo or redo
    /// past a Develop step): after its `logic` and after every command.
    pub fn reconcile(&mut self, lr: &mut Session, px: &mut PixelSession) -> Result<(), String> {
        self.copy_back(px);
        let Some(jump) = self.history.requested.take() else { return Ok(()) };
        let at = |h: &History, p: &Pointer| h.current.as_ref().is_some_and(|c| same(c, p));
        // the target is still ahead: a state some step owns, or a document on PhotoCraft's redo
        let owns = |h: &History, p: &Pointer| h.steps.iter().any(|s| s.pixel.iter().any(|q| same(q, p)));
        let ahead = |h: &History, p: &Pointer| h.redo_docs.iter().any(|d| same(d, p));
        match jump {
            Jump::Undo(p) if owns(&self.history, &p) => while !at(&self.history, &p) && owns(&self.history, &p) && self.undo_one(lr, px)? {},
            Jump::Redo(p) if ahead(&self.history, &p) => while !at(&self.history, &p) && ahead(&self.history, &p) && self.redo_one(lr, px)? {},
            _ => self.history.clear(RESET_PIXEL),
        }
        self.copy_back(px);
        Ok(())
    }

    /// Settle everything before a decision on the document: a slider drag open on the photo ends
    /// (and is its step); a Develop step still waiting for its render (a geometry change) waits
    /// for it here and lands it as its Pixel half, through `base`, which renders it (up to the
    /// ceiling; one that fails or takes longer undoes the step and is the error); PhotoCraft's
    /// edits are copied back and reconciled. Before every undo, redo, jump, save, close and
    /// persona switch: no caller can decide on a geometry change half done.
    pub fn flush(&mut self, lr: &mut Session, px: &mut PixelSession, base: &mut Base) -> Result<(), String> {
        base.finish(lr, px, self)?;
        self.reconcile(lr, px)
    }

    /// Undo the last step, in whichever engine made it. False when there is none.
    pub fn undo(&mut self, lr: &mut Session, px: &mut PixelSession, base: &mut Base) -> Result<bool, String> {
        self.flush(lr, px, base)?;
        let done = self.undo_one(lr, px)?;
        self.copy_back(px);
        Ok(done)
    }

    pub fn redo(&mut self, lr: &mut Session, px: &mut PixelSession, base: &mut Base) -> Result<bool, String> {
        self.flush(lr, px, base)?;
        let done = self.redo_one(lr, px)?;
        self.copy_back(px);
        Ok(done)
    }

    /// Go to row `index` of [`History::entries`] (0 = the original document), undoing or redoing
    /// as many steps as it takes; the engines move first, one copy-back at the end.
    pub fn goto(&mut self, lr: &mut Session, px: &mut PixelSession, base: &mut Base, index: usize) -> Result<(), String> {
        self.flush(lr, px, base)?;
        if index > self.history.steps.len() + self.history.redo.len() {
            return Err(format!("no history row {index}"));
        }
        while self.history.steps.len() > index && self.undo_one(lr, px)? {}
        while self.history.steps.len() < index && self.redo_one(lr, px)? {}
        self.copy_back(px);
        Ok(())
    }

    /// Commit a Develop half to LightCraft (journaled: it survives a restart). A failure leaves
    /// LightCraft as it was (`commit` applies the op or nothing).
    fn commit_develop(&mut self, lr: &mut Session, label: &str, settings: &Arc<DevelopSettings>, list: &Arc<Vec<HistoryStep>>) -> Result<(), String> {
        let id = self.source.photo;
        let edited = lr.catalog.photo(id).and_then(|p| p.edited.clone());
        let ops =
            vec![Op::SetDevelop { id, settings: settings.clone(), label: label.into(), edited }, Op::SetHistory { id, history: (**list).clone() }];
        lr.commit(label, Op::Batch { ops }).map_err(|e| e.to_string())?;
        self.history.develop_base = Some((settings.clone(), list.clone()));
        self.develop = (**settings).clone();
        Ok(())
    }

    /// The render for the pending Develop step failed: the step is dropped, the settings and the
    /// photo's History go back to the checkpoint before it (the Pixel document is untouched: no
    /// state was recorded), and the notice says so.
    pub fn follow_failed(&mut self, lr: &mut Session, why: &str) {
        if self.history.pending.take().is_none() {
            return;
        }
        let Some(step) = self.history.steps.pop() else { return };
        if let Some(d) = &step.develop
            && let Err(e) = self.commit_develop(lr, &format!("Undo {}", step.label), &d.before, &d.history_before)
        {
            self.history.clear(&format!("History reset: {e}"));
            return;
        }
        self.revision += 1;
        self.history.notice = Some(format!("Develop couldn\u{2019}t render \u{201c}{}\u{201d}: the edit was undone ({why})", step.label));
    }

    /// One of a step's Pixel states undone in PhotoCraft, the register following.
    fn pixel_undo(&mut self, px: &mut PixelSession) -> Result<(), String> {
        #[cfg(test)]
        if self.history.fail_moves.pop_front() == Some(true) {
            return Err("PhotoCraft refused the move (test)".into());
        }
        if !px.undo() {
            return Err("PhotoCraft is busy: nothing was undone".into());
        }
        let Some(st) = self.link.id().and_then(|id| state_of(px, id)) else { return Err("PhotoCraft lost the document".into()) };
        let h = &mut self.history;
        h.redo_docs.extend(h.current.take());
        h.seen.pop();
        h.current = Some(Arc::downgrade(&st.doc));
        Ok(())
    }

    /// The mirror of [`Document::pixel_undo`]. A redone state is the document that was current,
    /// which a host write may have replaced since the step recorded it: `p` takes the pointer
    /// afresh.
    fn pixel_redo(&mut self, px: &mut PixelSession, p: &mut Pointer) -> Result<(), String> {
        #[cfg(test)]
        if self.history.fail_moves.pop_front() == Some(true) {
            return Err("PhotoCraft refused the move (test)".into());
        }
        if !px.redo() {
            return Err("PhotoCraft is busy: nothing was redone".into());
        }
        let Some(st) = self.link.id().and_then(|id| state_of(px, id)) else { return Err("PhotoCraft lost the document".into()) };
        let h = &mut self.history;
        if let Some(pushed) = h.current.take() {
            *p = pushed.clone();
            h.seen.push(pushed);
        }
        h.redo_docs.pop();
        h.current = Some(Arc::downgrade(&st.doc));
        Ok(())
    }

    /// One step back, once both halves can move: its Pixel states undone in PhotoCraft (newest
    /// first), then its Develop half committed. Any of that failing never loses the step: see
    /// [`Document::keep_undo`]. No copy-back here (a jump copies once at the end).
    fn undo_one(&mut self, lr: &mut Session, px: &mut PixelSession) -> Result<bool, String> {
        // a jump PhotoCraft began reaches here past `flush`: never into a step half recorded
        if self.history.pending_follow() {
            return Err(RENDERING.into());
        }
        let Some(step) = self.history.steps.pop() else { return Ok(false) };
        if let Err(e) = ready(lr, px, self.source.photo, &step) {
            self.history.steps.push(step);
            return Err(e);
        }
        for moved in 0..step.pixel.len() {
            if let Err(e) = self.pixel_undo(px) {
                return self.keep_undo(px, step, moved, e);
            }
        }
        if let Some(d) = step.develop.clone()
            && let Err(e) = self.commit_develop(lr, &format!("Undo {}", step.label), &d.before, &d.history_before)
        {
            let moved = step.pixel.len();
            return self.keep_undo(px, step, moved, e);
        }
        self.revision += 1;
        self.history.redo.push(step);
        Ok(true)
    }

    /// The undo of `step` failed (`why`) with its `moved` newest states undone (its Develop half
    /// never moved: it goes last). They are redone, so both engines are back at the step, which
    /// is kept, and the notice says why; if they can't be, no step can say where the engines are,
    /// and the history starts over (fail safe), with a notice. Either way, `why` is the error.
    fn keep_undo(&mut self, px: &mut PixelSession, mut step: Step, moved: usize, why: String) -> Result<bool, String> {
        let from = step.pixel.len().saturating_sub(moved);
        for p in &mut step.pixel[from..] {
            if let Err(e) = self.pixel_redo(px, p) {
                self.history.clear(&format!("History reset: \u{201c}{}\u{201d} couldn\u{2019}t be undone ({why}) nor put back ({e})", step.label));
                return Err(why);
            }
        }
        self.history.notice = Some(format!("Couldn\u{2019}t undo \u{201c}{}\u{201d}: {why}", step.label));
        self.history.steps.push(step);
        Err(why)
    }

    /// The mirror of [`Document::undo_one`]: the states redone (oldest first), then the Develop
    /// half; any of that failing never loses the step: see [`Document::keep_redo`].
    fn redo_one(&mut self, lr: &mut Session, px: &mut PixelSession) -> Result<bool, String> {
        if self.history.pending_follow() {
            return Err(RENDERING.into());
        }
        let Some(mut step) = self.history.redo.pop() else { return Ok(false) };
        if let Err(e) = ready(lr, px, self.source.photo, &step) {
            self.history.redo.push(step);
            return Err(e);
        }
        for moved in 0..step.pixel.len() {
            if let Err(e) = self.pixel_redo(px, &mut step.pixel[moved]) {
                return self.keep_redo(px, step, moved, e);
            }
        }
        if let Some(d) = step.develop.clone()
            && let Err(e) = self.commit_develop(lr, &format!("Redo {}", step.label), &d.after, &d.history_after)
        {
            let moved = step.pixel.len();
            return self.keep_redo(px, step, moved, e);
        }
        self.revision += 1;
        self.history.steps.push(step);
        Ok(true)
    }

    /// The redo of `step` failed (`why`) with its `moved` oldest states redone: they are undone,
    /// and the step stays on the redo side, with a notice; failing that, the history starts over.
    fn keep_redo(&mut self, px: &mut PixelSession, step: Step, moved: usize, why: String) -> Result<bool, String> {
        for _ in 0..moved {
            if let Err(e) = self.pixel_undo(px) {
                self.history.clear(&format!("History reset: \u{201c}{}\u{201d} couldn\u{2019}t be redone ({why}) nor put back ({e})", step.label));
                return Err(why);
            }
        }
        self.history.notice = Some(format!("Couldn\u{2019}t redo \u{201c}{}\u{201d}: {why}", step.label));
        self.history.redo.push(step);
        Err(why)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pixel::tests::{TILE, anchor, attached, names, paint, render_sized};

    /// A document on the first demo photo, attached to PhotoCraft, its history started.
    fn open() -> (Session, PixelSession, Document) {
        let (lc, _) = crate::tests::demo();
        let (px, mut doc) = attached();
        doc.note_develop(&lc);
        (lc, px, doc)
    }

    /// One frame as the shell runs it: the settings mirrored, PhotoCraft copied back.
    fn tick(doc: &mut Document, lr: &Session, px: &mut PixelSession) {
        doc.note_develop(lr);
        doc.copy_back(px);
    }

    fn set_exposure(lr: &mut Session, id: PhotoId, exposure: f64) {
        let mut s = (*lr.develop_of(id).unwrap()).clone();
        s.light.exposure = exposure;
        lr.set_develop(id, s, "Exposure").unwrap();
    }

    fn exposure(lr: &Session, id: PhotoId) -> f64 {
        lr.develop_of(id).unwrap().light.exposure
    }

    /// The photo's History panel rows.
    fn rows(lr: &Session, id: PhotoId) -> Vec<String> {
        lr.catalog.photo(id).unwrap().history.iter().map(|s| s.label.clone()).collect()
    }

    fn labels(doc: &Document) -> Vec<String> {
        doc.history.entries().into_iter().map(|e| e.label).collect()
    }

    fn layers(doc: &Document) -> Vec<String> {
        names(&doc.pixel).into_iter().map(str::to_owned).collect()
    }

    fn pc_entries(px: &PixelSession) -> Vec<String> {
        px.active().unwrap().history.entries()
    }

    #[test]
    fn develop_and_pixel_steps_undo_in_reverse_order_and_redo_again() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        let h0 = rows(&lr, id);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        let h1 = rows(&lr, id);
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 2.0);
        tick(&mut doc, &lr, &mut px);
        let h2 = rows(&lr, id);
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(labels(&doc), ["Original", "Exposure", "Brush", "Exposure", "Pencil"]);
        assert_eq!(doc.history.position(), 4);
        let rev = doc.revision;
        // back, newest first, from whichever engine made the step
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((layers(&doc), exposure(&lr, id), rows(&lr, id)), (vec!["Develop".to_owned(), "Brush".into()], 2.0, h2.clone()));
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), doc.develop.light.exposure, rows(&lr, id)), (1.0, 1.0, h1.clone()));
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((layers(&doc), exposure(&lr, id)), (vec!["Develop".to_owned()], 1.0));
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), rows(&lr, id), doc.history.position()), (0.0, h0, 0));
        assert!(!doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap(), "nothing left");
        assert!(doc.revision > rev, "undo is an edit");
        assert_eq!(labels(&doc), ["Original", "Exposure", "Brush", "Exposure", "Pencil"], "the rows stay, greyed");
        // and forward again
        for (i, (want_layers, want_exposure)) in [(1, 1.0), (2, 1.0), (2, 2.0), (3, 2.0)].into_iter().enumerate() {
            assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap(), "redo {i}");
            assert_eq!((layers(&doc).len(), exposure(&lr, id)), (want_layers, want_exposure), "redo {i}");
        }
        assert!(!doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((rows(&lr, id).len(), doc.history.position()), (h2.len(), 4));
        assert_eq!(doc.take_notice(), None);
    }

    /// A 1×1 PNG.
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01,
        0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    /// A library on disk with one imported PNG, and a session on it.
    pub(crate) fn on_disk(tag: &str) -> (std::path::PathBuf, Session, PhotoId) {
        let dir = std::env::temp_dir().join(format!("studio-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("one.png");
        std::fs::write(&png, PNG).unwrap();
        let mut lr = Session::new().with_fs().with_system_clock();
        lr.open_library(dir.join("lib"), false).unwrap();
        let r = lr.execute("library.import", &serde_json::json!({"paths": [png.to_string_lossy()]})).unwrap();
        (dir, lr, PhotoId(r["imported"][0].as_u64().unwrap()))
    }

    #[test]
    fn an_undone_develop_edit_stays_undone_in_a_new_session_on_the_same_library() {
        let (dir, mut lr, id) = on_disk("journal");
        let mut doc = Document::from_photo(&lr, id).unwrap();
        let mut px = PixelSession::new();
        doc.attach(&mut px);
        tick(&mut doc, &lr, &mut px);
        let before = rows(&lr, id);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(rows(&lr, id).len(), before.len() + 1);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), rows(&lr, id)), (0.0, before.clone()));
        lr.persist().unwrap();
        lr.close_library().unwrap();
        drop(lr);
        // the undo went through `commit`, so it is in the journal
        let mut again = Session::new().with_fs().with_system_clock();
        again.open_library(dir.join("lib"), false).unwrap();
        let id = again.catalog.photos().next().unwrap().id;
        assert_eq!((exposure(&again, id), rows(&again, id)), (0.0, before));
        again.close_library().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slider_drag_is_one_step_from_the_pre_drag_settings_and_a_history_click_is_a_new_step() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        lr.selection.active = Some(id);
        lr.begin_interaction("Exposure").unwrap();
        for e in [0.5, 1.0, 1.5] {
            set_exposure(&mut lr, id, e);
            tick(&mut doc, &lr, &mut px);
            assert_eq!((doc.develop.light.exposure, doc.history.position()), (e, 0), "mirrored live, no step while dragging");
        }
        lr.end_interaction().unwrap();
        tick(&mut doc, &lr, &mut px);
        assert_eq!(labels(&doc), ["Original", "Exposure"]);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(exposure(&lr, id), 0.0, "back to before the drag, not to a notch of it");
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(exposure(&lr, id), 1.5);
        // a second edit, then a click on the first row of LightCraft's History panel
        let mut s = (*lr.develop_of(id).unwrap()).clone();
        s.light.contrast = 20.0;
        lr.set_develop(id, s, "Contrast").unwrap();
        tick(&mut doc, &lr, &mut px);
        let rows_before = rows(&lr, id);
        let first = rows_before.len() - 2;
        lr.execute("history.restore", &serde_json::json!({"index": first})).unwrap();
        tick(&mut doc, &lr, &mut px);
        assert_eq!(labels(&doc), ["Original", "Exposure", "Contrast", "History: Exposure"]);
        assert_eq!((exposure(&lr, id), lr.develop_of(id).unwrap().light.contrast), (1.5, 0.0));
        assert_eq!(rows(&lr, id).len(), rows_before.len() + 1, "a restore appends a row");
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((lr.develop_of(id).unwrap().light.contrast, rows(&lr, id)), (20.0, rows_before), "the row is gone again");
    }

    #[test]
    fn undo_during_a_slider_drag_ends_it_as_one_step_then_undoes_it() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        lr.selection.active = Some(id);
        lr.begin_interaction("Exposure").unwrap();
        set_exposure(&mut lr, id, 0.5);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert!(lr.interaction.is_none(), "the drag ended");
        assert_eq!((exposure(&lr, id), doc.develop.light.exposure), (0.0, 0.0));
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), labels(&doc)), (1.0, vec!["Original".to_owned(), "Exposure".into()]));
    }

    #[test]
    fn an_edit_in_either_engine_clears_the_others_redo() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert!(px.active().unwrap().history.can_redo() && doc.history.can_redo());
        // a Develop edit after a Pixel undo
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert!(!doc.history.can_redo() && !px.active().unwrap().history.can_redo(), "PhotoCraft's redo went too");
        assert_eq!(labels(&doc), ["Original", "Exposure"]);
        // a Pixel edit after a Develop undo
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert!(doc.history.can_redo());
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        assert!(!doc.history.can_redo());
        assert_eq!(labels(&doc), ["Original", "Pencil"]);
        assert!(!doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn a_rating_an_import_and_opening_a_file_add_no_step_and_undo_leaves_them() {
        let (dir, mut lr, id) = on_disk("nosteps");
        let mut doc = Document::from_photo(&lr, id).unwrap();
        let mut px = PixelSession::new();
        doc.attach(&mut px);
        tick(&mut doc, &lr, &mut px);
        lr.selection.active = Some(id);
        lr.execute("photo.rate", &serde_json::json!({"rating": 3, "ids": [id.0]})).unwrap();
        let png = dir.join("two.png");
        std::fs::write(&png, PNG).unwrap();
        lr.execute("library.import", &serde_json::json!({"paths": [png.to_string_lossy()]})).unwrap();
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.history.position(), 0);
        assert!(!doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(lr.catalog.photo(id).unwrap().rating, 3, "untouched");
        // a file whose settings differ from the library's: opened as "Open x.studio", not a step
        let bytes = doc.to_bytes().unwrap();
        set_exposure(&mut lr, id, 1.0);
        let mut opened = Document::from_bytes(&bytes).unwrap();
        opened.open_into(&mut lr).unwrap();
        opened.attach(&mut px);
        tick(&mut opened, &lr, &mut px);
        assert_eq!((exposure(&lr, id), opened.history.position()), (0.0, 0));
        assert_eq!(lr.undo.last().map(|e| e.label.as_str()), Some("Open one.studio"), "LightCraft's own stack has it");
        assert!(!opened.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), lr.catalog.photo(id).unwrap().rating), (0.0, 3));
        lr.close_library().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_in_another_document_leaves_the_first_alone() {
        let (mut lr, mut px, mut a) = open();
        let id_a = a.source.photo;
        let id_b = lr.catalog.photos().map(|p| p.id).find(|&p| p != id_a).unwrap();
        set_exposure(&mut lr, id_a, 1.0);
        tick(&mut a, &lr, &mut px);
        assert_eq!(a.history.position(), 1);
        let mut b = Document::from_photo(&lr, id_b).unwrap();
        b.attach(&mut px);
        tick(&mut b, &lr, &mut px);
        set_exposure(&mut lr, id_b, 0.5);
        tick(&mut b, &lr, &mut px);
        assert!(b.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id_b), exposure(&lr, id_a)), (0.0, 1.0));
        assert!(!b.undo(&mut lr, &mut px, &mut Base::new()).unwrap(), "A's step isn't B's");
        assert_eq!(exposure(&lr, id_a), 1.0);
    }

    #[test]
    fn a_develop_edit_to_another_photo_is_no_step_and_undo_touches_neither() {
        let (mut lr, mut px, mut doc) = open();
        let id_a = doc.source.photo;
        let id_b = lr.catalog.photos().map(|p| p.id).find(|&p| p != id_a).unwrap();
        set_exposure(&mut lr, id_b, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.history.position(), 0);
        assert!(!doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id_a), exposure(&lr, id_b)), (0.0, 1.0));
        // its drag doesn't hold ours back either
        lr.selection.active = Some(id_b);
        lr.begin_interaction("Exposure").unwrap();
        set_exposure(&mut lr, id_a, 2.0);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(labels(&doc), ["Original", "Exposure"]);
        lr.cancel_interaction().unwrap();
    }

    #[test]
    fn photocrafts_own_undo_is_studios_for_pixel_steps_and_a_jump_past_a_develop_step() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        // PhotoCraft undoes and redoes by itself (a scripted `edit.undo`): the newest steps are
        // Pixel, so that simply is Studio's undo and redo
        assert!(px.undo());
        doc.reconcile(&mut lr, &mut px).unwrap();
        assert_eq!((doc.history.position(), layers(&doc).len(), doc.history.can_redo()), (1, 2, true));
        assert!(px.redo());
        doc.reconcile(&mut lr, &mut px).unwrap();
        assert_eq!((doc.history.position(), layers(&doc).len(), doc.history.can_redo()), (2, 3, false));
        // with a Develop step after them, PhotoCraft's undo is put back and done as a Studio jump
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        px.execute("edit.undo", serde_json::json!({})).unwrap();
        doc.reconcile(&mut lr, &mut px).unwrap();
        assert_eq!((layers(&doc), exposure(&lr, id)), (vec!["Develop".to_owned(), "Brush".into()], 0.0), "Develop step undone too");
        assert_eq!((doc.history.position(), labels(&doc)), (1, vec!["Original".to_owned(), "Brush".into(), "Pencil".into(), "Exposure".into()]));
        assert_eq!(pc_entries(&px), ["Open", "Brush"]);
        assert_eq!(doc.take_notice(), None);
        // its redo brings back the Pixel step, which is the next Studio redo: no jump needed
        px.execute("edit.redo", serde_json::json!({})).unwrap();
        doc.reconcile(&mut lr, &mut px).unwrap();
        assert_eq!((layers(&doc).len(), exposure(&lr, id), doc.history.position()), (3, 0.0, 2));
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), doc.history.position()), (1.0, 3));
        // the mirror case: P1 D P2 undone to P1, then PhotoCraft redoes P2 by itself, past D
        let (mut lr, mut px, mut doc) = open();
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        doc.goto(&mut lr, &mut px, &mut Base::new(), 1).unwrap();
        assert_eq!((layers(&doc).len(), exposure(&lr, id)), (2, 0.0));
        px.execute("edit.redo", serde_json::json!({})).unwrap();
        doc.reconcile(&mut lr, &mut px).unwrap();
        assert_eq!((layers(&doc).len(), exposure(&lr, id), doc.history.position()), (3, 1.0, 3), "Develop step redone too");
        assert_eq!(pc_entries(&px), ["Open", "Brush", "Pencil"]);
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn a_fold_of_pixel_steps_is_merged_a_refused_edit_leaves_the_list_and_a_fold_across_develop_resets() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        // Edit › Purge › Undo folds the two Pixel steps into one, relabelled
        let st = px.active_mut().unwrap();
        assert!(st.history.purge_last());
        st.history.set_current_label("Both");
        tick(&mut doc, &lr, &mut px);
        assert_eq!(labels(&doc), ["Original", "Both"]);
        assert_eq!(doc.take_notice(), None);
        // a refused edit (the Develop layer deleted) leaves the list as it was
        let base = doc.base_layer().unwrap().id.0;
        px.execute("layer.delete", serde_json::json!({"layer": base})).unwrap();
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.take_notice().as_deref(), Some(crate::pixel::LOCKED));
        assert_eq!((labels(&doc), pc_entries(&px)), (vec!["Original".to_owned(), "Both".into()], vec!["Open".to_owned(), "Both".into()]));
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(layers(&doc), ["Develop"], "the fold undoes to before both");
        // P1 D P2, then P2 folded into P1: the fold reaches across the Develop step
        let (mut lr, mut px, mut doc) = open();
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap() && doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert!(px.active_mut().unwrap().history.purge_last());
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.take_notice().as_deref(), Some(RESET_PIXEL));
        assert_eq!((labels(&doc), doc.history.can_redo()), (vec!["Original".to_owned()], false));
        assert!(!px.active().unwrap().history.can_redo());
        assert!(!doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(doc.take_notice(), None, "said once");
    }

    #[test]
    fn a_library_change_clears_the_history_with_a_notice() {
        let (dir, mut lr, id) = on_disk("library");
        let mut doc = Document::from_photo(&lr, id).unwrap();
        let mut px = PixelSession::new();
        doc.attach(&mut px);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.history.position(), 1);
        lr.open_library(dir.join("other"), false).unwrap();
        tick(&mut doc, &lr, &mut px);
        assert_eq!((doc.history.position(), doc.take_notice().as_deref()), (0, Some(RESET_LIBRARY)));
        assert!(!doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        lr.close_library().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn retained(px: &PixelSession) -> usize {
        let st = px.active().unwrap();
        st.history.pixel_bytes(&st.doc)
    }

    #[test]
    fn with_a_small_cap_old_states_go_and_studio_steps_lose_a_contiguous_prefix() {
        let (mut lr, mut px, mut doc) = open();
        // every paint layer and every render is one tile; the current document (8 paints and
        // its render) fits, the older renders don't all
        let cap = 12 * TILE;
        doc.history.bytes_cap = cap;
        for i in 0..8 {
            paint(&mut px, &format!("Paint {i}"), i, i);
            tick(&mut doc, &lr, &mut px);
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut px, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        let st = px.active().unwrap();
        assert_eq!((st.history.max_states, st.history.max_bytes), (usize::MAX / 2, cap - doc.history.studio_bytes()), "Studio owns the caps");
        assert!(retained(&px) <= cap + TILE, "{} bytes: over the cap plus the newest state", retained(&px));
        let kept = st.history.past_len();
        assert!((1..8).contains(&kept), "{kept} states kept");
        // the register forgot the renders no state shows (the empty base before the first render
        // and the current render stay)
        assert_eq!(doc.link.written_len(), kept + 2);
        let want: Vec<String> = (8 - kept..8).map(|i| format!("Paint {i}")).collect();
        assert_eq!(labels(&doc)[1..], want[..], "the oldest steps went, as a prefix");
        // PhotoCraft's prefs re-applying its own caps are overridden on the next copy-back
        let st = px.active_mut().unwrap();
        (st.history.max_states, st.history.max_bytes) = (50, 8 << 30);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(px.active().unwrap().history.max_states, usize::MAX / 2);
        // redo states count and survive a trim
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        for _ in 0..4 {
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut px, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        assert!(doc.history.can_redo() && px.active().unwrap().history.can_redo());
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn five_thousand_exposure_edits_stay_under_budget_and_share_their_history_lists() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        let value = |i: usize| (i % 97) as f64 * 0.01 + 0.01;
        for i in 0..5000 {
            set_exposure(&mut lr, id, value(i));
            doc.note_develop(&lr);
        }
        tick(&mut doc, &lr, &mut px);
        assert_eq!(rows(&lr, id).len(), 200, "LightCraft's own list is full");
        assert_eq!(doc.history.position(), HISTORY_STEPS);
        let oldest = doc.history.steps[0].develop.as_ref().unwrap();
        assert_eq!((oldest.before.light.exposure, oldest.after.light.exposure), (value(3999), value(4000)), "a contiguous prefix was pruned");
        let bytes = doc.history.studio_bytes();
        assert!(bytes < 64 << 20, "{bytes} bytes for {HISTORY_STEPS} steps");
        let shared = doc.history.steps.windows(2).all(|w| {
            let (a, b) = (w[0].develop.as_ref().unwrap(), w[1].develop.as_ref().unwrap());
            Arc::ptr_eq(&a.history_after, &b.history_before)
        });
        assert!(shared, "neighbouring steps share one History list");
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(exposure(&lr, id), value(4998));
    }

    #[test]
    fn redo_survives_a_render_landing_after_an_undo() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        // the render for the undone settings lands: a host replacement, not an edit
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut px, r, s, anchor(1, 0.0, 0.0)).unwrap();
        tick(&mut doc, &lr, &mut px);
        assert!(doc.history.can_redo());
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(exposure(&lr, id), 1.0);
        // a render under Pixel steps the same: the current label is adopted, nothing else
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        let (r, s) = render_sized(64, 48);
        doc.write_base(&mut px, r, s, anchor(1, 0.0, 0.0)).unwrap();
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(layers(&doc), ["Develop", "Brush"]);
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn goto_jumps_to_any_row_with_the_engines_moved_first() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 2.0);
        tick(&mut doc, &lr, &mut px);
        let rows = doc.history.entries();
        assert_eq!(rows.iter().map(|e| (e.develop, e.pixel)).collect::<Vec<_>>(), [(false, false), (true, false), (false, true), (true, false)]);
        doc.goto(&mut lr, &mut px, &mut Base::new(), 1).unwrap();
        assert_eq!((exposure(&lr, id), layers(&doc).len(), doc.history.position()), (1.0, 1, 1));
        doc.goto(&mut lr, &mut px, &mut Base::new(), 3).unwrap();
        assert_eq!((exposure(&lr, id), layers(&doc).len(), doc.history.position()), (2.0, 2, 3));
        doc.goto(&mut lr, &mut px, &mut Base::new(), 0).unwrap();
        assert_eq!((exposure(&lr, id), layers(&doc).len(), doc.history.position()), (0.0, 1, 0));
        assert!(doc.goto(&mut lr, &mut px, &mut Base::new(), 4).unwrap_err().contains("no history row"));
        assert_eq!(doc.history.entries(), rows, "the rows stay");
    }

    /// A Develop step whose commit fails (here: the photo gone from the library as far as the
    /// commit is concerned) stays where it was, like a paired one: nothing moved, the step is
    /// kept, the notice says why, and it undoes once the commit works again. The redo side too.
    #[test]
    fn a_develop_step_whose_commit_fails_is_kept_and_says_why() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        let rows = labels(&doc);
        doc.source.photo = PhotoId(u64::MAX);
        let e = doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap_err();
        assert_eq!((exposure(&lr, id), doc.develop.light.exposure), (1.0, 1.0), "nothing moved");
        assert_eq!((labels(&doc), doc.history.position(), doc.history.can_redo()), (rows.clone(), 1, false), "the step is kept");
        let notice = doc.take_notice().unwrap();
        assert!(notice.starts_with("Couldn\u{2019}t undo \u{201c}Exposure\u{201d}") && notice.contains(&e), "{notice}");
        doc.source.photo = id;
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!(exposure(&lr, id), 0.0);
        // and on the redo side
        doc.source.photo = PhotoId(u64::MAX);
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).is_err());
        assert_eq!((exposure(&lr, id), doc.history.position(), doc.history.can_redo()), (0.0, 0, true), "still redoable");
        assert!(doc.take_notice().unwrap().starts_with("Couldn\u{2019}t redo \u{201c}Exposure\u{201d}"));
        doc.source.photo = id;
        assert!(doc.redo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((exposure(&lr, id), labels(&doc)), (1.0, rows));
        assert_eq!(doc.take_notice(), None);
    }

    /// Review: a lost prefix drops every step older than the oldest surviving Pixel state, the
    /// Develop steps among them (a contiguous global prefix), and a prune that takes every old
    /// state along with a new edit is still a prune, not a reset.
    #[test]
    fn a_lost_prefix_drops_every_step_older_than_the_oldest_surviving_state() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 2.0);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(labels(&doc), ["Original", "Exposure", "Brush", "Exposure"]);
        // PhotoCraft's prefs re-apply its own cap just before an edit: its `record` drops the
        // oldest state (the only one) before Studio sees either
        px.active_mut().unwrap().history.max_states = 1;
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.take_notice(), None, "a prune, not a reset");
        assert_eq!(labels(&doc), ["Original", "Pencil"], "both Develop steps older than the surviving state went with the Brush step");
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert_eq!((layers(&doc), exposure(&lr, id)), (vec!["Develop".to_owned(), "Brush".into()], 2.0));
        assert!(!doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap(), "nothing older is left");
        // the same through the byte cap: with room for one state only, each paint trims the one before
        let (mut lr, mut px, mut doc) = open();
        for i in 0..3 {
            set_exposure(&mut lr, id, f64::from(i) + 1.0);
            tick(&mut doc, &lr, &mut px);
            paint(&mut px, &format!("Paint {i}"), i, i);
            tick(&mut doc, &lr, &mut px);
            doc.history.bytes_cap = (i as usize + 1) * TILE + TILE / 2;
            let (r, s) = render_sized(64, 48);
            doc.write_base(&mut px, r, s, anchor(1, 0.0, 0.0)).unwrap();
        }
        assert_eq!(doc.take_notice(), None);
        assert_eq!(labels(&doc), ["Original", "Paint 2"]);
    }

    /// Review: PhotoCraft's redo can go away without its states changing (its own `clear_redo`
    /// after a failed compound command): Studio's redo goes with it, and redo does nothing
    /// rather than fail.
    #[test]
    fn photocrafts_redo_going_away_takes_studios_with_it() {
        let (mut lr, mut px, mut doc) = open();
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        assert!(doc.history.can_redo());
        px.active_mut().unwrap().history.clear_redo();
        tick(&mut doc, &lr, &mut px);
        assert!(!doc.history.can_redo());
        assert_eq!(doc.redo(&mut lr, &mut px, &mut Base::new()), Ok(false));
        assert_eq!(doc.take_notice(), None);
        assert_eq!(layers(&doc), ["Develop"]);
    }

    /// Task 2: a step with a Develop half moves only once both halves can: with a slider drag
    /// open on the photo nothing moves in either engine, and the step stays (the way PhotoCraft's
    /// own jump reaches `undo_one`, past `flush`, which would have ended the drag).
    #[test]
    fn a_step_with_a_develop_half_waits_for_a_drag_on_the_photo_and_moves_neither_engine() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        // a step owning both halves (what a geometry change records)
        let p = doc.history.steps.remove(0).pixel;
        doc.history.steps[0].pixel = p;
        let pixels = px.active().unwrap().doc.clone();
        lr.selection.active = Some(id);
        lr.begin_interaction("Exposure").unwrap();
        assert!(doc.undo_one(&mut lr, &mut px).unwrap_err().contains("dragged"));
        assert!(Arc::ptr_eq(&px.active().unwrap().doc, &pixels) && exposure(&lr, id) == 1.0, "neither half moved");
        assert_eq!((doc.history.position(), doc.history.can_redo()), (1, false));
        lr.end_interaction().unwrap();
        assert!(doc.undo_one(&mut lr, &mut px).unwrap());
        doc.copy_back(&mut px);
        assert_eq!((layers(&doc).len(), exposure(&lr, id)), (1, 0.0));
    }

    /// A step owning both halves (what a geometry change records): Brush's state, Exposure's
    /// settings. PhotoCraft's history: "Open", "Brush".
    fn paired() -> (Session, PixelSession, Document) {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        let p = doc.history.steps.remove(0).pixel;
        doc.history.steps[0].pixel = p;
        (lr, px, doc)
    }

    /// Review (HIGH): a Pixel move that fails never loses the step. PhotoCraft can't undo (its
    /// history went behind Studio's back; the way a jump PhotoCraft began reaches `undo_one`,
    /// past `flush`): nothing moved, so the step stays, and says why. The redo side the same.
    #[test]
    fn a_pixel_move_that_fails_keeps_the_step_where_it_was() {
        let (mut lr, mut px, mut doc) = open();
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        px.active_mut().unwrap().history.clear();
        let e = doc.undo_one(&mut lr, &mut px).unwrap_err();
        assert_eq!((labels(&doc), doc.history.position(), doc.history.can_redo()), (vec!["Original".to_owned(), "Brush".into()], 1, false));
        assert_eq!(doc.take_notice().unwrap(), format!("Couldn\u{2019}t undo \u{201c}Brush\u{201d}: {e}"));
        // the redo side
        let (mut lr, mut px, mut doc) = open();
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        px.active_mut().unwrap().history.clear_redo();
        let e = doc.redo_one(&mut lr, &mut px).unwrap_err();
        assert_eq!((doc.history.position(), doc.history.can_redo()), (0, true), "still on the redo side");
        assert_eq!(doc.take_notice().unwrap(), format!("Couldn\u{2019}t redo \u{201c}Brush\u{201d}: {e}"));
    }

    /// Review (HIGH): a paired step whose Develop commit fails puts its Pixel half back; when
    /// that fails too, the history starts over (fail safe) rather than keep a step the engines no
    /// longer match. Undo and redo (the review's LOW: the rollback inside `redo_one`).
    #[test]
    fn a_rollback_that_fails_too_starts_the_history_over() {
        let (mut lr, mut px, mut doc) = paired();
        let id = doc.source.photo;
        doc.source.photo = PhotoId(u64::MAX);
        // the Pixel half undoes, the commit fails, putting the Pixel half back fails
        doc.history.fail_moves = [false, true].into();
        let e = doc.undo_one(&mut lr, &mut px).unwrap_err();
        assert!(!e.is_empty());
        assert_eq!((doc.history.position(), doc.history.can_redo()), (0, false), "no step the engines don't match");
        assert!(doc.take_notice().unwrap().starts_with("History reset: \u{201c}Exposure\u{201d} couldn\u{2019}t be undone"));
        assert_eq!(exposure(&lr, id), 1.0, "the Develop half never moved");
        // redo: the Pixel half redoes, the commit fails, putting it back fails
        let (mut lr, mut px, mut doc) = paired();
        assert!(doc.undo(&mut lr, &mut px, &mut Base::new()).unwrap());
        doc.source.photo = PhotoId(u64::MAX);
        doc.history.fail_moves = [false, true].into();
        assert!(doc.redo_one(&mut lr, &mut px).is_err());
        assert_eq!((doc.history.position(), doc.history.can_redo()), (0, false));
        assert!(doc.take_notice().unwrap().starts_with("History reset: \u{201c}Exposure\u{201d} couldn\u{2019}t be redone"));
        assert_eq!(exposure(&lr, id), 0.0);
        // and a rollback that works keeps the step, as before
        let (mut lr, mut px, mut doc) = paired();
        let pixels = px.active().unwrap().doc.clone();
        doc.source.photo = PhotoId(u64::MAX);
        assert!(doc.undo_one(&mut lr, &mut px).is_err());
        assert!(Arc::ptr_eq(&px.active().unwrap().doc, &pixels));
        assert_eq!((doc.history.position(), doc.take_notice().unwrap().starts_with("Couldn\u{2019}t undo")), (1, true));
    }

    /// Review: PhotoCraft's own undo is taken as Studio's only when the steps it undid are all
    /// Pixel and on top; with a Develop step above them it is put back and done as a jump; and
    /// when what it left doesn't match (the current document replaced meanwhile) the history
    /// starts over.
    #[test]
    fn photocrafts_own_undo_is_accepted_only_for_pixel_steps_on_top() {
        let (mut lr, mut px, mut doc) = open();
        let id = doc.source.photo;
        paint(&mut px, "Brush", 1, 1);
        tick(&mut doc, &lr, &mut px);
        paint(&mut px, "Pencil", 2, 2);
        tick(&mut doc, &lr, &mut px);
        // two Pixel steps on top: accepted as is, no jump requested
        assert!(px.undo() && px.undo());
        tick(&mut doc, &lr, &mut px);
        assert_eq!((labels(&doc), doc.history.position(), layers(&doc).len()), (vec!["Original".to_owned(), "Brush".into(), "Pencil".into()], 0, 1));
        assert!(doc.history.requested.is_none());
        doc.goto(&mut lr, &mut px, &mut Base::new(), 2).unwrap();
        // a Develop step above them: PhotoCraft's undo is put back until the jump runs
        set_exposure(&mut lr, id, 1.0);
        tick(&mut doc, &lr, &mut px);
        assert!(px.undo());
        tick(&mut doc, &lr, &mut px);
        assert_eq!((layers(&doc).len(), exposure(&lr, id), doc.history.position()), (3, 1.0, 3), "put back, nothing undone yet");
        assert!(doc.history.requested.is_some());
        doc.reconcile(&mut lr, &mut px).unwrap();
        assert_eq!((layers(&doc).len(), exposure(&lr, id), doc.history.position()), (2, 0.0, 1), "the jump: Develop step undone first");
        assert_eq!(doc.take_notice(), None);
        // what PhotoCraft left doesn't match: it undid, and something replaced its document
        doc.goto(&mut lr, &mut px, &mut Base::new(), 3).unwrap();
        assert!(px.undo());
        let st = px.active_mut().unwrap();
        st.doc = Arc::new((*st.doc).clone());
        tick(&mut doc, &lr, &mut px);
        assert_eq!(doc.take_notice().as_deref(), Some(RESET_PIXEL));
        assert_eq!((doc.history.position(), doc.history.can_redo()), (0, false));
        assert!(!px.active().unwrap().history.can_redo());
    }
}
