//! Opt-in smoke test: launches the real app, so a window opens for a few seconds.
//! `cargo test -p studio --test smoke -- --ignored` (from PowerShell, as all cargo).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run `studio --smoke` with `dir` as its data folder; (succeeded, stdout, stderr).
fn run(dir: &std::path::Path, n: u32) -> (bool, String, String) {
    let (out, err) = (dir.join(format!("run{n}.out")), dir.join(format!("run{n}.err")));
    let mut child = Command::new(env!("CARGO_BIN_EXE_studio"))
        .arg("--smoke")
        .env("STUDIO_SMOKE_DIR", dir)
        .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err).unwrap()))
        .spawn()
        .unwrap();
    let t0 = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if t0.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let read = |p| std::fs::read_to_string(p).unwrap_or_default();
    (status.is_some_and(|s| s.success()), read(&out), read(&err))
}

#[test]
#[ignore]
fn smoke() {
    let dir = std::env::temp_dir().join(format!("studio-smoke-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // first launch: fresh state, Library; the script ends in Develop
    let (ok, out, err) = run(&dir, 1);
    assert!(ok && out.contains("smoke: ok"), "run 1 failed\n{out}\n{err}");
    assert!(out.contains("smoke: restored persona library"), "{out}");
    assert!(dir.join("app.ron").exists(), "the app saved its state");
    // relaunch: the saved persona comes back
    let (ok, out, err) = run(&dir, 2);
    assert!(ok && out.contains("smoke: ok"), "run 2 failed\n{out}\n{err}");
    assert!(out.contains("smoke: restored persona develop"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}
