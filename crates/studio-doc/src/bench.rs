//! How long the base layer takes on real photos (the must-fix before milestone 4): every
//! `samples/rw2/*.RW2`, at full size and at draft sizes, end to end through [`Base`] and broken
//! down. Opt-in and slow; skips when there are no samples (CI):
//!
//! `cargo test -p studio-doc --release --locked -- --ignored bench_base_render --nocapture`
//!
//! Prints a Markdown table and writes it to `target/bench/base-render.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lightcraft_engine::Session;
use lightcraft_engine::catalog::model::PhotoId;
use lightcraft_engine::pipeline::{OutputDepth, StageCache};

use crate::Document;
use crate::base::{Base, surface_from_deep};

fn samples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/rw2");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rw2"))).collect())
        .unwrap_or_default();
    files.sort();
    files
}

/// Sync until the base shows the document's settings; how long that took.
fn settle(base: &mut Base, session: &mut Session, doc: &mut Document) -> Duration {
    let mut pixel = photocraft_engine::Session::new();
    let t0 = Instant::now();
    loop {
        base.sync(session, &mut pixel, doc).unwrap();
        if base.up_to_date(doc) {
            return t0.elapsed();
        }
        assert!(t0.elapsed() < Duration::from_secs(600), "render never finished");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A new exposure for the photo (a different value each call): what a slider release does.
fn edit(session: &mut Session, id: PhotoId, n: &mut f64) {
    *n += 0.25;
    let mut s = (*session.develop_of(id).unwrap()).clone();
    s.light.exposure = *n % 2.0;
    session.set_develop(id, s, "Exposure").unwrap();
}

/// One render job of the photo's current settings at `w × h`; (render, 16-bit → surface) time.
fn job(session: &mut Session, id: PhotoId, (w, h): (usize, usize), depth: OutputDepth, stages: Option<&Arc<StageCache>>) -> (Duration, Duration) {
    let settings = (*session.develop_of(id).unwrap()).clone();
    let mut job = session.preview_job(id, w, h, true, &settings).unwrap();
    job.request.depth = depth;
    if let Some(st) = stages {
        job = job.with_stages(st.clone());
    }
    let t0 = Instant::now();
    let r = job.run();
    let render = t0.elapsed();
    let t1 = Instant::now();
    if depth == OutputDepth::U16 {
        surface_from_deep(r.rendered.as_ref().unwrap().deep.as_ref().unwrap()).unwrap();
    }
    let convert = t1.elapsed();
    session.accept(&r);
    (render, convert)
}

fn ms(d: Duration) -> String {
    format!("{:.0}", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore]
fn bench_base_render() {
    let files = samples();
    if files.is_empty() {
        println!("bench_base_render: skipped, no samples/rw2/*.RW2");
        return;
    }
    let dir = std::env::temp_dir().join(format!("studio-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    lightcraft_engine::gpu::set_enabled(false);
    let mut rows: Vec<(&str, Vec<String>)> = [
        "Import (ms)",
        "Full: first render, incl. decode (ms)",
        "Full: after an edit, Base end to end (ms)",
        "Full after an edit, plain job: LightCraft render (ms)",
        "Full after an edit, plain job: 16-bit to surface (ms)",
        "Full after an edit, job with a StageCache (ms)",
        "Draft 2560: first (ms)",
        "Draft 2560: after an edit (ms)",
        "Draft 1280: first (ms)",
        "Draft 1280: after an edit (ms)",
        "GPU 8-bit full, after an edit (ms)",
    ]
    .into_iter()
    .map(|r| (r, Vec::new()))
    .collect();
    let mut names = Vec::new();
    let mut sizes = Vec::new();
    for (i, file) in files.iter().enumerate() {
        names.push(file.file_name().unwrap().to_string_lossy().into_owned());
        // a fresh library per file: nothing decoded or cached from the one before
        let mut session = Session::new().with_fs().with_system_clock();
        session.open_library(dir.join(format!("lib{i}")), false).unwrap();
        let t0 = Instant::now();
        let r = session.execute("library.import", &serde_json::json!({"paths": [file.to_string_lossy()]})).unwrap();
        let import = t0.elapsed();
        let id = PhotoId(r["imported"][0].as_u64().unwrap());
        let mut doc = Document::from_photo(&session, id).unwrap();
        let mut n = 0.0;
        let mut put = |row: usize, v: String| rows[row].1.push(v);
        put(0, ms(import));
        // full size, end to end (what the document's base does today)
        let mut base = Base::new();
        put(1, ms(settle(&mut base, &mut session, &mut doc)));
        let full = (doc.pixel.size.width as usize, doc.pixel.size.height as usize);
        sizes.push(format!("{}×{}", full.0, full.1));
        edit(&mut session, id, &mut n);
        put(2, ms(settle(&mut base, &mut session, &mut doc)));
        // the same, broken down
        edit(&mut session, id, &mut n);
        let (render, convert) = job(&mut session, id, full, OutputDepth::U16, None);
        put(3, ms(render));
        put(4, ms(convert));
        // reusing LightCraft's per-view stage cache: one render fills it, the edit's render uses it
        let stages = Arc::new(StageCache::default());
        job(&mut session, id, full, OutputDepth::U16, Some(&stages));
        edit(&mut session, id, &mut n);
        let (render, convert) = job(&mut session, id, full, OutputDepth::U16, Some(&stages));
        put(5, ms(render + convert));
        // drafts, end to end
        for (row, edge) in [(6, 2560), (8, 1280)] {
            let mut draft = Base::new();
            draft.max_edge = Some(edge);
            put(row, ms(settle(&mut draft, &mut session, &mut doc)));
            edit(&mut session, id, &mut n);
            put(row + 1, ms(settle(&mut draft, &mut session, &mut doc)));
        }
        // the GPU's 8-bit render at full size (what a 16-bit GPU path could approach)
        lightcraft_engine::gpu::set_enabled(true);
        lightcraft_engine::gpu::warm_up();
        let t0 = Instant::now();
        while !lightcraft_engine::gpu::ready() && t0.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let gpu = match lightcraft_engine::gpu::unavailable_reason() {
            Some(why) => format!("n/a ({why})"),
            None => {
                job(&mut session, id, full, OutputDepth::U8, None);
                edit(&mut session, id, &mut n);
                let (render, _) = job(&mut session, id, full, OutputDepth::U8, None);
                match lightcraft_engine::gpu::last_fallback() {
                    Some(why) => format!("n/a (fell back: {why})"),
                    None => ms(render),
                }
            }
        };
        lightcraft_engine::gpu::set_enabled(false);
        put(10, gpu);
        session.close_library().unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
    let mut table = format!(
        "Base layer render, {} on {} ({} threads), release: {}\n\n| | {} |\n|---|{}\n",
        std::env::consts::ARCH,
        std::env::consts::OS,
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        cfg!(not(debug_assertions)),
        names.iter().zip(&sizes).map(|(n, s)| format!("{n} ({s})")).collect::<Vec<_>>().join(" | "),
        "---|".repeat(names.len()),
    );
    for (label, values) in &rows {
        table += &format!("| {label} | {} |\n", values.join(" | "));
    }
    println!("{table}");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("base-render.md"), table).unwrap();
}
