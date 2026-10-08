# Studio PRD — Affinity-style all-in-one app with Adobe UI

Oct 8, 2026 · David Ahn

## Overview

Studio (working name) is one desktop app that does photo library, raw development, pixel editing, vector and layout on a single shared document, like Affinity. It keeps Affinity's top bar for jumping between modes, but every mode looks and works like Lightroom Classic or Photoshop.

It is built in Rust on top of the open-source storytold Crafting Apps ([LightCraft](https://github.com/storytold/lightcraft), [PhotoCraft](https://github.com/storytold/photocraft), [VectorCraft](https://github.com/storytold/vectorcraft), [DesignCraft](https://github.com/storytold/designcraft)), so most editing features already exist as separate apps.

**The problem:** Affinity lets you do everything in one app, but its UI feels unfamiliar. Adobe's UI feels right, but the work is split across Lightroom and Photoshop, with round trips and exports between them.

## Goals and non-goals

**Goals**

- One document that every mode edits, with no export or reimport between modes.
- An Affinity-style top bar to switch modes in one click, keeping the current selection and zoom.
- Lightroom Classic layout and shortcuts in Library and Develop.
- Photoshop layout and shortcuts in Pixel (tools on the left, Layers and Properties on the right).
- Runs natively on ARM64 and x64, on Windows and Linux (macOS as a bonus). ARM64 is a hard requirement: every feature, including RW2 decoding and the film effects, must work on ARM64.
- Every action is a command, so Claude can drive the app over MCP, like LightCraft already does.

**Non-goals for v1**

- Opening Adobe or Affinity files beyond PSD and common image formats.
- Cloud sync, accounts or plugins.
- Pixel-perfect copies of Adobe icons or assets (not allowed by storytold's clean-room rules).

## UI and look

Affinity's top bar never moves; everything under it changes per persona and always looks like the Adobe app that persona replaces. The default theme is Adobe-style dark grey (confirmed).

Window layout, shown for the Pixel persona:

```
┌──────────────────────────────────────────────────────────────────────┐
│ TOP BAR (Affinity): [Library] [Develop] [Film] [Pixel] [Vector]       │
│                     [Layout] [Export]               trip-edit.studio  │
├──────────────────────────────────────────────────────────────────────┤
│ OPTIONS BAR (Photoshop): settings for the active tool                 │
├────┬───────────────────────────────────────┬─────────────────────────┤
│ T  │                                       │ Histogram + Navigator   │
│ O  │                                       │ (Lightroom)             │
│ O  │   CANVAS: one shared document,        ├─────────────────────────┤
│ L  │   the same image in every persona     │ Layers (Photoshop)      │
│ S  │                                       │ - Develop edits = base  │
│    │                                       │ - Pixel layers on top   │
│(PS)│                                       ├─────────────────────────┤
│    │                                       │ Properties (Photoshop)  │
├────┴───────────────────────────────────────┴─────────────────────────┤
│ FILMSTRIP (Lightroom): photos in the current album or folder          │
└──────────────────────────────────────────────────────────────────────┘
```

Develop would swap the tools and Layers for Lightroom's slider panels.

- **Top bar (Affinity):** persona buttons on the left, document name on the right. Ctrl+1 to Ctrl+7 jump between personas.
- **Library and Develop (Lightroom Classic):** left panel for Navigator, Presets and History; right panel of slider groups; filmstrip at the bottom; Lightroom shortcuts.
- **Pixel, Vector, Layout (Photoshop):** tool bar on the left, options bar under the top bar, dockable Layers, Properties and History panels on the right; Photoshop shortcuts (V, B, M, Ctrl+J). Shortcuts switch with the persona, so P means Pick in Library but Pen in Pixel, just like in Adobe's apps.
- **Theme:** dark grey, small type, dense panels with collapsible headers.

## Personas and v1 scope

Version 1 ships Library, Develop, Film and Pixel, which covers the Lightroom-to-Photoshop round trip. Vector and Layout come after. (Confirmed.)

| Persona | Feels like | Built from | Key features | Version |
| --- | --- | --- | --- | --- |
| Library | Lightroom Classic Library | LightCraft | Grid, filmstrip, ratings, flags, albums, search | v1 |
| Develop | Lightroom Classic Develop | LightCraft | Light, Color, Tone Curve, masking, crop, presets | v1 |
| Film | Lightroom Classic Develop layout | New + NP DCTL port | Halation, grain, blur, bleach, Saturation RGB, film stock builder | v1 |
| Pixel | Photoshop | PhotoCraft | Layers, adjustment layers, brushes, selections, healing | v1 |
| Vector | Illustrator-ish, Photoshop layout | VectorCraft | Pen and node tools, shapes, booleans | v2 |
| Layout | InDesign-ish, Photoshop layout | DesignCraft | Pages, text frames, master pages, print export | v3 |
| Export | Affinity Export persona | All | Slices, batch export, formats | v2 |

**The key v1 flow:** pick a photo in Library, develop it, click Pixel in the top bar, add layers and retouch, click Develop again and the edits are still there. No "Edit in Photoshop" export step. Pixel can also open a PNG or PSD directly.

## Feature: newer-format RW2 decoding (S9 first)

Studio must develop Lumix S9 RW2 files from the real raw data, not the embedded JPEG. Today LightCraft decodes older RW2 files but opens newer RW2 v4 files as preview only, and the S9 (2024) is likely on that newer format.

- **Done means:** an S9 RW2 opens without the preview-only badge, and highlights, shadows and white balance recover like a DNG converted from the same file.
- **Test files:** your own S9 shots, at several ISOs, in every raw quality setting the camera offers. Keep a few as fixtures in the repo's tests.
- **Reference to check against:** the same files converted with Adobe DNG Converter, which LightCraft already decodes fully.
- **Where it lives:** the `studio-raw-rw2` crate in Studio's fork, plugged into LightCraft's raw decoding.

**Decision: personal fork, newer RW2 format.** Studio keeps its own fork of LightCraft and never sends this decoder back to storytold. The decoder targets Panasonic's newer RW2 format plus a small per-camera table (colour matrix, black and white levels, sensor size and crop). The S9 is the first entry, tested against your own shots. Another Lumix model is added with one table entry plus its test files (free samples on raw.pixls.us); which models share the S9's format is checked during testing. Claude Code may study existing open-source raw decoders to learn how newer Lumix files work. Studio is GPL (see License and publishing), so borrowing from GPL-licensed decoders is allowed. Anything borrowed is rewritten in Rust to keep ARM64 builds simple.

**Fallback until it ships:** convert S9 files to DNG first.

## Feature: Lumix Real Time LUT on RAW

A RAW shot with a Real Time LUT opens in Studio with that same LUT already applied, as a normal editable step. The camera only bakes the LUT into the JPEG; the RAW stays untouched, so Studio applies it itself.

1. **LUT library:** import the same .cube and .vlt files you load on the camera (from LUMIX Lab or your PC). LightCraft already reads .cube; .vlt support is new.
2. **Detect the LUT:** read the RW2 metadata for the LUT name or slot and match it to the library on import. If the camera doesn't record it, offer "apply this LUT to the whole import" instead.
3. **Same starting point:** recreate the input the LUT expects (the camera's base Photo Style, or V-Log) before applying it, so colours match the camera's JPEG.
4. **Fully editable:** the LUT shows as a profile with an Amount slider, can be swapped or removed, and works with the film effects and masks like any other step.

**Done means:** for your RAW+JPEG pairs, Studio's render of the RAW with the detected LUT closely matches the camera's JPEG.

**First Claude Code task:** "Inspect these S9 RAW+JPEG pairs shot with different Real Time LUTs. Report whether the RW2 metadata records which LUT and base Photo Style were used."

## Feature: film effects (NP DCTL port)

Port Nikita Petrovich's four open-source DaVinci Resolve DCTLs into Studio's Film persona as GPU effects. The [repo](https://github.com/nikita-petrovich/np-public-dctl) is MIT-licensed, so porting is allowed as long as the license and credit are kept.

| Effect | What it does | Where in Studio |
| --- | --- | --- |
| Lazy Bleach | Bleach-bypass look: blends the frame's average colour back over the image | Film persona |
| Lazy Grain | Grain from random lift, gamma, gain and saturation per pixel | Film persona, as a second style next to LightCraft's grain |
| Lazy Blur | Per-pixel blur in random directions for filmic softness | Film persona |
| Saturation RGB | RGB saturation with 7 modes that don't crush shadows | Film persona |

- **How:** rewrite each DCTL (C-like GPU code) as a wgpu compute shader plus a matching CPU version, the same way LightCraft's develop steps work, so CPU and GPU match.
- **Credit:** add the MIT license text and author to `NOTICE` and the credits screen.
- **Watch out:** the author warns Lazy Bleach is very heavy at 4K and above. Run it on a smaller preview while dragging sliders, and at full size only on release and export.
- **Grain randomness:** seed it from the photo, so the grain looks the same every time you reopen it.
- **Not included:** his paid DCTLs on aescripts. Only the four in the public repo.

## Feature: halation

> **Superseded by [`PRD-halation.md`](PRD-halation.md)** (physically based, spektrafilm-based design). Where this section disagrees with it, `PRD-halation.md` wins.

Add a film halation effect: the warm red-orange glow film gets around bright highlights. It is written from scratch, inspired by the approach of [hotgluebanjo/halation-dctl](https://github.com/hotgluebanjo/halation-dctl), whose code can't be copied because the repo has no license.

- **Approach (from that README's description):** exponential falloff instead of a Gaussian or box blur, no edge detection or keying, and no heavy tinting of the highlights themselves.
- **Input:** scene-linear values. LightCraft's pipeline is already linear internally, so halation runs before the tone curve and output transform.
- **Controls:** Amount, Size (radius, relative to image size so previews and exports match), and Tint (default warm red).
- **Where:** the Film persona, at the top of its panel.
- **Speed:** a big radius is slow, so render a small draft while dragging and full quality on release and export.
- **Optional:** open an issue on that repo asking the author to add a license. If they add a permissive one, we can port it directly.

## Film effects: full integration

The film effects (halation, blur, bleach, Saturation RGB, grain) are part of the develop pipeline, not plug-ins bolted on top. They behave exactly like LightCraft's own sliders everywhere in the app. You add and tune them in the Film persona on the top bar, which sits between Develop and Pixel.

**Fixed pipeline order**, so effects stack the way film does:

> **Superseded by [`PRD-halation.md`](PRD-halation.md) › "Where it sits in the pipeline"** for every film effect: chromatic-aberration correction, highlight boost and Lazy Blur now come before halation. The list below is kept for history.

1. Raw decode, white balance, exposure (scene-linear)
2. Halation (needs scene-linear light)
3. Lazy Blur (film and lens softness)
4. Profile, tone curve, colour grading, Saturation RGB
5. Lazy Bleach (works on the graded image)
6. Grain (last, so it isn't blurred or smeared by later steps)
7. Output transform and export

**Every effect must:**

- Be non-destructive and saved in the document, XMP sidecar and presets.
- Work in masks, so you can add halation or grain to one area only.
- Show in History, undo in one step per slider drag, and copy, paste and sync across photos.
- Exist as commands, so Claude can drive it over MCP.
- Look the same in every persona that shows it, because all use the same code.
- Scale with image size, so a preview, your screen and a full export match.
- Run on GPU and CPU with matching results (within 1/255, LightCraft's own standard).
- Work on Windows and Linux, ARM64 and x64.

**Film stock builder:** instead of a few fixed looks, the Film persona lets you build any film stock from adjustable parts and save it as a stock you can reuse and share.

| Part of a stock | What you adjust |
| --- | --- |
| Tone response | Contrast curve, toe and shoulder (how shadows and highlights roll off), black and white point |
| Colour response | How each colour channel bleeds into the others, hue shifts per colour, saturation model (including Saturation RGB modes) |
| Halation | Strength, size, colour, which highlights trigger it |
| Grain | Amount, size, roughness, more grain in shadows or highlights, colour vs. mono grain |
| Softness | Lazy Blur amount and size |
| Bleach | Bleach-bypass amount |
| Print / LUT stage | Optional .cube or .vlt LUT on top, with an Amount slider |

- **Stocks are files:** each one is a small readable file you can save, duplicate, rename, import and share, plus an overall Amount slider (0 to 200%).
- **Start from a template:** ship a few generic starting stocks (for example warm colour negative, cool slide film, black-and-white) with descriptive names, not brand names.
- **Match a reference (later):** load a film scan or photo you like and have Studio suggest stock settings that get close, which you then fine-tune.
- **Works everywhere:** a stock applies like a preset, with masks, copy-paste, sync across photos, and MCP commands.

**Done means (automated tests):**

- Preview and export match pixel for pixel at the same size.
- GPU and CPU match within 1/255 for every effect.
- Same photo and settings give the same grain every time.
- Slider drags stay smooth (target: under 50 ms per draft update on a 24 MP S9 file on your ARM laptop).
- Every effect survives save, reopen, preset and copy-paste with identical output.

## Architecture

Studio is a Rust workspace that forks the storytold repos and adds three new layers on top: the shell, a shared command bus and the shared document.

Layers, top to bottom (NEW = written for Studio, REUSED = from storytold):

```
[NEW]    Studio shell: Affinity-style top bar, docking panels, dark theme
            │
[REUSED] Personas: Library (LightCraft) · Develop (LightCraft) · Film (new) ·
         Pixel (PhotoCraft) · Vector (VectorCraft) · Layout (DesignCraft)
            │
[NEW]    Command bus: every action is one command   ◄── MCP + CLI
            │
[NEW]    Shared document: layer tree, develop settings, one undo history,
         one save format
            │
[REUSED] Engines: Develop (LightCraft) · Pixel (PhotoCraft) ·
         Vector (VectorCraft) · Layout (DesignCraft)
            │
[REUSED] Shared Rust crates: color, raster, codecs, RAW decoders,
         wgpu GPU pipeline
```

The shared document is the hard, important part: it is what makes Pixel edits and Develop edits live in one file instead of two apps.

## Staying in sync with storytold

Studio must pull in storytold's updates in minutes, not days. The rule: **add around their code, don't edit inside it.**

- **Their code stays untouched.** LightCraft, PhotoCraft, VectorCraft and DesignCraft come in as Git dependencies pinned to a commit, not copied files. Updating is changing one commit hash.
- **Pinned commits** (in the workspace `Cargo.toml`): LightCraft `eeb136b` (Oct 8, 2026; milestone 2), PhotoCraft `d836e34` (Oct 9, 2026; milestone 3).
- **Studio's code lives in its own crates:** `studio-shell`, `studio-doc`, `studio-film`, `studio-raw-rw2`. They use storytold's public APIs and command registry instead of changing their files.
- **Plug in, don't patch.** The RW2 decoder registers as an extra raw decoder, and film effects register as extra pipeline steps. If storytold has no hook for that yet, add the smallest possible hook in their code. A generic hook like that can be offered back to storytold as a pull request, so the patch disappears once they merge it.
- **Unavoidable patches** live on one branch of the fork, each listed in `PATCHES.md` with why it exists. Keep this list as short as possible.
- **Weekly update check (automated):** a CI job pulls storytold's latest main into a test branch, builds on all four platforms, runs every test, and opens a pull request. Green means click merge; red shows exactly which patch or API broke.
- **Golden-image tests** catch look changes: a set of your S9 photos with fixed settings is rendered and compared to saved results, so an upstream colour-science change can't silently alter your edits.
- **Read their CHANGELOG and ROADMAP** before each merge, and drop Studio code that storytold now does itself (for example if they ship their own newer-RW2 decoder).

## License and publishing

Studio is licensed **GPL-3.0-or-later** and will be published on GitHub once it's polished.

- **Why GPL:** it lets the RW2 decoder borrow from GPL-licensed open-source decoders. Storytold's MIT/Apache code and the NP DCTLs (MIT) can be included in a GPL-3.0 app.
- **Credits:** keep storytold's and Nikita Petrovich's license texts in `NOTICE` and on the credits screen.
- **Halation** follows `PRD-halation.md` and ports code from spektrafilm (GPL-3.0, compatible with Studio's GPL). Its licence notice and credit (Andrea Volpato) go in `NOTICE` and the credits screen when the code is ported.
- **Giving back:** Studio's GPL code can't go into storytold's MIT/Apache repos. Small hooks written fresh for upstream (no borrowed code) can still be offered as pull requests.

## Milestones for Claude Code

Each milestone ends with something you can open and click. Give Claude Code one milestone at a time.

1. **Shell:** a new Rust app with the top bar, empty persona areas, dark Adobe-grey theme and the panel docking system. Switching personas works.
2. **Library + Develop:** embed LightCraft's library and develop views as two personas. Match Lightroom Classic panel positions and shortcuts (G, D, E, R, \\).
3. **Shared document:** define the one document model (layer tree + develop settings) and save format. Develop edits become a non-destructive base layer.
4. **Pixel persona:** bring in PhotoCraft tools with Photoshop's layout: tool bar left, options bar on top, Layers and Properties on the right.
5. **Round trip:** Library to Develop to Pixel and back on one document, with one undo history across personas.
6. **MCP:** one MCP server that exposes commands from every persona.
7. **ARM64 + Linux builds:** CI builds and runs tests on Windows ARM64, Windows x64, Linux ARM64 and Linux x64; downloadable builds for each (.exe and a Linux AppImage or .deb). Check this from milestone 1 onward, not only at the end.

**Side tracks (start any time, neither needs the shared document):**

- **RW2 decoding:** in a LightCraft fork. Good first Claude Code task: "Open these S9 RW2 files, report exactly why LightCraft marks them preview only, and propose a decoding plan."
- **Real Time LUT:** run the first Claude Code task in that section using your RAW+JPEG pairs.
- **Film effects:** port Saturation RGB first (simplest), then Grain, Blur, Bleach, and Halation (see `PRD-halation.md`), then the film stock builder. Each one ships as its own small change.

**Recommended models (Claude Code):**

| Task | Model | Effort |
| --- | --- | --- |
| Shell, personas, UI, film effects | Opus 5.5 | medium |
| Shared document model (milestone 3) | Fable 5.1 | high |
| RW2 decoder and Real Time LUT investigation | Fable 5.1 | high |
| Bug fixes in existing code | Opus 5.5 | high |
| Small tweaks | Sonnet 5.5 | low |

**Prompt to start Claude Code:** "Read PRD.md and the storytold lightcraft and photocraft repos. Plan milestone 1 only, show me the plan, then build it."

## Risks and open questions

- **Different engines:** LightCraft and PhotoCraft may not share a document or layer model today. Milestone 3 could mean real rewrites, not just glue.
- **Young repos:** the storytold apps change fast. Pin versions and update weekly through the automated check in Staying in sync with storytold.
- **UI framework:** LightCraft uses egui. Getting a Photoshop-grade docking layout in egui may need a docking crate (like egui_dock) or custom work.
- **Clean-room rules:** copy Adobe's layout and shortcuts, never its icons, fonts or presets.
- **Scope creep:** Vector and Layout are each as big as v1. Keep them out until the round trip works.

**Open questions**

- [ ] Final app name? ("Studio" for now.)
- [x] Pixel opening images: both. Open a PNG or PSD straight in Pixel, or send a photo from Library.
- [x] Contribute back or personal fork: personal fork.
- [x] License: GPL-3.0-or-later.
