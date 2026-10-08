# Progress

See [`PRD.md`](PRD.md) → *Milestones for Claude Code*.

## Milestones

- [ ] **1. Shell** — top bar, empty persona areas, dark Adobe-grey theme, panel docking; switching personas works. *(in progress)*
  - [x] 1. Workspace scaffold (Cargo workspace, lints, LICENSE, NOTICE, git)
  - [x] 2. Theme (dark grey tokens, egui + egui_dock styling)
  - [x] 3. Personas + commands (`persona.<id>`, Ctrl+1..7, disabled v2/v3 personas)
  - [x] 4. Top bar (persona buttons left, document name right)
  - [x] 5. Per-persona layouts + docking (placeholder panels)
  - [x] 6. Layout persistence + `window.resetWorkspace`
  - [ ] 7. App binary (eframe/wgpu, DX12 on Windows)
  - [ ] 8. Tests (egui_kittest)
  - [ ] 9. CI (Windows/Linux × x64/ARM64)
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
  - [ ] Halation (from scratch)
  - [ ] Film stock builder

## Notes

- Milestone 1 does not depend on storytold crates yet; egui/eframe pinned to 0.36 to match LightCraft and PhotoCraft.
