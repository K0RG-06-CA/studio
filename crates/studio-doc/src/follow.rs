//! Pixel layers follow a Develop geometry change (rotate, straighten, flip, crop, a change of the
//! render scale), as one recorded PhotoCraft state paired with the Develop step (milestone 5,
//! `docs/m5-plan.md` design §3–§4). Undo of the pair restores the exact pre-follow pixels: nothing
//! is ever resampled twice.
//!
//! The map from the old render's pixels to the new one's goes through LightCraft's own frame
//! ([`Frame`]): output pixel → normalised oriented image → lens-corrected, pre-perspective image →
//! the stored image (the user orientation undone) → the new orientation → the new perspective →
//! the new output. Sampled on a 5 × 5 grid and fitted with an affine on three points; a grid point
//! more than a quarter pixel off means the change isn't affine (a lens or Upright change): the
//! layers then stay where they are, and the notice says so. A whole-pixel translation (a crop) is
//! done exactly, as milestone 4 did; anything else runs PhotoCraft's own `edit.transform` per
//! top-level layer (groups move as one), nearest-neighbour for right angles and flips with whole
//! offsets, bicubic otherwise. The per-layer steps run under a copy of PhotoCraft's history, which
//! is put back afterwards, so the follow is one recorded state ("Develop: Rotate Left") and
//! `past_len()` grows by exactly one.
//!
//! Upstream limitations; each `compile_fail` example compiles, and so its doctest fails, once
//! upstream makes public what it names. The example after each names a public neighbour, so the
//! first can't pass on a typo.
//!
//! **L10** PhotoCraft has no multi-layer `edit.transform`, and the per-layer function under it is
//! `pub(crate)` (`d836e34`): hence one command per top-level layer under a copy of the history.
//!
//! ```compile_fail
//! let _ = photocraft_engine::transform_cmds::transform_layer;
//! ```
//!
//! ```
//! let _ = photocraft_engine::transform_cmds::transform_bounds;
//! ```
//!
//! **L11** LightCraft's lens warp maps rendered → source only (`eeb136b`): layers can't follow a
//! change of the lens corrections (they stay, with a notice). Upright, a homography, is a Studio
//! simplification left for later, not an upstream limit.
//!
//! ```compile_fail
//! let _ = lightcraft_engine::pipeline::optics::Warp::from_source;
//! ```
//!
//! ```
//! let _ = lightcraft_engine::pipeline::optics::Warp::from_settings;
//! ```

use std::sync::Arc;

use lightcraft_engine::pipeline::geometry::Frame;
use lightcraft_geom::Point;
use photocraft_doc::LayerId;
use photocraft_engine::Session;
use photocraft_geom::Size;
use photocraft_raster::Surface;

use crate::Document;
use crate::pixel::{Anchor, put_base, state_of, top_level_others};

/// How the photo's pixels moved from one render to the next.
#[derive(Clone, Debug, PartialEq)]
pub enum Map {
    Same,
    /// By whole pixels (a crop).
    Shift(i32, i32),
    /// `[a, b, c, d, e, f]`: `(x, y) → (a·x + c·y + e, b·x + d·y + f)`, as PhotoCraft's
    /// `edit.transform` takes it.
    Affine([f64; 6]),
    /// Not affine (what changed).
    Warped(&'static str),
}

/// A fitted grid point may be this far off (pixels) and still count as affine.
const FIT: f64 = 0.25;

/// The map from a render of `from` at `size0` to one of `to` at `size1`.
pub fn map(from: &Anchor, size0: Size, to: &Anchor, size1: Size) -> Map {
    if from.src != to.src {
        return Map::Warped("another source");
    }
    if from.lens != to.lens || from.settings.optics != to.settings.optics {
        return Map::Warped("the lens corrections changed");
    }
    if from.same_frame(to) && size0 == size1 {
        return Map::Same;
    }
    let (w0, h0) = (size0.width as usize, size0.height as usize);
    let (w1, h1) = (size1.width as usize, size1.height as usize);
    let f0 = Frame::with_lens(from.src.0, from.src.1, &from.settings, true, from.lens.as_ref());
    let f1 = Frame::with_lens(to.src.0, to.src.1, &to.settings, true, to.lens.as_ref());
    let point = |p: Point| -> Point {
        let n0 = f0.out_to_norm(w0, h0).apply(p);
        let c0 = f0.transformed_to_corrected(n0);
        let (sx, sy) = f0.orient.inverse().map(c0.x, c0.y, 1.0, 1.0);
        let (ox, oy) = f1.orient.map(sx, sy, 1.0, 1.0);
        let n1 = f1.corrected_to_transformed(Point::new(ox, oy));
        f1.norm_to_out(w1, h1).apply(n1)
    };
    let (w, h) = (w0 as f64, h0 as f64);
    let o = point(Point::new(0.0, 0.0));
    let (px, py) = (point(Point::new(w, 0.0)), point(Point::new(0.0, h)));
    let m = [(px.x - o.x) / w, (px.y - o.y) / w, (py.x - o.x) / h, (py.y - o.y) / h, o.x, o.y];
    let fitted = |p: Point| Point::new(m[0] * p.x + m[2] * p.y + m[4], m[1] * p.x + m[3] * p.y + m[5]);
    for i in 0..5 {
        for j in 0..5 {
            let p = Point::new(w * f64::from(i) / 4.0, h * f64::from(j) / 4.0);
            if fitted(p).dist(point(p)) > FIT {
                return Map::Warped("the perspective correction changed");
            }
        }
    }
    let whole = |v: f64| (v - v.round()).abs() < 1e-6;
    if m[..4].iter().zip([1.0, 0.0, 0.0, 1.0]).all(|(a, b)| (a - b).abs() < 1e-9) && whole(m[4]) && whole(m[5]) {
        return match (m[4].round() as i32, m[5].round() as i32) {
            (0, 0) => Map::Same,
            (dx, dy) => Map::Shift(dx, dy),
        };
    }
    Map::Affine(m)
}

/// `m` is a right angle or a flip with whole-pixel offsets: nothing to interpolate.
fn exact(m: &[f64; 6]) -> bool {
    let unit = |v: f64| v.abs() < 1e-9 || (v.abs() - 1.0).abs() < 1e-9;
    let whole = |v: f64| (v - v.round()).abs() < 1e-6;
    m[..4].iter().all(|&v| unit(v)) && whole(m[4]) && whole(m[5])
}

/// What the layers couldn't do, for the notice.
fn stayed(why: &str) -> String {
    format!("Pixel layers didn\u{2019}t follow the Develop change: {why}")
}

impl Document {
    /// The render for the pending Develop step (its anchor `to`) goes in as one recorded state:
    /// the new base pixels and size, and every layer moved with the photo. See the module doc.
    pub(crate) fn follow(&mut self, px: &mut Session, surface: Surface, size: Size, to: Anchor) -> Result<(), String> {
        let label = format!("Develop: {}", self.history.pending_label().unwrap_or("Develop"));
        self.copy_back(px);
        let st = self.state(px).ok_or("PhotoCraft lost the document")?;
        let before = st.doc.clone();
        let from = before.layers.iter().find(|l| crate::is_base(l)).and_then(photocraft_doc::Layer::surface).and_then(|s| self.link.anchor_of(s));
        let others = top_level_others(&before);
        let mut doc = (*before).clone();
        // ponytail: the selection is dropped (a mask would move, an outline wouldn't); upgrade:
        // `select.transformSelection` with the same matrix
        doc.selection = None;
        let mut note = None;
        let map = from.filter(|_| !others.is_empty()).map(|from| map(&from, before.size, &to, size));
        match map {
            None | Some(Map::Same) => {}
            Some(Map::Shift(dx, dy)) => {
                let targets = photocraft_engine::layer_multi_cmds::move_targets(&doc, &others);
                match photocraft_engine::layer_multi_cmds::moved(&doc, &targets, dx, dy) {
                    Ok(m) => doc = m,
                    Err(e) => note = Some(stayed(&e.to_string())),
                }
            }
            Some(Map::Affine(m)) => {
                // a layer the user locked in place stops the move: all of them stay (milestone 4)
                let locked = others.iter().find(|&&id| {
                    let k = doc.effective_locks(id);
                    k.position || k.all
                });
                if let Some(id) = locked {
                    let name = doc.layer(*id).map_or(String::new(), |l| l.name.clone());
                    note = Some(stayed(&format!("layer \u{201c}{name}\u{201d} is position-locked")));
                } else {
                    match self.transform_all(px, doc.clone(), &others, &m) {
                        Ok(d) => doc = d,
                        Err(e) => note = Some(stayed(&e)),
                    }
                }
            }
            Some(Map::Warped(what)) => note = Some(stayed(what)),
        }
        put_base(&mut doc, surface.clone(), size)?;
        // one state: the document before the follow, never coalesced into a nudge before it
        let st = self.state(px).ok_or("PhotoCraft lost the document")?;
        st.doc = before;
        st.coalesce = None;
        px.edit(&label, |d, _| {
            *d = doc;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        let st = self.state(px).ok_or("PhotoCraft lost the document")?;
        st.saved_revision = st.revision;
        st.last_damage = None;
        let now = st.doc.clone();
        self.pixel = (*now).clone();
        self.link.landed(now, surface, size, to);
        self.revision += 1;
        self.reconcile_pixel(px);
        self.history.absorb_follow();
        if let Some(why) = note {
            self.set_notice(why);
        }
        Ok(())
    }

    /// Every layer in `others` transformed by `m` through PhotoCraft's `edit.transform`, from
    /// `doc`, under a copy of the history (the per-layer steps leave no trace); the session is
    /// left as it was. Any error gives it all back.
    fn transform_all(
        &mut self,
        px: &mut Session,
        doc: photocraft_doc::Document,
        others: &[LayerId],
        m: &[f64; 6],
    ) -> Result<photocraft_doc::Document, String> {
        let id = self.link.id().ok_or("PhotoCraft lost the document")?;
        let st = state_of(px, id).ok_or("PhotoCraft lost the document")?;
        let saved = (st.doc.clone(), st.history.clone(), st.active_layer, std::mem::take(&mut st.selected_layers));
        st.doc = Arc::new(doc);
        let interpolation = if exact(m) { "nearest" } else { "bicubic" };
        let r = (|| {
            for &layer in others {
                let st = state_of(px, id).ok_or("PhotoCraft lost the document")?;
                let l = st.doc.layer(layer).ok_or("a layer went missing")?;
                if photocraft_engine::transform_cmds::transform_bounds(&st.doc, l).is_empty() {
                    continue;
                }
                st.active_layer = Some(layer);
                // the layer's pixels (a linked mask follows), whatever the Channels panel targets
                let params = serde_json::json!({"layer": layer.0, "matrix": m, "interpolation": interpolation, "target": "pixels"});
                px.execute("edit.transform", params).map_err(|e| e.to_string())?;
            }
            let st = state_of(px, id).ok_or("PhotoCraft lost the document")?;
            Ok((*st.doc).clone())
        })();
        if let Some(st) = state_of(px, id) {
            (st.doc, st.history, st.active_layer, st.selected_layers) = saved;
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use lightcraft_engine::Session as Lr;
    use lightcraft_engine::catalog::model::PhotoId;
    use lightcraft_engine::develop::DevelopSettings;
    use photocraft_color::PixelFormat;
    use photocraft_doc::{Layer, LayerMask};
    use photocraft_engine::DocState;
    use photocraft_geom::Rect;
    use serde_json::json;

    use super::*;
    use crate::base::Base;
    use crate::pixel::tests::paint;

    /// Both engines, the document on the first demo photo and its base, rendered (600 px).
    struct T {
        lr: Lr,
        px: Session,
        doc: Document,
        base: Base,
    }

    fn open() -> T {
        lightcraft_engine::gpu::set_enabled(false);
        let (lr, id) = crate::tests::demo();
        let mut doc = Document::from_photo(&lr, id).unwrap();
        let mut px = Session::new();
        doc.attach(&mut px);
        let mut base = Base::new();
        base.max_edge = Some(600);
        let mut t = T { lr, px, doc, base };
        t.settle();
        t
    }

    impl T {
        fn id(&self) -> PhotoId {
            self.doc.photo().unwrap()
        }

        /// One frame as the shell runs it: the base synced, PhotoCraft copied back.
        fn frame(&mut self) -> Result<(), String> {
            let r = self.base.sync(&mut self.lr, &mut self.px, &mut self.doc);
            self.doc.copy_back(&mut self.px);
            r
        }

        /// Frames until the base shows the settings and no Develop step waits for its render.
        fn settle(&mut self) {
            let t0 = Instant::now();
            loop {
                self.frame().unwrap();
                if self.base.up_to_date(&self.doc) && !self.doc.history.pending_follow() {
                    return;
                }
                assert!(t0.elapsed() < Duration::from_secs(300), "never settled");
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn develop(&mut self, label: &str, f: impl FnOnce(&mut DevelopSettings)) {
            let mut s = (*self.lr.develop_of(self.id()).unwrap()).clone();
            f(&mut s);
            self.lr.set_develop(self.id(), s, label).unwrap();
        }

        fn rotate_left(&mut self) {
            self.lr.execute("photo.rotateLeft", &json!({"ids": [self.id().0]})).unwrap();
        }

        fn st(&self) -> &DocState {
            self.px.active().unwrap()
        }

        fn pc(&self) -> Arc<photocraft_doc::Document> {
            self.st().doc.clone()
        }

        fn paint(&mut self, name: &str, x: i32, y: i32) {
            paint(&mut self.px, name, x, y);
            self.doc.copy_back(&mut self.px);
        }

        fn size(&self) -> (u32, u32) {
            (self.doc.pixel.size.width, self.doc.pixel.size.height)
        }

        fn undo(&mut self) -> bool {
            self.doc.undo(&mut self.lr, &mut self.px, &mut self.base).unwrap()
        }

        fn redo(&mut self) -> bool {
            self.doc.redo(&mut self.lr, &mut self.px, &mut self.base).unwrap()
        }

        fn rows(&self) -> Vec<(String, bool, bool)> {
            self.doc.history.entries().into_iter().map(|e| (e.label, e.develop, e.pixel)).collect()
        }
    }

    fn layer<'d>(doc: &'d photocraft_doc::Document, name: &str) -> &'d Layer {
        doc.layers.iter().find(|l| l.name == name).unwrap()
    }

    /// Where the painted pixel of layer `name` is (its content's top-left).
    fn dot(doc: &photocraft_doc::Document, name: &str) -> (i32, i32) {
        let b = photocraft_compose::bounds::content_bounds(layer(doc, name).surface().unwrap());
        (b.x0, b.y0)
    }

    /// The most opaque pixel of layer `name` (a resampled dot spreads; its centre is the peak).
    fn peak(doc: &photocraft_doc::Document, name: &str) -> (i32, i32) {
        let s = layer(doc, name).surface().unwrap();
        let b = photocraft_compose::bounds::content_bounds(s);
        let mut px = vec![[0.0f32; 4]; (b.width() * b.height()) as usize];
        s.read_rgba_into(b, &mut px);
        let i = (0..px.len()).max_by(|&i, &j| px[i][3].total_cmp(&px[j][3])).unwrap();
        (b.x0 + (i as u32 % b.width()) as i32, b.y0 + (i as u32 / b.width()) as i32)
    }

    fn base_surface(doc: &photocraft_doc::Document) -> &Surface {
        doc.layers.iter().find(|l| crate::is_base(l)).unwrap().surface().unwrap()
    }

    fn base_px(doc: &photocraft_doc::Document, x: i32, y: i32) -> [f32; 4] {
        let mut px = [[0.0; 4]];
        doc.layers.iter().find(|l| crate::is_base(l)).unwrap().surface().unwrap().read_rgba_into(Rect::from_xywh(x, y, 1, 1), &mut px);
        px[0]
    }

    fn near(a: [f32; 4], b: [f32; 4]) -> bool {
        (0..3).all(|c| (a[c] - b[c]).abs() < 0.03)
    }

    fn row(label: &str, develop: bool, pixel: bool) -> (String, bool, bool) {
        (label.to_owned(), develop, pixel)
    }

    #[test]
    fn a_rotation_moves_pixel_layers_with_the_photo_as_one_recorded_state() {
        let mut t = open();
        assert_eq!(t.size(), (600, 400));
        t.paint("Paint", 300, 100);
        let under = base_px(&t.pc(), 300, 100);
        let past = t.st().history.past_len();
        t.rotate_left();
        t.settle();
        assert_eq!(t.size(), (400, 600));
        // rotate left: (x, y) → (y, w − 1 − x), exactly (nearest neighbour, nothing to interpolate)
        assert_eq!(dot(&t.pc(), "Paint"), (100, 299));
        assert_eq!(dot(&t.doc.pixel, "Paint"), (100, 299), "copied back too");
        assert!(near(base_px(&t.pc(), 100, 299), under), "still over the same photo pixel");
        // one state, the per-layer transforms left none
        assert_eq!(t.st().history.past_len(), past + 1);
        assert_eq!(t.st().history.entries().last().map(String::as_str), Some("Develop: Rotate Left"));
        assert_eq!(t.rows(), [row("Original", false, false), row("Paint", false, true), row("Rotate Left", true, true)]);
        assert_eq!(t.doc.take_notice(), None);
    }

    #[test]
    fn a_flip_and_a_crop_that_changes_the_render_scale_keep_layers_on_their_photo_pixels() {
        let mut t = open();
        t.paint("Paint", 450, 300);
        let under = base_px(&t.pc(), 450, 300);
        t.lr.execute("photo.flipHorizontal", &json!({"ids": [t.id().0]})).unwrap();
        t.settle();
        assert_eq!(dot(&t.pc(), "Paint"), (149, 300), "mirrored exactly");
        assert!(near(base_px(&t.pc(), 149, 300), under));
        // keep the half of the frame the dot is in (LightCraft's crop rectangle is in the frame
        // before its flips, so the unflipped right half): the long side is the height now, so the
        // render scale grows by 1.5 and the layer is resampled (bicubic)
        t.develop("Crop", |s| s.crop.geometry.rect.x0 = 0.5);
        t.settle();
        assert_eq!(t.size(), (450, 600));
        // centre (450.5, 300.5) → cropped and scaled (225.75, 450.75) → flipped (224.25, 450.75)
        let (x, y) = peak(&t.pc(), "Paint");
        assert!((x - 224).abs() <= 1 && (y - 450).abs() <= 1, "({x}, {y})");
        assert!(near(base_px(&t.pc(), x, y), under), "over the same photo pixel");
        assert_eq!(t.doc.take_notice(), None);
        assert_eq!(t.st().history.entries().iter().filter(|e| e.starts_with("Develop: ")).count(), 2, "one state each");
    }

    /// The ask for task 2: a paired undo or redo checks both halves can run, does the Pixel half,
    /// and when the Develop commit then fails, puts the Pixel half back: both engines stay at the
    /// step, the step is kept, and the notice says why.
    #[test]
    fn a_paired_step_whose_develop_commit_fails_stays_where_it_was_in_both_engines() {
        let mut t = open();
        let id = t.id();
        t.paint("Paint", 300, 100);
        let unrotated = t.pc();
        t.rotate_left();
        t.settle();
        let rotated = t.pc();
        let (rows, position, past) = (t.rows(), t.doc.history.position(), t.st().history.past_len());
        let settings = t.lr.develop_of(id).unwrap();
        // the commit fails: the photo isn't in the library as far as it is concerned
        t.doc.source.as_mut().unwrap().photo = PhotoId(u64::MAX);
        let e = t.doc.undo(&mut t.lr, &mut t.px, &mut t.base).unwrap_err();
        assert!(Arc::ptr_eq(&t.pc(), &rotated), "the Pixel half was redone back: the same document");
        assert_eq!(t.lr.develop_of(id).unwrap(), settings, "the Develop half never moved");
        assert_eq!((t.rows(), t.doc.history.position(), t.doc.history.can_redo()), (rows.clone(), position, false), "the step is kept");
        assert_eq!((t.st().history.past_len(), t.st().history.can_redo()), (past, false));
        let notice = t.doc.take_notice().unwrap();
        assert!(notice.starts_with("Couldn\u{2019}t undo \u{201c}Rotate Left\u{201d}") && notice.contains(&e), "{notice}");
        // working again: both halves go back together
        t.doc.source.as_mut().unwrap().photo = id;
        assert!(t.undo());
        assert!(Arc::ptr_eq(&t.pc(), &unrotated), "the exact pre-follow document");
        assert_eq!(t.lr.develop_of(id).unwrap().orientation, Default::default());
        // the same on the redo side
        t.doc.source.as_mut().unwrap().photo = PhotoId(u64::MAX);
        assert!(t.doc.redo(&mut t.lr, &mut t.px, &mut t.base).is_err());
        assert!(Arc::ptr_eq(&t.pc(), &unrotated), "the Pixel half was undone back");
        assert_eq!(t.lr.develop_of(id).unwrap().orientation, Default::default());
        assert_eq!((t.doc.history.position(), t.doc.history.can_redo()), (position - 1, true), "still redoable");
        assert!(t.st().history.can_redo());
        assert!(t.doc.take_notice().unwrap().starts_with("Couldn\u{2019}t redo \u{201c}Rotate Left\u{201d}"));
        t.doc.source.as_mut().unwrap().photo = id;
        assert!(t.redo());
        assert!(Arc::ptr_eq(&t.pc(), &rotated));
        assert_eq!(t.lr.develop_of(id).unwrap(), settings);
        assert_eq!(t.doc.take_notice(), None);
    }

    #[test]
    fn straighten_there_and_back_undoes_bit_identical_without_a_render() {
        let mut t = open();
        let mut sel = Surface::new(PixelFormat::GRAY8);
        sel.write_region(Rect::from_xywh(90, 90, 80, 60), &[1.0; 80 * 60]);
        t.px.edit("Masked", |d, active| {
            let mut l = Layer::raster("Masked", PixelFormat::RGBA16);
            l.surface_mut().unwrap().write_region(Rect::from_xywh(100, 100, 50, 40), &[1.0, 0.0, 0.0, 1.0].repeat(50 * 40));
            let mut m = LayerMask::reveal_all();
            m.surface.write_region(Rect::from_xywh(110, 110, 10, 10), &[0.0; 100]);
            l.mask = Some(m);
            *active = Some(l.id);
            d.layers.push(l);
            d.selection = Some(sel);
            Ok(())
        })
        .unwrap();
        t.doc.copy_back(&mut t.px);
        let start = t.pc();
        assert!(start.selection.is_some());
        t.develop("Straighten", |s| s.crop.geometry.angle = 5.0);
        t.settle();
        let mid = t.pc();
        assert!(mid.selection.is_none(), "the selection is gone after a follow");
        assert_ne!(layer(&mid, "Masked").mask, layer(&start, "Masked").mask, "the mask moved with its layer");
        t.develop("Straighten", |s| s.crop.geometry.angle = 0.0);
        t.settle();
        let started = t.base.started();
        assert!(t.undo());
        assert!(Arc::ptr_eq(&t.pc(), &mid));
        assert!(t.undo());
        assert!(Arc::ptr_eq(&t.pc(), &start), "raster, mask, selection and canvas size: the very document");
        assert_eq!(t.doc.pixel, *start, "and Studio's copy");
        for _ in 0..3 {
            t.frame().unwrap();
        }
        assert_eq!(t.base.started(), started, "no render: the undone-to state's base shows the settings");
        assert!(t.base.up_to_date(&t.doc));
        assert_eq!(t.lr.develop_of(t.id()).unwrap().crop.geometry.angle, 0.0);
        assert_eq!(t.doc.take_notice(), None);
    }

    #[test]
    fn develop_changes_before_the_render_lands_join_the_waiting_step() {
        let mut t = open();
        t.paint("Paint", 300, 100);
        let (start, past) = (t.pc(), t.st().history.past_len());
        t.develop("Straighten", |s| s.crop.geometry.angle = 5.0);
        t.frame().unwrap();
        assert!(t.base.rendering() && t.doc.history.pending_follow());
        t.develop("Straighten", |s| s.crop.geometry.angle = 10.0);
        t.develop("Exposure", |s| s.light.exposure = 0.5);
        t.settle();
        assert_eq!(t.rows().len(), 3, "one step for all three: {:?}", t.rows());
        assert_eq!(t.st().history.past_len(), past + 1, "one state");
        assert!(t.undo());
        assert!(Arc::ptr_eq(&t.pc(), &start), "0\u{b0} pixels, exactly");
        let s = t.lr.develop_of(t.id()).unwrap();
        assert_eq!((s.crop.geometry.angle, s.light.exposure), (0.0, 0.0), "back to the checkpoint");
    }

    #[test]
    fn a_render_that_fails_undoes_the_waiting_step_and_leaves_pixel_alone() {
        lightcraft_engine::gpu::set_enabled(false);
        let (dir, mut lr, id) = crate::history::tests::on_disk("follow-fails");
        let mut doc = Document::from_photo(&lr, id).unwrap();
        let mut px = Session::new();
        doc.attach(&mut px);
        doc.note_develop(&lr);
        paint(&mut px, "Paint", 0, 0);
        doc.copy_back(&mut px);
        let st = px.active().unwrap();
        let (pixels, past, rows) = (st.doc.clone(), st.history.past_len(), lr.catalog.photo(id).unwrap().history.clone());
        // the file goes before any render; a rotation waits for one
        std::fs::remove_file(dir.join("one.png")).unwrap();
        lr.execute("photo.rotateLeft", &json!({"ids": [id.0]})).unwrap();
        let mut base = Base::new();
        let t0 = Instant::now();
        let e = loop {
            if let Err(e) = base.sync(&mut lr, &mut px, &mut doc) {
                break e;
            }
            assert!(t0.elapsed() < Duration::from_secs(60), "the render never failed");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(!doc.history.pending_follow());
        assert_eq!(lr.develop_of(id).unwrap().orientation, Default::default(), "the settings went back");
        assert_eq!(lr.catalog.photo(id).unwrap().history, rows, "and the photo's History");
        assert_eq!(doc.develop.orientation, Default::default());
        let st = px.active().unwrap();
        assert!(Arc::ptr_eq(&st.doc, &pixels) && st.history.past_len() == past, "Pixel untouched");
        assert_eq!(doc.history.entries().iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), ["Original", "Paint"], "no step");
        let notice = doc.take_notice().unwrap();
        assert!(
            notice.starts_with("Develop couldn\u{2019}t render \u{201c}Rotate Left\u{201d}: the edit was undone") && notice.contains(&e),
            "{notice}"
        );
        lr.close_library().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_redo_adopts_its_states_render_when_it_shows_the_settings_else_keeps_the_layers_and_renders() {
        let mut t = open();
        t.paint("Paint", 300, 100);
        t.rotate_left();
        t.settle();
        let rotated = t.doc.develop.hash64();
        // undo and redo the rotation with nothing between: the redone state shows its own render
        assert!(t.undo());
        t.settle();
        let started = t.base.started();
        assert!(t.redo());
        assert_eq!(t.doc.shown_hash(), Some(rotated), "adopted");
        t.frame().unwrap();
        assert!(t.base.up_to_date(&t.doc) && t.base.started() == started, "no render");
        // an exposure renders into the rotated document (a host write, not a state); undone
        // together with the rotation, the redone state shows that exposure, not the target's
        t.develop("Exposure", |s| s.light.exposure = 1.0);
        t.settle();
        t.doc.goto(&mut t.lr, &mut t.px, &mut t.base, 1).unwrap();
        t.settle();
        let past = t.st().history.past_len();
        let started = t.base.started();
        assert!(t.redo());
        assert_eq!(t.doc.shown_hash(), None, "stale: its render is of other settings");
        let at = dot(&t.pc(), "Paint");
        t.settle();
        assert_eq!(t.base.started(), started + 1, "rendered once");
        assert_eq!((t.doc.shown_hash(), dot(&t.pc(), "Paint")), (Some(rotated), at), "the layers stayed as the state has them");
        assert_eq!(t.st().history.past_len(), past + 1, "the render is no state");
        assert_eq!(t.doc.take_notice(), None);
    }

    /// The plan's case as written (task 2): rotate → exposure (rendered) → goto before both →
    /// redo the rotation → the base shows the rotation with the original exposure, not the
    /// exposure render the redone state carried.
    #[test]
    fn rotate_exposure_goto_before_both_then_redo_the_rotation_shows_the_original_exposure() {
        let mut t = open();
        t.paint("Paint", 300, 100);
        t.rotate_left();
        t.settle();
        let (rotated, original, orientation) = (t.doc.develop.hash64(), t.pc(), t.doc.develop.orientation);
        t.develop("Exposure", |s| s.light.exposure = 1.0);
        t.settle();
        assert_ne!(t.doc.shown_hash(), Some(rotated), "the exposure is rendered");
        t.doc.goto(&mut t.lr, &mut t.px, &mut t.base, 1).unwrap();
        t.settle();
        assert!(t.redo());
        t.settle();
        let s = t.lr.develop_of(t.id()).unwrap();
        assert_eq!((s.light.exposure, s.orientation), (0.0, orientation));
        assert_eq!(t.doc.shown_hash(), Some(rotated), "the base shows the rotation's own settings");
        let now = t.pc();
        let (ours, want) = (base_surface(&now), base_surface(&original));
        assert_eq!((t.size(), dot(&t.pc(), "Paint")), ((400, 600), (100, 299)));
        let mut a = vec![[0.0f32; 4]; 400 * 600];
        let mut b = a.clone();
        ours.read_rgba_into(Rect::from_xywh(0, 0, 400, 600), &mut a);
        want.read_rgba_into(Rect::from_xywh(0, 0, 400, 600), &mut b);
        let worst = a.iter().zip(&b).flat_map(|(p, q)| (0..3).map(move |c| (p[c] - q[c]).abs())).fold(0.0, f32::max);
        assert!(worst <= 2.0 / 65535.0, "the original exposure's pixels: off by {} LSB", worst * 65535.0);
        assert_eq!(t.doc.take_notice(), None);
    }

    /// Unique tile bytes over PhotoCraft's past states and its current document, counted here as
    /// upstream's `tile_bytes` does (layers, masks, channels, Quick Mask), independently of it.
    fn retained(st: &DocState) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut add = |s: &Surface| s.tiles().filter(|(_, t)| seen.insert(Arc::as_ptr(t) as usize)).map(|(_, t)| t.bytes().len()).sum::<usize>();
        let docs: Vec<_> = (0..st.history.past_len()).filter_map(|i| st.history.state(i)).chain([st.doc.clone()]).collect();
        let mut n = 0;
        for d in &docs {
            for (_, _, l) in d.walk() {
                n += l.surface().map_or(0, &mut add) + l.mask.as_ref().map_or(0, |m| add(&m.surface));
            }
            n += d.channels.iter().chain(&d.quick_mask).map(|c| add(&c.surface)).sum::<usize>();
        }
        n
    }

    /// Design §5: a geometry step's state holds a whole render, so the budget holds a handful.
    /// The cap is to a 600 px render (6 tiles) what 1 GiB is to a 24 MP one (384 tiles): the
    /// current document and four states fit either way (24 MP: 192 + 4 × 192 = 960 MiB ≤ 1024).
    /// The oldest steps go as a contiguous prefix (Develop steps older than the oldest surviving
    /// state with them), the bytes stay under the cap, undo stops at the oldest kept step, and
    /// nothing says "reset". Then a cap under two renders: every follow's own trim takes all
    /// older states, and the step it records is still paired, not a reset.
    #[test]
    fn geometry_steps_at_24_mp_scale_keep_four_and_drop_the_oldest_as_a_prefix() {
        let mut t = open();
        let render: usize = base_surface(&t.pc()).tiles().map(|(_, tile)| tile.bytes().len()).sum();
        assert_eq!(render, 6 * crate::pixel::tests::TILE, "a 600 \u{d7} 400 render is 6 tiles");
        let cap = crate::history::HISTORY_BYTES / 384 * 6;
        t.doc.history.bytes_cap = cap;
        let mut labels = Vec::new();
        for i in 0..10 {
            let e = format!("Exposure {i}");
            t.develop(&e, |s| s.light.exposure = f64::from(i + 1) * 0.1);
            t.settle();
            t.rotate_left();
            t.settle();
            labels.extend([e, "Rotate Left".to_owned()]);
            let bytes = retained(t.st());
            assert!(bytes <= cap, "after step {i}: {bytes} bytes over the cap of {cap}");
            assert_eq!(t.doc.take_notice(), None, "after step {i}");
        }
        let rows: Vec<String> = t.rows().into_iter().skip(1).map(|r| r.0).collect();
        assert_eq!(rows[..], labels[labels.len() - 7..], "four geometry steps and the Develop steps between them, a contiguous suffix");
        assert_eq!((t.st().history.past_len(), retained(t.st())), (4, 5 * render), "four states and the current document, a render each");
        // undo walks back to the oldest kept step and stops there
        for i in 0..rows.len() {
            assert!(t.undo(), "undo {i}");
        }
        assert!(!t.undo(), "the history ends at the oldest kept step");
        assert_eq!(t.lr.develop_of(t.id()).unwrap().light.exposure, f64::from(7) * 0.1, "before the oldest kept rotation, after its exposure");
        t.settle();
        assert_eq!(t.doc.take_notice(), None);
        // a cap under two renders: one state at a time, each follow's trim takes the one before
        let mut t = open();
        t.doc.history.bytes_cap = render + render / 2;
        for i in 0..3 {
            t.rotate_left();
            t.settle();
            assert_eq!(t.doc.take_notice(), None, "rotation {i}: a prune, not a reset");
            assert_eq!(t.rows(), [row("Original", false, false), row("Rotate Left", true, true)], "rotation {i}");
        }
        let started = t.base.started();
        assert!(t.undo());
        assert!(!t.undo(), "nothing older is left");
        for _ in 0..3 {
            t.frame().unwrap();
        }
        assert_eq!((t.base.started(), t.doc.take_notice()), (started, None), "no render: the kept state's base is its own");
    }

    #[test]
    fn strokes_between_geometry_changes_undo_and_redo_through_the_exact_states() {
        let mut t = open();
        let mut seen = vec![t.pc()];
        t.paint("A", 10, 10);
        seen.push(t.pc());
        t.rotate_left();
        t.settle();
        seen.push(t.pc());
        t.paint("B", 20, 30);
        seen.push(t.pc());
        t.lr.execute("photo.flipVertical", &json!({"ids": [t.id().0]})).unwrap();
        t.settle();
        seen.push(t.pc());
        t.paint("C", 5, 5);
        seen.push(t.pc());
        for (i, want) in seen.iter().enumerate().rev().skip(1) {
            assert!(t.undo());
            assert!(Arc::ptr_eq(&t.pc(), want), "undo to {i}");
        }
        for (i, want) in seen.iter().enumerate().skip(1) {
            assert!(t.redo());
            assert_eq!(*t.pc(), **want, "redo to {i}");
        }
        assert_eq!(t.doc.take_notice(), None);
    }

    #[test]
    fn a_layer_at_the_canvas_edge_survives_a_rotation_and_its_undo_whole() {
        let mut t = open();
        t.px.edit("Edge", |d, active| {
            let mut l = Layer::raster("Edge", PixelFormat::RGBA16);
            l.surface_mut().unwrap().write_region(Rect::from_xywh(590, 0, 10, 10), &[0.0, 1.0, 0.0, 1.0].repeat(100));
            *active = Some(l.id);
            d.layers.push(l);
            Ok(())
        })
        .unwrap();
        t.doc.copy_back(&mut t.px);
        let start = t.pc();
        t.rotate_left();
        t.settle();
        let b = photocraft_compose::bounds::content_bounds(layer(&t.pc(), "Edge").surface().unwrap());
        assert_eq!((b.x0, b.y0, b.width(), b.height()), (0, 0, 10, 10), "the corner turned to the top-left, nothing clipped");
        assert!(t.undo());
        assert_eq!(layer(&t.pc(), "Edge"), layer(&start, "Edge"));
    }

    #[test]
    fn a_locked_layer_a_refused_transform_or_a_perspective_change_moves_nothing_but_the_state_is_recorded() {
        // a position-locked layer: none move (milestone 4's rule)
        let mut t = open();
        t.paint("Free", 300, 100);
        t.paint("Pinned", 10, 10);
        t.px.edit("Lock", |d, _| {
            let id = d.layers.iter().find(|l| l.name == "Pinned").unwrap().id;
            d.layer_mut(id).unwrap().locks.position = true;
            Ok(())
        })
        .unwrap();
        t.doc.copy_back(&mut t.px);
        let past = t.st().history.past_len();
        t.rotate_left();
        t.settle();
        assert_eq!((dot(&t.pc(), "Free"), t.size()), ((300, 100), (400, 600)), "nothing moved, the base did");
        assert_eq!(t.st().history.past_len(), past + 1);
        assert!(t.doc.take_notice().unwrap().contains("\u{201c}Pinned\u{201d} is position-locked"));
        assert!(t.undo());
        assert_eq!(t.size(), (600, 400));
        // a group whose child is locked: PhotoCraft refuses its transform, and every layer stays
        let mut t = open();
        t.paint("Free", 300, 100);
        t.px.edit("Group", |d, _| {
            let mut child = Layer::raster("Child", PixelFormat::RGBA16);
            child.surface_mut().unwrap().write_region(Rect::from_xywh(50, 50, 1, 1), &[1.0; 4]);
            child.locks.position = true;
            d.layers.push(Layer::group("Group", vec![child]));
            Ok(())
        })
        .unwrap();
        t.doc.copy_back(&mut t.px);
        t.rotate_left();
        t.settle();
        assert_eq!(dot(&t.pc(), "Free"), (300, 100), "rolled back as a whole");
        assert!(t.doc.take_notice().unwrap().contains("is locked"));
        // a perspective change isn't affine: the layers stay, and say so
        let mut t = open();
        t.paint("Free", 300, 100);
        t.develop("Vertical", |s| s.geometry.vertical = 40.0);
        t.settle();
        assert_eq!(dot(&t.pc(), "Free"), (300, 100));
        assert!(t.doc.take_notice().unwrap().contains("perspective"));
    }

    #[test]
    fn a_straighten_drag_follows_once_from_the_pre_drag_pixels() {
        let mut dragged = open();
        dragged.paint("Paint", 300, 100);
        let id = dragged.id();
        dragged.lr.selection.active = Some(id);
        dragged.lr.begin_interaction("Straighten").unwrap();
        let started = dragged.base.started();
        for a in [1.0, 3.0, 5.0] {
            dragged.develop("Straighten", |s| s.crop.geometry.angle = a);
            dragged.frame().unwrap();
            assert!(!dragged.base.rendering() && !dragged.doc.history.pending_follow(), "nothing while dragging");
        }
        dragged.lr.end_interaction().unwrap();
        dragged.settle();
        assert_eq!(dragged.base.started(), started + 1, "one render");
        assert_eq!(dragged.rows().len(), 3, "one step");
        let mut once = open();
        once.paint("Paint", 300, 100);
        once.develop("Straighten", |s| s.crop.geometry.angle = 5.0);
        once.settle();
        assert_eq!(layer(&dragged.pc(), "Paint").surface(), layer(&once.pc(), "Paint").surface(), "resampled once, as a one-shot straighten");
    }

    #[test]
    fn a_render_for_another_generation_lands_nowhere() {
        let mut t = open();
        t.develop("Exposure", |s| s.light.exposure = 1.0);
        t.frame().unwrap();
        assert!(t.base.rendering());
        // the document gets a new home meanwhile (attached again)
        t.doc.attach(&mut t.px);
        let first_tile = |t: &T| t.pc().layers[0].surface().unwrap().tiles().next().map(|(_, tile)| Arc::as_ptr(tile) as usize);
        let shown = first_tile(&t);
        let started = t.base.started();
        let t0 = Instant::now();
        while t.base.started() == started {
            t.base.sync(&mut t.lr, &mut t.px, &mut t.doc).unwrap();
            assert!(t0.elapsed() < Duration::from_secs(300));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(first_tile(&t), shown, "the old generation's render was dropped (and a new one started)");
    }

    #[test]
    fn a_rotation_after_a_save_is_unsaved() {
        let mut t = open();
        t.doc.saved_revision = t.doc.revision;
        t.rotate_left();
        t.settle();
        assert!(t.doc.unsaved());
    }

    /// Review: the wait is `flush`'s own, so an undo (what Ctrl+Z runs) during a pending render
    /// completes once it has landed: the follow is recorded, then the whole step undone. Both
    /// with the render not even started (no frame since the rotation) and with it in flight.
    #[test]
    fn undo_during_a_pending_render_waits_for_it_then_undoes_the_whole_step() {
        for in_flight in [false, true] {
            let mut t = open();
            t.paint("Paint", 300, 100);
            let start = t.pc();
            t.rotate_left();
            if in_flight {
                t.frame().unwrap();
                assert!(t.base.rendering() && t.doc.history.pending_follow());
            }
            assert!(t.doc.undo(&mut t.lr, &mut t.px, &mut t.base).unwrap(), "in flight: {in_flight}");
            assert!(!t.doc.history.pending_follow());
            assert!(Arc::ptr_eq(&t.pc(), &start), "the exact pre-follow document");
            assert_eq!(t.lr.develop_of(t.id()).unwrap().orientation, Default::default());
            assert!(t.doc.redo(&mut t.lr, &mut t.px, &mut t.base).unwrap());
            assert_eq!(dot(&t.pc(), "Paint"), (100, 299), "the follow it waited for comes back");
            assert_eq!(t.doc.take_notice(), None);
        }
    }

    /// Review: the wait has a ceiling (30 s; zero here): a render past it undoes the waiting step,
    /// Pixel stays as it was, and the undo that waited is the error.
    #[test]
    fn a_render_past_the_ceiling_undoes_the_waiting_step() {
        let mut t = open();
        t.paint("Paint", 300, 100);
        let (start, rows) = (t.pc(), t.rows());
        t.base.ceiling = Some(Duration::ZERO);
        t.rotate_left();
        let e = t.doc.undo(&mut t.lr, &mut t.px, &mut t.base).unwrap_err();
        assert!(e.contains("longer than"), "{e}");
        assert!(!t.doc.history.pending_follow());
        assert_eq!(t.lr.develop_of(t.id()).unwrap().orientation, Default::default(), "the rotation was undone");
        assert!(Arc::ptr_eq(&t.pc(), &start), "Pixel untouched");
        assert_eq!(t.rows(), rows, "the step is gone, the one before it stays");
        let notice = t.doc.take_notice().unwrap();
        assert!(notice.starts_with("Develop couldn\u{2019}t render \u{201c}Rotate Left\u{201d}: the edit was undone"), "{notice}");
    }

    #[test]
    fn finish_waits_for_the_render_and_lands_it() {
        let mut t = open();
        t.paint("Paint", 300, 100);
        t.rotate_left();
        t.base.sync(&mut t.lr, &mut t.px, &mut t.doc).unwrap();
        assert!(t.doc.history.pending_follow());
        t.base.finish(&mut t.lr, &mut t.px, &mut t.doc).unwrap();
        assert!(!t.doc.history.pending_follow());
        assert_eq!((t.size(), dot(&t.doc.pixel, "Paint")), ((400, 600), (100, 299)));
    }
}
