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
- [x] **2. Library + Develop** — embed LightCraft's library and develop views; Lightroom Classic panel positions and shortcuts (G, D, E, R, \\).
  - [x] 1. Pin LightCraft (`eeb136b`) + spike: side panels fill dock tabs; the right panel can't be drawn twice per frame (id clash), so Develop's History shares Edit's group
  - [x] 2. `lightroom` module: one LightCraft instance in the shell, logic + font warm-up, Studio style restored
  - [x] 3. Library persona (Folders, grid/loupe, Keywording | Metadata, bottom bar)
  - [x] 4. Develop persona (Presets, loupe + filmstrip, Edit | History)
  - [x] 5. Shortcuts (G, E, D, R studio-side; `\` passed through; Ctrl+1..7 kept for personas)
  - [x] 6. App wiring (`Pictures\Studio Library` / `STUDIO_LIBRARY`, UI state saved, save on leaving Library/Develop and on exit, file pickers)
  - [x] 7. Render check, NOTICE, hand-check list
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
  - [ ] Halation (spektrafilm-based, GPL-3.0; credit in NOTICE when ported) — follow [`PRD-halation.md`](PRD-halation.md), which overrides PRD.md on halation
    - [ ] 1. Reference model on CPU (scatter + Mode A halation + highlight boost, unit tests; no UI)
    - [ ] 2. GPU port (WGSL separable blur + mip pyramid; CPU on the same algorithm; CPU/GPU parity test)
    - [ ] 3. Pipeline + UI (pipeline position, Film persona controls, presets, masks, history, MCP commands)
    - [ ] 4. Look check (compare with spektrafilm renders and the failure-mode checklist; tune)
    - [ ] 5. Mode B (physical kernel) behind the Advanced toggle, with its own tests
    - [ ] 6. Calibration (optional): fit presets to your own bracketed scans
  - [ ] Film stock builder

## Automated checks

No hand checks: every milestone is checked by headless tests (egui_kittest), offscreen renders
that Claude looks at, and the smoke test. Run everything with the validation commands in
`CLAUDE.md`; renders with `$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_personas`
(`target/renders/*.png`). Smoke test (opens a real window for ~5 s, twice; not while the user is
studying): `cargo test -p studio --test smoke -- --ignored`.

Former hand checks and the tests that replaced them:

- Window opens and stays open on DX12 → smoke test (`studio --smoke`).
- Top bar: personas clickable, v2/v3 greyed with "arrives in" tooltips, "Untitled" → `clicking_a_persona_*`, `disabled_personas_*`, `greyed_out_personas_and_tools_say_when_they_arrive`, `top_bar_shows_the_document_name`.
- Ctrl+1..4 switch, Ctrl+5..7 don't → `ctrl_1_to_4_switch_personas_and_ctrl_5_to_7_do_nothing`, `ctrl_numbers_only_switch_personas`.
- Each persona's layout; Film Effects with Halation first → `each_persona_shows_its_adobe_layout`, `film_sections_*`, renders (checked).
- Docking: regroup, split, float (canvas never floats), resize, collapse → `dragging_a_panel_tab_regroups_*`, `dropping_a_tab_on_a_groups_edge_splits_it`, `a_panel_dropped_away_from_the_split_buttons_floats_but_the_canvas_never_does`, `dragging_a_column_separator_resizes_it`, `dock_columns_resize_by_their_separators_around_lightcraft_panels`, `the_collapse_button_folds_a_panel_group`.
- Layouts kept across switches and relaunches → `each_persona_keeps_its_own_layout_*`, `shell_state_survives_save_and_restore`, `persona_panels_and_lightcraft_settings_come_back_after_a_relaunch`, smoke test (relaunch).
- Look (dark grey, small text, tool-strip tooltips) → renders (checked), `studio_theme_survives_*`, `greyed_out_personas_and_tools_*`.
- Right-click → Reset Workspace → `right_click_reset_workspace_puts_the_panels_back`.
- Library in the grid with Classic's panels; click selects → `library_shows_lightcraft_in_lightroom_classic_places`.
- Import by dropping files → `files_dropped_on_library_are_imported`.
- Picked photo in Develop, loupe/filmstrip/Edit/Presets → `develop_edits_the_photo_picked_in_library`, renders (checked).
- G/E/D/R, `\`, nothing while typing → `g_e_d_r_switch_modules_like_lightroom_classic`, `lightroom_keys_type_into_text_fields`, `lightroom_keys_do_nothing_in_pixel`.
- Edit, switch to Pixel, relaunch: edit kept → `leaving_develop_saves_its_edits`.
- Panel widths, Edit sections, settings after relaunch → `persona_panels_and_lightcraft_settings_come_back_after_a_relaunch`.
- Preset hover preview; History steps → `hovering_a_preset_previews_it_only_while_hovered`, `history_lists_develop_steps`.

## Notes

- Milestone 1 does not depend on storytold crates yet; egui/eframe pinned to 0.36 to match LightCraft and PhotoCraft.
- egui 0.36 `Panel::show(ui)` always draws inside the given `Ui`, so LightCraft's and PhotoCraft's own panels should nest inside a persona area in milestones 2 and 4 without upstream patches.
- egui_dock paints tab titles without accessibility info; `dock::Viewer::on_tab_button` adds it.
- **Milestone 2 (LightCraft `eeb136b`):** LightCraft copies Lightroom *desktop*, not Classic, so Studio
  draws its panels one by one into the dock in Classic's places (`crates/studio-shell/src/lightroom.rs`).
  Differences from Classic, from what LightCraft offers: no Navigator, Quick Develop, Snapshots or
  separate Histogram panel (the histogram heads Edit); History is a tab next to Edit (LightCraft's
  right panel can be drawn once per frame, see `tests_lightcraft::spike_b_*`); no filmstrip under
  the Library grid; LightCraft's in-window menus and search bar (`topbar`) are left out.
- LightCraft's CJK fonts (craft-fonts) aren't embedded: the build warns, and Chinese/Japanese/Korean
  text shows as boxes. Set `CRAFT_FONTS_DIR` to a craft-fonts checkout at build time to embed them.
- LightCraft's library lives in `Pictures\Studio Library` (or `STUDIO_LIBRARY`); LightCraft's UI
  state is saved in eframe's `app.ron` under `studio.lightcraft`.
- Saved shell state lives in `%APPDATA%\Studio\data\app.ron` (delete it to get the default layouts back).
