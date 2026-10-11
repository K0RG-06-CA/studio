# Implementation Plan: UI parity (roadmap 1, after milestone 5)

Status: approved 2026-10-10 (D1–D3 decided below); T0 in progress. Written 2026-10-10 from PRD.md › UI and look, CLAUDE.md's UI parity
rule, PROGRESS.md › Roadmap, Studio's shell at `9a5b797`, and a reading of LightCraft `eeb136b`
and PhotoCraft `d836e34` (upstream paths below are under each checkout's `crates/ui-egui/src/`).

## Goal

Library and Develop look exactly like LightCraft, and Pixel exactly like PhotoCraft. "Exactly"
covers theme, fonts, colours, panel layout and shortcuts. Film is Studio's own persona, drawn
with LightCraft's widgets and tokens. Studio adds only:

- the Affinity-style persona bar on top;
- what the shared document needs: Studio's History (in PhotoCraft's style), the save prompt, the
  error notice, the "Rendering Develop…" mark, and disabled states.

Nothing is recoloured or re-docked, so a storytold UI update ports with no restyling.

## What upstream offers (the facts this plan rests on)

- **LightCraft draws its whole window into a host `Ui`.**
  - Entry points: `LightcraftApp::logic(&Context)` (lib.rs:603) and `LightcraftApp::ui(&mut Ui)`
    (lib.rs:792). There is no eframe.
  - Every panel is egui 0.36's nestable `Panel::…show(ui)`, so its 42 pt top bar sits under
    Studio's bar. Nothing covers it.
  - Things outside the host area:
    - its overlays (notices, dialogs, the library-problem window, the second-window viewport);
    - two dim layers that cover the whole screen, as they do in LightCraft itself.
  - `ui` draws its own overlays and handles `ui.quit`.
  - It has one window and no modules: `ViewMode` (grid, detail, compare…) and `RightPanel` decide
    what shows.
  - Its shortcuts run in `logic`.
  - Style: installed once (`theme::apply`, `install_fonts`, both pub). Its `Tokens` are stored in
    ctx data under `Id::NULL`, and there is a dark theme only.
  - Its History (`RightPanel::Activity`) is a private fn. The row and header widgets are public
    (`right::header`, `widgets::section_header`, `widgets::slider`, `icons::paint`).
- **PhotoCraft draws its whole window through `eframe::App::ui(&mut app, ui, &mut Frame)`**
  (lib.rs:1081). Studio already passes eframe's hidden test frame for `logic` (L5).
  - Its panels are nestable `Panel::…show(ui)` too: title bar with menus, options bar, status
    bar, tool bar, right dock with its icon rail, and the document area.
  - Its overlays are ctx-level, and `ui` draws them all, Timeline included.
  - Its default theme is `ProMedium`, not the `Pro` Studio forces today. Tokens live in ctx data
    under `"photocraft-theme"`.
  - Its History panel is off by default (state.rs:326). Its History group sits between Navigator
    and Layers (dock.rs:44).
  - Every dock body (Layers, History, Properties…) is private, so there is no way to put a host
    panel in its dock (L1).
- **Fonts are per `Context`.** Both apps call `ctx.set_fonts` with their own definitions, and
  both define "semibold". Whichever app ran last wins, which already happens today after a visit
  to Pixel. Parity needs the persona's fonts installed when switching to it.
- **Both apps can be drawn bare in a kittest `Harness`.**
  - LightCraft: `LightcraftApp::new(Session::with_demo(), Services::default())`.
  - PhotoCraft: `Harness::builder().wgpu().build_eframe(..)` with `PhotocraftApp::setup_context`,
    as in its `examples/snapshot.rs`.
  - That gives same-state, same-renderer references to compare Studio against.

## 1. What gets deleted

| Where | What | Why it can go |
| --- | --- | --- |
| `crates/studio-shell/src/theme.rs` | The whole file: `Tokens`/`TOKENS`, `apply`, `visuals`, `dock_style`, `placeholder`, and its 2 tests | Studio's Adobe grey. Each persona now draws in its app's own style, and Studio's pieces read the persona app's `Tokens`. |
| `crates/studio-shell/src/dock.rs` | The whole file: `Tab`, `Spec`/`spec`, `sections`, `default_layout`, `stack`, `layout_is_usable`, `tabs_known`, `Viewer`, and its 2 tests | Studio's Adobe docking. Each app brings its own layout. |
| `Cargo.toml` (shell and workspace) | the `egui_dock` dependency | Nothing uses it after `dock.rs` goes. |
| `lib.rs` | `StudioShell.layouts`, the layout checks in `restore`, the `window.resetWorkspace` command, and the `persona_area` placeholders (options bar, tool strip, filmstrip, `DockArea`) | Re-docking. Old saves still load, because serde ignores the unknown `layouts` field (tested). |
| `lib.rs` | `LIGHTROOM_KEYS` (G/E/D/R), `view.grid`, `view.loupe`, `view.develop`, `tool.crop`, and `lightroom::Go` | Lightroom Classic keys layered over LightCraft. Parity means LightCraft's own keys. Decision D2. |
| `persona.rs` | `photoshop_layout()`, and the "looks like Adobe X" doc comments | No Studio layouts left. |
| `lightroom.rs` | `Side`, `show`, `tab`, `right`, `right_shown`, `canvas`, `canvas_title`, `overlays`, the quit relay in `logic`, and the style capture/restore | Studio drew LightCraft's panels one by one. `LightcraftApp::ui` now draws the whole window, overlays and quit included. |
| `pixel.rs` | `RAIL_W`, `DOCK_MIN_W`, `tool_bar`, `options_bar`, `canvas`, `panels`, `overlays`, `tokens()`, `style` and its swap in `logic`, and `ThemeKind::Pro` plus the forced panel flags (all but `history = false`) | Same story: `eframe::App::ui` draws PhotoCraft's whole window, overlays included, in its own theme. |
| `pixel.rs` doctests | The L7 pair (Timeline private) | PhotoCraft's own `ui` draws the Timeline windows, so the limitation no longer shows in Studio (see § 3). |
| `main.rs` | `studio_shell::theme::apply(&cc.egui_ctx)` | No Studio theme. |
| `NOTICE` | The theme credit for LightCraft's measured colours | Only if nothing else uses those values once `theme.rs` is gone. |
| Tests that assert Studio's layout or theme | `tests_ui`: `each_persona_shows_its_adobe_layout`, `dragging_a_panel_tab_regroups_*`, `dropping_a_tab_on_a_groups_edge_splits_it`, `a_panel_dropped_away_*`, `the_collapse_button_folds_*`, `dragging_a_column_separator_resizes_it`, `right_click_reset_workspace_*`, `corrupt_saved_layouts_*` (plus `corrupt_saves`, `leaf_*`, `leaves`, `windows`)<br>`tests_lightcraft`: `spike_a_*`, `spike_b_*`, `in_tab`, `assert_studio_style`, `studio_theme_survives_*`, `library_shows_lightcraft_in_lightroom_classic_places`, `dock_columns_resize_*`, `g_e_d_r_*`, `lightroom_keys_*`<br>`tests_photocraft`: `spike_a_photocraft_pieces_*`, `pixel_is_photoshops_layout_on_photocraft`, `pixel_leaf` | Each is replaced by a parity test (§ 4) or by the "stays" tests below. Behaviour tests that only used `pixel_leaf` to find the canvas switch to `app.last_canvas_rect`. |
| `PROGRESS.md` › Automated checks | Lines citing deleted tests | They are rewritten to cite the parity tests. |

## 2. What stays, and why

| What | Why |
| --- | --- |
| **Persona bar** (`top_bar`): persona buttons, Ctrl+1…7, tooltips on disabled personas, document name, busy mark | It's Studio's one addition (PRD). It is drawn in the current persona's app `Tokens` (LightCraft's `chrome`/`text` in Library, Develop and Film; PhotoCraft's in Pixel), so it matches what sits under it. |
| **Ctrl+1…5 taken before LightCraft** (it uses them for its Edit sections) | The PRD gives Ctrl+1…7 to the persona bar. This is a Studio choice, not an upstream limitation. The existing `ctrl_*` tests stay. |
| **Commands and `run`/`run_with`**, studio-doc unchanged | Every action is a command. Studio's document, history, follow and base layer don't depend on the look. |
| **Save prompt and error notice** (`egui::Modal`) | The shared document needs them. They draw with the global style, which is now the persona app's, so they look native with no styling of their own. |
| **Studio's History panel** (`history_panel.rs`) in Pixel | One history across both engines. Its logic is unchanged; it is restyled with PhotoCraft's `Tokens` and widgets (`widgets::card`, `row_selected`, `text_faint`). Placement: D1. |
| **"Rendering Develop…" mark** over Pixel's canvas | It tells the user why Pixel is waiting. It is now painted at PhotoCraft's own `app.last_canvas_rect` in its tokens. |
| **Barring Pixel during a pending follow** (HIGH 2) | Its mechanism changes. `ui.disable()` doesn't reach PhotoCraft's ctx-level windows, which its own `ui` now draws. Instead Studio puts an input-blocking layer over the persona area (egui's `Modal` backdrop, transparent). This is its own high-risk task, T3b. |
| **Pixel's Studio keys** (`DOCUMENT_KEYS`, `UNDO_KEYS`, `held`), dropped files taken before PhotoCraft, `release_on_focus_loss`, `raw_input_hook`, `apply_pending`, `sync_views` | PhotoCraft's own file keys and drops would act on its session, not on the document. These are the shared document's minimum. |
| **`Lightroom::logic`, `switch`, `may_close`, `ready`**; LightCraft's close check from other personas | Persona ↔ LightCraft view (D2), saving on leaving, and the unsaved-library guard. |
| **One engine's logic per frame** (`engine_logic`) | A milestone 5 invariant the history depends on. |
| **`Pixel::new` keeps `app.ui.panels.history = false`** | PhotoCraft's own History shows PhotoCraft's states only. The document's history is Studio's (M5). This is the one forced difference in Pixel's dock (L1, D1). |
| **`Persona::lightroom()`, `enabled()`, `version()`**; v2/v3 personas greyed | Disabled states are Studio's minimum. Vector, Layout and Export show nothing until their milestone, because they can't be opened. |

## 3. Upstream limitations that force a difference

Each one gets a test that fails once upstream fixes it, in the style of L1–L12 (a `compile_fail`
doctest naming the missing API, plus a compiling twin that calls a public neighbour, so a typo
can't make it pass).

- **L1, re-aimed: no host panel in PhotoCraft's dock.**
  - Its dock bodies (`dock_panels`, `dock_body`, `history`, `layers`) are private.
  - `dock::show(app, ui, shown, body)` is public but needs every body.
  - So Studio's History can't sit in PhotoCraft's History group. It shows as decision D1 says.
  - Test: the `compile_fail` doctest calls `photocraft_ui_egui::panels::dock_body` with its real
    signature (copied from panels.rs:1109). The twin calls the public `dock::show`.
  - Once it compiles, Studio passes `dock::show` a body that draws its History and hands every
    other tab to `dock_body`.
  - This replaces today's L1 doctest (`panels::layers`), which no longer matters because Studio
    no longer draws panels one by one.
- **L5, unchanged:** PhotoCraft runs only as an `eframe::App`. Studio passes
  `Frame::_new_kittest()` to `ui` as well as to `logic`. No test can name the API upstream would
  add, and an eframe bump breaks the build if it goes.
- **L7, retired:** the Timeline module is still private, but PhotoCraft's own `ui` now draws its
  windows, so Pixel shows them. The doctest pair goes, with a note in PROGRESS.
- **To confirm in T0** (written up as L13+ only if they turn out to be real, each with a test):
  - PhotoCraft's File menu (Open, Save, Close, New) acts on its own session. With Studio's
    `Services` pickers set to None, these items may do nothing, or may open a second document
    into the session.
  - PhotoCraft's Edit › Undo and its History rail button bypass Studio's undo keys. M5's
    identity reconcile should take them as Pixel steps; T0 proves it.
  - LightCraft's File and Edit menus: the same two questions for Develop undo (Studio's step
    recording).

  Each real one gets either a host hook test (`compile_fail` naming the hook) or, where nothing
  can be named, an L5-style note plus a behaviour test that pins the current upstream behaviour.

Not limitations (behaviour as each app ships, so it stays):

- LightCraft's dim layers cover the persona bar too.
- PhotoCraft's title bar drags or maximises the window.
- PhotoCraft's `fit_window` maximises an oversized window once.

## 4. Checking parity automatically

There are three layers, and none needs a person.

**a. Headless layout parity** (kittest, no GPU, runs in CI with `cargo test`):

- Build the bare upstream app in its own `Harness`. Its size is Studio's window minus the persona
  bar, and it has the same state:
  - the same demo library and picked photo for LightCraft;
  - for PhotoCraft, the same PNG opened from the same path, with its History panel off as in
    Studio.
- Run both until settled. Compare their accesskit trees: every labelled node's role, label and
  bounds. Studio's are shifted up by the bar height, and the bar's own nodes are dropped.
- They must be equal, except for nodes listed in a per-persona allow-list. Each allow-list entry
  names its reason (D1's History panel; the document title if Studio's name differs).
- Cases:
  - Library (grid, compare);
  - Develop (loupe with Edit, crop open);
  - Pixel (no document, a document with two layers, the brush tool picked, Image › Adjustments
    open).
- Style checks in the same file:
  - the ctx's `Visuals` and `Tokens` equal the bare app's;
  - the installed fonts equal the persona app's after Pixel → Library → Pixel → Develop
    (compared through the same accesskit text bounds plus a glyph width probe).
- A behaviour test: the persona bar's fill equals the persona app's `chrome` token.

**b. Pixel parity renders** (GPU, opt-in like `render_personas`): a new ignored test
`render_parity`, run with
`$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_`.

- It renders Studio and the bare app for the same cases, at the same size and pixels per point,
  on the same renderer.
- It crops Studio's persona bar off and diffs per pixel. A pixel counts as different above 2/255
  on any channel.
- It fails if more than 0.1% of the pixels outside the allow-listed rectangles differ.
- It writes `target/renders/parity/<case>-{studio,upstream,diff}.png`, and Claude looks at every
  diff image.
- `render_personas` stays, for Studio-only states (prompt, save error, locked layer, History,
  Film).

**c. Smoke test:**

- Drop `window.resetWorkspace`, `view.grid`, `view.loupe`, `view.develop` and `tool.crop` from
  `apps/studio/src/smoke.rs`, and use LightCraft's own commands (`lightcraft:` …) for the same
  steps.
- Add: Film, Pixel's History toggle (D1), and a PhotoCraft menu undo, if T0 says that's a Pixel
  step.
- Run it once at the end, after asking you (it opens a real window).

## 5. Tasks (one commit each, validation green before every commit)

Order: T0 → T1 → T2 → T3 → T3b → T4 → T5 → T6 → T7 → T8. Each task updates PROGRESS.md.

- **T0. Spike: pin what upstream does inside Studio.** Tests only, no production change.
  - LightCraft's `ui` nested under a 34 pt bar: its top bar starts at the bar's bottom.
  - PhotoCraft's `eframe::App::ui` in a child `Ui`: same.
  - Fonts: the last installer wins.
  - **Menu routing.** Inventory every upstream menu item that saves, opens, closes, quits or
    undoes:
    - PhotoCraft: the File menu, and Edit › Undo/Redo;
    - LightCraft: the File and Edit menus.

    Each one must go through Studio's save prompt, history, key queue and never-overwrite-source
    rule. For each item, T0 records whether it can be routed, and how (a `Services` hook, a
    command id Studio can intercept, `native_menu`). An item that can't be routed is hidden or
    disabled in T2/T3, and becomes a new limitation with a test (§ 3).
  - **History toggle (D1).** Can PhotoCraft's History rail button and Window › History be routed
    to Studio's panel, with PhotoCraft's own History never appearing, not even for one frame? If
    not, that is a new limitation with a test.
  - Result: the L13+ list is final, and the task list is revised before T1.
- **T1. Parity harness** (test-only `tests_parity.rs`): bare-app builders, the accesskit tree
  diff with allow-lists, and the GPU pixel diff writing `target/renders/parity/`. The first
  real case lands in T2, so every commit is green.
- **T2. Library and Develop are LightCraft's own window.**
  - `persona_area` calls `LightcraftApp::ui` in the area under the bar.
  - Persona ↔ view (D2).
  - LightCraft's style is set as the global style while its persona shows.
  - Delete the `lightroom.rs` pieces, the Classic keys and their commands, and the tests in § 1.
  - Read LightCraft's UI state from the new key, leaving the old one untouched. Test: an old
    config loads without error (D3).
  - Test: with a photo-less document, LightCraft's detail view doesn't take the bar into Develop
    (D2).
  - Parity tests (a) and (b) green for Library and Develop.
- **T3. Pixel is PhotoCraft's own window.**
  - Call `eframe::App::ui` in the area under the bar.
  - Use its default theme, unrecoloured.
  - Delete the `pixel.rs` pieces.
  - "Rendering Develop…" at `last_canvas_rect`.
  - L7 retired.
  - Parity tests green for Pixel, with History allow-listed until T4.
  - Until T3b lands, Pixel shows nothing while a follow is pending (PhotoCraft's `ui` isn't
    called), so it is never left open to input.
- **T3b. Pixel input block during a pending Develop render. HIGH RISK: run on `/effort high`.**
  - The input-blocking layer replaces `ui.disable()` and `Viewer.barred`, so Pixel stays
    visible while it waits.
  - Its test fails without the block. While a follow is pending, it covers:
    - the canvas;
    - the right dock;
    - PhotoCraft's own ctx-level windows (an open adjustment dialog, a floating panel);
    - its menus (title-bar menu clicks and keyboard shortcuts).
  - The HIGH 2 test stays.
- **T4. Studio's History in PhotoCraft's style and place (D1).**
  - Restyle with PhotoCraft's widgets and tokens.
  - PhotoCraft's own History controls (rail button, Window › History) open and close it.
  - L1 doctest re-aimed.
  - The History tests (`pixels_history_tab_*`) move to the new place.
  - Tests: the toggle, the restyle tokens, and a goto click.
- **T5. One style per persona.**
  - On a persona switch, install that app's fonts and style; Film uses LightCraft's.
  - The persona bar, prompt and error notice read the persona app's `Tokens`.
  - Delete `theme.rs` and `main.rs`'s `theme::apply`.
  - Tests:
    - every persona switch, in both directions and for every pair of enabled personas, ends
      with the right app's fonts installed (Film's are LightCraft's);
    - bar colours;
    - the prompt's fill.
- **T6. Film in LightCraft's conventions.**
  - Studio's own frame, built only from LightCraft's pub pieces and `Tokens`:
    - a loupe-coloured canvas;
    - a `panel_w` right panel with `right::header("Film")`;
    - six `widgets::section_header`s in the pipeline order, Halation first, with empty bodies.
  - No effects yet: that is roadmap 3, so we don't build ahead.
  - Tests:
    - the sections' order (moved from `dock.rs`);
    - the header's height and left inset equal LightCraft Develop's Edit header (accesskit
      bounds);
    - the panel fill equals LightCraft's token.
  - Film gets a render in `render_personas`.
- **T7. Delete docking.**
  - Remove `dock.rs`, `egui_dock`, `StudioShell.layouts`, `window.resetWorkspace` and
    `photoshop_layout`.
  - Test: an `app.ron` saved by milestone 5 (with `layouts`) restores.
  - Update the "Until UI parity lands" line in CLAUDE.md, PRD milestones 1/2/4 ("superseded",
    now done), and NOTICE.
- **T8. Close the check.**
  - Run the GPU parity renders and `render_personas`, and look at every PNG and diff.
  - Update the smoke script and run it (ask first).
  - Update PROGRESS.md's Automated checks and Known untested.
  - Tick roadmap 1.

## Decisions (approved 2026-10-10)

- **D1. Studio's History in Pixel is a floating panel.**
  - It is PhotoCraft-styled (its `card` with a History tab strip) and floats at the dock's left
    edge.
  - PhotoCraft's own History controls (the rail button, Window › History) open and close it.
  - PhotoCraft's own History panel must never appear, not even for one frame. If its button or
    menu can't be routed to Studio's panel, that is a new limitation with a test (found in T0).
  - L1's test flags when a real slot in the dock becomes possible.
- **D2. The persona bar follows LightCraft's view.**
  - `persona.library` shows the grid, and `persona.develop` shows the loupe with Edit.
  - When LightCraft's own keys or buttons change its view, the bar follows: detail → Develop,
    every other view → Library.
  - Develop stays disabled for photo-less documents. When it is disabled, the bar doesn't follow
    into it: LightCraft's view goes back to the grid, and a test covers this.
  - Studio's Lightroom Classic G/E/D/R keys and their four commands go, and LightCraft's own keys
    apply.
- **D3. A one-time reset of saved UI state.**
  - Studio's saved dock layouts are dropped.
  - LightCraft's saved UI state is read from a new key (`studio.lightcraft.2`), because Studio
    saved it with LightCraft's sidebar and presets forced off. The old key is left untouched.
  - A test checks that a config saved by milestone 5 (old LightCraft key, `layouts`) still loads
    without error.

## Risks

- **Font swaps on persona switch rebuild egui's font atlas** (one slow frame per switch). This is
  measured in T5. If it's too slow, both apps' fonts get merged under distinct family names
  (only if their "semibold" fonts differ).
- **The accesskit tree may miss unlabelled painter-only widgets** (PhotoCraft's rail icons,
  LightCraft's clock icons). The GPU diff covers those.
- **Async thumbnails and renders** make the two harnesses settle at different times. Both settle
  until LightCraft's `renderer.in_flight() == 0` and PhotoCraft's jobs are idle, as
  `render_personas` does.
- **PhotoCraft's `prefs_ui::tick` re-syncs the theme from its prefs every frame.** Studio no
  longer forces `Pro`, so it just works. Its theme toggle is the user's own.
