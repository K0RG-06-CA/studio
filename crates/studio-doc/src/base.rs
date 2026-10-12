//! The base layer's pixels: the photo as LightCraft renders it with the document's develop
//! settings, kept live. The settings' home is LightCraft's catalog while the photo is in the
//! library; the document mirrors them, and whenever they change the base is rendered again (at
//! the photo's size, 16-bit, on a worker thread) and written under whatever layers sit above it.
//! A render for a Develop step that changed the frame lands as that step's Pixel half instead
//! ([`crate::follow`]); until it does, the document's history says so (`pending_follow`), and
//! [`Base::finish`] waits for it.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError, channel};
use std::time::{Duration, Instant};

use lightcraft_engine::Session;
use lightcraft_engine::media::RenderResult;
use lightcraft_engine::pipeline::{DeepImage, DeepSamples, OutputDepth, Rendered, StageCache};
use photocraft_color::PixelFormat;
use photocraft_engine::Session as PixelSession;
use photocraft_geom::{Rect, Size};
use photocraft_raster::Surface;

use crate::Document;
use crate::pixel::Anchor;

/// Rows written per band, so a 24 MP render never needs a full-size float copy.
const BAND_ROWS: usize = 256;

/// What the worker sends back: LightCraft's result (its decoded source goes to the session's
/// cache) and the base surface built from it, there and not on the UI thread.
type Done = (RenderResult, Result<(Surface, Size), String>);

/// A follow's render may take this long before the Develop step is undone instead.
pub const FOLLOW_CEILING: Duration = Duration::from_secs(30);

/// Keeps a document's base layer in step with its develop settings.
#[derive(Default)]
pub struct Base {
    /// The render in flight: the document generation it started for, the settings hash it
    /// renders, and where its result arrives.
    pending: Option<(u64, u64, Anchor, Receiver<Done>)>,
    /// A render that failed: the settings hash and why. Not tried again until the settings change
    /// (a photo whose file is gone would otherwise be re-decoded every frame).
    failed: Option<(u64, String)>,
    /// Renders started so far.
    started: u64,
    /// Render no larger than this on the long side (the crop's native size otherwise): tests that
    /// shouldn't develop 24 MP on the CPU.
    pub max_edge: Option<usize>,
    /// LightCraft's stages kept between this photo's renders: a re-render after an edit redoes only
    /// what the edit changed (24 MP: ~0.9 s → ~0.43 s, see `bench.rs`).
    stages: Arc<StageCache>,
    /// A Pixel gesture (a stroke, a drag, a transform) is in progress: a finished render waits in
    /// `ready` until it ends, so the pixels under it don't change halfway. Set by the shell each
    /// frame before `sync`.
    pub hold: bool,
    /// A render that finished while `hold` was on: (settings hash, anchor, pixels, size).
    ready: Option<(u64, Anchor, Surface, Size)>,
    /// How long a Develop step may wait for its render, in frames ([`Base::sync`]) and in
    /// [`Base::finish`] together: [`FOLLOW_CEILING`] unless a test shortens it.
    pub ceiling: Option<Duration>,
    /// Since when a Develop step has been waiting for its render.
    waiting: Option<Instant>,
}

impl Base {
    pub fn new() -> Self {
        Base::default()
    }

    /// A fresh base (nothing mirrored, shown or pending) with the same render cap and ceiling.
    pub fn renewed(&self) -> Self {
        Base { max_edge: self.max_edge, ceiling: self.ceiling, ..Base::default() }
    }

    /// A render is in flight.
    pub fn rendering(&self) -> bool {
        self.pending.is_some()
    }

    /// How many renders were started so far.
    pub fn started(&self) -> u64 {
        self.started
    }

    /// The base shows the current settings (and nothing is in flight); always, without a photo.
    pub fn up_to_date(&self, doc: &Document) -> bool {
        doc.source.is_none() || (self.pending.is_none() && doc.shown_hash() == Some(doc.develop.hash64()))
    }

    /// The document's layer tree, for anything that flattens it or reads the base's pixels
    /// (exports, the canvas): only once the base shows the current settings, never a stale or
    /// half-way render.
    pub fn current<'d>(&self, doc: &'d Document) -> Result<&'d photocraft_doc::Document, String> {
        if self.up_to_date(doc) { Ok(&doc.pixel) } else { Err("the base layer is still rendering the current settings".into()) }
    }

    /// Once per frame: mirror the catalog's settings into `doc`, take a finished render into the
    /// base layer (in PhotoCraft's session, `pixel`, the pixels' one home), and start one when
    /// the base is stale and no slider drag is open (a drag then renders once, at release).
    /// Errors are LightCraft's (a photo gone from the library, a
    /// decode that failed); the base keeps its last pixels, and a failed render is reported again
    /// every frame, not retried, until the settings change. A Develop step whose render hasn't
    /// landed within the ceiling is undone, as in [`Base::finish`]. A document without a photo has
    /// no base: nothing to do.
    pub fn sync(&mut self, session: &mut Session, pixel: &mut PixelSession, doc: &mut Document) -> Result<(), String> {
        if doc.source.is_none() {
            return Ok(());
        }
        self.mirror(session, doc);
        let r = self.step(session, pixel, doc);
        if let Err(e) = &r
            && doc.history.pending_follow()
        {
            // the render a Develop step waited for can't come: the step goes back
            doc.follow_failed(session, e);
        }
        if !doc.history.pending_follow() {
            self.waiting = None;
            return r;
        }
        // nor may it take longer than the ceiling while only frames wait for it (Pixel takes no
        // input meanwhile: a render that hangs would keep it so)
        let ceiling = self.ceiling.unwrap_or(FOLLOW_CEILING);
        if self.waiting.get_or_insert_with(Instant::now).elapsed() >= ceiling {
            self.waiting = None;
            self.pending = None;
            let e = format!("the render took longer than {} s", ceiling.as_secs());
            doc.follow_failed(session, &e);
            return Err(e);
        }
        r
    }

    fn step(&mut self, session: &mut Session, pixel: &mut PixelSession, doc: &mut Document) -> Result<(), String> {
        let want = doc.develop.hash64();
        if let Err(e) = self.take(session, pixel, doc) {
            self.failed = Some((want, e.clone()));
            return Err(e);
        }
        if self.failed.as_ref().is_some_and(|(hash, _)| *hash != want) {
            self.failed = None;
        }
        if let Some((_, why)) = &self.failed {
            return Err(why.clone());
        }
        let waiting = self.ready.as_ref().is_some_and(|r| r.0 == want);
        if self.pending.is_none() && doc.shown_hash() != Some(want) && !waiting && session.interaction.is_none() {
            self.start(session, doc, want)?;
        }
        Ok(())
    }

    /// Wait for the render a Develop step is waiting for, and land it: [`Document::flush`] runs
    /// it before anything decides on the document (an undo, a save, a close, a persona switch),
    /// so a geometry change is never half done. A slider drag open on the photo ends first (its
    /// render is the one waited for). Nothing to wait for returns at once. A render that fails,
    /// can't start, or takes longer than the ceiling ([`FOLLOW_CEILING`]) undoes the step (see
    /// [`Document::follow_failed`]) and is the error.
    pub(crate) fn finish(&mut self, session: &mut Session, pixel: &mut PixelSession, doc: &mut Document) -> Result<(), String> {
        let r = self.wait_for_follow(session, pixel, doc);
        // landed or undone: the next step waits from its own start
        self.waiting = None;
        r
    }

    fn wait_for_follow(&mut self, session: &mut Session, pixel: &mut PixelSession, doc: &mut Document) -> Result<(), String> {
        let Some(photo) = doc.photo() else { return Ok(()) };
        if session.interaction.as_ref().is_some_and(|i| i.photo == photo) {
            session.end_interaction().map_err(|e| e.to_string())?;
        }
        self.mirror(session, doc);
        if !doc.history.pending_follow() {
            return Ok(());
        }
        // a render held for a Pixel gesture goes in now (one still open, a Free Transform or
        // type, is applied after it, by the shell's flush)
        self.hold = false;
        let ceiling = self.ceiling.unwrap_or(FOLLOW_CEILING);
        // counted from when the step began waiting, frames included
        let t0 = self.waiting.take().unwrap_or_else(Instant::now);
        while doc.history.pending_follow() {
            let Some((generation, want, anchor, rx)) = self.pending.take() else {
                let before = self.started;
                // a failed start undoes the step itself (`sync`)
                self.sync(session, pixel, doc)?;
                if self.started == before && doc.history.pending_follow() {
                    let e = "Develop couldn\u{2019}t start the render".to_owned();
                    doc.follow_failed(session, &e);
                    return Err(e);
                }
                continue;
            };
            let r = match rx.recv_timeout(ceiling.saturating_sub(t0.elapsed())) {
                Ok((result, converted)) => self.landed(session, pixel, doc, generation, want, anchor, result, converted),
                Err(RecvTimeoutError::Timeout) => Err(format!("the render took longer than {} s", ceiling.as_secs())),
                Err(RecvTimeoutError::Disconnected) => Err("the render thread stopped without a result".into()),
            };
            if let Err(e) = r {
                self.failed = Some((want, e.clone()));
                doc.follow_failed(session, &e);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Copy the catalog's settings into `doc` if they changed (a change is a Develop step of the
    /// document's history). `sync` does this at the start of a frame; call it before saving,
    /// since LightCraft's panels edit later in the frame.
    pub fn mirror(&mut self, session: &Session, doc: &mut Document) {
        doc.note_develop(session);
    }

    fn start(&mut self, session: &mut Session, doc: &Document, want: u64) -> Result<(), String> {
        let id = doc.photo().ok_or("the document has no photo to render")?;
        let photo = session.catalog.photo(id).ok_or_else(|| format!("photo {} isn't in the library", id.0))?;
        let (w, h) = (photo.width as usize, photo.height as usize);
        let anchor = Anchor::new(Arc::new(doc.develop.clone()), (w, h), photo.embedded_lens);
        // the size the crop has at the source's own resolution (rotation swaps the sides; a fit
        // into the photo's box would scale a crop up and a rotated photo down)
        let (nw, nh) = lightcraft_engine::pipeline::native_output_size(w, h, &doc.develop);
        // only ever smaller: LightCraft's fit would scale a small frame up to the box
        let scale = self.max_edge.map_or(1.0, |m| (m as f64 / nw.max(nh)).min(1.0));
        let (nw, nh) = ((nw * scale).round().max(1.0) as usize, (nh * scale).round().max(1.0) as usize);
        let mut job = session
            .preview_job(id, nw, nh, true, &doc.develop)
            .ok_or_else(|| format!("photo {} can't be rendered", id.0))?
            .with_stages(self.stages.clone());
        // 16 bits: LightCraft renders deep output on the CPU (its GPU kernel writes 8-bit)
        job.request.depth = OutputDepth::U16;
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("studio-base".into())
            .spawn(move || {
                let r = job.run();
                let surface = surface_from(&r.rendered);
                let _ = tx.send((r, surface));
            })
            .map_err(|e| format!("no thread for the render: {e}"))?;
        self.pending = Some((doc.generation, want, anchor, rx));
        self.started += 1;
        Ok(())
    }

    /// A finished render goes into the base layer; the document takes the render's size (crop,
    /// straighten and rotation change it). One for settings edited since it started, or for an
    /// earlier generation of the document, is dropped, pixels or error: the next `sync` renders
    /// the newest settings (edits made meanwhile coalesce into that one render).
    fn take(&mut self, session: &mut Session, pixel: &mut PixelSession, doc: &mut Document) -> Result<(), String> {
        // one that finished during a gesture goes in once it has ended (unless edited since)
        if !self.hold
            && let Some((want, anchor, surface, size)) = self.ready.take()
            && want == doc.develop.hash64()
        {
            write(pixel, doc, surface, size, anchor)?;
        }
        let Some((.., rx)) = &self.pending else { return Ok(()) };
        let (result, converted) = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                return Err("the render thread stopped without a result".into());
            }
        };
        let Some((generation, want, anchor, _)) = self.pending.take() else { return Ok(()) };
        self.landed(session, pixel, doc, generation, want, anchor, result, converted)
    }

    /// The render's result arrived: LightCraft takes its decoded source, and the pixels go in
    /// unless they are for another generation or other settings, or a gesture holds them.
    #[allow(clippy::too_many_arguments)]
    fn landed(
        &mut self,
        session: &mut Session,
        pixel: &mut PixelSession,
        doc: &mut Document,
        generation: u64,
        want: u64,
        anchor: Anchor,
        result: RenderResult,
        converted: Result<(Surface, Size), String>,
    ) -> Result<(), String> {
        session.accept(&result);
        if generation != doc.generation || want != doc.develop.hash64() {
            return Ok(());
        }
        let (surface, size) = converted?;
        if self.hold {
            self.ready = Some((want, anchor, surface, size));
            return Ok(());
        }
        write(pixel, doc, surface, size, anchor)
    }
}

/// A render into PhotoCraft, the pixels' one home: the Pixel half of the Develop step waiting
/// for it, else derived pixels, not an edit (the revision, and so "unsaved", doesn't move).
fn write(pixel: &mut PixelSession, doc: &mut Document, surface: Surface, size: Size, anchor: Anchor) -> Result<(), String> {
    if doc.history.pending_follow() { doc.follow(pixel, surface, size, anchor) } else { doc.write_base(pixel, surface, size, Some(anchor)) }
}

/// The base surface from a render: its 16-bit image, or why there is none.
fn surface_from(rendered: &Result<Rendered, String>) -> Result<(Surface, Size), String> {
    let rendered = rendered.as_ref().map_err(Clone::clone)?;
    let deep = rendered.deep.as_ref().ok_or("LightCraft returned no 16-bit render")?;
    surface_from_deep(deep)
}

/// An RGBA16 surface (alpha 1) and its size from a 16-bit RGB render, written in bands. The
/// render's shape is checked first: PhotoCraft's `write_region` asserts on a wrong length, and an
/// empty image would make a zero-size band.
pub(crate) fn surface_from_deep(deep: &DeepImage) -> Result<(Surface, Size), String> {
    let DeepSamples::U16(samples) = &deep.samples else { return Err("LightCraft returned the wrong sample type".into()) };
    let (w, h) = (deep.width, deep.height);
    if w == 0 || h == 0 {
        return Err(format!("empty render ({w}×{h})"));
    }
    if samples.len() != w.checked_mul(h).and_then(|n| n.checked_mul(3)).ok_or("render too large")? {
        return Err(format!("render of {w}×{h} came with {} samples", samples.len()));
    }
    let (width, height) = (u32::try_from(w).map_err(|e| e.to_string())?, u32::try_from(h).map_err(|e| e.to_string())?);
    let mut surface = Surface::new(PixelFormat::RGBA16);
    let mut band = Vec::with_capacity(w * 4 * h.min(BAND_ROWS));
    for (i, rows) in samples.chunks(w * 3 * BAND_ROWS).enumerate() {
        band.clear();
        for px in rows.as_chunks::<3>().0 {
            band.extend(px.iter().map(|&v| f32::from(v) / 65535.0));
            band.push(1.0);
        }
        let y = i32::try_from(i * BAND_ROWS).map_err(|e| e.to_string())?;
        let rows_h = u32::try_from(rows.len() / (w * 3)).map_err(|e| e.to_string())?;
        surface.write_region(Rect::from_xywh(0, y, width, rows_h), &band);
    }
    Ok((surface, Size { width, height }))
}

#[cfg(test)]
mod tests {
    use lightcraft_engine::develop::DevelopSettings;
    use lightcraft_engine::pipeline::OutputSpace;

    use super::*;
    use crate::tests::demo;
    use lightcraft_engine::catalog::model::PhotoId;

    /// A document on the first demo photo, its base capped at 600 px (the demo scenes are 24 MP,
    /// and CI's runners develop that on the CPU in a minute or more).
    fn open() -> (Session, Document, Base) {
        let (session, doc, mut base) = open_native();
        base.max_edge = Some(600);
        (session, doc, base)
    }

    /// The same, rendering at the crop's native size.
    fn open_native() -> (Session, Document, Base) {
        lightcraft_engine::gpu::set_enabled(false);
        let (session, id) = demo();
        let doc = Document::from_photo(&session, id).unwrap();
        (session, doc, Base::new())
    }

    /// Sync until the base is up to date (the render runs on a thread).
    fn settle(base: &mut Base, session: &mut Session, pixel: &mut PixelSession, doc: &mut Document) {
        let t0 = Instant::now();
        loop {
            base.sync(session, pixel, doc).unwrap();
            if base.up_to_date(doc) {
                return;
            }
            // a native 24 MP render on a 2-core CI runner can take a minute or more
            assert!(t0.elapsed() < Duration::from_secs(300), "render never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Mean of the composited document's red, green and blue (0..1).
    fn mean_rgb(doc: &Document) -> f32 {
        let b = photocraft_compose::flatten(&doc.pixel);
        b.px.iter().map(|p| (p[0] + p[1] + p[2]) / 3.0).sum::<f32>() / b.px.len() as f32
    }

    fn set_exposure(session: &mut Session, id: PhotoId, exposure: f64) {
        let mut s = (*session.develop_of(id).unwrap()).clone();
        s.light.exposure = exposure;
        session.set_develop(id, s, "Exposure").unwrap();
    }

    #[test]
    fn a_re_render_through_the_stage_cache_matches_a_plain_one_within_1_lsb() {
        let (mut session, doc, _) = open();
        let id = doc.photo().unwrap();
        let render = |session: &mut Session, stages: Option<&Arc<StageCache>>| {
            let s = (*session.develop_of(id).unwrap()).clone();
            let mut job = session.preview_job(id, 600, 600, true, &s).unwrap();
            job.request.depth = OutputDepth::U16;
            if let Some(st) = stages {
                job = job.with_stages(st.clone());
            }
            let r = job.run();
            session.accept(&r);
            match r.rendered.unwrap().deep.unwrap().samples {
                DeepSamples::U16(v) => v,
                DeepSamples::F32(_) => panic!("not 16-bit"),
            }
        };
        let stages = Arc::new(StageCache::default());
        render(&mut session, Some(&stages));
        // edits late and early in the pipeline, and one that changes the frame
        let edits: [fn(&mut DevelopSettings); 3] = [
            |s| s.light.exposure = 1.0,
            |s| s.light.contrast = 40.0,
            |s| {
                s.crop.geometry.rect.x1 = 0.6;
                s.crop.geometry.rect.y1 = 0.7;
            },
        ];
        for (i, edit) in edits.into_iter().enumerate() {
            let mut s = (*session.develop_of(id).unwrap()).clone();
            edit(&mut s);
            session.set_develop(id, s, "Edit").unwrap();
            let (cached, plain) = (render(&mut session, Some(&stages)), render(&mut session, None));
            assert_eq!(cached.len(), plain.len(), "edit {i}");
            let worst = cached.iter().zip(&plain).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(worst <= 1, "edit {i}: off by {worst}/65535");
        }
    }

    #[test]
    fn edits_during_a_render_coalesce_and_its_stale_result_is_dropped() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        let id = doc.photo().unwrap();
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert!(base.rendering());
        // three edits while the first render runs: no second render alongside it
        for e in [0.5, 1.0, 1.5] {
            set_exposure(&mut session, id, e);
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
            assert_eq!((doc.develop.light.exposure, base.started()), (e, 1));
        }
        // the first finishes for superseded settings: dropped, and the newest renders next
        let t0 = Instant::now();
        while base.started() == 1 {
            assert!(t0.elapsed() < Duration::from_secs(300), "the first render never finished");
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(doc.base_layer().unwrap().surface().unwrap().tile_count(), 0, "the stale render never reached the base");
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        assert_eq!(base.started(), 2, "one render for all three edits");
        // the same pixels as a base rendered straight at the newest settings
        let (mut other_session, mut other, mut other_base) = open();
        let mut other_pixel = PixelSession::new();
        set_exposure(&mut other_session, id, 1.5);
        settle(&mut other_base, &mut other_session, &mut other_pixel, &mut other);
        let (ours, theirs) =
            (photocraft_compose::flatten(base.current(&doc).unwrap()), photocraft_compose::flatten(other_base.current(&other).unwrap()));
        assert_eq!(ours.px.len(), theirs.px.len());
        let worst = ours.px.iter().zip(&theirs.px).flat_map(|(a, b)| (0..4).map(move |c| (a[c] - b[c]).abs())).fold(0.0, f32::max);
        assert!(worst <= 1.5 / 65535.0, "off by {} LSB", worst * 65535.0);
    }

    #[test]
    fn reading_the_base_waits_until_it_shows_the_current_settings() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        let id = doc.photo().unwrap();
        assert!(base.current(&doc).is_err(), "nothing rendered yet");
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert!(base.rendering() && base.current(&doc).is_err(), "in flight");
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let before = photocraft_compose::flatten(base.current(&doc).unwrap()).px;
        // an edit: stale from the sync that sees it until its render is in
        set_exposure(&mut session, id, 2.0);
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        let e = base.current(&doc).unwrap_err();
        assert!(e.contains("still rendering"), "{e}");
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let after = photocraft_compose::flatten(base.current(&doc).unwrap()).px;
        assert!(after.len() == before.len() && after != before, "the new render, not the old one");
        // a render that fails leaves the base stale: never readable as if current
        doc.source.as_mut().unwrap().photo = PhotoId(u64::MAX);
        doc.develop.light.exposure = -1.0;
        assert!(base.sync(&mut session, &mut pixel, &mut doc).is_err());
        assert!(base.current(&doc).is_err());
    }

    #[test]
    fn the_first_sync_renders_the_photo_into_the_base_as_lightcraft_renders_it() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        assert!(doc.base_layer().unwrap().surface().unwrap().tile_count() == 0);
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let (w, h) = (doc.pixel.size.width as usize, doc.pixel.size.height as usize);
        assert_eq!((w, h), (600, 400), "capped at 600 on the long side, never scaled up");
        let theirs = session.render_now(doc.photo().unwrap(), w, h).unwrap().image;
        assert_eq!((theirs.width, theirs.height), (w, h));
        let ours = photocraft_compose::flatten(&doc.pixel);
        assert_eq!(ours.px.len(), w * h);
        let worst =
            ours.px.iter().zip(&theirs.data).flat_map(|(o, t)| (0..3).map(move |c| (o[c] * 255.0 - f32::from(t[c])).abs())).fold(0.0, f32::max);
        assert!(worst <= 1.0, "base differs from LightCraft's render by {worst}/255");
        assert!(ours.px.iter().all(|p| p[3] == 1.0), "the base is opaque");
        assert!(!doc.unsaved(), "a render isn't an edit");
    }

    #[test]
    fn changing_develop_settings_in_the_catalog_re_renders_the_base() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let before = mean_rgb(&doc);
        let (id, rev) = (doc.photo().unwrap(), doc.revision);
        let mut s = (*session.develop_of(id).unwrap()).clone();
        s.light.exposure = 2.0;
        session.set_develop(id, s.clone(), "Exposure").unwrap();
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert_eq!((doc.develop.light.exposure, base.rendering(), base.up_to_date(&doc)), (2.0, true, false));
        assert!(doc.revision > rev, "mirroring the settings is a change");
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        assert!(mean_rgb(&doc) > before + 0.05, "{before} → {}", mean_rgb(&doc));
        // without a sync the document doesn't know (the mirror is the sync's job)
        let (mut other_session, mut other, mut other_base) = open();
        let mut other_pixel = PixelSession::new();
        settle(&mut other_base, &mut other_session, &mut other_pixel, &mut other);
        other_session.set_develop(id, s, "Exposure").unwrap();
        assert_eq!(other.develop.light.exposure, 0.0);
        assert!((mean_rgb(&other) - before).abs() < 1e-6);
    }

    #[test]
    fn a_slider_drag_renders_once_at_release() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let id = doc.photo().unwrap();
        session.selection.active = Some(id);
        session.begin_interaction("Exposure").unwrap();
        for e in [0.5, 1.0, 1.5] {
            let mut s = (*session.develop_of(id).unwrap()).clone();
            s.light.exposure = e;
            session.set_develop(id, s, "Exposure").unwrap();
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
            assert_eq!(doc.develop.light.exposure, e, "mirrored during the drag");
            assert!(!base.rendering() && !base.up_to_date(&doc), "no render while dragging");
        }
        session.end_interaction().unwrap();
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert!(base.rendering());
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        assert!(doc.develop.light.exposure == 1.5 && base.up_to_date(&doc));
        assert_eq!(base.started(), 2, "the first render, and one for the whole drag");
    }

    /// A 1×1 PNG.
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01,
        0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn a_render_that_fails_is_reported_every_frame_but_tried_once_until_the_settings_change() {
        lightcraft_engine::gpu::set_enabled(false);
        let dir = std::env::temp_dir().join(format!("studio-doc-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("gone.png");
        std::fs::write(&png, PNG).unwrap();
        let mut session = Session::new().with_fs().with_system_clock();
        session.open_library(dir.join("lib"), false).unwrap();
        let r = session.execute("library.import", &serde_json::json!({"paths": [png.to_string_lossy()]})).unwrap();
        let id = PhotoId(r["imported"][0].as_u64().unwrap());
        let mut doc = Document::from_photo(&session, id).unwrap();
        // the file disappears before the first render
        std::fs::remove_file(&png).unwrap();
        let mut base = Base::new();
        let mut pixel = PixelSession::new();
        let t0 = Instant::now();
        let first = loop {
            match base.sync(&mut session, &mut pixel, &mut doc) {
                Err(e) => break e,
                Ok(()) => {
                    assert!(base.rendering() && t0.elapsed() < Duration::from_secs(60), "the render never failed");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        };
        assert_eq!(base.started(), 1);
        for _ in 0..3 {
            assert_eq!(base.sync(&mut session, &mut pixel, &mut doc).unwrap_err(), first, "reported again");
            assert!(!base.rendering(), "not tried again");
        }
        assert_eq!(base.started(), 1);
        // new settings: one new attempt
        let mut s = doc.develop.clone();
        s.light.exposure = 1.0;
        session.set_develop(id, s, "Exposure").unwrap();
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert!(base.rendering() && base.started() == 2);
        session.close_library().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crop_resizes_the_document_with_the_base() {
        let (mut session, mut doc, mut base) = open_native();
        let mut pixel = PixelSession::new();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let full = doc.pixel.size;
        let photo = session.catalog.photo(doc.photo().unwrap()).unwrap();
        assert_eq!((full.width, full.height), (photo.width, photo.height), "native size");
        let id = doc.photo().unwrap();
        let mut s = (*session.develop_of(id).unwrap()).clone();
        s.crop.geometry.rect.x1 = 0.5;
        s.crop.geometry.rect.y1 = 0.5;
        session.set_develop(id, s.clone(), "Crop").unwrap();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let half = doc.pixel.size;
        assert!(
            half.width.abs_diff(full.width / 2) <= 1 && half.height.abs_diff(full.height / 2) <= 1,
            "native size, not scaled up: {full:?} → {half:?}"
        );
        let b = photocraft_compose::flatten(&doc.pixel);
        assert_eq!(b.px.len(), (half.width * half.height) as usize);
        assert!(b.px.iter().all(|p| p[3] == 1.0), "no stale pixels outside the crop");
        // rotated: the sides swap, nothing is scaled down
        s.crop = Default::default();
        session.set_develop(id, s, "Uncrop").unwrap();
        session.execute("photo.rotateLeft", &serde_json::json!({"ids": [id.0]})).unwrap();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let turned = doc.pixel.size;
        assert!(turned.width.abs_diff(full.height) <= 1 && turned.height.abs_diff(full.width) <= 1, "{full:?} → {turned:?}");
    }

    /// The base layer's pixel at (x, y).
    fn base_px(doc: &Document, x: i32, y: i32) -> [f32; 4] {
        let mut px = [[0.0; 4]];
        doc.base_layer().unwrap().surface().unwrap().read_rgba_into(Rect::from_xywh(x, y, 1, 1), &mut px);
        px[0]
    }

    #[test]
    fn a_pixel_layer_stays_on_its_photo_pixel_when_develop_crops() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        assert_eq!((doc.pixel.size.width, doc.pixel.size.height), (600, 400));
        // a Pixel edit: one painted pixel over photo pixel P = (300, 250)
        pixel
            .edit("Paint", |d, active| {
                let mut l = photocraft_doc::Layer::raster("Paint", PixelFormat::RGBA16);
                l.surface_mut().unwrap().write_region(Rect::from_xywh(300, 250, 1, 1), &[1.0, 0.0, 0.0, 1.0]);
                *active = Some(l.id);
                d.layers.push(l);
                Ok(())
            })
            .unwrap();
        doc.copy_back(&mut pixel);
        let p = base_px(&doc, 300, 250);
        // Develop crops a quarter off the top and the bottom (the long side, so the scale, stays)
        let id = doc.photo().unwrap();
        let mut s = (*session.develop_of(id).unwrap()).clone();
        (s.crop.geometry.rect.y0, s.crop.geometry.rect.y1) = (0.25, 0.75);
        session.set_develop(id, s, "Crop").unwrap();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        assert_eq!((doc.pixel.size.width, doc.pixel.size.height), (600, 200));
        let layer = doc.pixel.layers.iter().find(|l| l.name == "Paint").unwrap();
        let b = photocraft_compose::bounds::content_bounds(layer.surface().unwrap());
        assert_eq!((b.x0, b.y0), (300, 150), "moved up by the 100 px the crop took off");
        // and the photo pixel under it is P's
        let q = base_px(&doc, b.x0, b.y0);
        assert!((0..3).all(|c| (p[c] - q[c]).abs() < 0.02), "{p:?} vs {q:?}");
        assert_eq!(doc.take_notice(), None);
    }

    #[test]
    fn a_render_that_finishes_during_a_stroke_goes_in_when_the_stroke_ends() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        let id = doc.photo().unwrap();
        let shown = |doc: &Document| doc.base_layer().unwrap().surface().unwrap().tiles().next().map(|(_, t)| Arc::as_ptr(t));
        let (before, started) = (shown(&doc), base.started());
        // a stroke starts in Pixel; meanwhile Develop's exposure changes and its render finishes
        base.hold = true;
        set_exposure(&mut session, id, 1.0);
        let t0 = Instant::now();
        while base.rendering() || base.started() == started {
            assert!(t0.elapsed() < Duration::from_secs(300), "render never finished");
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..3 {
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        }
        assert_eq!(shown(&doc), before, "the pixels under the stroke didn't change");
        assert!(!base.up_to_date(&doc) && base.current(&doc).is_err(), "and the base isn't current");
        assert_eq!(base.started(), started + 1, "nor rendered again while it waits");
        // the stroke ends (PhotoCraft commits it over the old base), then the render goes in
        pixel
            .edit("Brush", |d, active| {
                let l = photocraft_doc::Layer::raster("Stroke", PixelFormat::RGBA16);
                *active = Some(l.id);
                d.layers.push(l);
                Ok(())
            })
            .unwrap();
        base.hold = false;
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert!(base.up_to_date(&doc));
        assert_ne!(shown(&doc), before);
        assert_eq!(doc.pixel.layers.len(), 2, "the stroke is kept");
        assert_eq!(pixel.active().unwrap().history.entries(), ["Open", "Brush"], "the render is no history step");
        // a render held while the settings changed again is dropped, never shown
        base.hold = true;
        set_exposure(&mut session, id, 1.5);
        let started = base.started();
        while base.rendering() || base.started() == started {
            base.sync(&mut session, &mut pixel, &mut doc).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        let held = shown(&doc);
        set_exposure(&mut session, id, -1.0);
        base.hold = false;
        base.sync(&mut session, &mut pixel, &mut doc).unwrap();
        assert_eq!(shown(&doc), held, "the held 1.5 render was stale: not written");
        settle(&mut base, &mut session, &mut pixel, &mut doc);
        assert_eq!(doc.develop.light.exposure, -1.0);
    }

    fn deep(width: usize, height: usize, samples: Vec<u16>) -> DeepImage {
        DeepImage { width, height, space: OutputSpace::Srgb, samples: DeepSamples::U16(samples) }
    }

    #[test]
    fn a_render_becomes_an_opaque_16_bit_surface_band_by_band() {
        // 2 wide, 300 tall: more than one band; the last pixel sits in the second band
        let (w, h) = (2usize, 300usize);
        let mut samples = vec![0u16; w * h * 3];
        samples[0..3].copy_from_slice(&[65535, 0, 32768]);
        let last = (h - 1) * w + 1;
        samples[last * 3..last * 3 + 3].copy_from_slice(&[0, 65535, 65535]);
        let (surface, size) = surface_from_deep(&deep(w, h, samples)).unwrap();
        assert_eq!((size.width, size.height), (2, 300));
        let mut px = [[0.0f32; 4]; 2];
        surface.read_rgba_into(Rect::from_xywh(0, 0, 2, 1), &mut px);
        assert!((px[0][0] - 1.0).abs() < 1e-4 && px[0][1] == 0.0 && (px[0][2] - 0.5).abs() < 1e-3 && px[0][3] == 1.0, "{px:?}");
        surface.read_rgba_into(Rect::from_xywh(0, 299, 2, 1), &mut px);
        assert_eq!(px[1], [0.0, 1.0, 1.0, 1.0], "{px:?}");
        assert_eq!(px[0], [0.0, 0.0, 0.0, 1.0], "black, opaque");
    }

    #[test]
    fn a_malformed_render_is_an_error_not_a_crash() {
        assert!(surface_from_deep(&deep(0, 0, Vec::new())).unwrap_err().contains("empty"));
        assert!(surface_from_deep(&deep(0, 5, Vec::new())).unwrap_err().contains("empty"));
        assert!(surface_from_deep(&deep(5, 0, Vec::new())).unwrap_err().contains("empty"));
        assert!(surface_from_deep(&deep(2, 2, vec![0; 11])).unwrap_err().contains("11 samples"));
        assert!(surface_from_deep(&deep(usize::MAX, 2, Vec::new())).unwrap_err().contains("too large"));
        let f32s = DeepImage { width: 1, height: 1, space: OutputSpace::Srgb, samples: DeepSamples::F32(vec![0.0; 3]) };
        assert!(surface_from_deep(&f32s).unwrap_err().contains("sample type"));
    }

    #[test]
    fn a_photo_gone_from_the_library_is_an_error_not_a_crash() {
        let (mut session, mut doc, mut base) = open();
        let mut pixel = PixelSession::new();
        doc.source.as_mut().unwrap().photo = PhotoId(u64::MAX);
        let e = base.sync(&mut session, &mut pixel, &mut doc).unwrap_err();
        assert!(e.contains("isn't in the library"), "{e}");
        assert!(!base.rendering());
    }
}
