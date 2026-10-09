# Implementation Plan: Milestone 5 — Round trip + one undo history (rev. 3)

Status: draft, not started. Written 2026-10-09 from PRD.md (milestone 5), PROGRESS.md (M4 deferrals,
limitations L1–L7), a reading of LightCraft `eeb136b` and PhotoCraft `d836e34`, and the merged
GPT + Claude review of rev. 1 (points 1–14 below).

## Requirements restatement

1. **One undo history per document, across personas.** Develop edits (LightCraft) and Pixel edits
   (PhotoCraft) undo/redo in the order they were made, from any persona, with Ctrl+Z /
   Shift+Ctrl+Z and commands (`edit.undo`, `edit.redo`, `history.goto`). A new edit in either
   engine clears redo in both. Undo never resamples pixels: every undo/redo restores a state that
   was recorded.
2. **Memory bounded** (24 MP RGBA16 base ≈ 192 MB; every Develop re-render a Pixel state still
   shows stays alive): Studio owns PhotoCraft's history cap, 1 GiB by default, and tests measure
   what is actually retained.
3. **Save/reopen:** the PRD says "one undo history" and "one save format", nothing about history
   surviving reopen. Decision: it doesn't (as Photoshop; PhotoCraft's `History` isn't
   serialisable, L9). LightCraft's per-photo History panel (catalog, persisted) stays as in M2.
4. **M4 deferrals:** open a PNG/JPEG/TIFF/PSD straight into Pixel (a document without a photo),
   Ctrl+O / Shift+Ctrl+S dialogs, files dropped on Pixel; Pixel layers follow a Develop
   rotate/straighten/flip/crop/scale.
5. **The round trip:** Library → Develop → Pixel and back on one document; clicking Pixel with a
   photo picked and no document open makes one (the PRD flow has no "new document" step).
6. **Save/render hardening** (review 12): a save or close flushes everything first; stale renders
   can never land; a failed save keeps the old file.

## Review points and how rev. 2 answers them

| # | Review point | Answer |
|---|---|---|
| 1 | Counts aren't a correctness mechanism; reconcile per engine; track identity; fail safe | Studio no longer delegates Develop undo to LightCraft's stack at all: a Develop step is a settings pair Studio recorded itself (§ Design 1). Pixel steps are tracked by the identity of PhotoCraft's history states (`Arc` pointers), reconciled right after `px.logic` and after every Studio command. LightCraft's logic and PhotoCraft's never run in the same frame (each runs only in its personas), so each reconcile sees one engine. Any Pixel change the identity diff can't name → the document's history is cleared, with a notice; never a guess. |
| 2 | No folds across the other engine; pruning removes a contiguous prefix | A fold of PhotoCraft states is accepted only when the folded states are the newest Studio steps and all Pixel; otherwise fail safe. A pruned Pixel state drops every Studio step older than it. |
| 3 | Per-document history; non-edit LightCraft entries aren't steps; A→B→undo never touches A | History lives on the document, cleared on new/open/close/replace and when the library changes. A Develop step is a change of **this photo's** develop settings; ratings, imports, "Open x.studio", other photos' edits are not steps and are never undone by Studio. With no document open, Ctrl+Z stays LightCraft's own. |
| 4 | End gestures, copy_back, reconcile before picking a target | `History::flush` (end LightCraft interaction, commit PhotoCraft's transform/type/pen, `copy_back`, reconcile) runs before every undo/redo/goto, save and close. Tests: undo during a slider drag, mid-stroke. |
| 5 | goto/bypass jumps move the engines, render once | Develop undo/redo commits settings; the base renders once a frame for the final settings; PhotoCraft is undone n times then copied back once. A paired step (§ Design 3) needs no render at all. |
| 6 | 1 GiB default; redo policy; oversized state; measure retained bytes and peak | `HISTORY_BYTES = 1 GiB`. Redo states count toward the cap and are never pruned (an edit clears them); the newest undo state is always kept (upstream `trim`), so the cap is a target with one state of slack, documented. Test walks states and sums unique tile bytes; peak during a follow/render is reported by `bench.rs`. |
| 7 | Undo must never inverse-resample; the follow is a recorded Pixel state paired with the Develop step; per-drag, from pre-drag pixels | § Design 3: every anchor-changing Develop edit records one PhotoCraft state (the follow); the Studio step holds both; undo/redo restore the recorded state and set the base's "current render" from it, no render, no resample. A drag renders once at release (LightCraft interaction open → no render), so one follow from the pre-drag pixels. |
| 8 | Atomic follow | Snapshot `st.doc` + `st.history` (Arc clones), preflight locks on every target, run, roll both back on any failure + notice. Never a partly transformed document. |
| 9 | A follow is an unsaved change | The follow bumps `Document.revision` like any edit (the Develop edit already did). PhotoCraft's own `saved_revision` is its close-guard flag, not Studio's; it stays clear (M4). Test: save → rotate → close prompts. |
| 10 | Affine check on a grid | 5×5 grid, 0.25 px tolerance. |
| 11 | Tests | All listed under Task 4. |
| 12 | Save/close flush; renders carry doc id + generation; temp + rename; tests | Task 5. `atomic_write` (temp + rename) is already used; pending renders get `(document generation, settings hash)`; late results for another document/generation are dropped. |
| 13 | Develop disabled for a source-less document; picking a photo with a document open prompts | Develop and Film are disabled (greyed, like v2 personas) while a source-less document is open; Library stays (to pick a photo). **Deviation:** picking in Library doesn't prompt by itself (M3/M4: picking never touches the document); going to Pixel with a different photo picked than the open document's prompts to replace through the existing save/discard/cancel prompt if unsaved, and replaces at once if saved (nothing is lost; the file reopens). Say so if you want the prompt even when saved. |
| 14 | Opening with unsaved changes prompts; Ctrl+S on an opened PSD/PNG never overwrites; warnings shown | Already: `document.open` goes through the prompt. An imported document has no `path` and no `default_path`, so Ctrl+S is Save As (dialog). Import warnings show in the error modal right after opening, before any edit. (The review text was cut after "before the"; assumed "before the document is edited".) |

### Rev. 3 (GPT second pass)

| # | Review point | Answer |
|---|---|---|
| B1 | A pending geometry follow is a barrier: Pixel edits, save, close, undo/redo/goto, persona switches wait; render failure rolls both halves back | § Design 3a. `flush` blocks on the render's receiver (worker thread; same order of time as the save itself, 30 s ceiling → failure path); every command that goes through `flush` therefore waits; `persona.*` and `pixel.*` wait/are refused while the barrier is up, PhotoCraft's `logic` is skipped (no input, canvas already says "Rendering Develop…", top bar shows a busy mark). Failure → commit settings-before + History-before, Pixel doc untouched (no state was recorded), notice. Tests as given. |
| B2 | Geometry edits while a follow is pending coalesce into one paired step from the last checkpoint | § 3a: the pending step absorbs every Develop change (geometry or not) until its render lands; `Base` already renders only the newest settings and drops stale results. Test 0→5→10 = one step; undo = 0° settings with 0° pixels, bit-exact. |
| B3 | A paired step owns exactly one PhotoCraft state | § 4: transforms run under a cloned history, the clone is restored, then one `record`. Test: `past_len` +1; undo returns `past_len` and pixels exactly. |
| R4 | Host base writes are never edits; k=0 never clears redo | § 1 reconcile: only ≥1 appended state is a new step; a changed current doc with the same state list is a host replacement or PhotoCraft's own coalesce (label adopted, redo untouched). Test: undo exposure → render lands → redo works. |
| R5 | Fold guard: removed states and the recipient must be a contiguous Pixel suffix of Studio's history | § 1: P1, D, P2 folding P2 into P1 → fail safe. Test. |
| S6 | Adopt a restored state's base only if its render's settings hash matches the target settings; else keep layers, mark stale, re-render | § 3b: `Written` carries the settings hash; mismatch → layers kept, `Base.shown = None`, a render starts. Test as given. |
| M7 | Develop-only steps aren't free; count Studio-owned bytes in the same budget, share History vectors via `Arc`, cap steps at 1000 | § 5: `Arc<Vec<HistoryStep>>` per boundary shared by neighbouring steps; `Step::bytes()` estimate; `HISTORY_STEPS = 1000`; Studio prunes Develop-only steps from the oldest end itself and lowers PhotoCraft's `max_bytes` by its own bytes. A Pixel step at the oldest end can only go when PhotoCraft pops it (new L12). Test as given. |

## What the upstream reading established

### LightCraft `eeb136b`

- Session-wide undo: `pub undo: Vec<UndoEntry { label, op, folder }>` / `pub redo` on `Session`
  (`crates/engine/src/lib.rs:149-150`), cap 1000 (drops index 0), in memory, cleared when a library
  opens (`library.rs:308-310`). `commit(label, op)` (lib.rs:376) applies, journals, pushes an undo
  entry with the inverse op, clears redo. `undo_step()` / `redo_step()` (lib.rs:443, 458).
- Ops (`crates/catalog/src/lib.rs`): `Op::SetDevelop { id, settings: Arc<DevelopSettings>, label,
  edited }`, `Op::PushHistory { id, step }`, `Op::SetHistory { id, history: Vec<HistoryStep> }`
  (lib.rs:115-121), batches. `develop_op(id, settings, label)` = Batch[SetDevelop, PushHistory]
  (lib.rs:597-603); its inverse (what LightCraft's own undo applies) is Batch[SetDevelop(old),
  SetHistory(old list)]. `commit` and `Op` are pub (Studio's `open.rs` already commits one).
  **To verify in Task 1:** `Op::Batch` and `Op::SetHistory` are constructible from outside; if not,
  the fallback is `develop_op` with the label "Undo X" (adds a History row, as `history.restore`
  does).
- `set_develop(id, settings, label)` commits unless an interaction is open on that photo (then it
  applies silently, no undo entry, no History step). `begin_interaction` / `end_interaction` /
  `cancel_interaction` are pub (lib.rs:606-628); a drag commits one entry at release; `session.
  interaction.is_some()` tells a host a drag is open. `edit.undo` ends the interaction first.
- `develop_of(id) -> Option<Arc<DevelopSettings>>`: a new `Arc` on every change (Studio's
  `Base::mirror` already detects changes by pointer).
- Ctrl+Z is skippable: `pub native_shortcuts: HashSet<String>` on `LightcraftApp`
  (`ui-egui/src/lib.rs:148`); `shortcuts::handle` skips any shortcut text in it (`"Cmd+Z"`,
  `"Cmd+Shift+Z"`), and returns early when `ctx.egui_wants_keyboard_input()`.
- Per-photo History: `Photo.history: Vec<HistoryStep { label, settings }>`
  (`catalog/src/model.rs:202-238`), `HISTORY_LIMIT = 200`, journaled, survives restart.
  `history.restore {index}` *appends* a step "History: X" (an edit, not a jump).
- Geometry (`crates/develop/src/settings.rs`, `crates/geom`): `orientation: Orientation`
  (EXIF-style; `rotated(cw)`, `map(x, y, w, h)`, `inverse()`), `crop.geometry: CropGeometry
  { rect (normalised, straightened frame), angle (degrees, −45..45) }`, `crop.flip_h/v`, `geometry:
  Geometry` (Upright/perspective, manual), `optics: Optics` (lens). Pipeline order: user
  orientation → lens → perspective → crop+straighten → flips, one resample.
- Point mapping, pub on `lightcraft_pipeline::geometry::Frame` (`pipeline/src/geometry.rs`):
  `new(src_w, src_h, s, apply_crop)`, `with_lens(.., Option<&EmbeddedLens>)`, `out_to_norm(out_w,
  out_h)`, `norm_to_out(..)`, `transformed_to_corrected(p)`, `corrected_to_transformed(p)`.
  Lens distortion exists only as rendered→source (`Warp::to_source`); no forward map (L11).
- Commands: `photo.rotateLeft` / `photo.rotateRight` (`cmd/library.rs:590-591`); flips toggle
  `crop.flip_h/v` (History label "Flip").

### PhotoCraft `d836e34`

- `History` lives in `photocraft-ops` (`ops/src/lib.rs:36`), not re-exported by the engine; every
  method Studio needs is reachable through `DocState.history`, so no new dependency. Private
  `undo: VecDeque<HistoryState { label, doc: Arc<Document>, layers }>` and `redo`; `pub
  max_states` (50), `pub max_bytes` (8 GB from prefs; 0 = unlimited).
- Methods (pub): `record(label, before, layers)` (clears redo, pops the front past `max_states`),
  `undo(current)`, `redo(current)`, `can_undo/can_redo`, `undo_label()` (current label),
  `redo_labels()`, `entries()` (past labels, then current), `state(i) -> Option<Arc<Document>>`
  (past state, read-only; `None` at/after the current), `past_len()`, `set_current_label`,
  `purge_last()` (newest state dropped, redo cleared, label kept), `clear_redo()`, `clear()`,
  `unique_bytes`, `pixel_bytes`, `trim(&current) -> usize`.
- `trim` counts unique tiles (by `Arc` pointer) over current + redo + undo states, drops the oldest
  undo states past `max_bytes`, always keeps the newest; runs only after `record` and in
  `apply_prefs`, never after a host's `st.doc = ...`. `max_states`/`max_bytes` are re-applied from
  prefs in `add_document` and `apply_prefs`.
- No jump-to-state, no snapshots, no callbacks, no `Serialize`. `History: Clone` (cheap);
  `analysis_cmds::compound` clones it, runs commands, restores it (= run commands with no step).
- `DocState` fields all pub (`doc`, `history`, `active_layer`, `selected_layers`, `revision`,
  `saved_revision`, `last_damage`, …); `Session::active_mut()` only (set_active first).
- `Session::{undo, redo}() -> bool` pub (false while a job runs on the doc); commands `edit.undo`,
  `edit.redo`, `edit.toggleLastState`. `PhotocraftApp::run` goes through
  `transform_tool::intercept` first (during Free Transform, undo/redo step the box's own history).
  Its History panel row click loops `app.run("edit.undo"|"edit.redo")`; its footer buttons are
  inert (L4). `Session::edit(label, f)` records a labelled step (`lib.rs:438`).
- Tiles are copy-on-write `Arc<Tile>` (256×256; RGBA16 tile = 512 KiB; 6000×4000 ≈ 192 MiB). A
  stroke's state costs its touched tiles; a base render written into the current doc stays alive
  in every state that shows the old one.
- Import: `photocraft_io::{import(name, bytes) -> ImportResult { document, warnings }, is_psd}`
  (`crates/io/src/lib.rs:142-154`): `.pcraft`, PSD/PSB, raw, SVG, flat (PNG/JPEG/TIFF). Flat → one
  "Background" raster layer (no alpha: transparency + position locked), EXIF-upright; mode/depth
  as decoded. PSD → groups, adjustment, text, smart objects, masks, blend modes, locks,
  `psd_blocks`; unmodelled things come back as warnings. The engine already depends on
  `photocraft-io`. No engine `file.open`; the UI's drop path needs `Services::import`, which Studio
  doesn't set.
- Transform: `edit.transform {layer?, matrix [a,b,c,d,e,f], interpolation, target}`
  (`transform_cmds.rs:328`): one top-level layer or group (recursive) per call; `position`/`all`
  locks (own or inherited) → error; with a selection only selected pixels move; text/shape
  composed, smart objects re-rendered, linked masks follow; canvas size unchanged.
  `transform_layer` is `pub(crate)`; no multi-layer form (L10). `layer_multi_cmds::{move_targets,
  moved}` as in M4 (whole-pixel, no step). `photocraft_doc::Layer::effective_locks` (or the
  equivalent the task finds) for the preflight.

### Studio (M4 as built)

- `StudioShell::ui` order (`crates/studio-shell/src/lib.rs:404-443`): close handling →
  `document_tick` → `shortcuts` → `lr.logic` (Library/Develop/Film only) → `px.logic` (Pixel only)
  → top bar → persona area → overlays → prompt → error. Studio's shortcuts run before either
  engine's; the two engines never run in one frame. PhotoCraft's menu bar isn't drawn in Pixel.
- `studio_doc::pixel`: `Link { id, copied, current, written: Vec<Written>, notice }`,
  `copy_back` (an edit), `write_base` (a render), `refuse` (PhotoCraft undo + `clear_redo`),
  `restored`, `place` (`moved` for a crop; `LAYERS_STAY` otherwise). `Anchor { frame, origin,
  scale }` from `base.rs::anchor`. `Base { mirrored, shown, pending: (hash, anchor, rx), failed,
  started, max_edge, stages, hold, ready }`; `Base::mirror` bumps `revision` on a settings change.
- `Document { name, source: Source, develop, film, pixel, path, revision, saved_revision, link }`;
  `.studio` = `document.json` + `pixel.pcraft`, written with `photocraft_format::atomic_write`
  (temp + rename).

## Design

### 1. Studio owns the history; the engines store the states

`studio_doc::history::History` on the document (cleared with it):

```
Step {
    label: String,
    develop: Option<(Arc<DevelopSettings> /*before*/, Arc<DevelopSettings> /*after*/,
                     Vec<HistoryStep> /*photo History before*/, Vec<HistoryStep> /*after*/)>,
    pixel: Vec<Pointer>,   // PhotoCraft states this step made, newest last (their Arc addresses)
}
steps: Vec<Step>, redo: Vec<Step>, pixel_seen: Vec<Pointer> + current: Pointer, follow: Option<Pending>
```

- **A Develop step** is a change of this photo's develop settings seen by `Base::mirror` while no
  LightCraft interaction is open (a drag is one step, at release). Label: the top of LightCraft's
  `undo` if it grew this frame, else "Develop". Ratings, imports, "Open x.studio", other photos'
  edits and anything else on LightCraft's stack are not steps and are never touched.
- **Develop undo/redo = a commit of the recorded settings**, not LightCraft's `undo_step`:
  `session.commit("Undo X", Batch[SetDevelop(before), SetHistory(history_before)])` — the same ops
  LightCraft's own undo applies, so Lightroom's History panel shows the step gone; redo commits
  the `after` pair. **Applied through `commit`, never `catalog.apply`:** `commit` pushes the op
  to `pending_log` (engine `lib.rs:379`), so it is journaled and survives a restart;
  `catalog.apply` alone is not journaled (`pending_log` is private) and would be lost. The cost
  is that LightCraft's own stack grows with "Undo X" entries (cap 1000, oldest dropped); Studio
  never reads that stack, so this is harmless. Test: undo → save the library → new Studio
  session on the same library → the undone settings are still undone (and the History row gone).
- **Capturing a pair:** `History.develop_base: Arc<DevelopSettings>` + the photo History list at
  the last step boundary. `Base::mirror` (start of `document_tick`, so it sees the previous
  frame's LightCraft logic; also in `flush`) compares `develop_of(photo)` with `develop_base`
  only while `session.interaction.is_none()`: different → one step `(develop_base, now)`, label
  = the label of LightCraft's top `undo` entry if the stack grew since the last boundary, else
  "Develop"; `develop_base = now`. During a drag the interaction is open: `mirror` keeps copying
  the settings for the live base, but records nothing; at `end_interaction` (release, or Studio's
  `flush`) one step from the pre-drag settings. A click in LightCraft's per-photo History panel
  is `history.restore` → a commit → a normal new step "History: X" (undoing it removes the row
  that restore appended). Studio's own undo/redo commits set `develop_base` themselves, so they
  are never seen as new steps. A change Studio didn't make (even LightCraft's own undo, if ever
  triggered) is simply a new step: no bypass case on this side. **Verify first:**
  `Op::Batch`/`Op::SetHistory` constructible (fallback: `develop_op("Undo X")`).
- **One engine per frame, by construction, not assumption:** `StudioShell::ui` gets one
  `engine_logic(ctx)` that `match`es the persona and runs either `lr.logic` (Library, Develop,
  Film) or `px.logic` (Pixel) and reconciles that engine right after; there is no second call
  site. No `debug_assert` in non-test code (the crate forbids panics there). Test: switch
  personas across frames and check that per frame at most one of `Lightroom.frames` /
  `Pixel.frames` advanced, and that a `persona.*` command issued mid-frame doesn't run the second
  engine until the next frame.
- **A Pixel step** = the PhotoCraft states recorded since the last reconcile. Identity, not
  counts: Studio keeps the `Arc` addresses of every past state (`history.state(i)`) and of the
  current doc; after `px.logic` and after every Studio command it diffs:
  - old list is a prefix of the new one with k ≥ 1 new states → k Pixel steps (labels from
    `entries()`), Studio redo cleared, LightCraft redo cleared (it is unused anyway).
  - the state list is unchanged and only the current doc changed (k = 0) → **not an edit**: a
    host replacement (`write_base`, `restored`, a refused edit put back) or PhotoCraft's own
    coalesce of the current step (a nudge); the current label is adopted, nothing is pushed,
    redo is **not** cleared. Studio's own writes are the common case, so this must be exact: a
    redo after an exposure undo whose render landed in between still works.
  - new list = old minus its last k and `redo_labels()` grew by k → PhotoCraft undid k itself
    (its `toggleLastState`, a scripted `pixel.edit.undo`): redo them back in PhotoCraft and run
    the Studio undo for those steps (Develop steps between included), engines moved first, one
    copy_back at the end.
  - the mirror case for an engine-side redo.
  - new list = old minus its last k, no redo growth → a fold (`purge_last` + relabel) or a
    refused edit of M4's `refuse`: accepted only if the k removed states **and the surviving
    state they fold into** are a contiguous all-Pixel suffix of Studio's history (P1 P2 → P1';
    P1 D P2 folding P2 into P1 → fail safe, since the fold reaches across D).
  - new list lost a prefix of m states right after Studio's own `trim` → pruned: drop every
    Studio step older than the oldest surviving Pixel state (contiguous prefix of the global
    history, Develop steps included).
  - anything else (a lost prefix without a trim, both ends changed, a current doc that is no past
    state after an undo) → **fail safe:** `steps` and `redo` cleared, PhotoCraft's redo cleared,
    notice "History reset: PhotoCraft changed it in a way Studio can't follow". Never a guess.
- A refused edit (M4 `refuse`) is reconciled by the same diff right after the command (net: no
  change or a dropped Pixel step), not special-cased.
- **Scope and lifetime:** `History` is a field of `Document`; `replace`/`close` drop it; a library
  change (`session.catalog` identity) clears it with a notice. With no document open the Ctrl+Z
  keys are left to LightCraft (its `native_shortcuts` entries are inserted only while a document
  is open).

### 2. Flush before any decision

`History::flush(lr, px, doc)`: `session.end_interaction()` if open (a drag becomes its step),
`px.apply_pending()` (transform/type/pen committed), `copy_back`, reconcile. Runs before
`edit.undo`, `edit.redo`, `history.goto`, `document.save/saveAs`, `close`, `new`, `open`, the quit
prompt and `unsaved()`.

### 3. Geometry changes are paired steps; undo never resamples

- **Anchor** carries the map's inputs (source size, `Arc<DevelopSettings>`, embedded lens, scale)
  and `frame` hashes only the parts that can't be mapped (`optics`, `geometry`).
- When a Develop step changes the anchor, the step is marked **pending** and the base render that
  lands for it is written as a **recorded PhotoCraft state** ("Develop: Rotate Left"): new base
  pixels + size + every layer transformed. That state's pointer joins the Develop step → one
  Studio step.

#### 3a. A pending follow is a barrier

- While a step is pending (from the Develop change at release until its render has landed as a
  state): PhotoCraft's `logic` is skipped and `pixel.*` commands return "Develop is rendering";
  `persona.*`, `edit.undo/redo`, `history.goto`, `document.save/saveAs/close/new/open` and the
  quit prompt go through `flush`, which **waits** for the render (blocking `recv` on the
  worker's channel, 30 s ceiling) and lands it before going on. The top bar shows a busy mark,
  the Pixel canvas its existing "Rendering Develop…". No Pixel edit can ever interleave with a
  follow, so a paired step is always exactly D + one state.
- **Coalescing:** every Develop change made while the step is pending (another straighten notch,
  an exposure, a History-panel click) is absorbed into it: `after` and the History list move,
  `before` stays the last checkpoint. `Base` already renders only the newest settings and drops
  stale results, so one render lands, one state is recorded, one step 0→10.
- **Render failure** (or the 30 s ceiling): the step is dropped, the settings and the photo
  History are committed back to the checkpoint (`before`), the Pixel doc is untouched (no state
  was recorded), notice "Develop couldn't render …: the edit was undone". `Base.failed` keeps
  reporting as in M3 until the settings change.
- **Undo of a paired step:** flush; `session.undo()` in PhotoCraft (exact pre-follow pixels,
  masks, selection, canvas size); commit the settings-before; adopt the restored state's base
  (§ 3b), so no render starts and `restored()` has nothing to place. Redo is the mirror. Bit
  identical both ways.

#### 3b. Adopting a restored state's base

- Every `Written` register entry carries the **settings hash** its render was made from. When an
  undo/redo/goto lands on a state, the base it shows is looked up: hash == the target settings'
  hash → adopt it (`link.current`, `Base.shown` set, no render); otherwise the layers are kept
  as the state has them, `Base.shown = None` (stale) and a render of the target settings starts
  and lands as a host replacement (not a state, not an edit). The anchor is the same in both
  cases (an anchor change always recorded a state), so nothing is ever moved or resampled here;
  if the anchors differ anyway → fail safe: state left as it is, notice.
- Non-anchor Develop edits (exposure…) record no Pixel state (the base is derived); a Pixel state
  from before one shows an older render → the mismatch path above re-renders.
- M4's `moved`-for-a-crop and `LAYERS_STAY` paths go: a crop is a follow like any other
  (whole-pixel translation still uses `moved`, exact, inside the recorded state).
- **Within one straighten/crop drag** LightCraft's interaction is open → no render → one follow
  at release from the pre-drag pixels (M3's "a drag renders once at release"); the on-canvas crop
  tool uses interactions too. Test.

### 4. The follow itself (atomic)

- Map old render px → new render px through `Frame::with_lens(..)`: `out_to_norm` →
  `transformed_to_corrected` → orientation change (`from.inverse()` then `to.map`, normalised) →
  `corrected_to_transformed` → `norm_to_out`; sampled on a 5×5 grid, affine fitted on three
  points, every grid point within 0.25 px, else **not affine** (Upright or lens changed) → the
  recorded state holds the new base + size only, layers unmoved, notice (M4's wording, naming
  what changed).
- Affine: snapshot `st.doc` + `st.history` (Arc clones); preflight: every top-level non-base
  layer's `effective_locks` (position/all) → if any is locked, no layer moves, notice (M4 rule);
  `doc.selection = None` (`ponytail:` dropped; upgrade `select.transformSelection`); whole-pixel
  translation → `moved`; else per layer `execute("edit.transform", {layer, matrix,
  interpolation})` (nearest for right angles/flips with whole offsets, bicubic otherwise); any
  error → restore both snapshots, notice. The per-layer commands each record a state under the
  cloned history; the clone is **restored** afterwards (those states go), then the base pixels +
  size are written and **one** state is recorded (`Session::edit("Develop: …")`), so
  `past_len()` grows by exactly one. `Document.revision += 1`; PhotoCraft's `saved_revision`
  kept equal (its guard, not Studio's).

### 5. Memory

- `pub const HISTORY_BYTES: usize = 1 << 30; pub const HISTORY_STEPS: usize = 1000;` One budget
  for both engines' states **and Studio's own steps**. Studio sets `st.history.max_states =
  usize::MAX / 2` and `max_bytes = HISTORY_BYTES − studio_bytes` on attach and on every reconcile
  (PhotoCraft's prefs re-apply 50 / 8 GB otherwise), and calls `st.history.trim(&st.doc)` after
  every `write_base` and `copy_back`, then reconciles (prune case).
- **Develop-only steps aren't free:** a step holds two `Arc<DevelopSettings>` and two photo
  History lists (up to 200 `HistoryStep`s). The lists are `Arc<Vec<HistoryStep>>` per boundary,
  so neighbouring steps share one (`after` of one is `before` of the next) and a 5000-edit run
  holds ~5000 settings + ~5000 list `Arc`s, not 10 000 vectors. `Step::bytes()` estimates
  label + `size_of::<DevelopSettings>()` × 2 + the list's `len × size_of::<HistoryStep>()` when
  the step is the first to hold that `Arc` (`ponytail:` struct sizes, no deep walk). Studio
  prunes from the oldest end when `steps.len() > HISTORY_STEPS` or `studio_bytes +
  photocraft_retained > HISTORY_BYTES`: a Develop-only step is dropped at once; a step that
  owns a Pixel state can only go when PhotoCraft pops that state — upstream `History` drops the
  oldest state only inside `record` (`max_states`) or `trim` (bytes), never on demand (**L12**),
  so Studio lowers `max_bytes` and waits for the next trim; the prefix rule holds throughout
  (nothing newer than an undroppable Pixel step is dropped).
- Policy (documented in `history.rs`): redo states count and are never pruned (an edit clears
  them); the newest undo state is always kept (upstream `trim`), so retained bytes ≤ cap + one
  state; a document whose single state exceeds the cap keeps one step of undo.
- **Paired geometry steps count in full.** Each one's state holds a whole base render (192 MB at
  24 MP) plus the resampled layers, and upstream `tile_bytes` walks every layer surface (the base
  included), so they count toward `HISTORY_BYTES`: at 24 MP the cap holds about **five** paired
  steps (fewer with layers), then the oldest is pruned. Non-geometry Develop edits cost nothing
  in history (no state). Test: k geometry steps → retained bytes ≈ k × render bytes and the
  prune lands at the expected k.
- **A pruned step's render is gone, and nothing can still want it:** pruning drops the Studio
  step and everything older (contiguous prefix), so neither its Pixel state nor its Develop half
  can be undone to; the register entry whose tiles are gone is dropped (`gone()`); `link.current`
  keeps the current render strongly; redo states are never pruned, so a redo target always
  exists; `restored()` only ever runs for a state that is being undone/redone to, whose tiles
  are alive by definition. Test: prune past a paired step → the History list ends there, undo
  stops there, no notice, no render started.
- Tests sum unique tile bytes over `state(i)` docs + current + redo (Studio helper mirroring
  `tile_bytes`) and assert ≤ cap + newest state; `bench.rs` reports the peak during a follow and a
  render at 24 MP (release, real file), not asserted.

### 6. Renders: identity, not just the hash

`Base.pending` carries `(document generation, settings hash, anchor)`; `Document` gets a
`generation: u64` bumped by `replace`/`attach`; a result for another generation or another hash
is dropped; `Base::renewed` drops the receiver. (`atomic_write` already does temp + rename; a
failed write leaves the old file.)

### 7. Documents without a photo

`source: Option<Source>`: no base layer, `Base::sync` returns early (`up_to_date` true), no lock
guard, no follow, `default_path` None. Develop and Film personas disabled while such a document
is open (button greyed with "Open a library photo for Develop"); Library stays. `path` is never
the imported file: Ctrl+S is Save As.

## Patterns to mirror

| Category | Source | Pattern |
|---|---|---|
| Module shape | `crates/studio-doc/src/pixel.rs` | module doc explains the design; link state on `Document`; notices via `take_notice` |
| Errors | `studio-doc` throughout | `Result<_, String>`; never panic; a refusal becomes a notice in the shell's error modal |
| Commands | `crates/studio-shell/src/lib.rs:156` `run_with` | string ids, JSON params, documented in the `run_with` doc comment |
| Engine-side tests | `pixel.rs::tests` (`attached`, `paint`, `undo`) | headless with both sessions, `Base.max_edge = 600` |
| Shell tests | `tests_photocraft.rs` (`studio()`, `pixel_edited`, `press`) | kittest harness; renders checked by the offscreen test |
| Upstream limitations | `pixel.rs` module doc `compile_fail` doctests + `l2_*`/`l3_*` runtime tests | each has a test that fails once upstream fixes it |
| Progress | `PROGRESS.md` M4 section | one line per task, limitations block, "Left for later" |

## Files to change

| File | Action | Why |
|---|---|---|
| `crates/studio-doc/src/history.rs` | CREATE | `History`: steps, identity reconcile, flush, undo/redo/goto, pairing, cap, trim, fail safe |
| `crates/studio-doc/src/lib.rs` | UPDATE | `pub mod history`; `history` + `generation` on `Document`; `source: Option<Source>`; `from_image` |
| `crates/studio-doc/src/pixel.rs` | UPDATE | new `Anchor`; follow as a recorded state; atomic transform; `restored` same-anchor only; trim after writes; L8–L10 doctests |
| `crates/studio-doc/src/base.rs` | UPDATE | `anchor()` with the map's inputs; pending carries generation; reuse a state's render on a paired undo; no-op without a source |
| `crates/studio-doc/src/file.rs`, `open.rs` | UPDATE | source-less files; `open_into` only with a source |
| `crates/studio-doc/src/bench.rs` | UPDATE | peak bytes during a follow and a render |
| `crates/studio-doc/Cargo.toml`, workspace `Cargo.toml` | UPDATE | `photocraft-io` at the pinned rev |
| `crates/studio-shell/src/lib.rs` | UPDATE | `edit.undo/redo`, `history.goto`; routing of `pixel.edit.*`; reconcile after `px.logic` and every command; flush before save/close/prompt; shortcuts; `pick_open_path`; dropped files; `persona.pixel` auto document / replace; Develop+Film disabled without a source |
| `crates/studio-shell/src/persona.rs` | UPDATE | per-document disabling of Develop/Film |
| `crates/studio-shell/src/pixel.rs` | UPDATE | `panels.history = false`; L4 note retired |
| `crates/studio-shell/src/dock.rs` | UPDATE | Pixel default layout gets Studio's "History" tab; an old saved Pixel layout resets |
| `crates/studio-shell/src/history_panel.rs` | CREATE | the egui list (steps, current highlighted, click = goto) |
| `crates/studio-shell/src/lightroom.rs` | UPDATE | `native_shortcuts` gets `Cmd+Z`/`Cmd+Shift+Z` while a document is open |
| `crates/studio-shell/src/tests_photocraft.rs`, `tests_lightcraft.rs`, `tests_ui.rs` | UPDATE | new tests; `files_dropped_on_pixel_dont_open_in_photocraft`, `l4_*`, `after_a_develop_rotation_pixel_layers_stay_*` replaced |
| `apps/studio/src/main.rs` | UPDATE | `pick_open_path` (rfd) |
| `apps/studio/src/smoke.rs` | UPDATE | M5 script steps; writes `smoke.png` |
| `PROGRESS.md`, `PRD.md` (one note), `NOTICE` (if needed) | UPDATE | bookkeeping |

## Tasks (one commit each; validation before every commit)

### Task 1 — `studio_doc::history`: steps, identity reconcile, undo/redo, memory

- **Action:** Design §1, §2, §5 without the geometry pairing (§3 lands in Task 2, but the `Step`
  shape already has `pixel: Vec<Pointer>` and `develop: Option<..>`). Verify `Op::Batch` /
  `Op::SetHistory` first. `flush`, `reconcile_pixel`, `note_develop` (called from `Base::mirror`'s
  change path with the label), `undo`, `redo`, `goto`, `entries()` for the panel, `clear(reason)`
  with notice. Cap takeover and trim.
- **Mirror:** `pixel.rs` module doc; `Base::mirror` for the settings-change hook.
- **Validate (headless, both sessions):**
  - D P D P → undo ×4 in reverse order (settings and layers checked), redo ×4; Lightroom's History
    panel list equals the list before each undone step;
  - undo → library saved → a new session on the same library: the undone settings stay undone
    (journaled through `commit`);
  - a slider drag (`begin_interaction`, several `set_develop`, `end_interaction`) is one pair,
    from the pre-drag settings; a `history.restore` click is a normal new step "History: X";
  - one engine per frame: across persona switches at most one of `Lightroom.frames` /
    `Pixel.frames` advances per frame;
  - a Develop edit after a Pixel undo clears PhotoCraft's redo; a Pixel edit after a Develop undo
    clears Studio's redo;
  - a rating, `library.import` and "Open x.studio" add no step and are untouched by undo;
  - document A edited, document B opened, undo in B does nothing to A (A's settings unchanged);
  - undo during a slider drag ends the drag (one step) then undoes it; undo mid-stroke (pointer
    held) → the stroke is committed first;
  - `px.execute("edit.undo")` directly, with a Develop step after the Pixel step → Studio jump
    (Develop step undone too, lists consistent, one copy_back);
  - a fold (`purge_last` + relabel) of the newest Pixel step is merged; a fold reaching across a
    Develop step → history cleared + notice; a refused edit (M4 `refuse`) leaves the list as
    before;
  - a library change clears the history with a notice;
  - memory: with `max_bytes` small, after k renders + strokes retained bytes (unique tiles over
    states + current + redo) ≤ cap + newest state, the register's old renders are `gone()`, and
    the Studio steps lost a contiguous prefix; PhotoCraft prefs re-applying 50 / 8 GB are
    overridden on the next reconcile; redo states survive a trim;
  - 5000 exposure edits on a photo with a full 200-step History: Studio-owned bytes stay under
    budget, `steps.len() == HISTORY_STEPS`, the oldest were pruned as a contiguous prefix,
    neighbouring steps share their History `Arc`;
  - undo exposure → its render lands (host replacement) → redo still works (redo not cleared by
    the k = 0 reconcile);
  - P1 D P2, then a fold of P2 into P1 (`purge_last` + relabel) → fail safe, notice.

### Task 2 — Pixel layers follow a Develop rotate / straighten / flip / crop / scale, as a paired step

- **Action:** Design §3, §3a, §3b, §4, §6: new `Anchor`; the follow recorded as one PhotoCraft
  state paired with the Develop step; the barrier (`flush` waits, Pixel logic skipped, commands
  refused, busy mark), coalescing into the pending step, failure rollback; the map through
  `Frame`; atomic transform; hash-checked base adoption on undo/redo; `pending` with generation.
  `LAYERS_STAY` reworded; `moved` kept for whole-pixel crops.
- **Mirror:** `place`/`restored` in `pixel.rs`; `moved` error handling.
- **Validate (headless):**
  - `photo.rotateLeft`, straighten 5°, `crop.flip_h`, a crop, a crop at another `max_edge` scale:
    a painted pixel lands on the rotated photo pixel (within 1 px); PhotoCraft's `past_len()` grew
    by exactly one with the label "Develop: …";
  - straighten 0→5→0 then undo ×2: raster **and** masks bit-identical to before, selection and
    canvas size restored; no render started by the undos (`base.started()` unchanged);
  - barrier: rotate → `document.save` + `close` in the same frame → the file has the rotated
    settings **and** the transformed layers; rotate → `pixel.paint.stroke` during the render is
    refused; `persona.pixel` during the render returns after the render landed (one state);
  - render failure (photo file removed) → settings and History back to the checkpoint, Pixel doc
    untouched, notice, no step;
  - straighten 0→5→10 before the first render lands → one step; undo → 0° settings with 0°
    pixels, bit-exact;
  - `past_len()` after a follow = before + 1 (the per-layer transforms left no states); undo of
    the pair returns `past_len()` and pixels exactly;
  - rotate → exposure (rendered) → goto before both → redo the rotation → the base shows the
    rotation with the **original** exposure (hash-matched adoption); a state whose base hash
    doesn't match the target settings keeps its layers and re-renders;
  - strokes between geometry changes: undo/redo walk the exact states;
  - a layer touching the canvas edge survives rotate + undo intact (nothing clipped away);
  - through a paired step's undo, old states line up with the old base;
  - a position-locked layer → nothing moved, state still recorded (base + size), notice; an
    Upright change the same; a refused `edit.transform` → whole doc rolled back, notice; a
    selection is gone after a follow;
  - a straighten drag (`begin_interaction` … several `set_develop` … `end_interaction`) renders
    once and transforms from the pre-drag pixels (no double resample: compare with a one-shot
    straighten);
  - a render that arrives for an undone generation/settings is dropped;
  - save → rotate → `unsaved()` true (close prompts).

### Task 3 — Shell: commands, shortcuts, Studio's History panel, reconcile points

- **Action:** commands `edit.undo`, `edit.redo`, `history.goto {index}`; `pixel.edit.undo | redo
  | toggleLastState` route to Studio's; reconcile after `px.logic` and after every `run_with`;
  `flush` before save/close/new/open/quit prompt/`unsaved()`. Shortcuts Ctrl+Z / Shift+Ctrl+Z /
  Alt+Ctrl+Z in every persona while a document is open (`consume_exact`; not while
  `egui_wants_keyboard_input`); LightCraft's `native_shortcuts` entries while a document is open;
  in Pixel with a Free Transform / type / pen session open, Ctrl+Z goes to PhotoCraft's own
  `app.run("edit.undo")` (its intercept; M4's nudge test). Studio's **History** tab in Pixel's
  dock (one list, persona tag per row, current highlighted, click = `history.goto`); PhotoCraft's
  panel off; an old saved Pixel layout resets. L4 retires with its test.
- **Mirror:** `shortcuts()`/`consume_exact`; `dock::tabs_known`; `lightroom.rs` for a Studio tab.
- **Validate (kittest):** Ctrl+Z in Develop undoes a Pixel step made after the last Develop edit
  and vice versa; Shift+Ctrl+Z redoes; the History tab lists both kinds in order and a click
  jumps (settings + layers); Free Transform nudge undo unchanged; Ctrl+Z in a LightCraft text
  field leaves the history alone; with no document open Ctrl+Z in Library undoes a rating
  (LightCraft's own); offscreen render `pixel-history.png` looked at.

### Task 4 — A document without a photo: PNG/JPEG/TIFF/PSD straight into Pixel

- **Action:** Design §7. `Document::from_image(name, bytes) -> Result<(Document, Vec<String>),
  String>` via `photocraft_io::import`; `document.open {path}`: `.studio` as now, else import,
  warnings → error modal "Opened with warnings" right after opening. `photocraft-io` added at the
  pinned rev. Ctrl+O → `document.open` through `pick_open_path` (rfd in `main.rs`, fake in
  tests); Shift+Ctrl+S → `document.saveAs` through the save hook; a file dropped on Pixel →
  `document.open` (prompt if unsaved). `persona.pixel` with no document and a photo picked →
  `document.new`; with a document open for another photo than the one picked → replace through
  the existing prompt if unsaved, at once if saved (review 13 deviation, see table). Develop and
  Film disabled while a source-less document is open.
- **Mirror:** `run_document`; `pick_save_path`; the drop test for the drop path.
- **Validate:** a PNG (1×1 and a 16-bit one) and a PSD fixture (written by `photocraft_io::export`
  with a group, a mask and a blend mode) open with their layers; a PSD with something unmodelled
  shows its warning; no base layer, no render (`base.started() == 0`); edits, undo, save as
  `.studio` through the dialog (Ctrl+S never writes the PNG/PSD), reopen without a source; a bad
  file is an error; Ctrl+O with a fake picker; a drop opens (and prompts if unsaved); Develop/Film
  buttons disabled and `persona.develop` returns an error; `persona.pixel` makes a document from
  the picked photo, and replaces a saved one picked differently, prompts for an unsaved one.

### Task 5 — Save/close flush and render identity (review 12)

- **Action:** anything not already landed in Tasks 1–2: `flush` on every save/close path incl.
  `save_for_exit`; generation on pending renders; `atomic_write` failure path checked (old file
  kept, error shown).
- **Validate:** same-frame paint → save (the stroke is in the file); undo → save (the undone
  stroke isn't); a late render for an undone document/settings is dropped and the file is
  unchanged; a save interrupted (unwritable target) keeps the old file and shows the error.

### Task 6 — Round trip end to end, smoke, docs, limitations

- **Action:** journey kittest: pick → `persona.pixel` (auto document) → Develop edit → Pixel paint
  → `photo.rotateLeft` (paint follows) → Ctrl+Z ×4 from Develop, then from Pixel, in order → redo
  → save → reopen (settings and layers; history empty) → close. Smoke script: write `smoke.png`
  at start; `document.open smoke.png`, `pixel.layer.new.layer`, `edit.undo`, `document.close`;
  on the photo document: `lightcraft:develop.set`, `persona.pixel`, `pixel.layer.new.layer`,
  `edit.undo` ×2, `edit.redo` ×2, `lightcraft:photo.rotateLeft`, `edit.undo`. PROGRESS.md (M5
  ticks, limitations, "Left for later": Upright/lens follow, history persistence, selection
  follow, History tab in Develop, prompt on picking when saved), PRD.md note (undo history is per
  session), NOTICE if `photocraft-io` needs it.
- **Upstream limitations, each with a test that fails once fixed:**
  - **L8** PhotoCraft `History` can't jump to or replace a state: Studio jumps step by step, and
    an old base render stays alive in every state that shows it until the byte cap prunes it
    (`compile_fail` doctest naming `History::state_mut`; as with L5, no runtime test can name the
    API upstream would add).
  - **L9** `History` has no `Serialize`: undo doesn't survive save/reopen (`compile_fail`
    doctest through `DocState.history`).
  - **L10** No multi-layer `edit.transform`; `transform_layer` is `pub(crate)`: one command per
    top-level layer inside a snapshot/rollback (`compile_fail` doctest).
  - **L11** LightCraft's lens warp is rendered→source only: layers can't follow a change of the
    lens corrections (`compile_fail` naming `Warp::from_source`; module path verified). Upright
    (a homography) is Studio's own simplification, under "Left for later".
  - **L12** PhotoCraft `History` can't drop its oldest state on demand (only inside `record` via
    `max_states` or `trim` via bytes): a Pixel step at the oldest end of Studio's history goes
    only at the next trim, so Studio's step cap can lag by one Pixel state (`compile_fail`
    doctest naming `History::pop_oldest`).
  - L4 retired (PhotoCraft's History panel no longer shown).
- **Validate:** full validation; offscreen renders; smoke test on DX12 (ask first: real window);
  CI after push (only when asked).

## Validation (every task)

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Milestone end: `$env:WGPU_BACKEND="dx12"; cargo test -p studio-shell -- --ignored render_personas`
(look at `pixel-history.png`), then `cargo test -p studio --test smoke -- --ignored` (real window: ask).

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| `Op::Batch`/`SetHistory` not constructible from outside | Low | fallback `develop_op("Undo X")` (a History row, as `history.restore` already does); checked first thing in Task 1 |
| A paired step's follow state is large (base + resampled layers per geometry change) | Certain | that is the price of exact undo; bounded by `HISTORY_BYTES`; measured in `bench.rs` |
| Follow at 24 MP per straighten release is slow (adds to the open MEDIUM frame-time item) | Medium | release only; bench; not fixed here |
| The barrier stalls the UI for the render's length on save/close/switch (~0.4–0.9 s at 24 MP, up to 30 s on a slow decode) | Certain | busy mark; same order as the save itself; the ceiling fails into the rollback path |
| `flush` blocking on the worker channel while LightCraft holds a lock the worker needs | Low | the worker only owns its `PreviewJob`; `session.accept` runs after `recv` on the UI thread (M3 design) |
| `Option<Source>` ripples through base/open/file/tests | High (churn) | mechanical |
| Fail-safe clears fire in practice (a PhotoCraft path Studio didn't foresee) | Low | the notice says what happened; the smoke test and journey test would show it |
| LightCraft's stack fills with "Undo X" entries (cap 1000, drops oldest) | Certain, harmless | Studio never reads it |

## Acceptance

- [ ] Tasks 1–6 complete, one commit each, PROGRESS.md updated after each
- [ ] Validation passes on every commit; offscreen renders looked at; smoke test passes on DX12
- [ ] Undo across personas in order, per document; redo cleared by an edit in either engine; no
      undo ever resamples (bit-identical tests); retained bytes measured ≤ `HISTORY_BYTES` + one state
- [ ] PNG/PSD open straight into Pixel; Pixel layers follow rotate/straighten/flip/crop/scale
- [ ] Limitations L8–L11 each have a test that fails once upstream fixes them; L4 retired
- [ ] Patterns mirrored, not reinvented; no upstream code edited

## Complexity: Large

Tasks 1 and 2 are the real work (history core with identity reconcile; paired geometry steps with
exact undo). Tasks 3–6 are wiring, hardening and tests. Order: 1 → 2 → 3 → 4 → 5 → 6.
