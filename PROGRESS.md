# Progress

See [`PRD.md`](PRD.md) → *Milestones for Claude Code*.

## Milestones

- [x] **1. Shell** — top bar, empty persona areas, dark Adobe-grey theme, panel docking; switching personas works.
  - [x] 1. Workspace scaffold (Cargo workspace, lints, LICENSE, NOTICE, git)
  - [x] 2. Theme (dark grey tokens, egui + egui_dock styling)
  - [x] 3. Personas + commands (`persona.<id>`, Ctrl+1..7, disabled v2/v3 personas)
  - [x] 4. Top bar (persona buttons left, document name right)
  - [x] 5. Per-persona layouts + docking (placeholder panels)
  - [x] 6. Layout persistence + `window.resetWorkspace`
  - [x] 7. App binary (eframe/wgpu, DX12 on Windows)
  - [x] 8. Tests (egui_kittest)
  - [x] 9. CI (Windows/Linux × x64/ARM64) — `.github/workflows/ci.yml`; first run green on all four (Oct 8, 2026, private repo K0RG-06-CA/studio)
- [x] **2. Library + Develop** — embed LightCraft's library and develop views; Lightroom Classic panel positions and shortcuts (G, D, E, R, \\).
  - [x] 1. Pin LightCraft (`eeb136b`) + spike: side panels fill dock tabs; the right panel can't be drawn twice per frame (id clash), so Develop's History shares Edit's group
  - [x] 2. `lightroom` module: one LightCraft instance in the shell, logic + font warm-up, Studio style restored
  - [x] 3. Library persona (Folders, grid/loupe, Keywording | Metadata, bottom bar)
  - [x] 4. Develop persona (Presets, loupe + filmstrip, Edit | History)
  - [x] 5. Shortcuts (G, E, D, R studio-side; `\` passed through; Ctrl+1..7 kept for personas)
  - [x] 6. App wiring (`Pictures\Studio Library` / `STUDIO_LIBRARY`, UI state saved, save on leaving Library/Develop and on exit, file pickers)
  - [x] 7. Render check, NOTICE, hand-check list
- [x] **3. Shared document** — one document model (layer tree + develop settings) and save format; Develop edits as a non-destructive base layer.
  - [x] 1. Pin PhotoCraft (`d836e34`; builds on ARM64) + `studio-doc` scaffold: `Document` (name, source photo, LightCraft `DevelopSettings`, reserved `Film`, PhotoCraft layer tree), `document.json` header
  - [x] 2. `Document::from_photo`: 16-bit RGB layer tree with the locked, marked "Develop" base layer
  - [x] 3. Live base: re-render (CPU, 16-bit) when the settings change and no drag is open; the document size follows the render
  - [x] 4. `.studio` save/load (one zip: `document.json` + `pixel.pcraft`, base pixels not stored)
  - [x] 5. Open into a session: find the photo by hash → path/scene → import (an id alone means nothing in another library); the file's settings win, as a History step
  - [x] 6. Shell wiring: `document.new/save/saveAs/open/close`, `run_with` (params), name in the top bar, reopen at launch
  - [x] 7. Smoke script (document.new → saveAs → close → open; reopened on the relaunch) + docs
- [ ] **4. Pixel persona** — PhotoCraft tools with Photoshop layout (tool bar left, options bar top, Layers and Properties right).
  - [x] 1. Spike (`tests_photocraft::spike_*`): PhotoCraft's `PhotocraftApp` runs through its
    `eframe::App` with eframe's test frame (`Frame::_new_kittest`, eframe 0.36.2); its tool bar,
    options bar, right dock and `canvas_view` draw in Studio's places; its fonts and LightCraft's
    share one context (both personas still render after the other installed its fonts). Found:
    `History::purge_last` drops the state but not the step's label, so a refused edit is undone
    with PhotoCraft's own undo + `clear_redo`; Free Transform / type sessions are public
    (`app.ui.transform`, `app.ui.text_edit`), stroke and drag state isn't (a held pointer button
    stands in); the crop rectangle is readable (`crop.geometry.rect`). L1 doctest in `pixel.rs`.
  - [x] 2. One source of truth: while a document is open its pixels live in PhotoCraft's session
    (`studio_doc::pixel`; the shell's `pixel: Option<Pixel>` holds PhotoCraft's editor, set by the
    app like `lightroom`). `Document::pixel` is the copy Studio saves: `copy_back` (an edit,
    `revision` moves; selecting a layer doesn't) and `write_base` (`Base::take` puts renders into
    PhotoCraft, no history step, not an edit). PhotoCraft's own "unsaved" is kept clear (its close
    guard and autosave stay quiet). `document.new/open` attach, `close` takes it out;
    `unsaved()` copies first, so saves, the prompt and `save_for_exit` see Pixel edits.
  - [x] 3. The base stays out of PhotoCraft's undo, which restores whole documents: every render
    written is kept in a register by weak tile pointers (an entry goes once no state holds its
    pixels), so a state from history showing an earlier render is recognised and gets the current
    one back (pixels and size, no history step) on the next copy-back; undo/redo, the history cap
    (`max_states`/`max_bytes`) and pruning don't depend on step counts. Pixel layers stay on the
    same photo pixels across a Develop crop: each render carries an `Anchor` (frame hash without
    the crop rectangle, crop origin in source pixels, scale) and the layers move by the origin's
    change (PhotoCraft's `move_targets`/`moved`), through undo too. Another frame change
    (rotate, straighten, flip, lens/transform) leaves them where they were, with a notice
    (`Document::take_notice`; shown from task 6); a position-locked user layer stops the move,
    with a notice.
  - [x] 4. Lock guard: a new document in PhotoCraft is checked before it's copied back. The base
    must stay the one marked layer at the bottom of the root, keep every property but `visible`
    (hiding is allowed; a `setProps` that also unlocks or changes anything else is refused as a
    whole), show pixels Studio wrote, and the canvas keeps its render's size. A breaking edit is
    taken back with PhotoCraft's own undo + `clear_redo` (history as before; a good edit made in
    the same frame stays), else the last copy goes back in; notice `LOCKED`. Checks the result,
    so menus, panel drags, shortcuts and commands are all covered.
  - [x] 5. A render that finishes during a Pixel gesture waits (`Base::hold`, set by the shell in
    Pixel while a pointer button is down, Free Transform is open or type is being edited): the
    pixels under a stroke don't change halfway, the base reads as not current meanwhile, nothing
    re-renders, and it goes in (no history step) on the frame the gesture ends, unless the
    settings changed since (then the newest renders).
  - [x] 6. Shell wiring (`pixel::Pixel`, as `lightroom.rs`): PhotoCraft's `logic` runs only in
    Pixel; its style is captured once (its sizes in Studio's visuals) and used only for its
    pieces, its Pro tokens recoloured to Studio's greys. Its tool bar and options bar replace the
    placeholders (`TOOLS` gone); its `canvas_view` fills the canvas tab (titled with the
    document's name; "Rendering Develop…" while the base is behind; "No document…" without one);
    its right dock (Properties, Navigator/Histogram/Info, History, Layers; Color off for room)
    fills Pixel's one "Layers" tab, sized by `request_dock_width`, its own resize handle clipped.
    A saved Pixel layout from before (placeholder tabs) resets to default (`dock::tabs_known`).
    The copy-back's notices show in the error modal. M1's docking tests moved to Film's
    placeholder tabs. Test harnesses report an 8192 px texture limit (kittest's headless 2048
    trips egui's debug assert on PhotoCraft's CPU canvas). Renders `pixel-document.png`,
    `pixel-locked.png` checked.
  - [x] 7. Commands and shortcuts: `pixel.tool {name}` and `pixel.<PhotoCraft command>` (copied
    back at once; a change to the Develop layer returns `LOCKED`). PhotoCraft's own shortcuts
    run inside its `logic`, only in Pixel, after Studio's: Ctrl+1..7 and Ctrl+Q stay Studio's,
    and Studio takes Photoshop's document keys there (Ctrl+S `document.save`, Ctrl+N
    `document.new`, Ctrl+W `document.close`; Ctrl+O and Shift+Ctrl+S do nothing until their
    dialogs) so PhotoCraft's file commands never act on its session. Ctrl+Z / Shift+Ctrl+Z are
    PhotoCraft's undo: Pixel edits only, Develop's history untouched. Tests: a Pixel edit asks
    before close, replace, the window's close button and Ctrl+Q, and Save writes it; Ctrl+S
    saves the `.studio`; files dropped on Pixel open nothing (PhotoCraft as Studio sets it up
    has no import service), so no guard was needed.
  - [x] 8. PhotoCraft's overlays drawn after Pixel's dock in its style (dialogs: adjustments,
    filters, Liquify, Camera Raw, wide angle; floating panels, palette, notices, job progress;
    test: Ctrl+L opens Levels over Pixel). Left out: its unsaved prompt and TIFF options (saving is
    Studio's) and Timeline windows (private module: L7 doctest). L4 test: the History footer's
    Delete state / New snapshot / New document buttons do nothing (found as the one row of three
    24-pt buttons; their tooltips don't show headless; the same clicks on a History row do act).
  - [ ] 9. App wiring, smoke script, renders, NOTICE
- [ ] **5. Round trip** — Library → Develop → Pixel and back on one document, one undo history across personas.
- [ ] **6. MCP** — one MCP server exposing commands from every persona.
- [ ] **7. ARM64 + Linux builds** — CI on Windows ARM64/x64 and Linux ARM64/x64; downloadable .exe and AppImage/.deb.

## Side tracks

- [x] **RW2 decoding** — investigated (`docs/rw2-investigation.md`, 2026-10-08): LightCraft `eeb136b` already decodes the S9 (raw format 8, landed upstream 2026-10-07); all 5 samples decode bit-identical to Adobe DNG Converter 18.3, and Studio's import path marks none preview-only. **No decoder crate is needed.** PRD.md reworded accordingly (2026-10-08); replaced by the track below.
- [ ] **S9 colour profile** — the gap the investigation found: RW2 files carry no colour matrix, so LightCraft fits a per-file look to the embedded JPEG with relative WB, while a DNG of the same file has calibrated matrices and absolute Kelvin. Decision: Studio's fork ships matrices recorded from the author's own DNGs (credited in `NOTICE`).
  - [ ] **Blocked on samples:** shoot the section 6 list of the investigation doc (ISO ladder, clipped and dark frames, every aspect ratio and crop mode, high-res mode, each LUT) into `samples/rw2/` and convert each with Adobe DNG Converter (installed, 18.3) into `samples/rw2/dng/`.
  - [ ] `studio-raw-rw2` crate: per-camera colour table (S9 first: ColorMatrix1/2 with illuminants A/D65, ForwardMatrix1/2, black 128, white 4095, baseline exposure −0.05) with values read from the DNGs; no decoder.
  - [ ] Plug in through the engine's public `Session.media.file_loader` / `file_probe` hook (wrap `with_fs()`'s defaults; no LightCraft patch). Upstream ask if needed: split `files::load_bytes_now` so a patched `RawImage` can enter the DNG colour path (doc section 5b).
  - [ ] Two looks per photo, as a command: **Camera** (LightCraft's current JPEG fit, relative WB) and **Neutral** (the table's DNG matrices, absolute Kelvin).
  - [ ] Golden tests: bit-exact RW2-vs-DNG decode on one fixture pair (`lightcraft_raw::decode` data, CFA, black, active/crop, WB within 1e-5) plus `probe_bytes(..).preview_only == None`; Neutral render vs the DNG render at 600 px within ΔE 1 / |ΔL*| 0.2 on the scripted edits (doc section 5c).
- [ ] **Real Time LUT** — inspect S9 RAW+JPEG pairs for LUT / Photo Style metadata; .vlt import; apply detected LUT as an editable profile.
  - [ ] Before starting, shoot labelled RAW+JPEG pairs (one per LUT plus one without) into `samples/lut/` (gitignored).
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
- **Milestone 3** (`studio-doc` unit tests + shell tests): the base layer equals LightCraft's own render within 1/255 → `the_first_sync_renders_the_photo_into_the_base_as_lightcraft_renders_it`; a settings change re-renders it (and a drag renders once, at release; a crop resizes the document) → `changing_develop_settings_in_the_catalog_re_renders_the_base`, `a_slider_drag_renders_once_at_release`, `a_crop_resizes_the_document_with_the_base`, `editing_in_develop_re_renders_the_documents_base_layer`; save/open round trip, bad files → `save_and_open_keep_everything_but_the_base_pixels`, `bad_files_are_errors_not_crashes`; opening into a library (found by hash, imported, the file's settings as a History step) → `open::tests::*`; the commands, the top-bar name, relaunch → `document_new_needs_a_picked_photo_and_puts_its_name_in_the_top_bar`, `a_saved_document_comes_back_after_a_relaunch`, smoke test (document saved, reopened at the relaunch, base rendered on the real app).

## Open items

- **Film track needs a pipeline hook in LightCraft** (smallest possible, offered upstream per PRD › Staying
  in sync): LightCraft `eeb136b` has no way to register an extra develop step, and `PRD-halation.md` needs
  halation on scene-linear light before the tone curve, which its render API doesn't expose (its
  `F32Linear` output is after the curve and clamped). The document only reserves `Film` settings.

## Must fix before milestone 4 (done)

- [x] **Done: an unsaved document always asks (save, don't save, cancel) and a failed save is shown.**
  Was: **A never-saved document is dropped silently on exit**, and a failed exit save only goes to stderr
  (`apps/studio/src/main.rs` `on_exit`). Harmless today, since a document holds only develop settings
  and those live in the library, but the moment M4 adds pixel layers it is data loss: M4 must start
  with a close/quit prompt (save, discard, cancel) for an unsaved document, and a visible error when
  saving fails.
  - [x] Prompt on `document.new` / `open` / `close` (`document.prompt.save|discard|cancel`)
  - [x] Window close and Ctrl+Q (every persona) go through the prompt (`app.quit`; LightCraft's own
    Cmd+Q in Library/Develop, Studio's elsewhere; one Close, one prompt)
  - [x] Save of a document with no file picks one (save dialog; cancelling it keeps the prompt)
  - [x] Visible save error (`document.dismissError`); `on_exit` saves whatever is still unsaved, a
    never-saved document into `<library>/Recovered/` (no dialog, never over an earlier one)
  - [x] Offscreen renders of the prompt and the error (`prompt.png`, `save-error.png`); smoke script
    edits, quits (must stop at the prompt), cancels and saves; smoke test passes on DX12 (2026-10-09,
    both launches, the relaunch reopens the document in Develop)
- [x] **Done: full-size base re-render after an edit ~0.9 s → ~0.44 s (release, 24 MP S9) through
  LightCraft's `StageCache`.** Was: **Full-size 16-bit base render is CPU-only and slow** (CI runners took over 60 s for 24 MP). Measure
  it on this machine with a real S9 file and fix before Pixel ships (draft size while editing, GPU
  16-bit, or render in bands).
  - [x] Benchmark `bench_base_render` (`crates/studio-doc/src/bench.rs`, opt-in, skips without
    `samples/rw2`). This machine (Windows ARM64, 12 threads), release, 24 MP S9: first render incl.
    decode 1.6–2.0 s; full after an edit ~0.9 s (LightCraft ~0.67 s + 16-bit → surface ~0.21 s);
    **with a StageCache ~0.43 s**; draft 2560 ~0.17 s, 1280 ~0.055 s; GPU 8-bit full ~1.1 s (no
    faster than the CPU). Dev profile: full after an edit ~1.1 s. The 60 s was CI's 2-core runners.
  - [x] Fix: the base re-renders through LightCraft's `StageCache` (warm full size ~0.43 s), one
    render in flight, edits made meanwhile coalesce into one render of the newest settings, a
    stale result is dropped; `Base::current` hands out the layer tree only when the base is up to
    date. No draft. GPU 8-bit is slower than the CPU here (~1.1 s vs ~0.9 s), so no upstream
    request for a GPU 16-bit path. Future option if Pixel feels laggy: a 1280 px draft (~55 ms).
  - [x] Measured after the fix, base after an edit end to end, 24 MP: release ~0.9 s → ~0.44 s
    (12 MP: 0.45 → 0.23 s); dev profile ~1.1 s → ~0.7 s. Dependencies already build at
    `opt-level = 2` in dev (`Cargo.toml` since the scaffold), so CI's >60 s was its 2-core runners,
    not a debug build: no profile change.

## Review (M4 must-fixes): LOW findings left open

From the code review of the must-fixes (the HIGH and both MEDIUM were fixed, one commit each, with a
test that fails without the fix):

- The exit save runs after eframe has already persisted the shell state (`save` before `on_exit`), so
  a document recovered into `Recovered/` isn't reopened at the next launch; it's only in the folder.
  Reachable only by exiting past the prompt.
- ~~A prompt left showing after another command closed the document (MCP) stays up; its Save then
  fails without a word.~~ Fixed: the prompt goes with its document, and every failed save of an open
  document is shown (a cancelled save dialog isn't a failure).
- More `serde_json::json!({"path": path})` on a `PathBuf` (`Pending::Open`, `save_for_exit`), which
  panics on a non-UTF-8 path; same fix as the M3 finding below.
- The `Recovered/` file name is the document's name as is; a `.studio` header could carry path
  separators (unreachable today: only a document without a file goes there, and opened ones have one).
- Ctrl+Q does nothing in Library/Develop while a text field has focus (LightCraft's rule), but quits
  in Film/Pixel even then (Studio consumes it).
- The `StageCache` keeps up to two full-size stages per open document; memory not measured.
- Non-drag edits arriving faster than a render (scripted) keep dropping stale results; the base
  updates once they pause.

## Review (milestone 3): LOW findings left open

From the code review of milestone 3 (HIGH/MEDIUM were fixed, one commit each, with tests):

- `serde_json::json!({"path": path})` (shell `reopen`, smoke `params`) panics on a non-UTF-8 path; build the
  value from `to_string_lossy` instead.
- `Document::from_bytes` accepts any pixel mode/depth/size from the file and more than one base-marked
  layer; normalise or reject (RGB/U16, a size cap, exactly one `stdv` layer).
- `open_into` falls back to the source path even when the stored content hash differs (a photo replaced
  on disk opens with the old document); a deleted catalog entry makes `library.import` report nothing
  imported, giving the opaque "couldn't be imported".
- `document.close` keeps the last `base_error`; undo/redo re-mirrors and bumps `revision`, so a document
  can't return to "saved" (track the saved settings hash instead).
- `session.interaction` is global: a drag left open on another photo blocks base renders.
- Errors are `String` throughout `studio-doc`; a small error enum would let callers tell transient from
  permanent failures.
- `lightcraft_engine::gpu::set_enabled(false)` in tests is process-global (all tests want the CPU, so harmless).

## Known untested

- Native file dialogs (Import Photos, Import Presets, Open Library): OS dialogs need a person to pick a file. The document commands take paths; their dialogs and menu come with the Pixel persona (M4).
- Dragging files in from Explorer: tests hand egui the dropped files; the OS → winit drag-and-drop path isn't exercised.
- Real keyboard and mouse input through the OS: tests inject egui events and the smoke test runs commands, so OS key mapping (e.g. Ctrl as Cmd) isn't covered.
- Readability at the real display's scaling: renders are at 1.0 points per pixel.
- Windows x64 and Linux x64/ARM64: CI runs fmt, clippy and the headless tests there, but not the renders or the smoke test (no GPU/display on the runners).

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
- **Milestone 3 (PhotoCraft `d836e34`): the document and the library photo.** A `.studio` document is
  made from one library photo (`document.new` on the photo picked in Library) and records it: its
  `PhotoId` (per library), LightCraft's content hash (finds the same file in any library) and its
  source path or demo scene. While the document is open and its photo is in the library, the develop
  settings have one home, the catalog (LightCraft edits them: History, versions, presets all keep
  working); the document mirrors them every frame and re-renders its base layer (16-bit sRGB, on
  LightCraft's CPU path since its GPU kernel is 8-bit; only when no slider drag is open, so a drag
  renders once at release). The file carries a copy of the settings; on open the photo is found by
  hash, then by path/scene, else its file is imported, and the file's settings win as the History
  step "Open <name>.studio". The base layer is PhotoCraft's bottom layer "Develop" (locked, marked by
  an opaque `stdv` block); its pixels are never stored, the document resizes with the render (crop,
  straighten), and rendering isn't an edit (no `*`). The file is one STORE zip (`document.json` +
  `pixel.pcraft`, PhotoCraft's own zip code); the default path is beside the source file
  (`<file>.studio`, keeping the photo's extension so a RAW+JPEG pair gets two documents).
  `Base.max_edge` caps the render's long side (M4's draft renders; the tests set 600 px, since the
  demo photos are 24 MP and CI's runners develop that on the CPU in a minute or more).
  Reserved, not built: `film` settings (PRD-halation order), one undo history (M5: `revision`),
  the command bus (M6: `run_with(id, params)`).
