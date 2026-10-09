# RW2 side track: investigation (S9, LightCraft eeb136b)

*Investigation only, 2026-10-08. No decoder code was written and LightCraft was not modified.
Everything below was measured on this machine (Windows 11 ARM64) with the scratchpad probe in the appendix.*

## Summary

**The PRD's premise is stale: LightCraft at the pinned commit `eeb136b` already decodes every S9 RW2 file in
`samples/rw2/` from the real sensor data, bit-identical to Adobe DNG Converter's output, and Studio's import path
does not mark them preview-only.** Panasonic's newest encoding ("raw format 8", the GH6-and-later strip format
the S9 writes) landed upstream in LightCraft on 2026-10-07 (commits `6bd64e4`, `1ba0421`, `fc4b054`), one day before
Studio pinned `eeb136b`, which contains them. PRD.md's "newer-format RW2 decoding" section was written against the
older state.

Consequences, in order of what to do about them:

1. **No `studio-raw-rw2` crate is needed for the S9.** Building one would duplicate 1,300 lines of working, tested
   upstream code. The side track shrinks to (a) a regression test that keeps it working across LightCraft updates
   and (b) colour, which is the real gap (section 5).
2. **Colour is where the S9 falls short of "like a DNG".** RW2 files carry no colour matrix. LightCraft renders them
   with a per-file look fitted to the camera's embedded JPEG, with *relative* white balance (no Kelvin). A DNG of
   the same file carries Adobe's two calibrated matrices and absolute white balance. That is the PRD's "highlights,
   shadows and white balance recover like a DNG" test, and it is a per-camera-table problem, not a decoding problem.
3. **If a future Lumix body is preview-only**, LightCraft has no decoder registry, but the engine already has a
   public file-level hook (`Session.media.file_loader` / `file_probe`) that Studio can wrap without patching;
   the smallest upstream hook for a true decoder fallback is about ten lines (section 3).

Proposed PRD change (not applied; PRD.md is untouched): see section 7.

## 1. The five sample files

All five are `DC-S9` files from the same body (serial from the DNG `WG4JB006818`), identical in format:

| Field | Value (all five) | Source |
| --- | --- | --- |
| Make / Model | `Panasonic` / `DC-S9` | IFD0 `0x010f` / `0x0110` |
| PanasonicRawVersion (`0x0001`) | `"0500"` | IFD0 (ExifTool's name; LightCraft doesn't read it) |
| Raw format (`0x002d`) | **8** (the GH6-and-later strip format) | IFD0 |
| Compression (`0x000b`) | *absent* (format-8 files carry no compression tag) | IFD0 |
| Bits per sample (`0x000a`) | **12** | IFD0; the S9 has no 14-bit mode (electronic shutter only, see section 6) |
| Sensor (`0x0002`/`0x0003`) | 6016 × 4016 | IFD0 |
| Borders (`0x0004`–`0x0007`) | top 8, left 8, bottom 4008, right 8 → active area 6000 × 4000 at (8, 8) | IFD0 |
| CFA (`0x0009`) | 1 = RGGB | IFD0 |
| Black (`0x001c`–`0x001e`) | 128, 128, 128 | IFD0 |
| Linearity limits (`0x000e`–`0x0010`) | 4095 | IFD0 (LightCraft ignores them by design) |
| Strips (`0x0044`–`0x0048`) | 2 vertical strips, full height 4016; widths 3136 + 2880 (files 035/116/121) or 3168 + 2848 (1760/1797) | IFD0 |
| Format-8 fixed tags (`0x0039`–`0x003f`, `0x0041`, `0x0043`) | equal to LightCraft's `FORMAT8_FIXED` corpus values (no `Unsupported` guard trips) | IFD0 |
| Code table (`0x0040`) | 17 (length, code) pairs, longest 12 bits | IFD0 |
| Embedded JPEGs | `0x002e` JpgFromRaw 1920 px (300–480 KB) **and** `0x0127` a full 6000 × 4000 JPEG (2.0–4.6 MB), which is what `embedded_preview` returns | IFD0 |
| Distortion (`0x0119`) | present; LightCraft reads it as a `WarpRectilinear` opcode (`opcodes.list3` = 1) | IFD0 |
| Colour matrix | **none in the file** | — |

Per file:

| File | ISO (`0x0037` = Exif) | Exposure | Lens | Orientation | Default crop (`0x002f`–`0x0032`) | Sample range (12-bit) | White level LightCraft measured |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1000035.RW2 | 640 | 1/800 s f/8 | LUMIX S 18-40/F4.5-6.3 | Rotate270 | **4272 × 2848 at (864, 576)** (in-camera crop) | 124–4095, mean 195 | 4095 |
| P1000116.RW2 | 100 | 1/30 s f/8 | 18-40 | Rotate90 | full 6000 × 4000 | 126–4095, mean 643 | 4095 |
| P1000121.RW2 | 100 | 1/1000 s f/7.1 | 18-40 | Normal | full | 140–4095, mean 606 | 4095 |
| P1001760.RW2 | 640 | 1/30 s f/10 | LUMIX S 24-60/F2.8 | Normal | full | 117–1476, mean 389 (nothing clipped) | 4095 |
| P1001797.RW2 | 800 | 1/30 s f/11 | 24-60 | Normal | full | 127–4095, mean 2119 (heavily clipped) | **4080** (saturation plateau) |

Timing on this machine: `probe_info` 1–5 ms, full `decode` 140–270 ms per file (24.2 M samples, CPU).

**Embedded preview vs the camera JPEGs.** LightCraft's `embedded_preview` returns the 6000 × 4000 JPEG from tag
`0x0127` for every file. Compared with the two camera JPEGs (48 × 48 greyscale thumbnails, mean absolute
difference on a 0–255 scale):

| Pair | Camera JPEG | Embedded preview | Same image? |
| --- | --- | --- | --- |
| P1000121 | 6000 × 4000, 11.5 MB | 6000 × 4000, 4.2 MB | yes: diff 0.04 / 255 (not byte-identical; different encode) |
| P1000035 | **4272 × 2848**, 6.2 MB | 6000 × 4000, 4.6 MB | yes, after cropping the preview to the file's default crop: diff 0.30 / 255 (25.8 uncropped) |

So the embedded preview always shows the whole sensor, while the camera JPEG shows the in-camera crop. LightCraft's
RW2 reader takes that crop (tags `0x002f`–`0x0032`) as the default crop, which matches what Adobe does (its DNG stores
it as `DefaultUserCrop`, tag `0xc7b5`, and keeps `DefaultCropSize` 6000 × 4000).

## 2. Why "preview only" appears, and whether it applies here

**It does not apply to these files on `eeb136b`.** Both LightCraft entry points succeed:

- `lightcraft_raw::probe_info` and `lightcraft_raw::decode` return `Ok` for all five (section 1).
- The engine's import probe, `lightcraft_engine::files::probe_bytes` (`crates/engine/src/files.rs:97`), returns
  `preview_only: None` for all five. This is the function `Session::with_fs()` installs as `media.file_probe`
  (`files.rs:401-405`), and Studio uses exactly that (`apps/studio/src/main.rs:71`, `crates/studio-doc/src/base.rs:320`,
  `open.rs:118`). The engine's loader, `files::load_bytes` (`files.rs:204`), decodes them as raw (`SourceInfo.raw = true`).

**Where the decision is made, for the record** (file:line in LightCraft `eeb136b`):

1. `crates/raw/src/vendor/rw2.rs:204-216` `encoding()`: the only places an RW2 is refused. Format 8 is accepted for
   12–16 bits (line 212). `RawError::Unsupported` is returned for raw formats 1 and 3, compression 34826, bit depths
   outside the table, format-8 files whose fixed tags differ from the corpus (`rw2.rs:370-376`, `FORMAT8_FIXED`), and
   prefix codes longer than 12 bits (`rw2.rs:413`). None of these trigger on the S9 files.
2. `crates/raw/src/lib.rs:179-191` `decode_with()`: dispatches `RawFormat::Rw2` to that vendor module.
3. `crates/engine/src/files.rs:102-119` `probe_bytes()`: on `Err(RawError::Unsupported(why))` it describes the file
   from its embedded preview and sets `ProbeInfo.preview_only = Some(why)`; `files.rs:216-221` `load_bytes_now()` does
   the same for pixels (renders the embedded JPEG).
4. `crates/engine/src/import.rs:526`, `:941`, `:1060`: the import copies `preview_only` into the catalog photo
   (`crates/catalog/src/model.rs:279`, `Photo::preview_only`, persisted in `catalog.snap`).
5. UI: the grid badge `crates/ui-egui/src/panels/grid.rs:612-623`, the loupe pill `panels/detail.rs:468` and
   `:692-703` ("Preview only — editing the camera's embedded JPEG (…)"), the Edit-panel notice `panels/edit.rs:86-118`,
   and `previewOnly` in `catalog.query` (`crates/engine/src/cmd/query.rs:30`).

**The one way these files could still show the badge in Studio: a stale catalog.** `preview_only` is written at
import time and stored. A photo imported with a LightCraft build older than 2026-10-07 (any pre-format-8 commit)
would carry `"Panasonic raw format 8 with 12-bit samples"` in the catalog until the file is re-probed. Only two
commands re-probe: `photo.reload` (`crates/engine/src/cmd/convert.rs:215-243`, via `content_op()` at `:173`, which
compares the fresh `preview_only` with the stored one) and `photo.relink` (`cmd/missing.rs:72-77`). Nothing refreshes
it automatically on open. Studio has never pinned anything older than `eeb136b` (its first pin, commit `0c06142`), and
the Studio Library on this machine (`Pictures\Studio Library\catalog.snap`, 133 bytes) is empty, so this case cannot
have happened here; it can happen to a LightCraft user's existing library, and the fix is Reload. There is also a
thumbnail cache (`RENDER_CACHE_VERSION`, bumped by `fc4b054`) that upstream already invalidated for format 8.

**Which case we are in:** case "LightCraft decodes fully and Studio shows no badge". Studio has no code of its own on
this path (no `preview_only`, `file_loader` or `probe_bytes` references in `crates/` or `apps/`); it inherits the
engine's behaviour unchanged.

## 3. Hooks: is there a public way to add a raw decoder?

**In `lightcraft-raw`: no.** `decode_with()` is private and matches on a closed `RawFormat` enum; `mod vendor` is
private; `probe()` already claims `IIU\0` files as `Rw2`. There is no trait, registry or callback, and the
`Unsupported` path goes straight to the embedded preview.

**In `lightcraft-engine`: yes, at the file level.** `Session.media` (`crates/engine/src/media.rs:200-202`) has three
public `Option<Arc<dyn Fn …>>` fields:

```rust
pub type FileLoader  = Arc<dyn Fn(&str, usize) -> Result<(Rgb32f, SourceInfo), String> + Send + Sync>; // media.rs:108
pub type FileProbe   = Arc<dyn Fn(&str) -> Result<ProbeInfo, String> + Send + Sync>;                   // media.rs:1011
pub type PreviewLoader = Arc<dyn Fn(&str, usize) -> Option<Rgba8> + Send + Sync>;                       // media.rs:112
```

`Session::with_fs()` fills them with the filesystem defaults (`files.rs:401-407`); `lightcraft-web` installs its own
(`apps/lightcraft-web/src/store.rs:162`). Studio can wrap them after `with_fs()`:
try `studio_raw::decode(bytes)` first, else fall through to `lightcraft_engine::files::probe_bytes` / `load_bytes`
(both `pub`). This covers import, thumbnails, previews and the develop view without touching LightCraft. It does
**not** cover the four places that call `lightcraft_raw::decode` directly: export at full size
(`crates/engine/src/export.rs:1402`), Convert to DNG (`cmd/convert.rs:38`), Photo Merge (`crates/merge/src/frame.rs:112`)
and camera-profile pooling (`camera_profiles.rs:206`). Those would still see the embedded preview for an unsupported
file.

**Smallest upstream hook** (only if a body ever needs it; not needed for the S9): a process-wide fallback in
`lightcraft-raw`, consulted only when the built-in decoder says `Unsupported`:

```rust
// crates/raw/src/lib.rs
pub type FallbackDecoder = fn(&[u8], /* header_only: */ bool) -> Option<Result<RawImage>>;
static FALLBACK: std::sync::OnceLock<FallbackDecoder> = std::sync::OnceLock::new();
/// Register a decoder tried for files the built-in ones report as unsupported. First call wins.
pub fn set_fallback_decoder(f: FallbackDecoder) -> bool { FALLBACK.set(f).is_ok() }

fn decode_with(bytes: &[u8], mode: Mode) -> Result<RawImage> {
    let r = match probe(bytes).ok_or(RawError::NotRaw)? { /* unchanged */ };
    match (r, FALLBACK.get()) {
        (Err(RawError::Unsupported(_)), Some(f)) => f(bytes, mode == Mode::Header).unwrap_or(r),
        (r, _) => r,
    }
}
```

About 10 lines, one file, no new public types beyond a `fn` pointer, and it reaches every caller including export,
convert and merge. It is generic (any format), carries no Studio code, and could be offered upstream as a small PR
per PRD "Plug in, don't patch". The engine-level wrap above is the no-patch alternative and is what Studio should use
first if the need arises.

## 4. Open-source decoders that handle newer Lumix RW2

| Project | Format-8 support | Licence | Notes for Studio (GPL-3.0-or-later) |
| --- | --- | --- | --- |
| **LightCraft** `crates/raw/src/vendor/rw2.rs` (1,312 lines) | yes: formats 2/4/5/6/7/8 and the 2005–2007 16-bit files, verified on 178 CC0 files from 118 bodies | MIT / Apache-2.0 | already ours. Clean-room: its header states no decoder source was read, only ExifTool tag names and black-box measurement. Bit-identical to Adobe on the S9 (section 5). |
| **LibRaw** `src/decoders/pana8.cpp` | yes (first implementation, 2022, Alex Tutubalin; S5 II since March 2023) | LGPL-2.1 **or** CDDL-1.0 | GPL-compatible under LGPL-2.1. Reads tags `0x39`, `0x3a`, `0x3b`, `0x40`, `0x41`; 4-coefficient predictor per 4-column group, 2×2 output blocks, optional gamma table from the tags. |
| **rawspeed** (darktable, RawTherapee) `PanasonicV8Decompressor.cpp` (472 lines) | yes: `Rw2Decoder.cpp` dispatches versions 4–8, V8 for 12/14/16 bits | LGPL-2.1-or-later; header credits LibRaw LLC 2022–2024, Daniel Vogelbacher 2024, Kolton Yager 2025 (a port of LibRaw's) | `data/cameras.xml` has `DC-S9` (black 128, white 4095, one D65 matrix) and the GH6, GH7, G9M2, S5M2, S5M2X, S1M2, S1RM2. The matrices are Adobe's `ColorMatrix2` values. |
| **rawler** (dnglab, Rust) `rawler/src/decoders/rw2/v8decompressor.rs` (495 lines) | yes (`decode_panasonic_v8`); `SUPPORTED_CAMERAS.md` lists DC-S9 ✅ 3:2, GH6, GH7, G9M2, S5M2, S5M2X, S1RM2 | LGPL-2.1; SPDX header "Originally written by LibRaw LLC" | the only Rust one, so the closest thing to a drop-in. Its `data/cameras/panasonic/s9.toml` holds both Adobe matrices (`A` and `D65`), whitepoint 4095, RGGB. Pulling it in would add a second TIFF stack and a second raw crate to the ARM64 build for no gain while LightCraft decodes. |
| **libopenraw** (Hubert Figuière) | S9 since 0.4.0-alpha (2024, "Panasonic S9, G9 II"), "Colour matrices for S9" in a later alpha; GH7, S1RII, S1II in 2025 alphas; Panasonic decompression contributed by DorotaC | LGPL-3.0-or-later | Rust + C; alpha. |
| **dcraw** | no (unmaintained since 2018, before format 8) | GPL-2.0 / "dcraw licence" | irrelevant. |

Every open implementation of format 8 descends from LibRaw's 2022 work; LightCraft's is the one independent
(clean-room) one. All measured facts above are the same across them (two strips, 17-entry prefix table, 2×2 cell
order), which is why LightCraft matches Adobe bit for bit.

Licences for Studio: everything in the table is usable in a GPL-3.0 program (MIT, Apache-2.0, LGPL-2.1, LGPL-3.0).
The per-camera matrices in rawspeed's and rawler's tables are copied from Adobe's DNG Converter; the projects ship them
under their own licences, and so does LightCraft's upstream policy *refuse* to ("never Adobe data", `docs/parity.md`).
Studio's fork can take a different line (section 5), but it should say so in `NOTICE`.

## 5. Decoding plan: what `studio-raw-rw2` should be now

### 5a. Decoding: nothing to build; one regression test to keep

Verified today with the probe (appendix), for all five files:

```
RW2 vs DNG samples (full sensor 6016x4016): identical 24160256 of 24160256 (100.0000%), max |diff| 0
```

LightCraft's RW2 decode and its decode of Adobe DNG Converter 18.3's lossless DNG of the same file give the same
16-bit sample at every one of the 24.2 million sensor sites, with the same CFA (RGGB), black (128 per channel) and
as-shot white balance (DNG `AsShotNeutral` reciprocals equal the RW2 `0x0024`–`0x0026` multipliers to 6 digits).
Geometry matches too: active area 6000 × 4000 at (8, 8) (Adobe stores it as `DefaultCropOrigin`/`Size` on a 6016 × 4016
`ActiveArea`); the in-camera crop of P1000035 is LightCraft's default crop and Adobe's `DefaultUserCrop`.

Differences that are not bugs: Adobe writes `WhiteLevel` 4095 always, LightCraft measures the saturation plateau
(4080 on the clipped P1001797), which maps every saturated sample to ≥ 1.0 by design; Adobe adds `BaselineExposure`
−0.05 EV and a noise profile from its own calibration.

So the "decoder" part of the PRD feature is done upstream. What Studio keeps is a **golden-decode test** so a future
LightCraft bump cannot silently regress it (PRD "Golden-image tests"):

- Fixture: one S9 RW2 plus its DNG, both under 25 MB (`samples/` is gitignored; a fixture pair goes in
  `crates/studio-raw-rw2/tests/fixtures/` or, better, Git LFS once the repo is public; until then an `#[ignore]` test
  pointing at `samples/rw2/` like LightCraft's own corpus test).
- Assertion: `lightcraft_raw::decode(rw2).data == lightcraft_raw::decode(dng).data`, equal `cfa`, `black.values`,
  active area and crop, `wb_multipliers` within 1e-5 of `1 / AsShotNeutral`. Bit-exact, no tolerance.
- Second assertion: `lightcraft_engine::files::probe_bytes(..).preview_only.is_none()` (the user-visible symptom).
- Cost: ~0.5 s per pair on this machine; use one pair, not five, in CI.

### 5b. Colour: the actual gap, and the per-camera table

What LightCraft does today with an RW2 (no matrix in the file):

- `lightcraft_raw::color::xyz_to_camera` falls back to "camera RGB ≈ linear sRGB" (`color.rs:84-90`).
- Because the format is in `camera_preview::file_local_look` (`crates/engine/src/camera_preview.rs:32`), the loader
  fits a per-file matrix + tone curve + hue/sat table to the file's embedded JPEG (`fit_preview`, `camera_preview.rs:37`;
  `docs/camera-preview-colour.md`), gated by goodness-of-fit. On all five S9 files the fit was **accepted**
  (`SourceInfo.camera_tone = Some`), with `relative_wb = true` and as-shot reported as the neutral reference 6500 K / 0.
- Upstream policy: no measured camera calibration database, and never Adobe's matrices.

What a DNG of the same file gets instead: `ColorMatrix1`/`2` (illuminants 17 = Standard A and 21 = D65),
`ForwardMatrix1`/`2`, a 90 × 30 `ProfileHueSatMap` and a 36 × 8 × 16 `ProfileLookTable` ("Adobe Standard"), absolute
Kelvin white balance, `BaselineExposure` −0.05, noise profile and a `DefaultUserCrop`. That is why "like a DNG"
means colour, not decoding.

**Proposal: `studio-raw-rw2` becomes `studio-camera` (or stays named, but holds a table, not a decoder):**

| Column | S9 entry | Source |
| --- | --- | --- |
| `model` | `DC-S9` (`UniqueCameraModel` `Panasonic DC-S9`) | file |
| `color_matrix_1`, illuminant 17 (A) | `[1.2599 -0.7524 0.0720; -0.4116 1.1989 0.2399; -0.0184 0.0760 0.6980]` | Adobe DNG Converter 18.3 output of our own file (same values in rawler `s9.toml` `A`) |
| `color_matrix_2`, illuminant 21 (D65) | `[0.9983 -0.3890 -0.0841; -0.4180 1.2164 0.2263; -0.0249 0.1139 0.5766]` | same (rawspeed `cameras.xml`, rawler `D65`) |
| `forward_matrix_1` / `_2` | `[0.4409 0.4176 0.1058; 0.2049 0.7639 0.0311; 0.0759 0.0003 0.7489]` / `[0.4594 0.3471 0.1578; 0.2504 0.7023 0.0473; 0.1023 0.0032 0.7195]` | same |
| `black`, `white` | 128 (all four CFA sites), 4095 (12-bit) | file tags; equals Adobe |
| `sensor`, `active`, `crop` | 6016 × 4016; (8, 8, 6000, 4000); in-camera crop from `0x002f`–`0x0032` | file; LightCraft already reads these |
| `baseline_exposure` | −0.05 EV | Adobe |
| `black_offset` | none for format 8 (formats 4/6 add 15; `rw2.rs` header) | LightCraft |

Rather than a new decoder, the crate supplies **`ColorData` for a `(make, model)`** and Studio injects it where
LightCraft would otherwise fall back: the engine `file_loader` wrap from section 3 can call `lightcraft_raw::decode`,
then set `raw.color.color_matrix`, `forward_matrix`, `illuminant`, `baseline_exposure` from the table before handing
the `RawImage` to the same colour code the DNG path uses (`files::load_bytes` is monolithic today, so the wrap would
re-run its body with the patched `RawImage`; the smallest upstream hook for *this* is a `pub fn load_raw(raw: RawImage,
bytes: &[u8], max_edge) -> …` split out of `load_bytes_now`, about a 5-line refactor). With a matrix present,
`has_matrix` is true, `fit_preview` returns `None`, white balance becomes absolute Kelvin, and the S9 renders through
the identical path as its DNG. Adobe's hue/sat and look tables are a second step (the DNG's `ProfileHueSatMap` can be
read from a converted DNG once and stored; 8,100 + 13,824 floats per camera).

Licence note for the table: the matrices are Adobe's calibration published in every DNG Converter output and
redistributed by rawspeed (LGPL) and rawler (LGPL) and libopenraw (LGPL). Studio's PRD explicitly allows borrowing
under GPL; the table should credit the source in `NOTICE` and say the numbers came from converting the author's own
files. LightCraft upstream would not accept this table (its policy), which is fine: the table is Studio-only, and
the engine hook stays generic.

### 5c. Tests against DNG references

1. **Decode parity (5a):** bit-exact sample equality RW2 vs DNG, per fixture pair. Catches a decoder regression
   upstream or a wrong strip/table reading for a new firmware.
2. **Metadata parity:** CFA, black, white (≤ 4095), active area, default crop vs `DefaultUserCrop`, WB multipliers
   vs `1/AsShotNeutral` (1e-5), distortion opcode present when `0x0119` word 7 says "on".
3. **Colour parity (after 5b):** render the RW2 through Studio (table injected) and the DNG through LightCraft with
   identical develop settings at 600 px (`Base.max_edge` style, so CI stays fast), compare mean CIE76 ΔE and mean L*
   on a 200 px proxy, as LightCraft's own `camera-preview-colour.md` does. Target: ΔE < 1 and |ΔL*| < 0.2 with the
   matrices alone (same maths, same numbers, so it should be near zero; anything larger is a bug in the injection).
   Golden PNG of the RW2 render stored for the weekly upstream-merge CI.
4. **"Like a DNG" on real edits:** the PRD's wording (highlights, shadows, WB). Scripted: +2 EV on P1001760 (dark,
   unclipped), −2 EV + Highlights −100 on P1001797 (clipped), WB 3200 K on P1000121; assert RW2 and DNG renders agree
   within the ΔE above. That is also the test that proves WB went from relative to absolute.
5. **Other bodies:** one CC0 file from raw.pixls.us per format-8 body (GH6, GH7, G9M2, S5M2, S5M2X, S1M2, S1RM2) run
   through test 1 against a DNG produced by DNG Converter (its licence allows converting; the DNGs are derived from
   CC0 input). LightCraft already includes several of these in its 178-file corpus, so these are cheap.

## 6. Extra sample files to shoot or convert

The S9 has fewer knobs than the PRD assumed: it has no mechanical shutter and reads the sensor in its 12-bit mode
only (DPReview's review: "the lack of a mechanical shutter forces it to use the faster 12-bit readout mode"), and the
menu offers no raw compression or bit-depth setting. So "every raw quality setting" collapses to one. Still worth
shooting, each as RAW+JPEG (Fine), to cover what the files vary in:

| Shoot | Why | Have |
| --- | --- | --- |
| ISO ladder: 100 (base), 200, 400, 800, 1600, 3200, 6400, 12800, 25600, 51200, plus extended 50 and 102400 if enabled | per-ISO black/white and noise; format-8 fixed tags might change at extended ISOs (the one guard that could trip) | 100, 640, 800 |
| A frame with blown highlights at ISO 100 and at ISO 6400 | saturation plateau vs 4095; highlight reconstruction "like a DNG" | one at ISO 800 |
| A near-black frame (lens cap, 1/8000) at ISO 100 and 6400 | black level per CFA site; mapped-out pixels | none |
| Each aspect ratio: 3:2, 4:3, 16:9, 1:1, 65:24, 2:1 | default-crop tags; rawspeed/rawler list only 3:2 for the S9 | 3:2 + one 4272 × 2848 crop (P1000035, Ex. Tele Conv. / hybrid zoom?) |
| Ex. Tele Conv. / crop zoom on and off, and APS-C crop lens mode | the P1000035 crop: confirm which setting produced it and that the tags describe it | one |
| Each shutter mode the S9 exposes (E-shutter, silent) and burst at 30 fps | any mode that changes bit depth or strip layout | single frames only |
| High Resolution mode (if used; produces a larger RW2) | different sensor size and possibly a different strip table; a separate camera-table row | none |
| A portrait frame (orientation tag 6/8) and an upside-down one | orientation vs crop handling | Rotate90, Rotate270 |
| One frame per Real Time LUT and Photo Style, RAW+JPEG | feeds the LUT side track (same files) | 2 pairs |
| A frame with a non-Lumix / adapted lens | `0x0119` distortion data absent or zero | none |
| **Adobe DNG Converter** output for every RW2 above (installed here: 18.3, `C:\Program Files\Adobe\Adobe DNG Converter\`, command line `-c -p0 -d <dir>`; ARM64 build, ~1.6 s per file) | the reference for every test in 5c; done today for the five files into `samples/rw2/dng/` (gitignored) | 5 |
| Public CC0 samples: GH6, GH7, G9M2, S5M2, S5M2X, S1M2, S1RM2 from raw.pixls.us (+ their DNGs) | other format-8 bodies for the per-camera table | none |

Keep in the repo (fixtures): one full S9 pair (RW2 + DNG, ~40 MB; LFS or an `#[ignore]` test on `samples/`) and, for
CI speed, a 600-px-wide crop written with LightCraft's `write_dng` from a decoded RW2 is **not** a substitute (it would
no longer exercise the strip decoder); the full file is needed for test 1.

## 7. Proposed PRD change (not applied)

Replace the first paragraph of "Feature: newer-format RW2 decoding (S9 first)" with:

> LightCraft (pinned `eeb136b`, 2026-10-08) decodes Panasonic raw format 8 (GH6 and later, including the S9) from the
> sensor data; on the author's S9 files its decode is bit-identical to Adobe DNG Converter's. The remaining gap is
> colour: RW2 files carry no colour matrix, so LightCraft fits a per-file look to the embedded JPEG with relative white
> balance, while a DNG of the same file carries calibrated matrices and absolute Kelvin. **Done means:** an S9 RW2
> renders through the same colour path as its DNG (matrices from a per-camera table, absolute WB), and a golden test
> keeps RW2-vs-DNG decode bit-exact across LightCraft updates.

and rename the crate's purpose from "decoder" to "camera table + decode-parity tests". The "Fallback until it ships:
convert S9 files to DNG first" line can go.

## Appendix A: probe source (scratchpad, re-runnable)

The probe is a standalone cargo project outside the repo (it depends on LightCraft's crates at the pinned commit;
nothing in Studio changes). Recreate it in any directory with the two files below and run
`cargo run -- <dir with *.RW2>`; a `<stem>.JPG` beside a file is compared, and a `dng/<stem>.dng` beside it is decoded
and compared sample by sample. Run cargo from PowerShell on this machine (Git Bash shadows `link.exe`).

### `Cargo.toml`

```toml
[package]
name = "rw2probe"
version = "0.1.0"
edition = "2024"

[dependencies]
lightcraft-raw = { git = "https://github.com/storytold/lightcraft", rev = "eeb136b9eec179805307c4626596f0ccc074368b" }
lightcraft-tiff = { git = "https://github.com/storytold/lightcraft", rev = "eeb136b9eec179805307c4626596f0ccc074368b" }
lightcraft-engine = { git = "https://github.com/storytold/lightcraft", rev = "eeb136b9eec179805307c4626596f0ccc074368b" }
image = { version = "0.25", default-features = false, features = ["jpeg"] }

[profile.dev]
opt-level = 3
```

### `src/main.rs`

```rust
//! rw2probe: what LightCraft (eeb136b) makes of a set of RW2 files, plus the raw PanasonicRaw IFD0 tags and
//! a comparison of the embedded preview against the camera JPEG next to the file.
//!
//! Usage: rw2probe <dir>   (every *.RW2 in <dir>; a <stem>.JPG beside it is compared when present)

use image::GenericImageView;
use lightcraft_tiff::{Tiff, Value};
use std::path::Path;
use std::time::Instant;

fn value_text(v: &Value, max: usize) -> String {
    let trunc = |items: Vec<String>| {
        let n = items.len();
        let mut s = items.into_iter().take(max).collect::<Vec<_>>().join(" ");
        if n > max {
            s.push_str(&format!(" … ({n} values)"));
        }
        s
    };
    match v {
        Value::Ascii(s) => format!("{s:?}"),
        Value::Byte(b) | Value::Undefined(b) => {
            if b.len() <= 8 {
                format!("{:02x?} {:?}", b, String::from_utf8_lossy(b))
            } else {
                format!("{} bytes, first {:02x?}", b.len(), &b[..8])
            }
        }
        Value::Short(x) => trunc(x.iter().map(|v| v.to_string()).collect()),
        Value::Long(x) => trunc(x.iter().map(|v| v.to_string()).collect()),
        Value::SShort(x) => trunc(x.iter().map(|v| v.to_string()).collect()),
        Value::SLong(x) => trunc(x.iter().map(|v| v.to_string()).collect()),
        Value::Rational(x) => trunc(x.iter().map(|(a, b)| format!("{a}/{b}")).collect()),
        Value::SRational(x) => trunc(x.iter().map(|(a, b)| format!("{a}/{b}")).collect()),
        other => format!("{other:?}").chars().take(80).collect(),
    }
}

/// PanasonicRaw tag names (ExifTool's PanasonicRaw table, names only).
fn tag_name(tag: u16) -> &'static str {
    match tag {
        0x0001 => "PanasonicRawVersion",
        0x0002 => "SensorWidth",
        0x0003 => "SensorHeight",
        0x0004 => "SensorTopBorder",
        0x0005 => "SensorLeftBorder",
        0x0006 => "SensorBottomBorder",
        0x0007 => "SensorRightBorder",
        0x0008 => "SamplesPerPixel",
        0x0009 => "CFAPattern",
        0x000a => "BitsPerSample",
        0x000b => "Compression",
        0x000e => "LinearityLimitRed",
        0x000f => "LinearityLimitGreen",
        0x0010 => "LinearityLimitBlue",
        0x0011 => "RedBalance",
        0x0012 => "BlueBalance",
        0x0013 => "WBInfo",
        0x0017 => "ISO",
        0x0018 => "HighISOMultiplierRed",
        0x0019 => "HighISOMultiplierGreen",
        0x001a => "HighISOMultiplierBlue",
        0x001b => "NoiseReductionParams",
        0x001c => "BlackLevelRed",
        0x001d => "BlackLevelGreen",
        0x001e => "BlackLevelBlue",
        0x0024 => "WBRedLevel",
        0x0025 => "WBGreenLevel",
        0x0026 => "WBBlueLevel",
        0x0027 => "WBInfo2",
        0x002d => "RawFormat",
        0x002e => "JpgFromRaw",
        0x002f => "CropTop",
        0x0030 => "CropLeft",
        0x0031 => "CropBottom",
        0x0032 => "CropRight",
        0x0037 => "ISO (32-bit)",
        0x0039..=0x003f => "(format-8 fixed; LightCraft FORMAT8_FIXED)",
        0x0040 => "(format-8 code table)",
        0x0041 | 0x0043 => "(format-8 fixed; LightCraft FORMAT8_FIXED)",
        0x0044 => "(format-8 strip offsets)",
        0x0045 => "(format-8 strip left columns)",
        0x0046 => "(format-8 strip bit lengths)",
        0x0047 => "(format-8 strip widths)",
        0x0048 => "(format-8 strip heights)",
        0x010f => "Make",
        0x0110 => "Model",
        0x0111 => "StripOffsets",
        0x0112 => "Orientation",
        0x0116 => "RowsPerStrip",
        0x0117 => "StripByteCounts",
        0x0118 => "RawDataOffset",
        0x0119 => "DistortionInfo",
        0x011e => "Gamma?",
        0x0120 => "CameraIFD",
        0x0127 => "Multishot",
        0x0131 => "Software",
        0x0132 => "ModifyDate",
        0x8769 => "ExifIFD",
        0x8825 => "GPSInfo",
        0xc4a5 => "PrintIM",
        _ => "",
    }
}

fn jpeg_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok().map(|i| i.dimensions())
}

/// 48×48 greyscale thumbnail, values 0..=255, after fitting both images to the same size.
fn fingerprint(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok()?;
    let small = img.resize_exact(48, 48, image::imageops::FilterType::Triangle).to_luma8();
    Some(small.into_raw())
}

fn mad(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(&x, &y)| (x as f64 - y as f64).abs()).sum::<f64>() / a.len() as f64
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: rw2probe <dir>");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rw2")))
        .collect();
    files.sort();
    for path in files {
        probe(&path);
    }
}

fn probe(path: &Path) {
    println!("\n==================== {} ====================", path.file_name().unwrap().to_string_lossy());
    let bytes = std::fs::read(path).expect("read file");
    println!("size: {} bytes; probe(): {:?}", bytes.len(), lightcraft_raw::probe(&bytes));

    // --- raw IFD0 tags ---
    let opts = lightcraft_tiff::ParseOptions { max_ifds: 256, ..Default::default() };
    match Tiff::parse_with(&bytes, &opts) {
        Ok(t) => {
            println!("tiff: order {:?}, {} IFDs in the chain", t.order, t.ifds.len());
            if let Some(ifd0) = t.ifds.first() {
                println!("IFD0 ({} entries):", ifd0.entries.len());
                for e in &ifd0.entries {
                    let max = if (0x0039..=0x0048).contains(&e.tag) { 12 } else { 8 };
                    println!("  {:#06x} {:<42} {:?}[{}] = {}", e.tag, tag_name(e.tag), e.field_type(), e.count(), value_text(&e.value, max));
                }
                if let Some(exif) = ifd0.exif.as_deref() {
                    for tag in [0x8827u16, 0x829a, 0x829d, 0x9003, 0xa405, 0x8830, 0x8832, 0x9204] {
                        if let Some(e) = exif.get(tag) {
                            println!("  exif {:#06x} = {}", tag, value_text(&e.value, 4));
                        }
                    }
                }
            }
        }
        Err(e) => println!("tiff parse error: {e}"),
    }

    // --- LightCraft's reading ---
    let t0 = Instant::now();
    match lightcraft_raw::probe_info(&bytes) {
        Ok(info) => {
            println!(
                "probe_info ({:?}): format {:?}, {}x{} cpp {} bits {}, cfa {:?}, active {:?}, crop {:?}, orientation {:?}",
                t0.elapsed(),
                info.format,
                info.width,
                info.height,
                info.cpp,
                info.bits,
                info.cfa.as_ref().map(|c| c.pattern.clone()),
                info.active_area,
                info.crop,
                info.orientation
            );
            println!(
                "  model {:?} make {:?} iso {:?} exposure {:?} f/{:?} software {:?} lens {:?}",
                info.metadata.model, info.metadata.make, info.metadata.iso, info.metadata.exposure_time, info.metadata.f_number, info.metadata.software, info.metadata.lens_model
            );
            println!(
                "  colour: illuminant {:?} matrix1 {} matrix2 {} forward {} as_shot_neutral {:?} wb {:?} baseline_exposure {}",
                info.color.illuminant,
                info.color.color_matrix[0].is_some(),
                info.color.color_matrix[1].is_some(),
                info.color.forward_matrix[0].is_some(),
                info.color.as_shot_neutral,
                info.wb_multipliers,
                info.color.baseline_exposure
            );
            println!("  opcodes: list1 {} list2 {} list3 {}", info.opcodes.list1.len(), info.opcodes.list2.len(), info.opcodes.list3.len());
        }
        Err(e) => println!("probe_info ERROR: {e}"),
    }
    let t0 = Instant::now();
    match lightcraft_raw::decode(&bytes) {
        Ok(img) => {
            let n = img.data.len();
            let (mut mn, mut mx, mut sum, mut zeros) = (u16::MAX, 0u16, 0u64, 0usize);
            if let lightcraft_raw::RawData::U16(v) = &img.data {
                for &s in v {
                    mn = mn.min(s);
                    mx = mx.max(s);
                    sum += s as u64;
                    zeros += (s == 0) as usize;
                }
            }
            println!(
                "decode OK in {:?}: {} samples, min {} max {} mean {:.1} zeros {}, black {:?} white {:?} linearized {}",
                t0.elapsed(),
                n,
                mn,
                mx,
                sum as f64 / n.max(1) as f64,
                zeros,
                img.black.values,
                img.white,
                img.linearized
            );
        }
        Err(e) => println!("decode ERROR in {:?}: {e}", t0.elapsed()),
    }

    // --- embedded preview vs camera JPEG ---
    let preview = lightcraft_raw::embedded_preview(&bytes);
    match &preview {
        Some(p) => println!("embedded preview: {} bytes, dims {:?}, colour space {:?}", p.len(), jpeg_dims(p), lightcraft_raw::embedded_preview_color_space(&bytes)),
        None => println!("embedded preview: none"),
    }
    let jpg = path.with_extension("JPG");
    if let Ok(j) = std::fs::read(&jpg) {
        println!("camera JPEG {}: {} bytes, dims {:?}", jpg.file_name().unwrap().to_string_lossy(), j.len(), jpeg_dims(&j));
        if let (Some(p), Some(fj)) = (&preview, fingerprint(&j)) {
            let same_bytes = p.as_slice() == j.as_slice();
            // the preview shows the whole active area; the camera JPEG shows the default crop
            let crop = lightcraft_raw::probe_info(&bytes).ok().map(|i| i.crop);
            let pimg = image::load_from_memory_with_format(p, image::ImageFormat::Jpeg).ok();
            let cropped = match (pimg, crop) {
                (Some(img), Some(c)) if c.width > 1 && c.height > 1 && (c.width as u32, c.height as u32) != img.dimensions() => {
                    println!("  preview cropped to the default crop {:?} before comparing", c);
                    Some(img.crop_imm(c.x as u32, c.y as u32, c.width as u32, c.height as u32))
                }
                (img, _) => img,
            };
            if let Some(ci) = cropped {
                let fp = ci.resize_exact(48, 48, image::imageops::FilterType::Triangle).to_luma8().into_raw();
                println!("  identical bytes: {same_bytes}; 48x48 grey mean abs diff: {:.2} / 255 (same image if small)", mad(&fp, &fj));
            }
        }
    }

    // --- the same file converted by Adobe DNG Converter (dng/<stem>.dng beside it): LightCraft's two decodes compared ---
    let dng_path = path.parent().unwrap().join("dng").join(path.with_extension("dng").file_name().unwrap());
    if let Ok(dbytes) = std::fs::read(&dng_path) {
        let t0 = Instant::now();
        match (lightcraft_raw::decode(&bytes), lightcraft_raw::decode(&dbytes)) {
            (Ok(r), Ok(d)) => {
                println!(
                    "DNG {} decode OK in {:?}: {}x{} bits {} cfa {:?} active {:?} crop {:?} black {:?} white {:?}; illuminants {:?} matrix1 {} matrix2 {} forward {} as_shot_neutral {:?} baseline_exposure {} opcodes3 {}",
                    dng_path.file_name().unwrap().to_string_lossy(),
                    t0.elapsed(),
                    d.width,
                    d.height,
                    d.bits,
                    d.cfa.as_ref().map(|c| c.pattern.clone()),
                    d.active_area,
                    d.crop,
                    d.black.values,
                    d.white,
                    d.color.illuminant,
                    d.color.color_matrix[0].is_some(),
                    d.color.color_matrix[1].is_some(),
                    d.color.forward_matrix[0].is_some(),
                    d.color.as_shot_neutral,
                    d.color.baseline_exposure,
                    d.opcodes.list3.len()
                );
                if let (lightcraft_raw::RawData::U16(a), lightcraft_raw::RawData::U16(b)) = (&r.data, &d.data) {
                    if (r.width, r.height) == (d.width, d.height) {
                        let (mut eq, mut maxd, mut sumd) = (0usize, 0u32, 0u64);
                        for (&x, &y) in a.iter().zip(b) {
                            let dd = (x as i32 - y as i32).unsigned_abs();
                            eq += (dd == 0) as usize;
                            maxd = maxd.max(dd);
                            sumd += dd as u64;
                        }
                        println!(
                            "  RW2 vs DNG samples (full sensor {}x{}): identical {} of {} ({:.4}%), max |diff| {}, mean |diff| {:.5}",
                            r.width,
                            r.height,
                            eq,
                            a.len(),
                            100.0 * eq as f64 / a.len() as f64,
                            maxd,
                            sumd as f64 / a.len() as f64
                        );
                    } else {
                        println!("  RW2 vs DNG: different sizes {}x{} vs {}x{}", r.width, r.height, d.width, d.height);
                    }
                }
                if let Some(n) = d.color.as_shot_neutral {
                    println!("  DNG AsShotNeutral reciprocal {:?} vs RW2 wb_multipliers {:?}", n.map(|v| (1.0 / v) as f32), r.wb_multipliers);
                }
            }
            (Err(e), _) | (_, Err(e)) => println!("DNG comparison ERROR: {e}"),
        }
    } else {
        println!("no DNG at {}", dng_path.display());
    }

    // --- Studio's actual import + load path (lightcraft-engine, what Session::with_fs installs) ---
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    match lightcraft_engine::files::probe_bytes(&name, &bytes) {
        Ok(info) => println!(
            "engine probe_bytes: {}x{} kind {:?} preview_only {:?} as_shot_wb {:?} embedded_lens {}",
            info.width,
            info.height,
            info.kind,
            info.preview_only,
            info.as_shot_wb,
            info.embedded_lens.is_some()
        ),
        Err(e) => println!("engine probe_bytes ERROR: {e}"),
    }
    let t0 = Instant::now();
    match lightcraft_engine::files::load_bytes(&bytes, 1200) {
        Ok((img, src)) => {
            let (mut sum, n) = ([0f64; 3], (img.width * img.height) as f64);
            for px in &img.data {
                for (s, v) in sum.iter_mut().zip(px.iter()) {
                    *s += *v as f64;
                }
            }
            println!(
                "engine load_bytes(1200) OK in {:?}: {}x{}, raw {} relative_wb {} as_shot {}K/{} camera_tone(look fitted) {} lens {}; mean linear Rec.2020 RGB {:.4?}",
                t0.elapsed(),
                img.width,
                img.height,
                src.raw,
                src.relative_wb,
                src.as_shot_temp,
                src.as_shot_tint,
                src.camera_tone.is_some(),
                src.lens.is_some(),
                sum.map(|s| s / n)
            );
        }
        Err(e) => println!("engine load_bytes ERROR: {e}"),
    }
}
```

### `dngtags.py` (DNG tag dump used for section 5b; `python -I dngtags.py <dir with *.dng>`)

```python
"""Print the DNG colour and level tags of each *.dng in a directory (plain TIFF walk, no deps)."""
import struct, sys, glob, os

TYPES = {1: ("B", 1), 2: ("c", 1), 3: ("H", 2), 4: ("I", 4), 5: ("II", 8), 6: ("b", 1), 7: ("B", 1), 8: ("h", 2), 9: ("i", 4), 10: ("ii", 8), 11: ("f", 4), 12: ("d", 8), 13: ("I", 4)}
NAMES = {
    0x0100: "ImageWidth", 0x0101: "ImageLength", 0x0102: "BitsPerSample", 0x0103: "Compression", 0x0106: "Photometric",
    0x0110: "Model", 0x0131: "Software", 0x828d: "CFARepeatPatternDim", 0x828e: "CFAPattern", 0xc612: "DNGVersion",
    0xc614: "UniqueCameraModel", 0xc61a: "BlackLevel", 0xc61d: "WhiteLevel", 0xc61f: "DefaultCropOrigin", 0xc620: "DefaultCropSize",
    0xc621: "ColorMatrix1", 0xc622: "ColorMatrix2", 0xc623: "CameraCalibration1", 0xc624: "CameraCalibration2",
    0xc627: "AnalogBalance", 0xc628: "AsShotNeutral", 0xc62a: "BaselineExposure", 0xc62b: "BaselineNoise", 0xc62c: "BaselineSharpness",
    0xc62f: "LinearResponseLimit", 0xc65a: "CalibrationIlluminant1", 0xc65b: "CalibrationIlluminant2", 0xc68d: "ActiveArea",
    0xc714: "ForwardMatrix1", 0xc715: "ForwardMatrix2", 0xc6f4: "ProfileName", 0xc725: "ProfileLookTableDims", 0xc6f9: "ProfileHueSatMapDims",
    0xc740: "OpcodeList1", 0xc741: "OpcodeList2", 0xc74e: "OpcodeList3", 0xc761: "NoiseProfile", 0xc7a7: "NewRawImageDigest",
    0xc7a3: "ProfileToneCurve", 0xc7a5: "DefaultBlackRender", 0xc7a4: "BaselineExposureOffset", 0xc6fe: "ColorimetricReference",
    0xcd3f: "ProfileGainTableMap", 0xc71e: "SubTileBlockSize", 0x014a: "SubIFDs", 0xc619: "BlackLevelRepeatDim", 0xc61c: "BlackLevelDeltaV",
    0xcd31: "ProfileGainTableMap2", 0xcd49: "ImageSequenceInfo", 0xc7aa: "CalibrationIlluminant3", 0xc7a9: "ColorMatrix3",
    0xc7ab: "ForwardMatrix3", 0xc7ac: "ProfileHueSatMapData3", 0xcd30: "IlluminantData1", 0xcd31: "IlluminantData2",
}

def read_ifd(b, off, e, depth, seen):
    if off in seen or off <= 0 or off + 2 > len(b):
        return
    seen.add(off)
    n = struct.unpack(e + "H", b[off:off + 2])[0]
    print(f"{'  ' * depth}IFD @{off} ({n} entries)")
    subs = []
    for i in range(n):
        p = off + 2 + 12 * i
        tag, typ, cnt, val = struct.unpack(e + "HHI4s", b[p:p + 12])
        if typ not in TYPES:
            continue
        fmt, sz = TYPES[typ]
        total = sz * cnt
        data = val[:total] if total <= 4 else b[struct.unpack(e + "I", val)[0]:][:total]
        name = NAMES.get(tag, "")
        if typ == 2:
            text = data.split(b"\0")[0].decode("latin1")
            vals = [text]
        elif typ in (5, 10):
            vals = [f"{a}/{c}" if c else f"{a}/0" for a, c in struct.iter_unpack(e + fmt, data[: (total // 8) * 8])]
        else:
            vals = [v[0] for v in struct.iter_unpack(e + fmt, data[: (total // sz) * sz])]
        if tag == 0x014a or (tag == 0xc740 or tag == 0xc741 or tag == 0xc74e):
            if tag == 0x014a:
                subs = list(vals)
                shown = vals
            else:
                nops = struct.unpack(">I", data[:4])[0] if len(data) >= 4 else 0
                ops = []
                q = 4
                for _ in range(nops):
                    if q + 16 > len(data):
                        break
                    oid, ver, flags, ln = struct.unpack(">IIII", data[q:q + 16])
                    ops.append(f"op{oid}(len {ln})")
                    q += 16 + ln
                shown = [f"{nops} opcodes: " + ", ".join(ops)]
        else:
            shown = vals
        if name or tag >= 0xc600:
            s = " ".join(str(v) for v in shown[:12]) + (f" … ({len(shown)} values)" if len(shown) > 12 else "")
            print(f"{'  ' * depth}  {tag:#06x} {name:<24} [{cnt}] {s}")
    for s in subs:
        read_ifd(b, int(s), e, depth + 1, seen)
    nxt = struct.unpack(e + "I", b[off + 2 + 12 * n: off + 6 + 12 * n])[0]
    if nxt:
        read_ifd(b, nxt, e, depth, seen)

for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.dng"))):
    b = open(path, "rb").read()
    e = "<" if b[:2] == b"II" else ">"
    print(f"\n==== {os.path.basename(path)} ({len(b)} bytes) ====")
    read_ifd(b, struct.unpack(e + "I", b[4:8])[0], e, 0, set())
```

## Appendix B: probe output, 2026-10-08 (summary lines; the full run is ~420 lines)

```
==================== P1000035.RW2 ====================
probe_info (1.8941ms): format Rw2, 6016x4016 cpp 1 bits 12, cfa Some([0, 1, 1, 2]), active Rect { x: 8, y: 8, width: 6000, height: 4000 }, crop Rect { x: 864, y: 576, width: 4272, height: 2848 }, orientation Rotate270
decode OK in 172.9154ms: 24160256 samples, min 124 max 4095 mean 195.1 zeros 0, black [128.0, 128.0, 128.0, 128.0] white [4095.0] linearized false
embedded preview: 4565465 bytes, dims Some((6000, 4000)), colour space None
camera JPEG P1000035.JPG: 6150656 bytes, dims Some((4272, 2848))
  preview cropped to the default crop Rect { x: 864, y: 576, width: 4272, height: 2848 } before comparing
  identical bytes: false; 48x48 grey mean abs diff: 0.30 / 255 (same image if small)
DNG P1000035.dng decode OK in 214.1632ms: 6016x4016 bits 16 cfa Some([0, 1, 1, 2]) active Rect { x: 0, y: 0, width: 6016, height: 4016 } crop Rect { x: 8, y: 8, width: 6000, height: 4000 } black [128.0, 128.0, 128.0, 128.0] white [4095.0]; illuminants [17, 21] matrix1 true matrix2 true forward true as_shot_neutral Some([0.510978, 1.0, 0.571429]) baseline_exposure -0.05 opcodes3 1
  RW2 vs DNG samples (full sensor 6016x4016): identical 24160256 of 24160256 (100.0000%), max |diff| 0, mean |diff| 0.00000
  DNG AsShotNeutral reciprocal [1.9570314, 1.0, 1.7499987] vs RW2 wb_multipliers Some([1.9570313, 1.0, 1.75])
engine probe_bytes: 2848x4272 kind Raw preview_only None as_shot_wb Some((6500.0, 0.0)) embedded_lens true
engine load_bytes(1200) OK in 314.3077ms: 800x1200, raw true relative_wb true as_shot 6500K/0 camera_tone(look fitted) true lens true; mean linear Rec.2020 RGB [0.0199, 0.0248, 0.0314]
==================== P1000116.RW2 ====================
probe_info (1.1761ms): format Rw2, 6016x4016 cpp 1 bits 12, cfa Some([0, 1, 1, 2]), active Rect { x: 8, y: 8, width: 6000, height: 4000 }, crop Rect { x: 0, y: 0, width: 6000, height: 4000 }, orientation Rotate90
decode OK in 141.8603ms: 24160256 samples, min 126 max 4095 mean 643.3 zeros 0, black [128.0, 128.0, 128.0, 128.0] white [4095.0] linearized false
embedded preview: 3239829 bytes, dims Some((6000, 4000)), colour space None
DNG P1000116.dng decode OK in 209.5026ms: 6016x4016 bits 16 cfa Some([0, 1, 1, 2]) active Rect { x: 0, y: 0, width: 6016, height: 4016 } crop Rect { x: 8, y: 8, width: 6000, height: 4000 } black [128.0, 128.0, 128.0, 128.0] white [4095.0]; illuminants [17, 21] matrix1 true matrix2 true forward true as_shot_neutral Some([0.484848, 1.0, 0.600939]) baseline_exposure -0.05 opcodes3 1
  RW2 vs DNG samples (full sensor 6016x4016): identical 24160256 of 24160256 (100.0000%), max |diff| 0, mean |diff| 0.00000
  DNG AsShotNeutral reciprocal [2.0625021, 1.0, 1.6640624] vs RW2 wb_multipliers Some([2.0625, 1.0, 1.6640625])
engine probe_bytes: 4000x6000 kind Raw preview_only None as_shot_wb Some((6500.0, 0.0)) embedded_lens true
engine load_bytes(1200) OK in 273.3348ms: 800x1200, raw true relative_wb true as_shot 6500K/0 camera_tone(look fitted) true lens true; mean linear Rec.2020 RGB [0.1408, 0.1655, 0.1901]
==================== P1000121.RW2 ====================
probe_info (1.3519ms): format Rw2, 6016x4016 cpp 1 bits 12, cfa Some([0, 1, 1, 2]), active Rect { x: 8, y: 8, width: 6000, height: 4000 }, crop Rect { x: 0, y: 0, width: 6000, height: 4000 }, orientation Normal
decode OK in 136.5264ms: 24160256 samples, min 140 max 4095 mean 605.6 zeros 0, black [128.0, 128.0, 128.0, 128.0] white [4095.0] linearized false
embedded preview: 4208819 bytes, dims Some((6000, 4000)), colour space None
camera JPEG P1000121.JPG: 11549696 bytes, dims Some((6000, 4000))
  identical bytes: false; 48x48 grey mean abs diff: 0.04 / 255 (same image if small)
DNG P1000121.dng decode OK in 210.9922ms: 6016x4016 bits 16 cfa Some([0, 1, 1, 2]) active Rect { x: 0, y: 0, width: 6016, height: 4016 } crop Rect { x: 8, y: 8, width: 6000, height: 4000 } black [128.0, 128.0, 128.0, 128.0] white [4095.0]; illuminants [17, 21] matrix1 true matrix2 true forward true as_shot_neutral Some([0.484848, 1.0, 0.600939]) baseline_exposure -0.05 opcodes3 1
  RW2 vs DNG samples (full sensor 6016x4016): identical 24160256 of 24160256 (100.0000%), max |diff| 0, mean |diff| 0.00000
  DNG AsShotNeutral reciprocal [2.0625021, 1.0, 1.6640624] vs RW2 wb_multipliers Some([2.0625, 1.0, 1.6640625])
engine probe_bytes: 6000x4000 kind Raw preview_only None as_shot_wb Some((6500.0, 0.0)) embedded_lens true
engine load_bytes(1200) OK in 250.8465ms: 1200x800, raw true relative_wb true as_shot 6500K/0 camera_tone(look fitted) true lens true; mean linear Rec.2020 RGB [0.1499, 0.1573, 0.1483]
==================== P1001760.RW2 ====================
probe_info (1.3233ms): format Rw2, 6016x4016 cpp 1 bits 12, cfa Some([0, 1, 1, 2]), active Rect { x: 8, y: 8, width: 6000, height: 4000 }, crop Rect { x: 0, y: 0, width: 6000, height: 4000 }, orientation Normal
decode OK in 141.96ms: 24160256 samples, min 117 max 1476 mean 389.3 zeros 0, black [128.0, 128.0, 128.0, 128.0] white [4095.0] linearized false
embedded preview: 3496979 bytes, dims Some((6000, 4000)), colour space None
DNG P1001760.dng decode OK in 222.4449ms: 6016x4016 bits 16 cfa Some([0, 1, 1, 2]) active Rect { x: 0, y: 0, width: 6016, height: 4016 } crop Rect { x: 8, y: 8, width: 6000, height: 4000 } black [128.0, 128.0, 128.0, 128.0] white [4095.0]; illuminants [17, 21] matrix1 true matrix2 true forward true as_shot_neutral Some([0.469725, 1.0, 0.65641]) baseline_exposure -0.05 opcodes3 1
  RW2 vs DNG samples (full sensor 6016x4016): identical 24160256 of 24160256 (100.0000%), max |diff| 0, mean |diff| 0.00000
  DNG AsShotNeutral reciprocal [2.1289053, 1.0, 1.5234381] vs RW2 wb_multipliers Some([2.1289063, 1.0, 1.5234375])
engine probe_bytes: 6000x4000 kind Raw preview_only None as_shot_wb Some((6500.0, 0.0)) embedded_lens true
engine load_bytes(1200) OK in 266.5636ms: 1200x800, raw true relative_wb true as_shot 6500K/0 camera_tone(look fitted) true lens true; mean linear Rec.2020 RGB [0.0837, 0.0847, 0.0825]
==================== P1001797.RW2 ====================
probe_info (635.1µs): format Rw2, 6016x4016 cpp 1 bits 12, cfa Some([0, 1, 1, 2]), active Rect { x: 8, y: 8, width: 6000, height: 4000 }, crop Rect { x: 0, y: 0, width: 6000, height: 4000 }, orientation Normal
decode OK in 138.0972ms: 24160256 samples, min 127 max 4095 mean 2119.1 zeros 0, black [128.0, 128.0, 128.0, 128.0] white [4080.0] linearized false
embedded preview: 2027797 bytes, dims Some((6000, 4000)), colour space None
DNG P1001797.dng decode OK in 204.8011ms: 6016x4016 bits 16 cfa Some([0, 1, 1, 2]) active Rect { x: 0, y: 0, width: 6016, height: 4016 } crop Rect { x: 8, y: 8, width: 6000, height: 4000 } black [128.0, 128.0, 128.0, 128.0] white [4095.0]; illuminants [17, 21] matrix1 true matrix2 true forward true as_shot_neutral Some([0.494208, 1.0, 0.641604]) baseline_exposure -0.05 opcodes3 1
  RW2 vs DNG samples (full sensor 6016x4016): identical 24160256 of 24160256 (100.0000%), max |diff| 0, mean |diff| 0.00000
  DNG AsShotNeutral reciprocal [2.0234394, 1.0, 1.5585938] vs RW2 wb_multipliers Some([2.0234375, 1.0, 1.5585938])
engine probe_bytes: 6000x4000 kind Raw preview_only None as_shot_wb Some((6500.0, 0.0)) embedded_lens true
engine load_bytes(1200) OK in 239.7672ms: 1200x800, raw true relative_wb true as_shot 6500K/0 camera_tone(look fitted) true lens true; mean linear Rec.2020 RGB [0.7245, 0.7377, 0.8021]
```
