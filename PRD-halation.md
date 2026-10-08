# Studio PRD — Physically based halation

Oct 8, 2026 · David Ahn · Companion to `PRD.md` (this file covers only halation; where they disagree, this file wins for halation)

## Summary

Build a halation effect for Studio's Film persona that models the real physics: light that passes through the emulsion reflects off the back of the film base and re-exposes the red (and a little of the green) layer around bright highlights. It works on **scene-linear exposure, before the film's tone curve, with no threshold or keying**, and its size is set in **micrometres on the film**, not pixels.

The design follows **spektrafilm** (formerly agx-emulsion, by Andrea Volpato), the best open reference. It is **GPL-3.0**, the same licence as Studio, so its structure and parameters may be ported with credit. A second, optional "physical" mode builds the kernel from film base thickness, refractive index and anti-halation backing strength.

Research behind this PRD: [`halation-research.md`](halation-research.md) (keep it next to this file).

## Why it happens (what the code must respect)

- **Mechanism:** light the emulsion didn't absorb crosses the clear base, reflects off its back surface (total internal reflection beyond the critical angle, plus a few % Fresnel reflection), and re-enters the emulsion a short distance away.
- **Colour comes from layer order, not a tint:** colour negative is coated blue → yellow filter → green → red → base. Reflected light hits red first, green a little, blue almost never. So channel weights are **R ≫ G > B ≈ 0**. Halos look orange near very bright sources (green also exposed) and red farther out.
- **Linear, no threshold:** halation is a fixed small fraction (about 1–8%) of the light, so it is added in linear exposure. It only *looks* thresholded because the film's log tone curve hides it unless a source is roughly 4–7 stops brighter than its surroundings.
- **Size is physical:** set by base thickness. Geometry predicts a ring at r ≈ 2·t·tan(arcsin(1/n)) ≈ 1.83·t for triacetate base, about 230–260 µm for 35mm (130 µm base). Smaller formats get relatively bigger halos automatically.
- **Anti-halation backing** (rem-jet, dyed base, silver undercoat) absorbs most of it. Strength scales like 10^(−D) for a backing and 10^(−2D) for a dyed base (light crosses it twice). CineStill 800T = Vision3 500T with rem-jet removed, hence its strong halos.
- **Not the same as:** lens flare / diffusion filters (colour-neutral, before the film) or irradiation (short-range scatter inside the emulsion). These are separate stages.

## Where it sits in the pipeline

This replaces the film-effects order in `PRD.md` for the steps it touches:

1. Raw decode, white balance, exposure (scene-linear f32)
2. **Chromatic-aberration correction** (before halation, so purple fringes don't become coloured halos)
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

All maths in f32 scene-linear, per RGB channel. Radii in µm, converted to pixels from the frame size (`pixel_size_um = film_width_um / image_width_px`).

**Highlight boost** (above a knee, so clipped light sources still drive a halo):
`y = x + max·k·(exp(a·dx) − a·dx − 1)`, where `dx` is the amount above the knee. Default off for non-raw files; tie it to LightCraft's highlight reconstruction if one exists.

**Emulsion scatter (energy-preserving):**
`E1 = (1 − s)·E0 + s·[(1 − w)·G(σc) ⊛ E0 + w·Exp(λ) ⊛ E0]`
- Defaults per R/G/B: σc = 2.2 / 2.0 / 1.6 µm, λ = 9.3 / 9.7 / 9.1 µm, w = 0.78 / 0.65 / 0.67.
- The exponential is approximated as 3 Gaussians: weights (0.1633, 0.6496, 0.1870) at σ = (0.536λ, 1.5236λ, 2.7684λ). spektrafilm calls these "placeholder fits"; keep them configurable.

**Halation (multi-bounce):**
`E2 = E1 + A·Σ(k=1..N) a_k · G(σh·√k) ⊛ E1`, with `a_k ∝ ρ^(k−1)`, then divide by `(1 + a_total)` so total energy is preserved (renormalise on by default).
- Defaults: A = 0.05 / 0.015 / 0 (R/G/B), σh = 65 µm (still) or 50 µm (cine), N = 3, ρ = 0.5.

**Backing presets** (set A per channel):

| Preset | R / G / B strength | Example |
|---|---|---|
| Strong backing | 0.015 / 0.005 / 0 | Vision3, modern colour negative |
| Weak backing | 0.08 / 0.02 / 0 | Some still films |
| None (rem-jet removed) | 0.30 / 0.10 / 0.015 | CineStill-like |

These numbers are spektrafilm's tuned values, not published measurements. Treat them as starting points.

### Mode B — Physical (optional, behind a toggle)

Builds the halation kernel from film properties instead of Gaussians:
- Inputs: base thickness `t` (µm), refractive index `n`, backing density `D`, backing position (backing vs dyed base).
- Kernel: a weak Fresnel inner disc for r < r_c, a total-internal-reflection ring at `r_c = 2·t·tan(arcsin(1/n))`, weighted by a forward-peaked angular distribution, plus fainter repeat rings at k·r_c.
- The kernel is then softened by the scatter core (Mode A's scatter stage) so the ring isn't razor sharp.
- Gain: × 10^(−D) for a backing, × 10^(−2D) for a dyed base.
- Defaults: t = 130 µm, n = 1.48 (triacetate). Polyester ≈ 1.65 (unverified literature value).

Mode B exists because the halo size is genuinely unsettled: geometry says ~240 µm, spektrafilm uses 50–65 µm Gaussians, and other ports enlarged it by eye. Validation (below) decides which mode is the default long-term.

## Controls (Film persona)

Top of the Film persona's panel, in a "Halation" group:

- **Amount** (overall multiplier on A, 0–200%)
- **Backing**: Strong / Weak / None / Custom (sets per-channel strengths)
- **Size** (σh in µm; shown as "Small / Medium / Large" with the µm value in a tooltip)
- **Film format**: 35mm / 120 / 16mm / Custom width (sets µm→pixel conversion)
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
- Energy: with renormalise on, total image energy changes < 0.1%.
- No threshold: a dim uniform image gets a proportional (tiny) halation, not zero.
- Colour: a white point source produces R > G > B in the halo; B ≈ 0 with default presets.
- Resolution independence: the same image at 25%, 50% and 100% size gives the same halo after rescaling (within tolerance).
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

1. **Reference model on CPU:** scatter + Mode A halation + highlight boost, with unit tests (energy, colour, resolution independence, determinism). No UI yet.
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
