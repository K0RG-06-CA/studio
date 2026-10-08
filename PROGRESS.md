# Progress

See [`PRD.md`](PRD.md) → *Milestones for Claude Code*.

## Milestones

- [x] **1. Shell** — top bar, empty persona areas, dark Adobe-grey theme, panel docking; switching personas works. *(CI runs for the first time once the repo has a GitHub remote)*
  - [x] 1. Workspace scaffold (Cargo workspace, lints, LICENSE, NOTICE, git)
  - [x] 2. Theme (dark grey tokens, egui + egui_dock styling)
  - [x] 3. Personas + commands (`persona.<id>`, Ctrl+1..7, disabled v2/v3 personas)
  - [x] 4. Top bar (persona buttons left, document name right)
  - [x] 5. Per-persona layouts + docking (placeholder panels)
  - [x] 6. Layout persistence + `window.resetWorkspace`
  - [x] 7. App binary (eframe/wgpu, DX12 on Windows)
  - [x] 8. Tests (egui_kittest)
  - [x] 9. CI (Windows/Linux × x64/ARM64) — `.github/workflows/ci.yml`; not run yet (no remote)
- [ ] **2. Library + Develop** — embed LightCraft's library and develop views; Lightroom Classic panel positions and shortcuts (G, D, E, R, \\).
- [ ] **3. Shared document** — one document model (layer tree + develop settings) and save format; Develop edits as a non-destructive base layer.
- [ ] **4. Pixel persona** — PhotoCraft tools with Photoshop layout (tool bar left, options bar top, Layers and Properties right).
- [ ] **5. Round trip** — Library → Develop → Pixel and back on one document, one undo history across personas.
- [ ] **6. MCP** — one MCP server exposing commands from every persona.
- [ ] **7. ARM64 + Linux builds** — CI on Windows ARM64/x64 and Linux ARM64/x64; downloadable .exe and AppImage/.deb.

## Side tracks

- [ ] **RW2 decoding** — report why LightCraft marks S9 RW2 files preview-only, propose and build the `studio-raw-rw2` decoder.
- [ ] **Real Time LUT** — inspect S9 RAW+JPEG pairs for LUT / Photo Style metadata; .vlt import; apply detected LUT as an editable profile.
- [ ] **Film effects**
  - [ ] Saturation RGB
  - [ ] Lazy Grain
  - [ ] Lazy Blur
  - [ ] Lazy Bleach
  - [ ] Halation (from scratch) — follow [`PRD-halation.md`](PRD-halation.md), which overrides PRD.md on halation
    - [ ] 1. Reference model on CPU (scatter + Mode A halation + highlight boost, unit tests; no UI)
    - [ ] 2. GPU port (WGSL separable blur + mip pyramid; CPU on the same algorithm; CPU/GPU parity test)
    - [ ] 3. Pipeline + UI (pipeline position, Film persona controls, presets, masks, history, MCP commands)
    - [ ] 4. Look check (compare with spektrafilm renders and the failure-mode checklist; tune)
    - [ ] 5. Mode B (physical kernel) behind the Advanced toggle, with its own tests
    - [ ] 6. Calibration (optional): fit presets to your own bracketed scans
  - [ ] Film stock builder

## Milestone 1: check by hand

Everything was tested headless (egui_kittest). Nothing was clicked through in a real window yet. Run
from PowerShell: `cargo run -p studio`. Reference renders of each persona:
`target/renders/{library,develop,film,pixel}.png` (refresh with
`$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_personas`).

- [ ] The window opens and stays open (DX12 on Windows; it used to crash at startup with Vulkan in the backend set).
- [ ] Top bar: Library, Develop, Film, Pixel clickable; Vector, Layout, Export greyed out with an "arrives in v2/v3" tooltip; "Untitled" on the right.
- [ ] Ctrl+1 … Ctrl+4 switch personas; Ctrl+5 … Ctrl+7 do nothing.
- [ ] Each persona matches its render: Lightroom-style left/right columns + filmstrip (Library, Develop, Film); tool strip, options bar and tabbed right dock (Pixel). Film Effects lists Halation first.
- [ ] Docking feels right: drag a tab into another group, to a group's edge (split) and out of the window (floating panel); drag the separators to resize; the ▼ buttons collapse a group. The canvas tab can't float.
- [ ] Rearrange a persona, switch away and back: the layout is kept. Quit and relaunch: the active persona and all layouts come back.
- [ ] Look: dark grey, small text, readable at your display scaling; tooltips on the tool-strip letters.
- [ ] Note: `window.resetWorkspace` has no menu item yet (only the command; tested headless). A Window menu comes with a later milestone.

## Notes

- Milestone 1 does not depend on storytold crates yet; egui/eframe pinned to 0.36 to match LightCraft and PhotoCraft.
- egui 0.36 `Panel::show(ui)` always draws inside the given `Ui`, so LightCraft's and PhotoCraft's own panels should nest inside a persona area in milestones 2 and 4 without upstream patches.
- egui_dock paints tab titles without accessibility info; `dock::Viewer::on_tab_button` adds it.
- Saved shell state lives in `%APPDATA%\Studio\data\app.ron` (delete it to get the default layouts back).
