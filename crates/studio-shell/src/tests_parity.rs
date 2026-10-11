//! UI parity harness (T1, `docs/ui-parity-plan.md` § 4): checks that a persona is its storytold
//! app as the app ships, with no person looking.
//!
//! (a) headless: every labelled accessible node (role, label, bounds) of Studio, moved up by the
//!     persona bar, equals the bare app's of the same state, outside allow-listed regions; plus
//!     the ctx's style (visuals, fonts, both apps' tokens);
//! (b) GPU, opt-in: Studio's render with the persona bar cropped off, diffed per pixel against
//!     the bare app's (`render_parity`, PNGs in `target/renders/parity/`).
//!
//! The bare apps are hosted as Studio hosts them (`logic` then the whole-window `ui`), set up as
//! each app sets itself up. The real cases land with T2 (Library, Develop) and T3 (Pixel); until
//! then the harness checks itself, here and in the T0 spike.

use egui::{Rect, Ui};
use egui_kittest::kittest::{NodeT, Queryable, by};
use egui_kittest::{Harness, HarnessBuilder};
use lightcraft_ui_egui::{LightcraftApp, Services};
use photocraft_ui_egui::PhotocraftApp;
use std::collections::BTreeMap;

/// Studio's window (points).
pub(crate) const SIZE: [f32; 2] = [1400.0, 900.0];

/// Studio's persona bar height (points).
pub(crate) fn bar_h() -> f32 {
    crate::theme::TOKENS.top_bar_h
}

/// The transparent margin kittest puts around a `build_ui_state` app (points), above the persona
/// bar in Studio's harness as above the bare app's top bar.
const KITTEST_MARGIN: f32 = 8.0;

/// The storytold app a persona is.
#[derive(Clone, Copy, Debug)]
pub(crate) enum App {
    LightCraft,
    PhotoCraft,
}

/// A region where Studio may differ from the bare app, and why (bare app's points).
pub(crate) struct Allow {
    pub rect: Rect,
    pub reason: &'static str,
}

/// A stand-in for Studio's persona bar: an empty panel of [`bar_h`].
fn bar(ui: &mut Ui) {
    egui::Panel::top("parity.bar").exact_size(bar_h()).resizable(false).frame(egui::Frame::NONE).show(ui, |_| {});
}

/// The size of the area under the bar, or of the whole window.
fn size(under_bar: bool) -> [f32; 2] {
    if under_bar { SIZE } else { [SIZE[0], SIZE[1] - bar_h()] }
}

/// LightCraft on its demo library with the first photo picked (no GPU work).
pub(crate) fn demo_lightcraft() -> LightcraftApp {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    app.session.selection.active = app.session.catalog.photos().next().map(|p| p.id);
    app
}

/// The bare LightCraft window (`under_bar`: drawn under a stand-in persona bar, at Studio's size).
pub(crate) fn lightcraft(builder: HarnessBuilder<LightcraftApp>, app: LightcraftApp, under_bar: bool) -> Harness<'static, LightcraftApp> {
    let mut fresh = false;
    builder.with_size(size(under_bar)).build_ui_state(
        move |ui, app: &mut LightcraftApp| {
            let ctx = ui.ctx().clone();
            if !std::mem::replace(&mut fresh, true) {
                // egui as its desktop app gets it from eframe, not kittest's test style (no
                // animations, no cursor blink, the dark theme pinned)
                ctx.set_theme(egui::ThemePreference::System);
                ctx.options_mut(|o| {
                    o.dark_style = std::sync::Arc::new(egui::Theme::Dark.default_style());
                    o.light_style = std::sync::Arc::new(egui::Theme::Light.default_style());
                });
            }
            if under_bar {
                bar(ui);
            }
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    )
}

/// PhotoCraft with the eframe frame its `logic`/`ui` take (L5).
pub(crate) struct Pc {
    pub app: PhotocraftApp,
    frame: eframe::Frame,
    set_up: bool,
}

/// PhotoCraft with Studio's test document open.
pub(crate) fn demo_photocraft() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
    app.session.add_document(crate::tests_photocraft::pixel_doc(), None);
    app.sync_views();
    app
}

/// The bare PhotoCraft window, set up on its first frame as its desktop app is (default theme).
pub(crate) fn photocraft(builder: HarnessBuilder<Pc>, app: PhotocraftApp, under_bar: bool) -> Harness<'static, Pc> {
    builder.with_size(size(under_bar)).build_ui_state(
        move |ui, pc: &mut Pc| {
            let ctx = ui.ctx().clone();
            if !std::mem::replace(&mut pc.set_up, true) {
                // egui as its desktop app gets it from eframe (see `lightcraft`)
                ctx.set_theme(egui::ThemePreference::System);
                ctx.options_mut(|o| {
                    o.dark_style = std::sync::Arc::new(egui::Theme::Dark.default_style());
                    o.light_style = std::sync::Arc::new(egui::Theme::Light.default_style());
                });
                PhotocraftApp::setup_context(&ctx, Default::default());
            }
            if under_bar {
                bar(ui);
            }
            eframe::App::logic(&mut pc.app, &ctx, &mut pc.frame);
            eframe::App::ui(&mut pc.app, ui, &mut pc.frame);
        },
        Pc { app, frame: eframe::Frame::_new_kittest(), set_up: false },
    )
}

/// Run frames until `busy` has been false for 12 in a row (background renders done).
pub(crate) fn settle<S>(h: &mut Harness<'_, S>, busy: impl Fn(&S) -> bool) {
    // as `settle_base`: a 2-core CI runner can keep a render waiting over a minute
    settle_within(h, busy, std::time::Duration::from_secs(300));
}

fn settle_within<S>(h: &mut Harness<'_, S>, busy: impl Fn(&S) -> bool, limit: std::time::Duration) {
    let t0 = std::time::Instant::now();
    let mut quiet = 0;
    while quiet < 12 {
        // comparing a half-rendered app would fail somewhere far from the cause
        assert!(t0.elapsed() < limit, "the app never settled in {limit:?}");
        h.run_steps(1);
        quiet = if busy(h.state()) { 0 } else { quiet + 1 };
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A labelled accessible node: `Role "label"`, its state, and its bounds.
struct Node {
    text: String,
    rect: Rect,
}

/// Every labelled accessible node.
fn nodes<S>(h: &Harness<'_, S>) -> Vec<Node> {
    h.query_all(by().predicate(|n| n.label().is_some_and(|l| !l.is_empty())))
        .map(|n| {
            let a = n.accesskit_node();
            // its state too: a checked box, a disabled button, a slider's value look different
            let state = format!(
                "{}{}{}{}{}",
                a.toggled().map(|t| format!(" {t:?}")).unwrap_or_default(),
                a.is_selected().map(|s| format!(" selected={s}")).unwrap_or_default(),
                if a.is_disabled() { " disabled" } else { "" },
                a.value().map(|v| format!(" value={v:?}")).unwrap_or_default(),
                a.numeric_value().map(|v| format!(" value={v}")).unwrap_or_default(),
            );
            Node { text: format!("{:?} {:?}{state}", a.role(), a.label().unwrap_or_default()), rect: n.rect() }
        })
        .collect()
}

/// Studio's nodes under the persona bar, moved up by it into the bare app's coordinates. The bar's
/// own nodes, wholly inside its band, are left out; content scrolled up past the window's top
/// (starting above the band) stays.
fn under_bar(nodes: Vec<Node>) -> Vec<Node> {
    let band = (KITTEST_MARGIN - 0.5)..=(KITTEST_MARGIN + bar_h() + 0.5);
    nodes
        .into_iter()
        .filter(|n| !(band.contains(&n.rect.min.y) && band.contains(&n.rect.max.y)))
        .map(|n| Node { rect: n.rect.translate(egui::vec2(0.0, -bar_h())), ..n })
        .collect()
}

/// The nodes not wholly inside an allowed region, as sorted `Role "label" [x0 y0 x1 y1]` lines.
fn shown(nodes: Vec<Node>, allow: &[Allow]) -> Vec<String> {
    let mut lines: Vec<String> = nodes
        .into_iter()
        .filter(|n| !allow.iter().any(|a| a.rect.contains_rect(n.rect)))
        .map(|n| format!("{} [{:.0} {:.0} {:.0} {:.0}]", n.text, n.rect.min.x, n.rect.min.y, n.rect.max.x, n.rect.max.y))
        .collect();
    lines.sort();
    lines
}

/// Lines in one tree and not the other: `+n` only in Studio's (n more times), `-n` only in the
/// bare app's.
fn tree_diff(studio: &[String], upstream: &[String]) -> Vec<String> {
    let mut count: BTreeMap<&str, i64> = BTreeMap::new();
    for l in studio {
        *count.entry(l).or_default() += 1;
    }
    for l in upstream {
        *count.entry(l).or_default() -= 1;
    }
    count.into_iter().filter(|(_, c)| *c != 0).map(|(l, c)| format!("{c:+} {l}")).collect()
}

/// An allow-list may hide at most a quarter of a case's nodes or pixels, and the case must compare
/// something. The largest planned allowance, D1's floating History panel with L13's menu slot, is
/// about a tenth of the window: a quarter leaves it room, while no allow-list can hide the canvas
/// (about half the window) or a whole column of panels and still pass.
fn allow_cap(hidden: usize, total: usize, what: &str) -> Result<(), String> {
    if total == 0 || hidden * 4 > total {
        return Err(format!("the allow-list hides {hidden} of {total} {what}, over a quarter"));
    }
    Ok(())
}

fn reasons(allow: &[Allow]) -> Vec<String> {
    allow.iter().map(|a| format!("{:?}: {}", a.rect, a.reason)).collect()
}

/// Studio's accessibility tree (under the persona bar) equals the bare app's, outside `allow`.
pub(crate) fn assert_tree_parity<A, B>(studio: &Harness<'_, A>, upstream: &Harness<'_, B>, allow: &[Allow]) {
    let all = nodes(upstream).len();
    let up = shown(nodes(upstream), allow);
    if let Err(e) = allow_cap(all - up.len(), all, "nodes") {
        panic!("{e}\nallowed: {:#?}", reasons(allow));
    }
    // a harness that drew nothing would pass
    assert!(up.len() > 10, "the bare app shows almost nothing: {up:#?}");
    let diff = tree_diff(&shown(under_bar(nodes(studio)), allow), &up);
    assert!(
        diff.is_empty(),
        "Studio differs from the bare app (+ Studio only, - bare app only):\n{}\nallowed: {:#?}",
        diff.join("\n"),
        reasons(allow)
    );
}

/// The installed fonts: the families, and each face by name, size and a hash of its bytes (not
/// the bytes, megabytes per face).
fn fonts(ctx: &egui::Context) -> String {
    use std::hash::{Hash, Hasher};
    ctx.fonts(|f| {
        let d = f.definitions();
        let faces: Vec<String> = d
            .font_data
            .iter()
            .map(|(name, data)| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                data.font.hash(&mut h);
                format!("{name}: {} bytes #{:x} face {} {:?}", data.font.len(), h.finish(), data.index, data.tweak)
            })
            .collect();
        format!("{:?} {faces:?}", d.families)
    })
}

/// What makes a ctx look and behave the way it does, part by part: its style (the parts most
/// likely to differ named first, then the whole), the options an app sets up and its fonts.
fn style(ctx: &egui::Context) -> [(&'static str, String); 7] {
    let s = ctx.global_style();
    [
        ("visuals", format!("{:?}", s.visuals)),
        ("text styles", format!("{:?}", s.text_styles)),
        ("spacing", format!("{:?}", s.spacing)),
        ("interaction", format!("{:?}", s.interaction)),
        ("style", format!("{s:?}")),
        ("options", ctx.options(|o| format!("{:?} zoom_with_keyboard={} {:?}", o.theme_preference, o.zoom_with_keyboard, o.input_options))),
        ("fonts", fonts(ctx)),
    ]
}

/// The app's theme tokens, where its `theme::apply` stores them (None: not stored).
fn tokens(ctx: &egui::Context, app: App) -> Option<String> {
    ctx.data(|d| match app {
        App::LightCraft => d.get_temp::<lightcraft_ui_egui::theme::Tokens>(egui::Id::NULL).map(|t| format!("{t:?}")),
        App::PhotoCraft => d.get_temp::<photocraft_ui_egui::theme::Tokens>(egui::Id::new("photocraft-theme")).map(|t| format!("{t:?}")),
    })
}

/// The first part of Studio's style that differs from the bare app's, then the persona app's
/// tokens. Only that app's: Studio's ctx holds both apps' tokens. Tokens missing on either side
/// fail, so a renamed id upstream can't make both sides match on nothing.
fn style_diff(studio: &egui::Context, upstream: &egui::Context, app: App) -> Option<String> {
    let differ = |name: &str, s: &str, u: &str| format!("Studio's {name} differ from the bare app's:\nStudio: {s}\nbare:   {u}");
    if let Some(((name, s), (_, u))) = style(studio).into_iter().zip(style(upstream)).find(|((_, s), (_, u))| s != u) {
        return Some(differ(name, &s, &u));
    }
    let name = format!("{app:?} tokens");
    match (tokens(studio, app), tokens(upstream, app)) {
        (Some(s), Some(u)) => (s != u).then(|| differ(&name, &s, &u)),
        (s, u) => Some(format!("{name} are missing (Studio: {}, bare app: {})", s.is_some(), u.is_some())),
    }
}

/// Studio draws with the bare app's style.
pub(crate) fn assert_style_parity(studio: &egui::Context, upstream: &egui::Context, app: App) {
    if let Some(d) = style_diff(studio, upstream, app) {
        panic!("{d}");
    }
}

/// A pixel differs above this on any channel (of 255).
const TOLERANCE: u8 = 2;

/// Studio's render (RGBA8, `width` px wide: the bare app's size plus the persona bar's rows `bar`)
/// against the bare app's. Pixels inside `allow` (px) don't count. Returns the differing
/// pixels, the counted pixels, and a diff image the bare app's size: red where different, blue
/// where allowed, the bare app at a quarter brightness elsewhere.
fn pixel_diff(studio: &[u8], upstream: &[u8], width: usize, bar: std::ops::Range<usize>, allow: &[Rect]) -> (usize, usize, Vec<u8>) {
    let row = width * 4;
    let studio = [studio.get(..bar.start * row).unwrap_or_default(), studio.get(bar.end * row..).unwrap_or_default()].concat();
    assert_eq!(studio.len(), upstream.len(), "Studio's render minus the bar is not the bare app's size");
    let (mut differ, mut counted) = (0, 0);
    let mut image = Vec::with_capacity(upstream.len());
    for (i, (s, u)) in studio.as_chunks::<4>().0.iter().zip(upstream.as_chunks::<4>().0).enumerate() {
        let p = egui::pos2((i % width) as f32 + 0.5, (i / width) as f32 + 0.5);
        let px = if allow.iter().any(|r| r.contains(p)) {
            [0, 0, 160, 255]
        } else {
            counted += 1;
            if s.iter().zip(u).any(|(a, b)| a.abs_diff(*b) > TOLERANCE) {
                differ += 1;
                [255, 0, 0, 255]
            } else {
                [u[0] / 4, u[1] / 4, u[2] / 4, 255]
            }
        };
        image.extend_from_slice(&px);
    }
    (differ, counted, image)
}

/// The verdict on a render of `total` pixels, `counted` of them outside the allow-list: more
/// than 0.1% of the counted pixels differing fails.
fn render_verdict(differ: usize, counted: usize, total: usize) -> Result<(), String> {
    allow_cap(total - counted.min(total), total, "pixels")?;
    if differ * 1000 > counted {
        return Err(format!("{differ} of {counted} pixels differ"));
    }
    Ok(())
}

/// Renders Studio and the bare app (same renderer, size and pixels per point), writes
/// `target/renders/parity/<case>-{studio,upstream,diff}.png`, and fails if more than 0.1% of the
/// pixels outside `allow` differ.
pub(crate) fn assert_render_parity<A, B>(case: &str, studio: &mut Harness<'_, A>, upstream: &mut Harness<'_, B>, allow: &[Allow]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/renders/parity");
    std::fs::create_dir_all(&dir).unwrap();
    let ppp = upstream.ctx.pixels_per_point();
    assert_eq!(studio.ctx.pixels_per_point(), ppp);
    let (s, mut u) = (studio.render().unwrap(), upstream.render().unwrap());
    s.save(dir.join(format!("{case}-studio.png"))).unwrap();
    u.save(dir.join(format!("{case}-upstream.png"))).unwrap();
    let px: Vec<Rect> =
        allow.iter().map(|a| Rect::from_min_max((a.rect.min.to_vec2() * ppp).to_pos2(), (a.rect.max.to_vec2() * ppp).to_pos2())).collect();
    let top = (KITTEST_MARGIN * ppp).round() as usize;
    let bar = top..top + (bar_h() * ppp).round() as usize;
    let (differ, counted, diff) = pixel_diff(&s, &u, u.width() as usize, bar, &px);
    u.copy_from_slice(&diff);
    u.save(dir.join(format!("{case}-diff.png"))).unwrap();
    if let Err(e) = render_verdict(differ, counted, u.len() / 4) {
        panic!("{case}: {e} (see {case}-diff.png)\nallowed: {:#?}", reasons(allow));
    }
}

/// The harness against itself on the GPU: each bare app under a stand-in bar against the bare
/// app. Opt-in: `$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_`.
#[test]
#[ignore]
fn render_parity() {
    let busy = |app: &LightcraftApp| app.renderer.in_flight() > 0;
    let (mut hosted, mut bare) =
        (lightcraft(Harness::builder().wgpu(), demo_lightcraft(), true), lightcraft(Harness::builder().wgpu(), demo_lightcraft(), false));
    settle(&mut hosted, busy);
    settle(&mut bare, busy);
    assert_render_parity("self-lightcraft", &mut hosted, &mut bare, &[]);
    let (mut hosted, mut bare) =
        (photocraft(Harness::builder().wgpu(), demo_photocraft(), true), photocraft(Harness::builder().wgpu(), demo_photocraft(), false));
    settle(&mut hosted, |_| false);
    settle(&mut bare, |_| false);
    assert_render_parity("self-photocraft", &mut hosted, &mut bare, &[]);
    for case in LIGHTCRAFT_CASES {
        let (mut studio, mut bare) = lightcraft_case(case, Harness::builder().wgpu(), Harness::builder().wgpu());
        assert_render_parity(case, &mut studio, &mut bare, &[]);
    }
    for case in PHOTOCRAFT_CASES {
        let (mut studio, mut bare, allow) = photocraft_case(case, Harness::builder().wgpu(), Harness::builder().wgpu());
        assert_render_parity(case, &mut studio, &mut bare, &allow);
    }
}

// --- T2: Library and Develop are LightCraft's own window ---

/// Library (grid, compare) and Develop (the loupe with Edit, crop open).
const LIGHTCRAFT_CASES: [&str; 4] = ["library-grid", "library-compare", "develop-edit", "develop-crop"];

/// Studio in `case`, and the bare LightCraft in the same state: the same demo library and picked
/// photo, its menus hidden as in Studio (D4: LightCraft as it looks with a native menu bar), the
/// same texture limit. Both settled.
fn lightcraft_case(
    case: &str,
    studio: HarnessBuilder<crate::StudioShell>,
    bare: HarnessBuilder<LightcraftApp>,
) -> (Harness<'static, crate::StudioShell>, Harness<'static, LightcraftApp>) {
    let mut s = crate::tests_lightcraft::studio_with(studio.with_size(SIZE));
    crate::tests_lightcraft::pick_first(&mut s);
    let mut app = demo_lightcraft();
    app.native_menu = true;
    app.ui.view = lightcraft_ui_egui::state::ViewMode::PhotoGrid;
    let mut b = lightcraft(bare, app, false);
    crate::tests_lightcraft::real_gpu_textures(&mut b);
    s.run_steps(3);
    b.run_steps(3);
    let run = |app: &mut LightcraftApp, id: &str| app.run(id, serde_json::json!({})).map(|_| ()).map_err(|e| e.to_string());
    if case.starts_with("develop") {
        // the bar's Develop: the loupe with Edit
        s.state_mut().run("persona.develop").unwrap();
        b.state_mut().ui.view = lightcraft_ui_egui::state::ViewMode::Detail;
        b.state_mut().ui.right = lightcraft_ui_egui::state::RightPanel::Edit;
    }
    let command = match case {
        "library-compare" => Some("view.compare"),
        "develop-crop" => Some("panel.crop"),
        _ => None,
    };
    if let Some(id) = command {
        let lr = s.state_mut().lightroom.as_mut().unwrap();
        assert_eq!(run(&mut lr.app, id), run(b.state_mut(), id), "{case}: {id}");
    }
    // the case is what it says (a command that did nothing on both sides would compare the grid)
    use lightcraft_ui_egui::state::{RightPanel, ViewMode};
    let want = match case {
        "library-grid" => (ViewMode::PhotoGrid, None),
        "library-compare" => (ViewMode::Compare, None),
        "develop-edit" => (ViewMode::Detail, Some(RightPanel::Edit)),
        _ => (ViewMode::Detail, Some(RightPanel::Crop)),
    };
    for ui in [&s.state().lightroom.as_ref().unwrap().app.ui, &b.state().ui] {
        assert_eq!((ui.view, want.1.map(|_| ui.right)), want, "{case}");
    }
    settle(&mut s, |s: &crate::StudioShell| s.lightroom.as_ref().is_some_and(|lr| lr.app.renderer.in_flight() > 0));
    settle(&mut b, |app: &LightcraftApp| app.renderer.in_flight() > 0);
    (s, b)
}

#[test]
fn library_and_develop_are_lightcrafts_own_window() {
    for case in LIGHTCRAFT_CASES {
        let (s, b) = lightcraft_case(case, Harness::builder(), Harness::builder());
        let persona = if case.starts_with("develop") { crate::Persona::Develop } else { crate::Persona::Library };
        assert_eq!(s.state().persona, persona, "{case}");
        assert_tree_parity(&s, &b, &[]);
        assert_style_parity(&s.ctx, &b.ctx, App::LightCraft);
    }
}

// --- T3: Pixel is PhotoCraft's own window ---

/// Pixel with no document, with the test image and a second layer, the brush picked, Levels
/// (Image › Adjustments) open.
const PHOTOCRAFT_CASES: [&str; 4] = ["pixel-empty", "pixel-layers", "pixel-brush", "pixel-levels"];

/// Studio in `case`, and the bare PhotoCraft in the same state: the same PNG, given to it as
/// Studio gives it its document, its History panel off and its menus hidden as in Studio, the same texture limit. Both
/// settled. The allow-list: Studio's floating History (D1, until T4 puts it in PhotoCraft's
/// style and place).
fn photocraft_case(
    case: &str,
    studio: HarnessBuilder<crate::StudioShell>,
    bare: HarnessBuilder<Pc>,
) -> (Harness<'static, crate::StudioShell>, Harness<'static, Pc>, Vec<Allow>) {
    let mut s = crate::tests_lightcraft::studio_with(studio.with_size(SIZE));
    // no photo picked: Pixel opens empty, or with the image file
    s.state_mut().lightroom.as_mut().unwrap().app.session.selection.active = None;
    s.run_steps(3);
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
    app.ui.panels.history = false;
    crate::pixel::hide_menus(&mut app);
    if case == "pixel-empty" {
        s.state_mut().run("persona.pixel").unwrap();
    } else {
        let png = crate::tests_images::image(&crate::tests_lightcraft::temp_dir(case), "art.png");
        s.state_mut().run_with("document.open", &serde_json::json!({"path": png})).unwrap();
        // the document Studio gives PhotoCraft for it: imported as Studio imports it
        let mut doc = photocraft_io::import("art.png", &std::fs::read(&png).unwrap()).unwrap().document;
        doc.name = "art".into();
        app.session.add_document(doc, None);
        app.sync_views();
    }
    let mut b = photocraft(bare, app, false);
    crate::tests_lightcraft::real_gpu_textures(&mut b);
    s.run_steps(3);
    b.run_steps(3);
    if case != "pixel-empty" {
        s.state_mut().run("pixel.layer.new.layer").unwrap();
        b.state_mut().app.run("layer.new.layer", serde_json::json!({})).unwrap();
    }
    if case == "pixel-brush" {
        s.state_mut().run_with("pixel.tool", &serde_json::json!({"name": "brush"})).unwrap();
        b.state_mut().app.ui.tool = photocraft_ui_egui::state::Tool::from_name("brush").unwrap();
    }
    if case == "pixel-levels" {
        for h in [&mut s as &mut dyn KeyTarget, &mut b] {
            h.ctrl(egui::Key::L);
        }
    }
    s.run_steps(2);
    // saving is Studio's: PhotoCraft's own "unsaved" is kept clear in Studio, so in the bare app
    if let Some(st) = b.state_mut().app.session.active_mut() {
        st.saved_revision = st.revision;
    }
    settle(&mut s, |_| false);
    settle(&mut b, |_| false);
    let layers = |app: &PhotocraftApp| app.session.active().map(|st| st.doc.layers.len());
    assert_eq!(layers(&s.state().pixel.as_ref().unwrap().app), layers(&b.state().app), "{case}");
    let up = egui::vec2(0.0, -bar_h());
    let history = s.ctx.memory(|m| m.area_rect(egui::Id::new("studio.history")));
    let mut allow: Vec<Allow> = history
        .map(|r| Allow { rect: r.translate(up).expand(SHADOW), reason: "Studio's History (D1), until T4, with its shadow" })
        .into_iter()
        .collect();
    if let (Some(studio), Some(bare)) = (dialog(&s.ctx), dialog(&b.ctx)) {
        let reason = "PhotoCraft centres its dialogs (and their shadows) on the whole window, the persona bar included";
        allow.push(Allow { rect: studio.translate(up).union(bare).expand(SHADOW), reason });
    }
    (s, b, allow)
}

/// How far a popup's shadow reaches past it (egui's default popup shadow: 8 pt blur, 6 pt offset).
const SHADOW: f32 = 16.0;

/// Where PhotoCraft's open dialog is (a modal: a foreground area; not its window-wide backdrop).
fn dialog(ctx: &egui::Context) -> Option<Rect> {
    let small = |r: &Rect| r.width() < SIZE[0] / 2.0;
    ctx.memory(|m| {
        m.layer_ids().filter(|l| l.order == egui::Order::Foreground).filter_map(|l| m.area_rect(l.id)).filter(small).reduce(|a, b| a.union(b))
    })
}

/// A harness a key can be pressed in, whatever its state.
trait KeyTarget {
    fn ctrl(&mut self, key: egui::Key);
}

impl<S> KeyTarget for Harness<'_, S> {
    fn ctrl(&mut self, key: egui::Key) {
        let modifiers = egui::Modifiers::COMMAND;
        for pressed in [true, false] {
            self.input_mut().events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers });
            self.run_steps(1);
        }
    }
}

#[test]
fn pixel_is_photocrafts_own_window_in_parity() {
    for case in PHOTOCRAFT_CASES {
        let (s, b, allow) = photocraft_case(case, Harness::builder(), Harness::builder());
        assert_eq!(s.state().persona, crate::Persona::Pixel, "{case}");
        assert_tree_parity(&s, &b, &allow);
        assert_style_parity(&s.ctx, &b.ctx, App::PhotoCraft);
    }
}

fn node(text: &str, rect: [f32; 4]) -> Node {
    Node { text: text.into(), rect: Rect::from_min_max(egui::pos2(rect[0], rect[1]), egui::pos2(rect[2], rect[3])) }
}

#[test]
fn the_tree_diff_names_a_moved_node_on_both_sides() {
    let s = shown(vec![node("Button \"Open\"", [0.0, 1.0, 10.0, 11.0])], &[]);
    let u = shown(vec![node("Button \"Open\"", [0.0, 0.0, 10.0, 10.0])], &[]);
    assert_eq!(tree_diff(&s, &u), ["-1 Button \"Open\" [0 0 10 10]", "+1 Button \"Open\" [0 1 10 11]"]);
    assert!(tree_diff(&u, &u).is_empty());
}

#[test]
fn the_tree_diff_counts_a_repeated_node() {
    let n = || node("Label \"Exposure\"", [0.0, 0.0, 10.0, 10.0]);
    assert_eq!(tree_diff(&shown(vec![n(), n()], &[]), &shown(vec![n()], &[])), ["+1 Label \"Exposure\" [0 0 10 10]"]);
}

#[test]
#[should_panic(expected = "never settled")]
fn an_app_that_never_settles_fails_rather_than_being_compared_half_done() {
    settle_within(&mut Harness::new_ui_state(|_, _: &mut ()| {}, ()), |_| true, std::time::Duration::from_millis(50));
}

#[test]
fn the_tree_diff_sees_a_widgets_state() {
    let ui = |on: bool, enabled: bool, v: f32| {
        Harness::new_ui_state(
            move |ui, s: &mut (bool, f32)| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.checkbox(&mut s.0, "Grid");
                    ui.add(egui::Slider::new(&mut s.1, 0.0..=1.0).text("Exposure"));
                });
            },
            (on, v),
        )
    };
    let tree = |h: Harness<'_, (bool, f32)>| shown(nodes(&h), &[]);
    let base = tree(ui(true, true, 0.5));
    assert!(tree_diff(&base, &tree(ui(true, true, 0.5))).is_empty());
    assert!(!tree_diff(&base, &tree(ui(false, true, 0.5))).is_empty(), "toggled");
    assert!(!tree_diff(&base, &tree(ui(true, false, 0.5))).is_empty(), "disabled");
    assert!(!tree_diff(&base, &tree(ui(true, true, 0.7))).is_empty(), "value");
}

#[test]
fn studios_tree_leaves_out_the_persona_bar() {
    // kittest's margin, then the bar, then the persona
    let m = KITTEST_MARGIN;
    let bar = node("Button \"Pixel\"", [100.0, m + 4.0, 150.0, m + 30.0]);
    let top = node("Button \"File\"", [10.0, m + bar_h(), 40.0, m + bar_h() + 20.0]);
    assert_eq!(shown(under_bar(vec![bar, top]), &[]), [format!("Button \"File\" [10 {m} 40 {}]", m + 20.0)]);
}

#[test]
fn content_scrolled_above_the_view_is_not_taken_for_the_persona_bar() {
    // a grid row scrolled up past the window's top, its bottom edge level with the bar
    let row = node("Button \"LC01354.JPG\"", [12.0, -200.0, 342.0, KITTEST_MARGIN + 10.0]);
    assert_eq!(shown(under_bar(vec![row]), &[]).len(), 1);
}

#[test]
fn only_nodes_wholly_inside_an_allowed_region_are_left_out() {
    let allow = [Allow { rect: Rect::from_min_max(egui::pos2(100.0, 0.0), egui::pos2(200.0, 50.0)), reason: "test" }];
    let inside = node("Window \"History\"", [110.0, 10.0, 190.0, 40.0]);
    let across = node("Button \"Layers\"", [90.0, 10.0, 120.0, 40.0]);
    assert_eq!(shown(vec![inside, across], &allow), ["Button \"Layers\" [90 10 120 40]"]);
}

/// RGBA8 pixels, all `v`.
fn flat(n: usize, v: u8) -> Vec<u8> {
    vec![v; n * 4]
}

#[test]
fn a_pixel_differs_only_above_two_levels() {
    let u = flat(4, 100);
    let mut s = flat(4, 102);
    assert_eq!(pixel_diff(&s, &u, 2, 0..0, &[]).0, 0);
    s[5] = 103;
    let (differ, counted, image) = pixel_diff(&s, &u, 2, 0..0, &[]);
    assert_eq!((differ, counted), (1, 4));
    assert_eq!(&image[4..8], [255, 0, 0, 255]);
    assert_eq!(&image[..4], [25, 25, 25, 255]);
}

#[test]
fn the_pixel_diff_crops_the_persona_bar_and_skips_allowed_pixels() {
    // Studio: the bare app's 2×2 with a 2 px bar row (white) after its first row
    let mut s = flat(2, 0);
    s.extend(flat(2, 255));
    s.extend(flat(2, 0));
    let mut u = flat(4, 0);
    u[12..16].copy_from_slice(&[9, 9, 9, 9]);
    let allow = [Rect::from_min_max(egui::pos2(1.0, 1.0), egui::pos2(2.0, 2.0))];
    let (differ, counted, image) = pixel_diff(&s, &u, 2, 1..2, &allow);
    assert_eq!((differ, counted), (0, 3));
    assert_eq!(&image[12..16], [0, 0, 160, 255]);
    assert_eq!(pixel_diff(&s, &u, 2, 1..2, &[]).0, 1);
}

#[test]
fn renders_fail_above_a_thousandth_of_the_pixels() {
    assert!(render_verdict(1, 1000, 1000).is_ok());
    assert!(render_verdict(2, 1000, 1000).is_err());
}

#[test]
fn a_render_fails_when_its_allow_list_hides_over_a_quarter_of_it() {
    assert!(render_verdict(0, 750, 1000).is_ok());
    assert!(render_verdict(0, 749, 1000).is_err());
    assert!(render_verdict(0, 0, 1000).is_err(), "nothing compared");
}

#[test]
#[should_panic(expected = "the allow-list hides")]
fn a_tree_case_fails_when_its_allow_list_hides_over_a_quarter_of_the_nodes() {
    let (mut hosted, mut bare) = (lightcraft(Harness::builder(), demo_lightcraft(), true), lightcraft(Harness::builder(), demo_lightcraft(), false));
    hosted.run_steps(8);
    bare.run_steps(8);
    // the right panel and the right end of the top bar: the trees match, but too much is hidden
    let right = Allow { rect: Rect::from_min_max(egui::pos2(1000.0, 0.0), egui::pos2(SIZE[0], SIZE[1])), reason: "test" };
    let all = nodes(&bare).len();
    let left = shown(nodes(&bare), std::slice::from_ref(&right)).len();
    assert!((all - left) * 4 > all && left > 10, "the test's rect hides {} of {all}", all - left);
    assert_tree_parity(&hosted, &bare, &[right]);
}

#[test]
fn the_style_check_names_what_differs() {
    // fonts exist from the first pass on
    let (a, b) = (Harness::new_ui(|_| {}), Harness::new_ui(|_| {}));
    put_lightcraft_tokens(&a.ctx);
    put_lightcraft_tokens(&b.ctx);
    assert_eq!(style_diff(&a.ctx, &b.ctx, App::LightCraft), None);
    b.ctx.global_style_mut(|s| s.visuals.panel_fill = egui::Color32::RED);
    assert!(style_diff(&a.ctx, &b.ctx, App::LightCraft).is_some_and(|m| m.starts_with("Studio's visuals differ")));
}

/// LightCraft's theme tokens, as its `theme::apply` stores them.
fn put_lightcraft_tokens(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, lightcraft_ui_egui::theme::Tokens::default()));
}

#[test]
fn a_missing_theme_token_fails_instead_of_matching() {
    let (a, b) = (Harness::new_ui(|_| {}), Harness::new_ui(|_| {}));
    let missing =
        |a: &egui::Context, b: &egui::Context| style_diff(a, b, App::LightCraft).is_some_and(|m| m.contains("LightCraft tokens are missing"));
    assert!(missing(&a.ctx, &b.ctx), "on both sides");
    put_lightcraft_tokens(&a.ctx);
    assert!(missing(&a.ctx, &b.ctx), "on the bare app's side");
    assert!(missing(&b.ctx, &a.ctx), "on Studio's side");
    put_lightcraft_tokens(&b.ctx);
    assert_eq!(style_diff(&a.ctx, &b.ctx, App::LightCraft), None);
}

#[test]
fn the_style_check_covers_spacing_the_whole_style_and_the_options() {
    let named = |change: &dyn Fn(&egui::Context)| {
        let (a, b) = (Harness::new_ui(|_| {}), Harness::new_ui(|_| {}));
        change(&b.ctx);
        style_diff(&a.ctx, &b.ctx, App::LightCraft).map(|m| m.lines().next().unwrap_or_default().to_owned())
    };
    assert_eq!(named(&|c| c.global_style_mut(|s| s.spacing.item_spacing.x += 1.0)).as_deref(), Some("Studio's spacing differ from the bare app's:"));
    assert_eq!(named(&|c| c.global_style_mut(|s| s.animation_time += 1.0)).as_deref(), Some("Studio's style differ from the bare app's:"));
    // PhotoCraft's setup turns egui's keyboard zoom off
    assert_eq!(named(&|c| c.options_mut(|o| o.zoom_with_keyboard = false)).as_deref(), Some("Studio's options differ from the bare app's:"));
}

#[test]
fn a_font_difference_is_named_without_the_fonts_bytes() {
    let (a, mut b) = (Harness::new_ui(|_| {}), Harness::new_ui(|_| {}));
    let mut fonts = egui::FontDefinitions::default();
    let face = fonts.font_data.values().next().unwrap().clone();
    fonts.font_data.insert("Extra".into(), face);
    b.ctx.set_fonts(fonts);
    b.run();
    let m = style_diff(&a.ctx, &b.ctx, App::LightCraft).unwrap();
    assert!(m.starts_with("Studio's fonts differ") && m.contains("Extra"), "{m:.300}");
    assert!(m.len() < 20_000, "a {} byte message", m.len());
}
