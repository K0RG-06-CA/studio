# Studio — instructions for Claude Code

Studio is an Affinity-style all-in-one photo app with Adobe-style personas, built in Rust on the storytold Crafting Apps.

## Source of truth
- **Follow [`PRD.md`](PRD.md).** It defines the goals, personas, scope per version and milestones. Work one milestone at a time; don't build ahead.
- For halation, follow PRD-halation.md; it overrides PRD.md on halation, including the film pipeline order for every film effect (e.g. Lazy Blur runs before halation).
- **Track progress in [`PROGRESS.md`](PROGRESS.md).** Update it after every task: tick what's done, note what's in progress.

## Workflow
- Break each milestone into small tasks. After every task: run validation, and only when it passes, update `PROGRESS.md` and commit (one commit per task).
- Validation (must pass before a commit):
  ```bash
  cargo fmt --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo test --workspace --locked
  ```
- Never push or add a remote unless asked.

## Checking a milestone
- **No "check by hand" lists, ever.** Every milestone is checked by automated tests: headless egui_kittest tests for behaviour (clicks, keys, drags, saving), and the opt-in offscreen render test (`$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_personas`), whose PNGs in `target/renders/` Claude looks at itself.
- Before calling a milestone done, also run the smoke test (`cargo test -p studio --test smoke -- --ignored`): the real app on DX12 runs a script through every persona and quits; any panic, error log or GPU failure fails it. Extend its script (`apps/studio/src/smoke.rs`) with each milestone's new commands. It opens a real window, so only run it when the user isn't studying (ask if unsure).
- What truly can't be automated goes in one line under "Known untested" in `PROGRESS.md`. Never ask the user to check something by hand.

## Toolchain (Windows dev machine)
- Use the MSVC toolchain (`aarch64-pc-windows-msvc`) with VS Build Tools 2022 (ARM64 C++ tools), not MSYS2/clang or `*-gnullvm`.
- **Always run cargo from PowerShell, never Git Bash:** Git Bash's `/usr/bin/link` shadows MSVC's `link.exe` and every link step fails.

## Rules (mirrored from storytold)
- **Add around storytold's code, don't edit inside it.** LightCraft, PhotoCraft etc. come in as Git dependencies pinned to a commit. Unavoidable patches go in `PATCHES.md` with a reason.
- **Never crash.** Non-test code has no `unwrap`/`expect`/`panic!`/`todo!`/`unreachable!` and no `unsafe` (enforced by crate-level lints; `clippy.toml` allows them in tests only). Return errors.
- **Every action is a command** with a string id (e.g. `persona.develop`), dispatched through `StudioShell::run`, so MCP can drive it later.
- **egui/eframe versions match storytold's** (currently 0.36). Bump in lockstep with them.
- **Clean-room:** copy Adobe's layout and shortcuts, never its icons, fonts or assets. Credit borrowed code/values in `NOTICE`.
- **License:** GPL-3.0-or-later.
- **ARM64 is a hard requirement** (this dev machine is Windows ARM64); CI covers Windows/Linux × x64/ARM64.

## Layout
- `crates/studio-shell` — top bar, personas, theme, docking (egui + egui_dock, no eframe; headless-testable with egui_kittest).
- `apps/studio` — the binary (eframe, wgpu; DX12 only on Windows).
