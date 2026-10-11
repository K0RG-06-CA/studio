# Studio — instructions for Claude Code

Studio is an Affinity-style all-in-one photo app built in Rust on the storytold Crafting Apps; each persona looks exactly like the storytold app it embeds.

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
  cargo test --workspace --locked -- --test-threads=4
  ```
  Local runs use `--test-threads=4`: with every core at once, a loaded dev machine can keep LightCraft's demo renders waiting past the tests' time limits. CI runs the plain command.
- Never push or add a remote unless asked.

## Checking a milestone
- **No "check by hand" lists, ever.** Every milestone is checked by automated tests: headless egui_kittest tests for behaviour (clicks, keys, drags, saving), and the opt-in offscreen render test (`$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_personas`), whose PNGs in `target/renders/` Claude looks at itself.
- Before calling a milestone done, also run the smoke test (`cargo test -p studio --test smoke -- --ignored`): the real app on DX12 runs a script through every persona and quits; any panic, error log or GPU failure fails it. Extend its script (`apps/studio/src/smoke.rs`) with each milestone's new commands. It opens a real window, so only run it when the user isn't studying (ask if unsure).
- What truly can't be automated goes in one line under "Known untested" in `PROGRESS.md`. Never ask the user to check something by hand.

## Toolchain (Windows dev machine)
- Use the MSVC toolchain (`aarch64-pc-windows-msvc`) with VS Build Tools 2022 (ARM64 C++ tools), not MSYS2/clang or `*-gnullvm`.
- **Always run cargo from PowerShell, never Git Bash:** Git Bash's `/usr/bin/link` shadows MSVC's `link.exe` and every link step fails.
- **Never read or write source files through PowerShell 5.1 without `-Encoding UTF8`:** its default is the ANSI code page, so a read-then-write garbles every non-ASCII character (`›`, `…`, `×`) into mojibake. `Set-Content -Encoding UTF8` also adds a BOM, so prefer `[IO.File]::ReadAllText`/`WriteAllText` (UTF-8, no BOM) or the Edit tool.

## Rules (mirrored from storytold)
- **Add around storytold's code, don't edit inside it.** LightCraft, PhotoCraft etc. come in as Git dependencies pinned to a commit. Unavoidable patches go in `PATCHES.md` with a reason.
- **Never crash.** Non-test code has no `unwrap`/`expect`/`panic!`/`todo!`/`unreachable!` and no `unsafe` (enforced by crate-level lints; `clippy.toml` allows them in tests only). Return errors.
- **Every action is a command** with a string id (e.g. `persona.develop`), dispatched through `StudioShell::run`, so MCP can drive it later.
- **egui/eframe versions match storytold's** (currently 0.36). Bump in lockstep with them.
- **UI parity:** each persona looks identical to its storytold app (Library/Develop = LightCraft, Pixel = PhotoCraft): its own theme, fonts, colours, panel layout and shortcuts. Film is Studio's own persona (no storytold app to match): built from storytold's widgets, theme and panel conventions, so it looks like it belongs next to LightCraft. Studio adds only the Affinity-style persona bar on top and the minimum the shared document needs (Studio's History tab styled as a native storytold panel in PhotoCraft's History spot, notices, prompts, disabled states). No Adobe recolouring or re-docking: upstream UI updates must port with no restyling. See PRD.md › UI and look.
- **Clean-room:** never Adobe's icons, fonts or assets. Credit borrowed code/values in `NOTICE`.
- **License:** GPL-3.0-or-later.
- **ARM64 is a hard requirement** (this dev machine is Windows ARM64); CI covers Windows/Linux × x64/ARM64.

## Layout
- `crates/studio-shell` — persona bar, personas hosting the storytold apps' own UI (egui, no eframe; headless-testable with egui_kittest). Until UI parity lands it still has Studio's own theme and egui_dock layouts.
- `crates/studio-doc` — the shared document (no egui): LightCraft develop settings as a live base layer under PhotoCraft's layer tree; the `.studio` file.
- `apps/studio` — the binary (eframe, wgpu; DX12 only on Windows).
