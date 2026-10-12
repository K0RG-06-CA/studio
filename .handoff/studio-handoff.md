# Studio — handoff

Updated Oct 12, 2026 (afternoon), during UI parity (T3b running). The advisor keeps this file current.
Lives in `.handoff/` (gitignored). The repo is public, so this file must never be committed.

## What I'm building
**Studio** is an Affinity-style all-in-one photo/design app: one shared document and Affinity's persona top bar. It's Rust + egui 0.36, built on the open-source storytold Crafting Apps (LightCraft, PhotoCraft) as pinned Git dependencies. Claude Code writes all the code, using the ECC plugin.

Personas: Library, Develop, Film, Pixel (v1). Vector, Layout and Export come later and are greyed out.

**UI direction:** each persona looks **identical to its storytold app**: Library/Develop = LightCraft, Pixel = PhotoCraft. That means their theme, fonts, colours, layout and menus. **Film** is Studio's own persona and is built from storytold's widgets and conventions. Studio adds only the persona bar, a floating History panel, notices, prompts and disabled states.

## Key files (`C:\Users\ahnsu\Desktop\studio`)
- `PRD.md`: the main spec. `PRD-halation.md` + `halation-research.md` override it on halation/film order.
- `CLAUDE.md`: rules for Claude Code.
- `PROGRESS.md`: checklist, limitations L1–L19, LOW lists, "Going public", "Left for later".
- `docs/ui-parity-plan.md`: the UI parity plan (D1–D4, T0–T8, T4/T8 to-do lists).
- `docs/m5-plan.md`: the M5 design as built (one history, barrier).
- `.reports/latest.md`: the builder's last report (gitignored), if set up.
- `.codex-review/conversation.md`: the Codex review log (gitignored).

## Status
- **M1–M5 done.**
- **UI parity:** T0, T1, T2, T2m, T3 and T3m are done and green in CI. **T3b is running now** (HIGH RISK, effort high). It covers three fixes from T3m's LOWs plus the Pixel input block during a pending Develop render. After T3b: a Codex range review in the auditor pane, then T4–T8 on medium.
- **Repo is public** (Oct 12): `K0RG-06-CA/studio`. Secret scanning, push protection and fork-PR approval are on, and the account email is private. The audit found no secrets and no photos in history. NOTICE, README and CI hardening are done.
- **CI:** 4 runners (Windows/Linux, x64/ARM64), free now. studio-shell runs on `--test-threads=2`, because the render tests time out under parallel load. The x64 parity test runs about 150–180 s per wait (the limit is 300 s). If it creeps past ~250 s, move T8's shared lock earlier.

## UI parity decisions (approved)
- **D1:** Studio's History is a floating panel. PhotoCraft's own History never shows.
- **D2:** the persona bar follows LightCraft's view. Develop is disabled for documents without a photo.
- **D3:** one-time layout reset. Studio reads a new config key and leaves the old one untouched.
- **D4:** both apps' menu bars sit in the persona-bar row. They're built from their public menu models and go through a fail-closed routing table: any unlisted item is disabled, and a CI test walks every item.
- **Ctrl+S** saves the `.studio` document in every persona (never an XMP). Save Metadata to File keeps no shortcut.
- Open Library is off while a document is open. Delete is off for the document's photo. Batch/exports/processors must never write to originals.
- **L15:** cover, never restore. A covered control is better than undoing after the fact.
- Ctrl+Alt+Shift+O = Open As. `document.revert` and `document.saveACopy` are Studio commands.

## How M5 works (still the core)
- **One history, owned by Studio.** Develop steps are settings pairs. Pixel steps are PhotoCraft states. When it's ambiguous, Studio clears the history and shows a notice; it never guesses.
- **Barrier:** while a Develop render is pending, all Pixel input is blocked, and save/close/undo/persona switch wait. After 30 s it rolls back.
- **Key queue:** in Pixel, Ctrl+S/W/N/O and the undo keys run in press order at the end of the frame, after a stroke ends.
- **Documents without a photo** open in Pixel, and saves only ever write `.studio`.

## Workflow
- **Builder:** Claude Code, Opus 5.5. Medium normally; **high** for history/barrier/key queue/save-undo routing. Launch with `claude --effort high` (session only). `/effort auto` clears a saved level.
- **Reviewer:** `/codex-review range <base>..HEAD` from a second Claude Code pane (the auditor) in herdr. Codex is read-only, `ecc:review-pr` verifies its findings, and I pick the fixes. Use it on high-risk tasks and milestone ends; medium tasks get `/ecc:code-review`.
- **Advisor:** agy (Gemini), read-only, rules in `.handoff/advisor-rules.md`. I say "check", and it verifies the report and gives me the next prompt.
- **Only one pane touches the code at a time.**
- Routine: `/exit` → `claude [--effort high]` → `/ecc:resume-session` (nothing after it) → check model + effort → paste the prompt.
- Fresh session at around 50–60% context. Never `/compact`.

## Lessons (don't retry)
- Fable 5.1 gets falsely flagged on this codebase, so use Opus.
- Per-task reviews miss cross-task bugs, so always do a whole-milestone review.
- CI on the default thread count makes the render tests time out. Keep 2 threads.
- Windows CI: one cargo command per step (a multi-line pwsh step only checks the last exit code).
- No PowerShell 5.1 file reads/writes without `-Encoding UTF8` (that caused mojibake). No Python heredoc edits; use Edit.
- Local tests: one crate at a time, `--test-threads=2`, browsers closed (RAM is tight).
- `cargo-deny` on ARM64 Windows: use the x64 release binary (ring needs clang).
- Don't rewrite git history to remove the old Proton email (force-push risk > benefit).

## Open items
- **T4's list:** the History rail button opens Studio's History; History takes no clicks while a PhotoCraft modal is open; the tab-strip cover is never a frame late after a resize/toggle (no clickable ×).
- **T8's list:** a shared lock for the render-heavy tests; the smoke script checks that the persona follows LightCraft's view.
- MEDIUM: Pixel frames over 0.4 s (24 MP Develop render on the UI thread). This is roadmap item 2.
- quick-xml RUSTSEC-2026-0194/0195 update; cargo-deny + Dependabot in CI; the apt package check after ubuntu-latest moves to 26 (Oct 19).
- L16 (Actions + Layer Comps unavailable in Pixel) is a user-visible gap.
- Film needs a LightCraft pipeline hook (halation before the tone curve), offered upstream.
- Korean/CJK font (Noto Sans KR). Real Time LUT + S9 profile are blocked on samples.

## Roadmap
1. **UI parity** (now): T3b → T4–T8, then a whole-milestone Codex review.
2. Pixel slow-frame fix (MEDIUM): Opus, high.
3. Film effects: Saturation RGB → Grain → Blur → Bleach → Halation (6 sub-steps) → stock builder. Opus, medium.
4. S9 colour profile (needs samples), 5. Real Time LUT, 6. M6 MCP server, 7. M7 downloadable builds.
- Idea: ship a "preview" build after the slow-frame fix.

## My machine
Windows 11 ARM64, Rust aarch64-pc-windows-msvc, VS Build Tools 2022 ARM64, DX12. 12 threads, tight RAM. `gh` is logged in as K0RG-06-CA. Plans: Claude Max, ChatGPT (Codex limits), Google AI Pro (agy).

## How I like answers
Short and scannable. Exact prompts to paste. Simple explanations when I ask.
