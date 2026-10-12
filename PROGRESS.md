# Progress

See [`PRD.md`](PRD.md) → *Milestones for Claude Code*.

## Roadmap (after milestone 5)

In this order, ahead of PRD milestone 6:

1. [ ] **UI parity** (direction change, 2026-10-10; PRD.md › UI and look): each persona looks
   identical to its storytold app (Library/Develop = LightCraft, Pixel = PhotoCraft: own
   theme, fonts, colours, panel layout and shortcuts); Film, Studio's own persona, uses
   storytold's widgets, theme and panel conventions to sit next to LightCraft. Studio keeps only the
   Affinity-style persona bar and the minimum the shared document needs (its History tab, styled
   as a native storytold panel in PhotoCraft's History spot; notices; prompts; disabled states).
   Undoes milestones 1, 2 and 4's Adobe recolouring and re-docking, so upstream UI updates port
   with no restyling. Plan: `docs/ui-parity-plan.md` (approved, D1–D3).
   - [x] T0 spike (`tests_parity_spike.rs`): both windows nest under a host bar; fonts: the last
     installer wins; neither app's menu clicks can be routed in place (LightCraft's Undo becomes
     a new Develop step; PhotoCraft's undoes two steps at once). New L13–L16 and decision D4
     (menus in the persona bar row) are in the plan's "T0 findings".
   - [x] T1 parity harness (`tests_parity.rs`): bare-app builders hosted as Studio hosts them
     (PhotoCraft set up with `setup_context`, its default theme), the accesskit tree diff with
     allow-lists, the style check (visuals, text styles, fonts, both apps' tokens), and the opt-in
     GPU diff `render_parity` (2/255 per channel, fails above 0.1%, PNGs in
     `target/renders/parity/`). Checked on itself: unit tests of each diff, the T0 nesting tests
     now go through it, and `render_parity` diffs each bare app under a stand-in bar against the
     bare app (0 pixels differ on DX12). The bar band is cropped below kittest's 8 pt margin.
     - Review fixes (one commit each, each test seen failing first): font check by hash, not
       bytes; widget state in the tree; `settle` fails after 300 s; persona bar nodes left out;
       whole `Style` and options compared.
     - Left for T2/T3 (LOW): rects compare rounded to whole points (a value at exactly x.5 could
       flip); PhotoCraft's `settle` has no busy signal (give raw cases one); a case whose allow
       rects cover the whole render passes with 0 pixels counted; fractional pixels per point
       would misalign the crop (cases use 1.0); kittest's cursor is in the render (only matters
       when hovering the bar); tokens are read by upstream's ids copied here, so a renamed id
       reads None on both sides; `demo_lightcraft` repeats `demo_shell`'s app setup.
   - D4 approved: Studio hosts both apps' menu bars in the persona bar row (T2m, T3m). No commit
     draws either app's in-window menus before they're routed. Ctrl+S saves the document in
     every persona (T2m). L14 and L16 accepted.
   - [x] T2 Library and Develop are LightCraft's own window (`LightcraftApp::ui` under the bar, in
     its own style; Studio's theme goes back when another persona shows). The persona follows
     LightCraft's view (D2); photo-less documents keep the bar out of Develop. Classic G/E/D/R
     keys and their commands, the per-panel drawing and its tests are gone. LightCraft's state
     is read from `studio.lightcraft.2` (D3; the old key untouched, tested). In-window menus
     hidden (`native_menu`); L13 note and test in `lightroom.rs`/`tests_lightcraft`. Parity
     (a) and (b) green for Library grid/compare and Develop Edit/crop, with no allow-list: the
     reference is LightCraft with its menus hidden (D4's native-menu layout), since hiding them
     also widens its search field. Keys while the menus are hidden: every LightCraft shortcut
     still runs in its `logic`; Studio takes only Ctrl+1…7 and, with a document, the undo keys
     (as before), so none is unavailable. Ctrl+S stays LightCraft's Save Metadata until T2m.
     - Review fixes (one commit each, each test seen failing first): Library never keeps a
       loupe opened between frames as its view; a relaunch in Develop stays in Develop; the D3
       test now fails with the old key; a test that LightCraft's style shows only in its
       personas (seen failing with the hand-back removed).
     - Left (LOW): Studio's style comes back after Pixel's `logic` on its first frame, right
       today only because PhotoCraft sets its theme a frame later; a persona-bar click switches
       after the engines' logic, so the new persona draws one frame in the old one's style; a
       click in LightCraft that opens its loupe moves the bar one frame late;
       `window.resetWorkspace` in Library/Develop resets a dock nobody draws and returns Ok
       (until T7); the smoke script doesn't check that the persona followed LightCraft's view
       (on T8's list in the plan);
       the bar and Studio's modals use LightCraft's text sizes in Library/Develop (T5 styles the
       bar from the persona's tokens).
   - [x] T2m Studio hosts LightCraft's menus in the persona bar row (`lightcraft_menu.rs`): its
     public model (`menubar::menu_bar`), drawn as its in-window menus are, in Library and
     Develop. Every item is classified in `route` (fail closed: an unclassified item is drawn
     disabled and never runs, and `every_lightcraft_menu_item_is_routed` walks the whole model
     and fails on it). Edit › Undo/Redo are Studio's history while a document is open
     (`lightcrafts_edit_undo_menu_item_is_studios_undo`, the T0 test flipped); Quit goes through
     Studio's close guard; Open Library (and the web-only Restore Library) are off while a
     document is open; Photo › Delete, Delete Permanently and Empty Recently Deleted are off for
     the document's photo, and so is LightCraft's Delete key outside its mask, spot and red-eye
     tools. Ctrl+S, Shift+Ctrl+S, Ctrl+N, Ctrl+O and Ctrl+W run `document.*` in every persona
     (Ctrl+S saves the `.studio` file, never an XMP, never touching the photo; tested in
     Library and Develop); the menu shows no shortcut on items whose key Studio takes (Save
     Metadata to File, Create Version, New Album). Each test seen failing with its fix
     reverted.
     - Deviation from the plan: Studio takes these keys before LightCraft's `logic` (as it
       already did for the undo keys) rather than through `native_shortcuts`; same effect, one
       mechanism.
     - Accepted parity difference (Studio owns the document keys): in Library and Develop,
       LightCraft's Create Version loses Ctrl+Shift+S and New Album loses Ctrl+N (both stay in
       its menus, with no shortcut), as Save Metadata to File loses Ctrl+S.
     - Review fixes (one commit each, each test seen failing with its fix reverted):
       LightCraft's Settings › Open Library… (and its library-problem window's Choose…) is off
       while a document is open: its folder dialog is held back while either shows. The
       document's photo never stays in Recently Deleted: LightCraft's own ways past the menu
       (context-menu Delete, its delete confirmation, Remove Folder from Library) are taken back
       the same frame with a notice, so Delete Permanently and Empty Recently Deleted can't reach
       it.
     - Left (LOW): Rename Photos, Convert to DNG and Find Missing relink the photo's file under
       the same `PhotoId`, so the document keeps rendering it, but the `.studio` header's source
       path (and, after Convert to DNG, its content hash) go stale: a reopen then finds or
       re-imports the original; the library-problem window's Retry and Temporary open a library
       by path, past the held folder dialog (only once the document's library has already
       failed); the Delete key on the document's photo does nothing, silently (the menu item is
       greyed, but no notice); Settings' greyed Open Library… says LightCraft's "not available
       here"; the File menu's "Quit LightCraft" quits Studio; the menu model is rebuilt every
       frame in Library and Develop (`menu_bar` plus a copy), even with no menu open; the Delete
       key in the Masking, Remove and Red Eye tools (left to LightCraft) has no test, nor has
       Ctrl+S with no document open (it does nothing, silently); the menu tests close menus with
       a click at a fixed spot of the bar and count Quit's frames exactly; Studio's restore of the
       document's photo is a step in LightCraft's own undo history.
     - L17 (after the review): LightCraft's engine has no command gate, so those deletes can't
       be refused, only taken back. A `compile_fail` doctest in `lightroom.rs` fails once it gains
       PhotoCraft's `session.authorize`; `taking_the_documents_photo_back_leaves_no_trace` checks
       the take-back adds no history step, leaves the document saved, and keeps the photo's id,
       source, hash, edits and file.
   - [x] T3 Pixel is PhotoCraft's own window (`eframe::App::ui` under the bar, its default
     theme unrecoloured; Studio's style goes back when another persona shows, each app's before
     the other's `logic`, which also fixes T2's LOW on the hand-off order). Studio's pieces of
     PhotoCraft (options bar, tool bar, canvas, dock tab, overlays, Pro theme in Studio's
     colours) and their tests are gone; L7 retired. Its in-window menus are hidden (an empty
     `native_menu` backend), its shortcuts still run. Parity (a) and (b) green for Pixel with no
     document, two layers, the brush, Levels open; allow-listed: Studio's History, and the open
     dialog, which PhotoCraft centres on the whole window (the persona bar included, so half the
     bar higher). Until T3b, Pixel draws nothing while a Develop geometry change renders (its
     `logic` doesn't run, so neither does its `ui`).
     - L15: its document tab's × and the tab's Close menu would close the document past Studio:
       Studio covers its tab strip (`l15_photocrafts_document_tab_cant_close_the_document`, seen
       failing without the cover).
     - Studio's History floats at the left edge of PhotoCraft's dock (D1) in Studio's look until
       T4.
     - L18: PhotoCraft's home screen (New document…, Open…) opens its own dialogs from its
       private start screen, and can't be covered like the tab strip (nothing public gives where
       its card is). With no Studio document open, Studio closes what it (or its palette) makes
       and says how to get one; with one open, `copy_back` already did. A `compile_fail` doctest
       in `pixel.rs` fails once the start screen is public;
       `l18_a_document_photocraft_makes_itself_leaves_studios_alone` checks the close never
       touches an open Studio document.
     - Review fixes (one commit each, each test seen failing first): PhotoCraft's window waits
       for its `logic` when Pixel is picked again by a click (a flag left from before was drawing
       it one frame without its logic, in Studio's style); quitting with Don't Save takes the
       document from PhotoCraft too (else a spurious "Studio opens documents" notice).
     - Left (LOW): PhotoCraft's theme preference isn't put back with its style (its tokens keep
       it looking right), and its first show sets egui's default dark/light styles that Studio's
       hand-back doesn't restore (as LightCraft's since T2); the tab-strip cover and the History
       follow `last_canvas_rect`, which is stale for a frame after a resize or panel toggle (and
       while a background open's card shows), so the × is uncovered for that frame; the cover
       also blocks a background open's own tab × (cancel); the History floats in PhotoCraft's
       full-screen and arranged views too, and a click on it raises it over PhotoCraft's modal
       dialogs; only the first of several dropped files opens (as before T3); PhotoCraft's Print
       and Export keys stay live (they can't write: Studio gives it no writer services), to
       re-check if those are ever wired.
   - Temporarily unavailable keys (T3 until T3m), dropped in Pixel: F12 Revert, Ctrl+Alt+W Close
     All, Ctrl+Alt+P Close Others, Ctrl+Alt+Shift+O Open As, Ctrl+Alt+S Save a Copy
     (`keys_of_photocraft_file_items_studio_will_hide_do_nothing`).
2. [ ] **Pixel slow-frame fix** (Open items: frames over 0.4 s); the deferred `bench.rs` peak
   bytes during a follow waits on it.
3. [ ] **Film**.
4. Then PRD milestone 6 (MCP).

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
- [x] **4. Pixel persona** — PhotoCraft tools with Photoshop layout (tool bar left, options bar top, Layers and Properties right).
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
  - [x] 9. App wiring (`main.rs`: `PhotocraftApp` with its GPU canvas on eframe's wgpu state;
    PhotoCraft's own undo cap applies: 50 states / 8 GB, its Memory Usage default, so Studio
    adds none), smoke script (Pixel: tool, new layer, undo, redo; the quit must stop at the
    prompt; save; the relaunch reopens the document with its Pixel layer), NOTICE (PhotoCraft's
    engine and editor).
  - [x] Smoke test on DX12 (2026-10-09, 76.6 s for both runs: Pixel edit, undo/redo, the quit prompt, the relaunch with the Pixel layer)

  **PhotoCraft limitations (`d836e34`), each with a test that fails once upstream fixes it:**
  - L1 Layers / History / Properties panels are private: Pixel shows PhotoCraft's whole right dock
    in one Studio tab (`pixel.rs` `compile_fail` doctest).
  - L2 Its engine and menus don't stop delete, arrange, move-below, layer properties, merge-down
    into or flatten on a fully locked layer (`studio_doc::pixel::tests::l2_*`,
    `tests_photocraft::l2_*`); Studio's lock guard takes them back.
  - L3 Its undo restores whole documents, layers the host wrote outside history included
    (`l3_*`); Studio's render register puts the current Develop render back.
  - ~~L4 History footer: Delete state / New snapshot / New document from state do nothing~~
    Retired in M5 task 3: PhotoCraft's History panel is off (Studio's History tab instead), so
    its test went too.
  - L5 Its editor runs only as an `eframe::App` (`logic`/`ui` take an `eframe::Frame` they don't
    use): Studio passes eframe's hidden test frame (`Frame::_new_kittest`, eframe 0.36.2). No
    test can name the API upstream would add; an eframe bump breaks the build if it goes.
  - L7 Its Timeline windows are a private module: not shown in Pixel (`compile_fail` doctest).
  (L6 from the plan, a hard-coded violet wash in the tool bar, doesn't show in the Pro theme
  Studio uses: `pixel-document.png`, so no limitation.)

  **Left for later (not M4):** opening a PNG/PSD straight into Pixel (M5, agreed); Ctrl+O and
  Shift+Ctrl+S in Pixel do nothing until there are open/save-as dialogs; Pixel layers don't follow
  a Develop rotate/straighten/flip yet (they stay, with a notice; M5) or a crop that changes the
  render scale (only tests render at a capped size); the Lightroom filmstrip under Pixel is still
  a placeholder; Pixel's undo is PhotoCraft's own, one shared history is M5.
- [x] **5. Round trip** (2026-10-10) — Library → Develop → Pixel and back on one document, one undo history across personas. Plan: [`docs/m5-plan.md`](docs/m5-plan.md) (rev. 3).
  - [x] 1. `studio_doc::history` (2026-10-09): Studio owns the history, the engines keep the
    states. A Develop step is a settings pair Studio records (before/after `Arc<DevelopSettings>`
    plus the photo's History list, lists shared between neighbouring steps), noted by
    `Document::note_develop` (what `Base::mirror` now is) while no drag is open on the photo,
    labelled by LightCraft's top undo entry when that entry touches the photo; undone/redone by
    `session.commit("Undo X", Batch[SetDevelop, SetHistory])`, so it is journaled (test: a new
    session on the same library) and Lightroom's History rows go and come back. A Pixel step is
    a PhotoCraft state tracked by identity (weak `Arc` pointers), diffed after every copy-back
    and render write (before the copy too, so an undo to a state showing an old render is seen
    before the current render goes back in): appended = new steps; same states, new current =
    host replacement or coalesce (label adopted, redo kept); removed with PhotoCraft's redo grown
    = its own undo (accepted as Studio's when the newest steps are Pixel, else put back and done
    as a Studio jump, Develop steps included, by `Document::reconcile`); removed without redo =
    fold / refused edit (merged into the Pixel step before, else reset); a lost prefix = its cap.
    Anything else resets the history with a notice. `flush` (ends the drag, reconciles) runs
    before `undo`/`redo`/`goto`; `entries()`/`position()` for the panel; `HISTORY_BYTES` 1 GiB /
    `HISTORY_STEPS` 1000 as one budget: Studio sets PhotoCraft's `max_states`/`max_bytes` on
    every reconcile (its prefs re-apply 50 / 8 GB otherwise), trims after every write, prunes
    Develop-only steps from the oldest end itself. Shell: `engine_logic` runs one engine's logic
    per frame by construction (test). Tests in `history.rs` (16) + `tests_photocraft::one_engine_*`;
    the M4 cap tests now use Studio's byte cap. Deviations from the plan, small: a lost prefix is
    always taken as PhotoCraft's cap (its states only ever go from the front, in `record` and
    `trim`), not only right after Studio's own trim; PhotoCraft's own undo/redo of a Pixel-only
    tail is simply Studio's (no redo-back); "undo mid-stroke commits the stroke" is a shell test
    (the stroke lives in PhotoCraft's UI), moved to task 3 in the plan. Review of task 1
    (2026-10-09, two fixes, one commit each, tests first): a prune that takes every old state
    along with an edit is a prune (first surviving state = the document that was current),
    and a lost prefix drops every step older than the oldest surviving Pixel state; PhotoCraft's
    redo going away takes Studio's with it. The shell doesn't call `flush`,
    `undo`, `reconcile` yet (task 3): Pixel steps are recorded through `copy_back`, Develop steps
    through `mirror`; Ctrl+Z in Pixel is still PhotoCraft's own, which the history accepts.
  - [x] 2. Pixel layers follow a Develop rotate / straighten / flip / crop / scale, as a paired step
    (2026-10-09). `studio_doc::follow`: a Develop step that changes the framing (orientation,
    crop, straighten, flips, lens, perspective) waits for its render (`History::pending_follow`),
    and the render lands as **one** recorded PhotoCraft state ("Develop: Rotate Left") joined to
    the step (Develop + Pixel). The map goes through LightCraft's `Frame` (output → normalised →
    lens-corrected → stored → new orientation → new output), 5 × 5 grid, affine fit on three
    points, 0.25 px; whole-pixel shift → `moved`; else `edit.transform` per top-level layer
    (nearest for right angles/flips, bicubic otherwise, `target: pixels`) under a cloned history
    that is put back, so `past_len` grows by exactly one; any error → whole document back,
    notice; a position-locked layer or a non-affine change (perspective/lens) → layers stay,
    base + size still recorded, notice; the selection is dropped (`ponytail:`). `Anchor` now
    carries the map's inputs (settings `Arc`, its hash, source size, embedded lens); a render
    landing in a state of the target settings is adopted (no render), one of other settings is
    put back by a whole-pixel shift or else left as the state has it and re-rendered (a host
    replacement). Barrier: while pending, Develop changes coalesce into the step; `flush` refuses
    (`RENDERING`); `Base::finish` waits (30 s ceiling) and lands it; a render that fails undoes
    the step (settings + photo History to the checkpoint, Pixel untouched, notice). Renders carry
    `Document.generation` (bumped by `attach`): another generation's lands nowhere. Shell:
    PhotoCraft's logic skipped and `pixel.*` refused while pending, `persona.*` and every
    `document.*` / the quit prompt (through `unsaved()`) wait for it first, busy mark in the top
    bar. **Paired undo/redo (user, 2026-10-09):** both halves checked first (no PhotoCraft job, no
    drag open on the photo); the Pixel half moves, then the Develop commit; if the commit fails,
    the Pixel half is redone/undone back, the step is kept, notice ("Couldn't undo X: …"); tested
    with a forced commit failure on both sides (mutation-checked). Tests: `follow.rs` (14),
    `history::a_step_with_a_develop_half_waits_*`, three shell barrier tests. The wait is
    `flush`'s own (user, 2026-10-09): `Document::flush/undo/redo/goto` take the `Base`, which
    starts the render if none is in flight, blocks up to the ceiling (30 s; `Base.ceiling` is
    the test knob) and lands it; a render that fails, can't start or passes the ceiling undoes
    the step and is the error. The shell's persona switch and `unsaved()` (every `document.*`,
    the quit prompt) go through it. Tests: an undo during a pending render (not started, and in
    flight) completes after it lands; a zero ceiling rolls back. The plan's "rotate → exposure →
    goto before both → redo the rotation → the original exposure" test is kept as written (its
    redone state carries the exposure render, so it goes through the re-render path; it fails if
    a render is ever reused by frame instead of by settings, mutation-checked); adoption itself
    is tested with an undo/redo of the rotation alone. A Develop-only step whose commit fails is
    kept with a notice too, as a paired one (user, 2026-10-09; it used to reset the history; task
    1's test rewritten, both sides). Memory (design §5, user, 2026-10-09): ten exposure +
    rotation rounds with the cap in the 24 MP ratio (16 MiB to a 6-tile render, as 1 GiB to 384
    tiles) keep exactly four geometry steps, a contiguous suffix, retained bytes (counted
    independently of upstream) = five renders, under the cap after every step; undo stops at the
    oldest kept step. Its tight-cap phase found a bug, fixed in the same commit: a follow's state
    was joined to its waiting Develop step only after the budget's prune, which took the step
    for a Develop-only one (reset); and a follow whose own trim took every older state drained
    the waiting step too. The diff now joins the follow to its step directly, and a lost prefix
    keeps the waiting step (both mutation-checked). Review of task 2 (Antigravity, 2026-10-09):
    HIGH fixed: a Pixel move failing during an undo or redo (or during the rollback after a
    failed Develop commit, the review's LOW) returned with the step popped and lost; now what
    moved is moved back and the step kept with a notice, or, if that fails too, the history
    starts over with a notice (replaces the planned task 3 item "fail-safe clear when
    PhotoCraft's undo/redo returns false mid-loop"); tests: PhotoCraft's history gone behind
    Studio's back, and injected move failures (`History::fail_moves`, tests only). MEDIUM not
    applicable: no step ever owns more than one PhotoCraft state (a Pixel step is made with one,
    a Develop step gets at most its follow), so a kept step can't point at a pruned state; this
    is now asserted after every reconcile and every Develop note in every studio-doc test
    (`History::assert_one_state_per_step`, tests only; mutation-checked: giving one step every
    state appended in one reconcile fails three tests) (2026-10-10). Not done here: `bench.rs` peak bytes during
    a follow (waits for the slow-frame fix); a single-channel Channels-panel restriction
    on `edit.transform` during a follow is untested.
  - [x] 3. Shell: commands, shortcuts, Studio's History panel, reconcile points (2026-10-10).
    Commands `edit.undo`, `edit.redo`, `edit.toggleLastState` (redo if there is one, else undo,
    as PhotoCraft's) and `history.goto {index}` on the document's one history; PhotoCraft's own
    `pixel.edit.undo|redo|toggleLastState` are routed to them. While PhotoCraft has a Free
    Transform, type or a pen path open, undo/redo/toggle are its own (the box's steps; M4's nudge
    test unchanged); a jump applies what is open first. Ctrl+Z / Shift+Ctrl+Z / Alt+Ctrl+Z in
    every persona while a document is open, taken by Studio before either engine's logic (so
    LightCraft's `native_shortcuts` aren't needed: the key never reaches it); never while a text
    field has the keyboard; with no document, LightCraft's own (a rating undone, test).
    Reconcile points: at the end of every frame (after PhotoCraft's logic, panels and dialogs)
    and after every `run_with`, so a LightCraft command scripted right after a Pixel edit is a
    step after it (test, red before). Undo mid-stroke: PhotoCraft's drag is private
    (`app.drag`, `finish_gesture` is `pub(crate)`), so a Studio can't end a stroke itself; an
    undo key pressed with the button down in Pixel is held and runs in the frame the gesture
    ends, after PhotoCraft committed it: the stroke is a step, then undone (test: redo brings
    it back). Studio's History tab sits above PhotoCraft's dock in Pixel (rows: Original, every
    step, the redo rows dimmed; the current one highlighted; "Develop", "Pixel" or both per row;
    a click is `history.goto`); PhotoCraft's History panel is off (L4 retired). A saved layout
    now resets when it lacks a panel the persona has (panels can't be closed, so it is an older
    save: M4's Pixel layout gets the History tab; two corrupt-save cases that drop a panel now
    reset too, test updated; the save/restore tests move a panel instead of removing it).
    Offscreen render `pixel-history.png` looked at (the persona tags were clipped by the
    margin-less tab; fixed). Mutation-checked: the end-of-frame reconcile, holding the key
    mid-stroke, the text-field guard and the document guard each turn their test red. Not
    done: Studio's History tab in Develop (left for later, plan); the smoke script's new
    commands (task 6). Review of task 3 (2026-10-10, one commit per fix, test red first):
    HIGH: with a pen path (or type) open, undo went to PhotoCraft's plain undo, which reached
    past a newer Develop step, so Studio's jump took that step and the Pixel step under it;
    only Free Transform (whose box PhotoCraft's `run` steps) is PhotoCraft's now, anything else
    open is applied first, then Studio's undo (design §2; the plan's "type / pen" routing was
    wrong: PhotoCraft has no undo of its own for them). MEDIUM: a Free Transform left open
    on leaving Pixel took Ctrl+Z in Develop (it stepped the hidden box, the Develop edit stayed);
    the shell's `flush` (every persona switch, `unsaved()`) now applies what PhotoCraft has open,
    after the barrier, as design §2's flush says. MEDIUM: a second undo key during one stroke
    replaced the first (Ctrl+Z twice undid only the stroke; Ctrl+Z then Shift+Ctrl+Z lost the
    undo); held keys are a queue now, each runs in order once the stroke is in: Ctrl+Z twice =
    the stroke and the step before it (user's question: defined, tested).
  - [x] 4. A document without a photo: PNG/JPEG/TIFF/PSD straight into Pixel (2026-10-10).
    `Document.source` is an `Option`; `Document::from_image` imports through PhotoCraft's own
    importers (`photocraft-io`, at the pinned rev): a PSD keeps its groups, masks and blend
    modes, a flat file is one layer at its depth; Studio's base mark on an imported layer is
    removed. Without a photo: no base layer, nothing renders (`Base::sync` does nothing,
    `up_to_date` is true), no Develop steps, `open_into` does nothing, no `default_path`. A
    `.studio` file may name no photo (then it has no base layer; a base without a photo, or a
    photo without a base, is an error). Shell: `document.open {path?}` opens a `.studio` or an
    image (no path: the open dialog, `pick_open_path`, rfd in the app, fakes in tests; Ctrl+O
    in Pixel); what the import approximated or dropped shows at once in the error modal,
    "Opened “x” with warnings", before any edit; every failed open is shown. A document
    without a photo opens in Pixel; Develop and Film are greyed for it (hover and the
    `persona.*` error say why). `document.saveAs` without
    a path asks through the save dialog (Shift+Ctrl+S in Pixel); a save only ever writes a
    `.studio` file, so an opened PNG/PSD is never written over (test: Ctrl+S asks, the PNG's
    bytes are unchanged, a save pointed at the PNG is refused). Opening anything with unsaved
    changes asks first: the command, Ctrl+O and a file dropped on Pixel (taken before
    PhotoCraft sees it; replaces `files_dropped_on_pixel_dont_open_in_photocraft`). Clicking
    Pixel with a photo picked and no document makes one; a document of another photo is
    replaced, at once if saved, else through the prompt (review 13 deviation). Interpretation,
    not in the plan's words: a document opened from an image is of no photo, so the photo
    picked in Library doesn't replace it (else every Library round trip would). An M4-saved
    file opens: `fixtures/m4-saved.studio` was written by M4's own code (a worktree at
    `fc42139`, its `edited()` + `save`), tested in studio-doc and in the shell (photo found,
    exposure 0.7, its layers, the base renders). Pixel's empty canvas now says "or open a file
    (Ctrl+O)". Offscreen render `pixel-image.png` looked at (Develop/Film greyed, the PSD's
    group, no Develop layer). Mutation-checked: the `.studio`-only save, Develop/Film off, the
    drop, not replacing a photo-less document, the warnings modal and opening in Pixel each
    turn a test red. Not done: raws and SVGs opened this way go through PhotoCraft too (its
    importer takes them; a raw belongs in Library, left for later).
    Review of task 4 (2026-10-10, one commit per fix, test red first). HIGH: opening a
    `.studio` (or reopening one at launch) left LightCraft's earlier pick in place, so the next
    Library-and-back-to-Pixel replaced the document with that photo (at once if saved, else
    through the prompt); the document's photo is now made the picked one whenever a document
    becomes the open one (a multi-selection that already has it active is kept). The M4
    fixture (9,216 bytes) names a demo scene, which LightCraft generates in code: both fixture
    tests passed with `samples/` moved away, the shell one's base render included. MEDIUM:
    the `.studio`-only save refused a name typed without an extension (Linux's save dialog
    passes it on as typed), which saved before task 4; such a name now gets `.studio`, only
    another extension is refused.
  - [x] 5. Save/close flush and render identity (2026-10-10). Most of it was already in place
    and tested: every `document.*` command, a window close and `save_for_exit` settle the
    document first (`unsaved()` → `flush`); renders carry (generation, settings hash) since
    task 2; saves go through PhotoCraft's `atomic_write` (temp file + rename). New tests: a
    late render for an undone Develop edit lands nowhere (the base keeps the saved settings,
    nothing renders again, the file stays byte for byte); a save over an existing file that
    can't complete (file and folder read-only) keeps the old bytes, shows why, leaves the
    document unsaved and no temp file. Neither needed code. What did (design change, approved
    by the user): Studio's keys in Pixel ran by kind at the start of the frame, before
    PhotoCraft committed a stroke, so Ctrl+S in the release frame or mid-stroke wrote the file
    without it, and Ctrl+Z then Ctrl+S in one frame saved first. Now the document keys and the
    undo keys in Pixel are taken in the order pressed (`take_keys`) into the `held` queue,
    which runs at the end of every frame in which no button is down, after the stroke is in.
    egui keeps a button down whose release it never sees (focus lost mid-stroke, Alt+Tab): the
    shell's raw input hook in Pixel releases every button down when the window loses focus, so
    the stroke ends (Photoshop's way) and held keys run. Tests: Ctrl+S in the release frame and
    mid-stroke save the stroke; an undone stroke isn't saved; a stroke cut short by focus loss
    ends and its held save runs. Mutation-checked: running Pixel keys at once, document keys
    before undo keys, no focus-loss release, the queue only on a release event, the stale-render
    hash check and marking a failed save saved each turn a test red. Closes the task 3 LOW on
    releases egui never sees.
    Review of task 5 (2026-10-10, one commit per fix, test red first). HIGH: a stroke dragged
    out of the window and then cut short by losing focus was released at (0,0) (egui forgets
    the pointer once it leaves), painting a segment to the window's corner; the release now
    goes where the shell last saw the pointer. Asked by the user: an undo key queued behind a
    close (Ctrl+W then Ctrl+Z in one frame) is dropped silently instead of showing "History: no
    document is open".
  - [x] 6. Round trip end to end, smoke, docs, limitations (2026-10-10):
    journey kittest (`journey_pick_pixel_develop_paint_rotate_undo_redo_save_reopen_close`:
    pick → Pixel makes the document → Develop exposure → a new layer and a brush stroke in
    Pixel → Rotate Left in Develop, the stroke's bounds turn with the photo → Ctrl+Z ×4 back to
    the original, each state as it was, then Shift+Ctrl+Z ×4, from Develop and again from Pixel
    → save → close → reopen: same settings, layers and paint, the history only the original →
    close, PhotoCraft empty); L8–L12 `compile_fail` doctests, each with a compiling twin (L9
    through a generic `T: Serialize` bound); smoke script with the M5 commands (`smoke.png`, an
    embedded 16 × 16 PNG written at start, opened, edited, undone, closed; on the photo's
    document a Develop edit and a Pixel layer undone ×2 and redone ×2, a rotation undone; the
    relaunch now expects 3 Pixel layers); PRD note (the undo history is per session).
  - [x] Smoke test on DX12 (2026-10-10, 91.5 s for both runs: the PNG opened, edited, undone and
    closed; the photo document's history walked across personas; the relaunch with 3 Pixel layers)
    Review of task 6 (2026-10-10, one commit per fix, test red first). MEDIUM: every
    `document.close` in the smoke script discarded, so the close right after the save no
    longer failed if the save left the document unsaved; only the image's close
    (`document.close discard`) discards now (unit test on the script's parameters).
  - Codex review of all of M5 (2026-10-10, one commit per fix, test red first):
    - HIGH: during a pending follow only PhotoCraft's logic was skipped; its canvas, panels
      and windows still took the pointer, so a stroke painted the unrotated layers and the
      follow left them unturned. Pixel's area and every dock tab are disabled meanwhile
      (egui_dock's tabs don't inherit a disabled `Ui`), its windows not drawn.
    - HIGH: PhotoCraft's coalescing (one key, one state: a slider drag) merged an edit into
      the state before a Develop step; a Develop step now clears its key. (Studio's undo and
      redo needed nothing: PhotoCraft's own `undo`/`redo` clear it, and a mutation of that
      showed no test could tell.)
    - HIGH: a crop redone over a later render kept the stale uncropped render as current and
      shifted the layers out of the crop's frame; a save before the next render wrote them
      misplaced. A render a shift away is kept only when it is of the settings wanted.
    - MEDIUM: one Pixel step at the oldest end blocked all pruning (L12's "lag by one" was
      unbounded: 1 Pixel edit + 5,000 exposure edits kept 5,001 steps). Design (the user's,
      with three changes agreed): past `HISTORY_STEPS` the oldest steps go as a prefix whatever
      they own; Studio stops tracking their PhotoCraft states (undo stops at its oldest step)
      and PhotoCraft drops them at its next `record` (`max_states` = tracked + 1, set at every
      copy-back, so a frame without a Pixel change no longer loses it); states no step owns
      going (all or some of them) take no step; over the bytes only Develop-only steps go
      (PhotoCraft's `trim` frees the pixels). Four tests; mutation-checked (no + 1, no cap, the
      old lost-prefix rule, Pixel steps kept past the cap, Pixel steps pruned on bytes).
    - CI green on all four runners (38111618518); smoke test on DX12 passed again (51.1 s).

  **Upstream limitations (M5), each with a test that fails once upstream fixes it:**
  - L8 PhotoCraft `History` can't jump to or replace a state: Studio jumps step by step, and an
    old base render stays alive in every state that shows it until the byte cap prunes it
    (`compile_fail` doctest naming `state_mut`, `history.rs`).
  - L9 PhotoCraft `History` isn't `Serialize`: undo doesn't survive save and reopen
    (`compile_fail` doctest, a `T: Serialize` bound on `DocState.history`).
  - L10 PhotoCraft has no multi-layer `edit.transform`, and `transform_cmds::transform_layer` is
    `pub(crate)`: a follow runs one command per top-level layer under a copy of the history
    (`compile_fail` doctest, `follow.rs`).
  - L11 LightCraft's lens warp maps rendered → source only: layers can't follow a change of the
    lens corrections (`compile_fail` doctest naming `optics::Warp::from_source`).
  - L12 PhotoCraft `History` can't drop its oldest state on demand (only inside `record` via
    `max_states` or `trim` via bytes): a Pixel step pruned past the step cap leaves its states in
    PhotoCraft, untracked, until its next `record` (`max_states` = tracked + 1) (`compile_fail`
    doctest naming `pop_oldest`).

  **Left for later (not M5):** layers following an Upright change (a homography; Studio's own
  simplification, not upstream) or a lens-correction change (L11); the undo history surviving
  save and reopen (L9); a Pixel selection following a Develop geometry change; a History tab in
  Develop (Pixel has Studio's; Develop shows LightCraft's own); asking before picking another
  photo replaces a saved document; raws and SVGs passed to `document.open` going through
  Library instead of PhotoCraft's importer.
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
- [ ] **Going public** (audit 2026-10-11: gitleaks clean over all history, no photos or EXIF ever committed)
  - [x] NOTICE credits everything PhotoCraft compiles in (JetBrains Mono, Lucide/Feather, the CC BY 3.0 Magnetic Lasso icon, SCOWL) and the crates with extra terms (jpeg-encoder IJG, epaint_default_fonts, libfuzzer-sys)
  - [x] README (what Studio is, pre-release, build, tests, licence, no-affiliation line)
  - [x] CI hardened per zizmor: read-only token, actions pinned to commit hashes, no persisted credentials, superseded runs cancelled

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
- Docking: regroup, split, float (canvas never floats), resize, collapse → `dragging_a_panel_tab_regroups_*`, `dropping_a_tab_on_a_groups_edge_splits_it`, `a_panel_dropped_away_from_the_split_buttons_floats_but_the_canvas_never_does`, `dragging_a_column_separator_resizes_it`, `the_collapse_button_folds_a_panel_group`.
- Layouts kept across switches and relaunches → `each_persona_keeps_its_own_layout_*`, `shell_state_survives_save_and_restore`, `persona_panels_and_lightcraft_settings_come_back_after_a_relaunch`, smoke test (relaunch).
- Look (dark grey, small text, tool-strip tooltips) → renders (checked), `greyed_out_personas_and_tools_*`; Library and Develop in LightCraft's own style → `library_and_develop_are_lightcrafts_own_window` (style check).
- Right-click → Reset Workspace → `right_click_reset_workspace_puts_the_panels_back`.
- Library and Develop are LightCraft's own window (grid, compare, loupe with Edit, crop) → `library_and_develop_are_lightcrafts_own_window`, `render_parity` (checked); a click selects → `develop_edits_the_photo_picked_in_library`.
- Import by dropping files → `files_dropped_on_library_are_imported`.
- Picked photo in Develop, loupe/filmstrip/Edit/Presets → `develop_edits_the_photo_picked_in_library`, renders (checked).
- LightCraft's own keys; the persona follows its view (D2) → `the_persona_follows_lightcrafts_own_view_keys`, `with_a_document_without_a_photo_lightcrafts_loupe_doesnt_take_the_bar_into_develop`; undo keys with a document → `undo_and_redo_keys_with_a_document_go_to_studios_history_in_library_and_develop`.
- Edit, switch to Pixel, relaunch: edit kept → `leaving_develop_saves_its_edits`.
- Panel widths, Edit sections, settings after relaunch → `persona_panels_and_lightcraft_settings_come_back_after_a_relaunch`.
- Preset hover preview; History steps → `hovering_a_preset_previews_it_only_while_hovered`, `history_lists_develop_steps`.
- **Milestone 3** (`studio-doc` unit tests + shell tests): the base layer equals LightCraft's own render within 1/255 → `the_first_sync_renders_the_photo_into_the_base_as_lightcraft_renders_it`; a settings change re-renders it (and a drag renders once, at release; a crop resizes the document) → `changing_develop_settings_in_the_catalog_re_renders_the_base`, `a_slider_drag_renders_once_at_release`, `a_crop_resizes_the_document_with_the_base`, `editing_in_develop_re_renders_the_documents_base_layer`; save/open round trip, bad files → `save_and_open_keep_everything_but_the_base_pixels`, `bad_files_are_errors_not_crashes`; opening into a library (found by hash, imported, the file's settings as a History step) → `open::tests::*`; the commands, the top-bar name, relaunch → `document_new_needs_a_picked_photo_and_puts_its_name_in_the_top_bar`, `a_saved_document_comes_back_after_a_relaunch`, smoke test (document saved, reopened at the relaunch, base rendered on the real app).

## Open items

- **User-visible gap (L16, accepted):** Pixel has no Actions or Layer Comps panels. PhotoCraft
  shows them with its History panel, which stays off because the document's history is
  Studio's.

- **Film track needs a pipeline hook in LightCraft** (smallest possible, offered upstream per PRD › Staying
  in sync): LightCraft `eeb136b` has no way to register an extra develop step, and `PRD-halation.md` needs
  halation on scene-linear light before the tone curve, which its render API doesn't expose (its
  `F32Linear` output is after the curve and clamped). The document only reserves `Film` settings.
- **MEDIUM: Pixel has frames over 0.4 s, source unknown.** Suspect: writing the 24 MP Develop render
  into the PhotoCraft session on the UI thread (full canvas redraw, `last_damage = None`). Measure
  headless first, before any window test.

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

## Review (milestone 4, Pixel)

HIGH and MEDIUM fixed, one commit each, each test seen failing without its fix:
- HIGH: Studio's document keys in Pixel matched with extra Shift/Alt (egui's `consume_shortcut`),
  so Shift+Ctrl+N (New Layer) replaced the document, Alt+Ctrl+W closed it… now exact
  (`chords_sharing_a_letter_*`).
- HIGH: Duplicate Layer on the Develop layer was refused (the copy carried the base's mark);
  the copy is now an ordinary, unlocked layer (`duplicating_the_develop_layer_*`).
- HIGH: Ctrl+C/X/V never reached PhotoCraft (egui-winit sends Copy/Cut/Paste events; the app now
  forwards `raw_input_hook` in Pixel) (`copy_cut_and_paste_reach_photocraft_in_pixel`).
- HIGH (found as MEDIUM, a crash): clicking Pixel after a document was made elsewhere panicked in
  PhotoCraft's Navigator (stale views); views are synced before Pixel draws
  (`clicking_pixel_after_a_document_was_made_elsewhere_draws_it`).
- MEDIUM: the canvas didn't do PhotoCraft's document-area bookkeeping (`last_canvas_rect`, Free
  Transform's steps) (`pixels_canvas_tells_photocraft_*`).
- MEDIUM: PhotoCraft's own file commands / extra documents: `pixel.file.*` refused, other
  documents closed, a closed Studio document reopened with a notice
  (`photocraft_holds_studios_document_and_nothing_else`).
- MEDIUM: an open Free Transform / type edit / pen path is applied before "unsaved?" is asked
  (`an_open_free_transform_is_applied_*`).
- MEDIUM: a refused edit is undone back to the first state that stands (a good edit built on it
  in the same frame left a step undoing to nothing) (`a_good_edit_after_a_bad_one_*`).

LOW, left open:
- PhotoCraft background jobs: a render landing mid-job would discard the job's result. Not
  reachable: Studio leaves PhotoCraft's `background_jobs` off (commands run inline). If it's ever
  turned on, hold renders while a job runs.
- `hold` covers a held button, Free Transform and type, not modal dialogs with live previews
  (Liquify, Camera Raw, filters) or a pen path.
- A notice from the copy-back (layers that stayed after an undo across a rotation) is returned as
  the error of an otherwise successful `pixel.*` command; the copy-back's notices replace an error
  already showing.
- A reopened `.studio` has no anchor for its saved base: if the catalog's crop changed since the
  save, Pixel layers aren't moved to follow it.
- PhotoCraft's `retain_gpu_documents` (part of its document area) isn't called: GPU canvas memory
  of a closed document is kept until the next one replaces it.
- Persona keys and Ctrl+Q still match with extra Shift/Alt; Ctrl+1 (PhotoCraft's Actual Pixels) is
  Studio's persona key by design.
- `canvas()` re-stores `views[idx]` after `canvas_view` (guarded index, redundant).
- `files_dropped_on_pixel_dont_open_in_photocraft` checks PhotoCraft's set-up behaviour, not Studio
  code.

After the review: CI ran out of disk on ubuntu-latest (`ld` Bus error, 30 MB left; PhotoCraft's
debug info), so CI builds without it (`CARGO_PROFILE_DEV_DEBUG: 0`). The smoke script ran several
steps in one slow frame (`app.quit`, then the prompt's cancel before the close request came back):
it now runs one step a frame, two frames apart at least.

## Review (M5 task 6): LOW findings left open

- L8 and L12 call the missing method (`st.history.state_mut(0)`, `pop_oldest()`) instead of
  naming it as a path: Studio can't name `photocraft_ops::History` without a new dependency, so
  a method upstream adds with other arguments keeps the doctest passing.
- The smoke script checks that the M5 commands run without an error, not what they did (the
  undo order, the follow): the kittests check that.
- The embedded `smoke.png` is only decoded by the smoke test (no unit test opens it).
- `lightcraft:` commands other than `photo.rotateLeft` all get the exposure parameters.
- The journey test destructures with `let … else { panic!() }` (tests only; allowed).

## Review (M5 task 5): LOW findings left open

- In Pixel, Studio's keys run at the end of the frame, PhotoCraft's own keys during it: two
  keys in one frame from both (Ctrl+Z then Shift+Ctrl+N) run PhotoCraft's first. Before task 5
  the mirror case (Shift+Ctrl+N then Ctrl+Z) was the wrong one.
- Persona keys (Ctrl+1..4) still run at once, ahead of document keys pressed before them in
  the same frame; a Ctrl+S then Ctrl+2 in one frame isn't a save (it was the same before).
- A real release and a focus loss in the same input batch get a second, synthetic release for
  the same button (egui's state is the previous frame's); for a short click that may count as a
  double click.
- `document.new` from Ctrl+N failing (no photo picked) is silent, as before task 5.
- `StudioShell::pointer` is `pub` for the same reason as `held` (`main.rs`'s
  `..Default::default()`).
- The interrupted-save test relies on a read-only folder refusing new files on Unix: it would
  fail run as root.

## Review (M5 task 4): LOW findings left open

- Several files dropped on Pixel at once: only the first opens, the rest silently not.
- Every import warning goes into one modal, uncapped (a PSD with hundreds of odd layers).
- A `.studio` file renamed without its extension goes the image way and fails as "not an
  image" (sniffing the zip would open it).
- The open dialog's image filter is a fixed list (no "All files"); PhotoCraft reads more.
- An image (like a `.studio`) is read whole and imported on the UI thread.
- `reopen()` clears the error modal when the launch's reopen fails (the app reports it; at
  launch nothing else is shown yet).

## Review (M5 task 3): LOW findings left open

- Pixel's History tab clones every row and lays every row out each frame (no `show_rows`):
  O(steps) per frame, at most `HISTORY_STEPS` (1000).
- ~~An undo key held for a stroke whose release egui never sees (button let go outside the
  window) runs at the next release.~~ Fixed in task 5: losing focus releases the buttons.
- A Free Transform opened before a Develop rotation stays open over the rotated layers until
  applied (its box predates the follow); applying it is PhotoCraft's own transform of the box.
- `history.goto` with a non-integer index says it "needs an `index`".
- `StudioShell::held` is `pub` only so `main.rs`'s `..Default::default()` compiles.
- An error from the end-of-frame reconcile (a jump PhotoCraft began that fails) takes the error
  modal, titled "History", over one set earlier in the frame.
- Untested: an undo key in the release frame itself waits behind the held ones.

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
- 2026-10-11: three lines of `tests_lightcraft.rs` held mojibake (`×` and `→` read as ANSI and
  written back, since `72f7746`); fixed, a repo scan found no others, and CLAUDE.md now forbids
  PowerShell 5.1 file reads and writes without UTF-8.
- **Milestone 2 (LightCraft `eeb136b`), superseded by UI parity T2** (Library and Develop are now
  LightCraft's own window): LightCraft copies Lightroom *desktop*, not Classic, so Studio
  drew its panels one by one into the dock in Classic's places.
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
