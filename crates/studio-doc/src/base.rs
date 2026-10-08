//! The base layer's pixels: the photo as LightCraft renders it with the document's develop
//! settings, kept live. The settings' home is LightCraft's catalog while the photo is in the
//! library; the document mirrors them, and whenever they change the base is rendered again (at
//! the photo's size, 16-bit, on a worker thread) and written under whatever layers sit above it.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use lightcraft_engine::Session;
use lightcraft_engine::develop::DevelopSettings;
use lightcraft_engine::media::RenderResult;
use lightcraft_engine::pipeline::{DeepSamples, OutputDepth};
use photocraft_color::PixelFormat;
use photocraft_doc::LayerContent;
use photocraft_geom::{Rect, Size};
use photocraft_raster::Surface;

use crate::Document;

/// Rows written per band, so a 24 MP render never needs a full-size float copy.
const BAND_ROWS: usize = 256;

/// Keeps a document's base layer in step with its develop settings.
#[derive(Default)]
pub struct Base {
    /// The catalog's settings as last mirrored (pointer compare: the catalog hands out `Arc`s).
    mirrored: Option<Arc<DevelopSettings>>,
    /// Hash of the settings the base pixels show; none until the first render.
    shown: Option<u64>,
    /// The render in flight: the settings hash it renders, and where its result arrives.
    pending: Option<(u64, Receiver<RenderResult>)>,
}

impl Base {
    pub fn new() -> Self {
        Base::default()
    }

    /// A render is in flight.
    pub fn rendering(&self) -> bool {
        self.pending.is_some()
    }

    /// The base shows the current settings (and nothing is in flight).
    pub fn up_to_date(&self, doc: &Document) -> bool {
        self.pending.is_none() && self.shown == Some(doc.develop.hash64())
    }

    /// Once per frame: mirror the catalog's settings into `doc`, take a finished render into the
    /// base layer, and start one when the base is stale and no slider drag is open (a drag then
    /// renders once, at release). Errors are LightCraft's (a photo gone from the library, a
    /// decode that failed); the base keeps its last pixels.
    pub fn sync(&mut self, session: &mut Session, doc: &mut Document) -> Result<(), String> {
        if let Some(now) = session.develop_of(doc.source.photo)
            && !self.mirrored.as_ref().is_some_and(|m| Arc::ptr_eq(m, &now))
        {
            if *now != doc.develop {
                doc.develop = (*now).clone();
                doc.revision += 1;
            }
            self.mirrored = Some(now);
        }
        self.take(session, doc)?;
        let want = doc.develop.hash64();
        if self.pending.is_none() && self.shown != Some(want) && session.interaction.is_none() {
            self.start(session, doc, want)?;
        }
        Ok(())
    }

    fn start(&mut self, session: &mut Session, doc: &Document, want: u64) -> Result<(), String> {
        let id = doc.source.photo;
        let (w, h) =
            session.catalog.photo(id).map(|p| (p.width as usize, p.height as usize)).ok_or_else(|| format!("photo {} isn't in the library", id.0))?;
        let mut job = session.preview_job(id, w.max(1), h.max(1), true, &doc.develop).ok_or_else(|| format!("photo {} can't be rendered", id.0))?;
        // 16 bits: LightCraft renders deep output on the CPU (its GPU kernel writes 8-bit)
        job.request.depth = OutputDepth::U16;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(job.run());
        });
        self.pending = Some((want, rx));
        Ok(())
    }

    /// A finished render goes into the base layer; the document takes the render's size (crop,
    /// straighten and rotation change it).
    fn take(&mut self, session: &mut Session, doc: &mut Document) -> Result<(), String> {
        let Some((want, rx)) = &self.pending else { return Ok(()) };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                return Err("the render thread stopped without a result".into());
            }
        };
        let want = *want;
        self.pending = None;
        session.accept(&result);
        let rendered = result.rendered?;
        let deep = rendered.deep.ok_or("LightCraft returned no 16-bit render")?;
        let DeepSamples::U16(samples) = &deep.samples else { return Err("LightCraft returned the wrong sample type".into()) };
        let (w, h) = (deep.width, deep.height);
        if samples.len() != w.checked_mul(h).and_then(|n| n.checked_mul(3)).ok_or("render too large")? {
            return Err(format!("render of {w}×{h} came with {} samples", samples.len()));
        }
        let (width, height) = (u32::try_from(w).map_err(|e| e.to_string())?, u32::try_from(h).map_err(|e| e.to_string())?);
        let mut surface = Surface::new(PixelFormat::RGBA16);
        let mut band = Vec::with_capacity(w * BAND_ROWS * 4);
        for (i, rows) in samples.chunks(w * 3 * BAND_ROWS).enumerate() {
            band.clear();
            for px in rows.as_chunks::<3>().0 {
                band.extend(px.iter().map(|&v| f32::from(v) / 65535.0));
                band.push(1.0);
            }
            let y = u32::try_from(i * BAND_ROWS).map_err(|e| e.to_string())?;
            let rows_h = u32::try_from(rows.len() / (w * 3)).map_err(|e| e.to_string())?;
            surface.write_region(Rect::from_xywh(0, y as i32, width, rows_h), &band);
        }
        let base = doc.base_layer_mut().ok_or("the document has no base layer")?;
        base.content = LayerContent::Raster(surface);
        doc.pixel.size = Size { width, height };
        doc.revision += 1;
        self.shown = Some(want);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::tests::demo;
    use lightcraft_engine::catalog::model::PhotoId;

    fn open() -> (Session, Document, Base) {
        lightcraft_engine::gpu::set_enabled(false);
        let (session, id) = demo();
        let doc = Document::from_photo(&session, id).unwrap();
        (session, doc, Base::new())
    }

    /// Sync until the base is up to date (the render runs on a thread).
    fn settle(base: &mut Base, session: &mut Session, doc: &mut Document) {
        let t0 = Instant::now();
        loop {
            base.sync(session, doc).unwrap();
            if base.up_to_date(doc) {
                return;
            }
            assert!(t0.elapsed() < Duration::from_secs(60), "render never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Mean of the composited document's red, green and blue (0..1).
    fn mean_rgb(doc: &Document) -> f32 {
        let b = photocraft_compose::flatten(&doc.pixel);
        b.px.iter().map(|p| (p[0] + p[1] + p[2]) / 3.0).sum::<f32>() / b.px.len() as f32
    }

    #[test]
    fn the_first_sync_renders_the_photo_into_the_base_as_lightcraft_renders_it() {
        let (mut session, mut doc, mut base) = open();
        assert!(doc.base_layer().unwrap().surface().unwrap().tile_count() == 0);
        settle(&mut base, &mut session, &mut doc);
        let (w, h) = (doc.pixel.size.width as usize, doc.pixel.size.height as usize);
        let theirs = session.render_now(doc.source.photo, w, h).unwrap().image;
        assert_eq!((theirs.width, theirs.height), (w, h));
        let ours = photocraft_compose::flatten(&doc.pixel);
        assert_eq!(ours.px.len(), w * h);
        let worst =
            ours.px.iter().zip(&theirs.data).flat_map(|(o, t)| (0..3).map(move |c| (o[c] * 255.0 - f32::from(t[c])).abs())).fold(0.0, f32::max);
        assert!(worst <= 1.0, "base differs from LightCraft's render by {worst}/255");
        assert!(ours.px.iter().all(|p| p[3] == 1.0), "the base is opaque");
        assert!(doc.revision > 0 && doc.unsaved());
    }

    #[test]
    fn changing_develop_settings_in_the_catalog_re_renders_the_base() {
        let (mut session, mut doc, mut base) = open();
        settle(&mut base, &mut session, &mut doc);
        let before = mean_rgb(&doc);
        let (id, rev) = (doc.source.photo, doc.revision);
        let mut s = (*session.develop_of(id).unwrap()).clone();
        s.light.exposure = 2.0;
        session.set_develop(id, s.clone(), "Exposure").unwrap();
        base.sync(&mut session, &mut doc).unwrap();
        assert_eq!((doc.develop.light.exposure, base.rendering(), base.up_to_date(&doc)), (2.0, true, false));
        assert!(doc.revision > rev, "mirroring the settings is a change");
        settle(&mut base, &mut session, &mut doc);
        assert!(mean_rgb(&doc) > before + 0.05, "{before} → {}", mean_rgb(&doc));
        // and without a sync nothing would have moved
        let (mut other_session, mut other, mut other_base) = open();
        settle(&mut other_base, &mut other_session, &mut other);
        other_session.set_develop(id, s, "Exposure").unwrap();
        assert!((mean_rgb(&other) - before).abs() < 1e-6);
    }

    #[test]
    fn a_slider_drag_renders_once_at_release() {
        let (mut session, mut doc, mut base) = open();
        settle(&mut base, &mut session, &mut doc);
        let id = doc.source.photo;
        session.selection.active = Some(id);
        session.begin_interaction("Exposure").unwrap();
        for e in [0.5, 1.0, 1.5] {
            let mut s = (*session.develop_of(id).unwrap()).clone();
            s.light.exposure = e;
            session.set_develop(id, s, "Exposure").unwrap();
            base.sync(&mut session, &mut doc).unwrap();
            assert_eq!(doc.develop.light.exposure, e, "mirrored during the drag");
            assert!(!base.rendering() && !base.up_to_date(&doc), "no render while dragging");
        }
        session.end_interaction().unwrap();
        base.sync(&mut session, &mut doc).unwrap();
        assert!(base.rendering());
        settle(&mut base, &mut session, &mut doc);
        assert!(doc.develop.light.exposure == 1.5 && base.up_to_date(&doc));
    }

    #[test]
    fn a_crop_resizes_the_document_with_the_base() {
        let (mut session, mut doc, mut base) = open();
        settle(&mut base, &mut session, &mut doc);
        let full = doc.pixel.size;
        let id = doc.source.photo;
        let mut s = (*session.develop_of(id).unwrap()).clone();
        s.crop.geometry.rect.x1 = 0.5;
        session.set_develop(id, s, "Crop").unwrap();
        settle(&mut base, &mut session, &mut doc);
        let half = doc.pixel.size;
        assert!(half.height == full.height && half.width.abs_diff(full.width / 2) <= 1, "{full:?} → {half:?}");
        let b = photocraft_compose::flatten(&doc.pixel);
        assert_eq!(b.px.len(), (half.width * half.height) as usize);
        assert!(b.px.iter().all(|p| p[3] == 1.0), "no stale pixels outside the crop");
    }

    #[test]
    fn a_photo_gone_from_the_library_is_an_error_not_a_crash() {
        let (mut session, mut doc, mut base) = open();
        doc.source.photo = PhotoId(u64::MAX);
        let e = base.sync(&mut session, &mut doc).unwrap_err();
        assert!(e.contains("isn't in the library"), "{e}");
        assert!(!base.rendering());
    }
}
