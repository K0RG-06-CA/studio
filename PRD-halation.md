# Studio PRD — Physically based halation

Oct 8, 2026 · David Ahn · Companion to `PRD.md` (this file covers only halation; where they disagree, this file wins for halation)

## Summary

Build a halation effect for Studio's Film persona that models the real physics: light that passes through the emulsion reflects off the back of the film base and re-exposes the red (and a little of the green) layer around bright highlights. It works on **scene-linear exposure, before the film's tone curve, with no threshold or keying**, and its size is set in **micrometres on the film**, not pixels.

The design follows **spektrafilm** (formerly agx-emulsion, by Andrea Volpato), the best open reference. It is **GPL-3.0**, the same licence as Studio, so its structure and parameters may be ported with credit. A second, optional "physical" mode builds the kernel from film base thickness, refractive index and anti-halation backing strength.

Research behind this PRD: [`halation-research.md`](halation-research.md) (keep it next to this file).

## Why it happens (what the code must respect)

- **Mechanism:** light the emulsion didn't absorb crosses the clear base, reflects off its back surface (total internal reflection beyond the critical angle, plus a few % Fresnel reflection), and re-enters the emulsion a short distance away.
- **Colour comes from layer order, not a tint:** colour negative is coated blue → yellow filter → green → red → base. Reflected light hits red first, green a little, blue almost never. So channel weights are **R ≫ G ≫ B, with B ≈ 0 in practice**. Halos look orange near very bright sources (green also exposed) and red farther out.
- **Linear, no threshold:** halation is a fixed small fraction (about 1–8%) of the light, so it is added in linear exposure. It only *looks* thresholded because the film's log tone curve hides it unless a source is roughly 4–7 stops brighter than its surroundings.
- **Size is physical:** set by base thickness. Geometry predicts a ring at r ≈ 2·t·tan(arcsin(1/n)) ≈ 1.83·t for triacetate base, about 230–260 µm for 35mm (130 µm base). Smaller formats get relatively bigger halos automatically.
- **Anti-halation backing** (rem-jet, dyed base, silver undercoat) absorbs most of it. Strength scales like 10^(−D) for a backing at the base–air interface (acts once) and 10^(−2D) for a dyed base or an undercoat between base and emulsion (light crosses it on the way down and again on the way back). The two-pass rule follows from the geometry; the grey-base figures quoted in the research don't confirm it numerically (see the note there), so calibration should check it. CineStill 800T = Vision3 500T with rem-jet removed, hence its strong halos.
- **Not the same as:** lens flare / diffusion filters (colour-neutral, before the film) or irradiation (short-range scatter inside the emulsion). These are separate stages.

## Where it sits in the pipeline

This replaces the film-effects order in `PRD.md` (its "Fixed pipeline order" list) for every film effect, not only halation. In particular **Lazy Blur now runs before halation**: build Lazy Blur in this position even if it ships before halation does.

1. Raw decode, white balance, exposure (scene-linear f32)
2. **Chromatic-aberration correction** (before halation, so purple fringes don't become coloured halos). Studio doesn't build its own: this is LightCraft's lens-correction CA removal if it exists, run at this position. If LightCraft has none, the step is skipped; halation doesn't depend on it.
3. **Highlight boost** (rebuild clipped highlight energy; see below)
4. Lens / diffusion softness (Lazy Blur, any diffusion filter) — scene light, before the film
5. **Emulsion scatter** (irradiation, short-range, energy-preserving)
6. **Halation** (this feature)
7. Profile, tone curve, colour grading, Saturation RGB
8. Lazy Bleach
9. Grain
10. Output transform and export

## The model

### Mode A — Standard (default, spektrafilm-style)

All maths in f32 scene-linear, per RGB channel. Radii in µm, converted to pixels from the **long side** of the film frame and of the **uncropped, unrotated source image**: `pixel_size_um = frame_long_side_um / source_long_side_px`. Using the long side keeps portrait and landscape the same; using the pre-crop source keeps a crop or rotation from changing the halo size.

**Kernel sampling:** every Gaussian is discretised by integrating it over each pixel's footprint (difference of erf at the pixel edges), never by point-sampling. This stays correct when σ is far below one pixel: the emulsion-scatter kernels (σc ≈ 2 µm ≈ 0.3 px at 6 µm/px, ≈ 0.08 px in a 25% draft) then become an almost-identity kernel, which is the physically right result at that resolution.

**Edges:** convolutions use half-sample symmetric reflection at the image border (`dcba|abcd`).

**Non-finite input:** NaN or ±inf pixels are treated as 0 before any blur, so one bad pixel can't spread across a halo. (Never-crash rule from `CLAUDE.md`.)

**Highlight boost** (above a knee, so clipped light sources still drive a halo):
`y = x + max·k·(exp(a·dx) − a·dx − 1)`, where `dx = min(x − knee, dx_max)` for `x > knee` (and `y = x` below the knee). `dx_max` keeps `a·dx ≤ 20`, so `exp` stays finite in f32 (e^20 ≈ 4.9·10^8); the result is also clamped to a finite ceiling. `max`, `k`, `a` and `knee` are spektrafilm's boost parameters: confirm what `max` means (a gain or a ceiling) in its code when porting, and record the answer here. Default off for non-raw files; tie it to LightCraft's highlight reconstruction if one exists.

**Emulsion scatter (energy-preserving):**
`E1 = (1 − s)·E0 + s·[(1 − w)·G(σc) ⊛ E0 + w·Exp(λ) ⊛ E0]`
- Defaults per R/G/B: σc = 2.2 / 2.0 / 1.6 µm, λ = 9.3 / 9.7 / 9.1 µm, w = 0.78 / 0.65 / 0.67.
- The exponential is approximated as 3 Gaussians: weights (0.1633, 0.6496, 0.1870) at σ = (0.536λ, 1.5236λ, 2.7684λ). spektrafilm calls these "placeholder fits"; keep them configurable.

**Halation (multi-bounce):**
Per channel c: `E2_c = E1_c + A_c·Σ(k=1..N) a_k · G(σh·√k) ⊛ E1_c`, with the bounce weights normalised so they sum to 1: `a_k = ρ^(k−1) / Σ(j=1..N) ρ^(j−1)`. Then `a_total_c = A_c·Σ a_k = A_c`, and with renormalise on (the default) the result is divided by `(1 + A_c)` per channel, so each channel's total energy is preserved. (This is Studio's definition; check it against spektrafilm's code when porting and note any difference here.)
- Defaults: Backing = **Standard** (A = 0.05 / 0.015 / 0 for R/G/B), σh = 65 µm (still) or 50 µm (cine), N = 3, ρ = 0.5.

**Backing presets** (set A per channel):

| Preset | R / G / B strength | Example |
|---|---|---|
| Strong backing | 0.015 / 0.005 / 0 | Vision3, modern colour negative |
| Standard (default) | 0.05 / 0.015 / 0 | spektrafilm's default strengths |
| Weak backing | 0.08 / 0.02 / 0 | Some still films |
| None (rem-jet removed) | 0.30 / 0.10 / 0.015 | CineStill-like |

These numbers are spektrafilm's tuned values, not published measurements. Treat them as starting points. Known inconsistency: the research gives a₁ red ≈ 0.08–0.25 for "none" and a physical halation fraction of about 1–8%, but spektrafilm's "none" preset uses 0.30. Implement 0.30 (spektrafilm's value) and let calibration or the look check settle it (open question below).

### Mode B — Physical (optional, behind a toggle)

Builds the halation kernel from film properties instead of Gaussians:
- Inputs: base thickness `t` (µm), refractive index `n`, backing density `D` **per channel** (R/G/B; e.g. the patent's blue-silver undercoat is 2.44 / 2.01 / 1.62), backing position: rem-jet backing at the base–air interface, dyed base, or antihalation undercoat (AHU) between base and emulsion.
- Kernel: a weak Fresnel inner disc for r < r_c, a total-internal-reflection ring at `r_c = 2·t·tan(arcsin(1/n))`, weighted by a forward-peaked angular distribution, plus fainter repeat rings at k·r_c.
- The kernel is then softened by the scatter core (Mode A's scatter stage) so the ring isn't razor sharp.
- Gain per channel: × 10^(−D) for a rem-jet backing (acts once), × 10^(−2D) for a dyed base or an undercoat (crossed on the way down and back).
- Defaults: t = 130 µm, n = 1.48 (triacetate). Polyester ≈ 1.65 (unverified literature value).

Mode B exists because the halo size is genuinely unsettled: geometry says ~240 µm, spektrafilm uses 50–65 µm Gaussians, and other ports enlarged it by eye. Validation (below) decides which mode is the default long-term.

## Controls (Film persona)

Top of the Film persona's panel, in a "Halation" group:

- **Amount** (overall multiplier on A, 0–200%)
- **Backing**: Strong / Standard / Weak / None / Custom (sets per-channel strengths; Standard is the default)
- **Size** (σh in µm; shown as "Small / Medium / Large" with the µm value in a tooltip)
- **Film format**: 35mm (36 mm long side) / 120 6×4.5 (56 mm) / 120 6×6 (56 mm) / 120 6×7 (70 mm) / 120 6×9 (84 mm) / 16mm (10.3 mm) / Custom long side (sets µm→pixel conversion)
- **Highlight boost** (off / amount)
- **Advanced** (collapsed): per-channel R/G/B strength, bounces N, ρ, scatter on/off, Mode A/B, and Mode B's t, n, D

No hue slider and no threshold slider: colour and the "threshold" both come from the physics. (If testing shows people need a hue tweak, add it under Advanced only.)

Halation is also a part of the **film stock builder** (see `PRD.md`): a stock file stores all of the above.

## Implementation

- **Crate:** `studio-film`, module `halation`. Registers as a pipeline step through the hook described in `PRD.md` ("plug in, don't patch").
- **GPU (wgpu/WGSL):** do not port spektrafilm's recursive IIR filter (serial per row, bad on GPU). Use:
  - Separable shared-memory Gaussian blur up to ~30–60 px radius.
  - A **mip-pyramid sum of Gaussians** (dual-filter-bloom style, tent/13-tap upsampling to avoid blocky halos) for large radii and the exponential tail.
  - FFT only if pyramid accuracy isn't good enough for Mode B's ring.
- **CPU:** must run **the same algorithm** as the GPU (pyramid, not IIR), so the 1/255 CPU/GPU match from `PRD.md` holds. spektrafilm's own output is a *look reference* with a looser tolerance, not a bit-exact target.
- **Draft vs final:** while dragging, compute on the preview-size image (µm conversion keeps the look the same); full resolution on release and export.
- **Masks, history, presets, copy/paste, MCP commands, XMP:** same rules as every film effect in `PRD.md`.
- **Platforms:** Windows and Linux, ARM64 and x64. f32 throughout.

## Licensing

- **spektrafilm code:** GPL-3.0 → may be ported. Add its licence notice and credit (Andrea Volpato) to `NOTICE` and the credits screen.
- **spektrafilm profiles (JSON):** CC BY-SA 4.0 → only copy with attribution and share-alike; prefer re-deriving our own.
- **spektrafilm LUTs:** separate non-resale licence → **do not include**.
- **hotgluebanjo/halation-dctl:** no licence → **do not copy any code**. Use only its README's list of failure modes as a test checklist.

## Validation

**Automated tests (must pass in CI):**
- Energy: with renormalise on and highlight boost **off**, each channel's total energy changes < 0.1%. Use a test image whose bright content sits at least 3·σ of the largest kernel away from every border, so the result doesn't depend on the edge mode. (Highlight boost adds energy by design and is tested separately.)
- No threshold (linearity): with renormalise **off**, (a) a dim uniform image gains exactly the predicted fraction per channel (E2/E1 = 1 + A_c), not zero; and (b) scaling a point source's exposure by any factor scales its halo by the same factor (dim and bright sources get the same relative halo). With renormalise on, (b) must still hold.
- Colour: a white point source produces R > G > B in the halo; B ≈ 0 with default presets.
- Resolution independence: render at 100%, downsample the result by area-averaging to 50% and 25%, and compare with rendering at 50% and 25% directly (within tolerance). Sub-pixel kernels follow the pixel-integrated sampling rule above.
- Rotation and crop: a portrait version of a landscape image (and a crop of it) gives the same halo size in µm.
- Robustness: an image containing NaN, ±inf and +20 EV pixels gives finite output everywhere, with highlight boost on.
- Format: switching 35mm → 16mm makes the halo relatively larger.
- CPU vs GPU within 1/255.
- Same input + settings → bit-identical output every run.
- Performance: draft update < 50 ms on a 24 MP S9 file on the ARM64 laptop.

**Failure-mode checklist** (from the hotgluebanjo README, checked visually on test images):
- Falloff looks like a plain Gaussian or box blur.
- Two-colour banding in the fading halo.
- Highlights themselves are over-tinted red.
- Halos appear on edges that aren't bright (keying/edge-detection artifacts).
- Midtones and shadows get washed out (a known complaint about one emulator vs real CineStill 800T).

**Calibration against real film (later, optional but valuable):** no published measurement of a modern stock's halation exists, so real scans are the only true check:
1. Photograph a small halogen-backlit pinhole in a dark room, bracketed +0 to +10 stops; shoot the same lens on a digital camera as a lens-flare control.
2. Add a backlit knife edge (gives the spread function; tail beyond ~100 µm is halation).
3. Include ND rings of known density in frame for intensity calibration.
4. Scan linearly (no sharpening/ICE), convert density back to exposure.
5. Compare CineStill 800T vs Vision3 (with rem-jet), and 135 vs 120 of the same stock.
Fit Mode A's strengths/σh and Mode B's t/n/D to these scans, and store the fitted values as presets.

## Milestones (for Claude Code)

Do these in order, one at a time, after the Film persona shell exists:

1. **Reference model on CPU:** scatter + Mode A halation + highlight boost, with unit tests (energy with boost off, linearity, colour, resolution independence, rotation/crop, robustness, determinism). Add spektrafilm's licence notice and credit to `NOTICE` in the same change as the first ported code. No UI yet.
2. **GPU port:** WGSL separable blur + mip pyramid; CPU switched to the same pyramid algorithm; CPU/GPU parity test.
3. **Pipeline + UI:** register the step in the right pipeline position; Film persona controls; presets; masks, history, MCP commands.
4. **Look check:** compare against spektrafilm renders of the same test images and the failure-mode checklist; tune.
5. **Mode B (physical kernel)** behind the Advanced toggle, with its own tests.
6. **Calibration (optional):** fit presets to your own bracketed scans.

**Model:** Fable 5.1 at high effort for milestones 1, 2 and 5 (maths and parity are subtle); Opus 5.5 at medium for 3 and 4.

**Start prompt:** "Read @PRD.md and @PRD-halation.md, and the research report. Study spektrafilm's halation code (GPL-3.0) for reference. Plan halation milestone 1 only, show me the plan, then build it with tests first."

## Open questions

- [ ] Which halo size is right for modern 35mm: ~65 µm Gaussian (spektrafilm) or the ~240 µm ring geometry predicts? (Answered by calibration.)
- [ ] Does LightCraft already have highlight reconstruction to reuse for the highlight boost?
- [ ] Should halation also run in the Pixel persona as a live filter, or only in Film?
- [ ] "None" backing strength: spektrafilm's 0.30 red vs the research's 0.08–0.25 a₁ range and 1–8% physical fraction. (Answered by the look check or calibration.)
- [ ] What exactly is `max` in spektrafilm's highlight boost (a gain or a ceiling)? (Answered when porting.)
- [ ] Does Studio's normalisation of the bounce weights (`Σ a_k = 1`, `a_total = A`) match spektrafilm's? (Answered when porting.)
