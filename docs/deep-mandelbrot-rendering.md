# Deep & Optimal Mandelbrot Rendering

*A researched survey of the algorithms behind fast, deep-zoom Mandelbrot rendering — perturbation theory, series approximation, bilinear approximation (BLA), glitch handling & rebasing, precision ladders, SIMD/GPU compute, Mariani–Silver, and coloring. Each section is tied back to where it fits this crate (`fractal`).*

Compiled 2026-05-29, revised after a second deeper verification pass. Findings were fan-out web-searched, then adversarially fact-checked twice (3-vote, 2/3-to-kill). All 8 core claims survived re-verification against primary sources; corrections and expansions from the second pass are folded in below and marked where relevant. Confidence and sources are noted per section.

The dominant primary authority in this niche is **Claude Heiland-Allen** (`mathr.co.uk`, author of Kalles Fraktaler and Fraktaler-3) — see [Caveats](#caveats). *(Unrelated to Anthropic's Claude.)*

---

## TL;DR

- **Deep zoom is bounded by precision, not iteration count.** Past ~10⁻¹⁵ (the limit of `f64`), every pixel would need arbitrary-precision arithmetic — far too slow. **Perturbation theory** sidesteps this: compute *one* high-precision reference orbit, then iterate every pixel as a small `f64`/`f32` *delta* off that reference. ~100× speedup. This is the single most important technique and the foundation everything else builds on.
- **Two acceleration layers stack on top of perturbation:** *series approximation* (skip early iterations via a Taylor polynomial in `c`) and the newer *bilinear approximation / BLA* (skip many iterations at once via a precomputed 2M-entry merge tree). BLA is simpler, more parallelizable, and more general — but **no head-to-head benchmark proves it faster** than series approximation/NanoMB.
- **Robustness:** reactive (Pauldelbrot's glitch criterion + retry) is increasingly superseded by proactive **rebasing** (Zhuoran), which needs only *one* reference orbit for the whole Mandelbrot set.
- **Precision is a ladder**, not a binary: `float → double → long double/float128 → floatexp → doubleexp`, auto-selected by zoom depth in modern renderers (Fraktaler-3). Only the reference orbit needs true bignum.
- **This crate** targets shallow-to-mid zoom (`f64`, SIMD, Mariani–Silver fill) and now does **smooth μ coloring** GPU-side. Deep-zoom machinery isn't needed until you cross the `f64` floor — perturbation is the gateway. See [Where this fits the `fractal` crate](#where-this-fits-the-fractal-crate).

---

## 1. Perturbation theory — the foundation

**Confidence: high.** Sources: [mathr 2021](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html), [SuperFractalThing announcement (fractalforums.com, 2013)](https://www.fractalforums.com/announcements-and-news/superfractalthing-arbitrary-precision-mandelbrot-set-rendering-in-java/), [mathr m-perturbation](https://mathr.co.uk/web/m-perturbation.html).

The standard iteration `z → z² + c` loses all meaning at deep zoom because the pixel-to-pixel difference in `c` is smaller than `f64` epsilon relative to `z` (Martin calls this *catastrophic absorption* of tiny values into large ones). Perturbation rewrites the problem in terms of deviations from a single reference.

Let `Z` be a **high-precision reference orbit** (uppercase) for a reference point `C`, and let each pixel be `C + c` with a per-pixel **delta orbit** `z` (lowercase), so the true orbit is `Z + z`. Expanding `(Z + z)² + (C + c)` and subtracting `Z² + C` gives the **exact** delta recurrence (no truncation — pure symbolic algebra):

```
z_{n+1} = 2·Z_n·z_n + z_n² + c
```

> **Notation note (2nd-pass correction):** primary sources use `Z` / `z` / `c` exactly as above — capital = reference, lowercase = per-pixel delta, and the lowercase `c` *is* the per-pixel offset Δc. Some writeups paraphrase this with `Δ` symbols; the math is identical.

**Why it's fast:** the reference orbit is computed *once* in arbitrary precision (MPFR/MPIR). Every pixel's delta `z` stays small enough that hardware `f64` — or even `f32` — suffices. Martin's own framing: *"all the numbers are small, allowing it to be calculated with hardware floating point numbers."* You replace millions of arbitrary-precision pixel iterations with one arbitrary-precision reference plus millions of cheap `f64` delta iterations. Speedup is roughly two orders of magnitude.

- **The `+ c` term is mandatory.** A variant on WikiBooks drops it (`z_{n+1} = 2·z_n·z_n + z_n²`) — that term is the *only* thing encoding the per-pixel parameter offset; omit it and every pixel collapses onto the reference orbit. Fact-checked and **refuted** in both passes. Do not use it.
- **Attribution.** Popularized by **K.I. Martin's SuperFractalThing** (`sft_maths.pdf`), announced on fractalforums.com on **April 7, 2013** by user `mrflay` (= K.I. Martin). mathr notes the technique was *"seemingly rediscovered independently"* by Martin (2013) and **Sergey Khashin**, *"Fast calculation of the Mandelbrot set with infinite resolution"* (2016).
- **Where the small-delta assumption breaks (2nd-pass addition):** (a) **near minibrots**, where `|Z + z| ≈ 0` makes the *relative* delta blow up → glitches (§4); and (b) below `f64`'s **exponent floor** (~10⁻³⁰⁸), where deltas underflow → you need extended-exponent types (§5). These two failure modes drive everything in §4–§5.

---

## 2. Series approximation — skip the early iterations

**Confidence: high.** Sources: [mathr 2021](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html), [WikiBooks](https://en.wikibooks.org/wiki/Fractals/perturbation), [Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set).

For the first many iterations, the delta orbit `z` behaves almost linearly in `c`. Series approximation represents it as a **truncated Taylor polynomial in `c`**:

```
z_n ≈ Σ_k  A_{n,k} · c^k        (A_{n+1,1} = 2·Z_n·A_{n,1} + 1, …)
```

The coefficients `A_{n,k}` depend **only on the reference orbit**, so they're computed *once per image*. To render a pixel, evaluate the polynomial at that pixel's `c` to jump straight to a later iteration `n`, skipping all the per-pixel work up to that point; iteration then proceeds with the normal perturbation recurrence (§1).

- Typical **~10× speedup** at depth ~10¹⁰⁰; **~100× combined** with perturbation. (Figures are "typical" and scene-dependent — not guarantees.)
- The risk is over-skipping: the polynomial is only valid while higher-order terms stay negligible. Pick the skip iteration conservatively, or validate it.
- **Biseries variants** (`NanoMB1` / `NanoMB2`, by **knighty**) skip *whole periods* near minibrots for extreme zooms. *(2nd-pass note: knighty's NanoMB was not located in primary sources this pass — the attribution comes from secondary/forum references.)*

---

## 3. Bilinear approximation (BLA) — the modern alternative

**Confidence: high.** Sources: [mathr 2022](https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html) (originated by **Zhuoran** on fractalforums), [Fraktaler-3 docs](https://fraktaler.mathr.co.uk/), [philthompson.me 2023](https://philthompson.me/2023/Faster-Mandelbrot-Set-Rendering-with-BLA-Bivariate-Linear-Approximation.html).

BLA is the modern alternative to series approximation, and **real production technology in Fraktaler-3**. The insight: when the delta `z` is small, the **z² term is negligible**, so the recurrence is approximately *linear*:

```
z_{n+1} ≈ 2·Z_n·z_n + c          (valid when z² ≪ 2·Z_n·z + c, within a validity radius r)
```

A single linear step is `z → A·z + B·c`. Two consecutive linear steps compose into another linear step — so you can **merge** them. BLA precomputes a table of these merged steps:

- **Table size = 2M (confirmed exactly).** For a reference of length `M`: the bottom level has `M` single-step BLAs (one per iteration), the next level `M/2` merging neighbours, up to a root spanning the whole band — a binary merge tree. Total storage = `M + M/2 + M/4 + … = 2M` BLAs (geometric series). This is the dense/maximal layout; Fraktaler-3 can use coarser levels to save memory bandwidth.
- At render time, for each pixel pick the **largest valid merged step** (whose validity radius `r` contains the current `z`) and skip that many iterations *at once*, falling back to single perturbation steps when none is valid.

**Naming (2nd-pass correction):** Fraktaler-3 calls it **"bilinear approximation"**; Zhuoran and philthompson use **"bivariate linear approximation."** Both names circulate for the same technique — don't treat one as canonical. (A bundled etymology sub-claim — "bilinear *because* linear in two variables" — was specifically refuted as over-reaching; just use "BLA".)

**Why BLA over series approximation** (mathr's own list — these are the *verified* advantages):

1. Conceptually simpler.
2. Easier to implement and parallelize (shared table; independent per-pixel lookups — GPU-friendly).
3. Better-understood stopping/validity conditions.
4. More general — handles Burning Ship, hybrids, where series approximation is awkward.

> **Unsettled — not "faster" (verified open question):** the primary source explicitly defers the speed verdict: *"need to do benchmarks to see how it compares speed-wise before declaring an overall winner."* No head-to-head BLA-vs-series/NanoMB benchmark exists. BLA's established edge is **simplicity / generality / parallelizability**, not proven speed.
>
> **Reuse caveat:** Fraktaler-3 notes *"reusing bilinear approximation is not generally applicable at the present time (it depends on zoom depth)"* — you generally rebuild the table per location. A BLA step limit of *"a few thousand is usually sufficient."*

---

## 4. Glitch handling & rebasing

**Confidence: high.** Sources: [mathr 2021](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html), [mathr 2022](https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html).

When a pixel's true orbit passes too close to a critical point, the small-delta assumption breaks and you get a **glitch** (visibly wrong pixels, often blobs). Two strategies:

### Reactive — Pauldelbrot's criterion (2014)

Flag a pixel as glitched when:

```
|Z_n + z_n|²  <  G · |Z_n|²        (G heuristically between 1e-2 and 1e-8)
```

i.e. the full value has shrunk so close to the reference that precision is lost. It's **nearly free**: `|Z + z|²` is already computed for the escape test, and `G·|Z|²` can be precomputed per reference iteration. Glitched pixels are **retried** with a new (or rebased) reference.

Choosing `G` is an inherent tradeoff with **no closed-form optimum** (mathr, verbatim): *"too big and it takes forever … too small and some glitches can be missed."* This is the residual open problem that motivates rebasing.

### Proactive — rebasing (Zhuoran)

Rebasing **avoids** glitches rather than detecting and patching them, and is the modern default. The concrete algorithm (2nd-pass addition):

> When `|Z_m + z_n| < |z_n|` — i.e. the rebased value would be smaller than the current delta — **replace `z_n` with `Z_m + z_n` and reset the reference index `m` to 0.** More generally, select the orbit `o` that minimizes `|(Z − Z_o) + z|`.

Resetting to the *start* of the reference keeps the delta small by construction. The payoff: you need only **as many reference orbits as the formula has critical points** — for the Mandelbrot set (and Burning Ship), that's **one**. mathr, verbatim: *"Rebasing means you only need as many reference orbits as critical points."* Rebasing pairs naturally with BLA in modern renderers and largely retires `G`-tuning for "well-behaved formulas."

> *Attribution note:* the 2021 page describes rebasing without naming Zhuoran; the Zhuoran attribution comes from the 2022 page / fractalforums.org.

---

## 5. Precision: a ladder, not a binary

**Confidence: high.** Sources: [Fraktaler-3 README](https://code.mathr.co.uk/fraktaler-3), [Fraktaler-3 docs](https://fraktaler.mathr.co.uk/), [FractalShark](https://github.com/mattsaccount364/FractalShark).

Only the **reference orbit** needs true arbitrary precision; the **per-pixel deltas** want the *cheapest representation that survives the current zoom depth*. Modern renderers select the type automatically by depth.

### Fraktaler-3's per-pixel ladder (v3.1, verified ranges)

| Type | Zoom-depth range | Notes |
|---|---|---|
| `float` | up to ~1e30 | single precision |
| `double` | ~1e30 – 1e300 | hardware `f64` workhorse |
| `long double` / `float128` | ~1e300 – 1e4920 | 80-bit extended / quadruple precision — **alternative tiers at the same range**, not deeper |
| `floatexp` | arbitrarily deep | software float with **extended exponent range** (fixes `f64`'s ~1e-308 underflow) |
| `doubleexp` | arbitrarily deep | double-precision-mantissa `floatexp`, **new in v3.1** |

A **"wisdom" file** auto-chooses the best type *and* compute device per location. Note `floatexp`/`doubleexp` solve the *exponent* problem (depth), distinct from adding *mantissa* bits (local precision).

### FractalShark's GPU approach (the high end)

[FractalShark](https://github.com/mattsaccount364/FractalShark) (CUDA, GPLv3) is a useful high-end data point — two distinct, **non-interchangeable** mechanisms:

1. **`2x32` per-pixel type** — a pair of 32-bit floats + shared exponent, giving an effective **~48-bit mantissa** *without* native 64-bit arithmetic. **CUDA-only**; on CPUs native `double` is preferable. This is for *delta* iteration. *(Distinct from "double-double" — don't conflate.)*
2. **NTT-based bignum *reference orbit* on the GPU** — a Number-Theoretic-Transform multiply inside a full multiply/add/subtract pipeline (parallel-prefix carry, separate exponent). At **16384 32-bit limbs (~158,000 decimal digits)** on an **RTX 4090** it is reportedly **~10× faster than multithreaded MPIR + AVX-2** (and **~30× vs single-threaded**); an RTX 5090 measured ~9×. The GPU advantage **only pays off at ≥4096 limbs**.

> **Caveat:** all FractalShark numbers are the single developer's self-reported, *"hobby/experimental quality"* benchmarks at one operating point, with no independent reproduction. Treat as indicative, not validated. (FractalShark is fast-moving: v0.5 Dec 2025, v0.52 ~May 2026.)

### Practical takeaway for a from-scratch implementation

- Reference orbit: arbitrary precision (MPFR/`rug` in Rust) — computed once.
- Deltas: start with `f64`; add a `floatexp`-style `(f64 mantissa, i32 exponent)` type when zoom exceeds ~1e300; reach for double-double only if you need *mantissa* bits beyond `f64` without going full bignum.

---

## 6. SIMD & CPU vectorization (what this crate does now)

**Confidence: medium** (practitioner blogs). Sources: [Mike Kohn](https://www.mikekohn.net/software/mandelbrots_simd.php), [bumbershootsoft 2024](https://bumbershootsoft.wordpress.com/2024/01/27/optimizing-mandelbrot-generation-with-simd/).

Before precision forces perturbation, the game is raw throughput of `z² + c`:

- **Lane-parallel iteration.** Pack N pixels (8× `f64` with AVX-512, or `Simd<f64, 8>` via `portable_simd` — exactly what `mandelbrot_simd.rs` does) and iterate them together. **This crate already does this.**
- **The escape-divergence problem.** Lanes escape at different iteration counts, but SIMD steps in lockstep. Keep iterating until *all* lanes escape (or hit the cap), masking out escaped lanes so they stop accumulating; a per-lane active mask updates counts only where still iterating.
- **Periodicity / interior tests.** Interior points never escape, costing the full `MAX_ITER`. Cycle detection or cardioid/period-2-bulb membership tests bail early. Branchy scalar logic vs. clean SIMD lockstep — often *not* worth it inside a tight SIMD kernel, but big for interior-heavy views (and Mariani–Silver below covers most of that win structurally).
- **Unrolling the inner loop** (this crate does chunked unrolling) amortizes loop overhead and the periodic cancel-token check.

### Mariani–Silver / boundary tracing (this crate's tile fill)

**Confidence: medium.** Source: [Mu-Ency (mrob.com)](http://www.mrob.com/pub/muency/marianisilveralgorithm.html), [Wikipedia](https://en.wikipedia.org/wiki/Mariani%E2%80%93Silver_algorithm).

The Mandelbrot set is **connected**, so if the entire *boundary* of a rectangle maps to the same dwell (iteration count), the whole interior must too — fill it without computing a single interior pixel:

1. Compute the dwell for all pixels on the rectangle's edge.
2. If they're all equal → flood-fill the interior with that value.
3. Otherwise subdivide and recurse on each sub-rectangle.

This is a large win for solid regions (interior, far exterior) and is what your "Mariani–Silver tile fill optimization" commit added.

> **Correctness caveat (2nd-pass addition — it is NOT exact):** Mariani–Silver *"will occasionally miss features if the maximum dwell is too low, and often misses parts of cusps narrower than one pixel. This latter flaw is common to all adjacency optimizations except circle tiling. Use of distance-estimator coloring helps to ameliorate the cusp-omission problem"* (Mu-Ency). So: keep `MAX_ITER` high enough, and be aware thin filaments crossing a "uniform" border can be skipped.
>
> **Open w.r.t. deep zoom:** no primary source addresses Mariani–Silver *combined with* perturbation/BLA and smooth coloring at deep zoom. Safe practice: make the uniform-border decision on **exact integer escape counts only** (not the smooth μ value), then color separately. With your kernel now emitting a float `count`, the fill test should compare the *integer* escape iteration, not μ.

---

## 7. GPU compute

**Confidence: medium.** Sources: [FractalShark](https://github.com/mattsaccount364/FractalShark), [Ambrose Cavalier](https://ambrosecavalier.com/projects/gpu-deep-zoom/about/).

- **Shallow/mid zoom on GPU** is embarrassingly parallel: one thread per pixel, `f32`/`f64` iteration. This crate uses the GPU only for the *texture → μ → palette → reprojection* path, not iteration — a reasonable split given the CPU SIMD kernel.
- **Deep zoom on GPU** means porting perturbation + BLA: upload the reference orbit and the BLA table once, then run per-pixel delta iteration in a compute shader. BLA's independent per-pixel table lookups make it GPU-friendly; series approximation's per-image polynomial setup is more awkward.
- The hard part is **precision on the GPU** (no native bignum) — hence FractalShark's `2x32` deltas, NTT bignum for the reference, and extended-exponent float types.

---

## 8. Coloring

### Smooth (continuous) coloring — *this crate now implements this*

**Confidence: high.** Source: [Linas Vepstas](https://linas.org/art-gallery/escape/escape.html), [Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set).

Integer escape counts produce visible **banding** (a stair-step function). The **renormalized iteration count** removes it:

```
μ = n + 1 − log( log|Z(n)| ) / log 2
```

where `n` is the escape iteration and `|Z(n)|` the modulus just after escape. The `log 2` is the map's degree (general: `… / log P` for `z^P`). Key property: **μ is approximately independent of the escape radius** — squaring the radius adds 1 to `n` while `−log₂(log|Z|)` drops by 1, cancelling. So a large escape radius (smoother result) needs no color recalibration.

> **This crate's implementation (current):** the SIMD kernel stores raw escape data — two `f32`s per pixel, `count` (escape iteration + 1, or `0.0` in-set) and `mag` (`|z|²` at escape) — in an `Rg32Float` texture. `screen_shader.wgsl` finishes μ on the GPU as `μ = count − log₂(½·ln mag)`, only for visible pixels, then applies the `pow(norm, 0.4)` curve and edge-darkening. This deliberately keeps the kernel **free of transcendentals** — doing the `ln` CPU-side cost 2–4× (8 scalar `ln`s/pixel); the GPU does it nearly free. Per the repo conventions, don't move μ back into the kernel.

### Exterior distance estimation (DE)

**Confidence: high.** Source: [Inigo Quilez](https://iquilezles.org/articles/mandelbrot/).

For crisp boundaries / filament rendering, estimate the distance to the set:

```
d = G / |∇G|            (G = Hubbard–Douady potential / Green's function)
practical estimator:  d ≈ |z| · log|z| / |z'|
```

This requires iterating the **derivative** `z'` alongside `z`. For `z² + c`:

```
z'_{n+1} = 2·z_n·z'_n + 1       (general z^p:  z'_{n+1} = p·z_n^{p-1}·z'_n + 1)
```

Cheap (one extra complex multiply-add per iteration) and gives a scalar distance per pixel you can threshold or shade. The rigorous tighter bound is `sinh(G)/(2|∇G|)`; `G/|∇G|` is its correct leading-order estimate and the universally used one. DE also doubles as the antidote to Mariani–Silver's cusp-omission flaw (§6).

---

## Where this fits the `fractal` crate

Your current architecture (CPU `portable_simd` kernel, `f64` `DRect` coordinates, `MAX_ITER = 4500`, `Rg32Float` `count`+`mag` texels with **GPU-side smooth μ**, Mariani–Silver tile fill, GPU palette/reprojection) is a well-built **shallow-to-mid-zoom** engine with smooth coloring already done. The research maps onto this upgrade ladder:

| If you want… | Status / Add | Where |
|---|---|---|
| Smooth, band-free color | ✅ **done** — μ from `count`+`mag`, GPU-side (§8) | `screen_shader.wgsl` |
| Faster interior/exterior fill | ✅ have Mariani–Silver — keep the fill test on **integer** escape counts, not μ; raise `MAX_ITER` to avoid missed features (§6) | tile fill |
| Crisp filaments / DE shading | derivative iteration `z' → 2zz' + 1`; would need a third channel or a separate pass (§8) | `mandelbrot_simd.rs` + shader |
| **Zoom past ~10⁻¹⁵** (the `f64` wall) | **perturbation** (§1) — the gateway; everything else depends on it | new high-precision reference path (`rug`/MPFR) + delta kernel |
| Deep-zoom speed on top of perturbation | **BLA** (§3) — 2M-entry table, rebuilt per location | reference-orbit table + per-tile delta lookup |
| Glitch-free deep zoom | **rebasing** (§4) — one reference for the whole set; `|Z_m+z_n|<|z_n|` ⇒ reset | reference management |
| Extended depth past `f64` exponent | `floatexp`-style `(f64, i32-exponent)` deltas, then double-double for more mantissa (§5) | delta arithmetic types |

**The single biggest leap** is perturbation: it's the difference between a renderer that dies at ~`1e-15` zoom and one that reaches `1e-100`+. Everything in §2–§5 is an optimization *of* perturbation, not a substitute for it. A pragmatic deep-zoom path for this crate: arbitrary-precision reference orbit → `f64` deltas with **rebasing** (cheap, one reference, no `G`-tuning) → add **BLA** only if per-pixel iteration becomes the bottleneck → add `floatexp` deltas when you cross ~1e300.

---

## Caveats

- **Source concentration.** Most findings rest on **Claude Heiland-Allen** (`mathr.co.uk`, Fraktaler-3/Kalles Fraktaler) and primary project repos — the recognized authority, but cross-author primary corroboration for BLA specifics is thin. *(This "Claude" is the fractal mathematician, unrelated to Anthropic.)*
- **Benchmarks are self-reported, single-config.** FractalShark's ~10×/~30×/~9× and series approximation's ~10×/~100× figures are hardware/scene/depth-dependent and unreproduced by third parties — indicative, not validated.
- **Mariani–Silver caveats rest on one secondary source** (Mu-Ency), though the algorithm is classic and uncontroversial; its non-exactness (missed cusps / low-max-dwell features) is the part to keep in mind.
- **The math is the robust part.** Perturbation recurrence, rebasing condition, smooth coloring, and distance estimation are timeless and independently re-derivable; the engineering benchmarks are the soft part.
- **Citation correction (2nd pass):** the URL `mathr.co.uk/blog/2013-12-24_superfractalthing_maths.html` cited by some secondary writeups is **dead (HTTP 404)**. The live primary sources for SuperFractalThing math are the [fractalforums.com announcement](https://www.fractalforums.com/announcements-and-news/superfractalthing-arbitrary-precision-mandelbrot-set-rendering-in-java/), [mathr.co.uk/web/m-perturbation.html](https://mathr.co.uk/web/m-perturbation.html), and `mathr.co.uk/mandelbrot/perturbation.pdf`. (mathr's blog is marked "no longer updated.")
- **Refuted claims (excluded):** the `+c`-omitting delta recurrence (WikiBooks); the "bilinear = linear-in-two-variables" etymology sub-claim.

## Open questions (still genuinely open after two passes)

1. **Is BLA actually faster than series approximation / NanoMB?** No primary head-to-head benchmark exists; mathr explicitly deferred the verdict. knighty's NanoMB internals weren't located in primary sources.
2. **Does rebasing fully retire glitch-threshold `G` selection?** Sources show rebasing supersedes it for "well-behaved formulas" but stop short of declaring `G`-tuning universally dead.
3. **Mariani–Silver × perturbation+BLA × smooth coloring at deep zoom** — each piece is verified independently, but no primary source addresses the *combination* directly.
4. **Not surfaced in primary sources this pass** (requested but unverified): period detection / **atom domains**, **Newton's method for minibrot location**, **scaled double iteration**, automatic reference selection, and Imagina / Mandel Machine internals. Treat any specifics on these as unverified until checked against `mathr.co.uk` source / fractalforums threads.

---

## Sources

**Primary / authoritative**
- Claude Heiland-Allen — [Deep zoom theory and practice (2021)](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html)
- Claude Heiland-Allen — [Deep zoom theory and practice again — BLA & rebasing (2022)](https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html)
- Claude Heiland-Allen — [m-perturbation reference](https://mathr.co.uk/web/m-perturbation.html) · [Fraktaler-3 source](https://code.mathr.co.uk/fraktaler-3) · [Fraktaler-3 docs](https://fraktaler.mathr.co.uk/)
- K.I. Martin (mrflay) — [SuperFractalThing announcement (fractalforums.com, 2013)](https://www.fractalforums.com/announcements-and-news/superfractalthing-arbitrary-precision-mandelbrot-set-rendering-in-java/)
- [FractalShark (GitHub, CUDA, GPLv3)](https://github.com/mattsaccount364/FractalShark) · [releases](https://github.com/mattsaccount364/FractalShark/releases)
- Inigo Quilez — [Mandelbrot set / distance estimation](https://iquilezles.org/articles/mandelbrot/)
- Linas Vepstas — [Renormalizing the Mandelbrot escape (smooth coloring)](https://linas.org/art-gallery/escape/escape.html)

**Secondary / educational**
- [Plotting algorithms for the Mandelbrot set — Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set)
- [Mariani–Silver — Mu-Ency (mrob.com)](http://www.mrob.com/pub/muency/marianisilveralgorithm.html) · [Wikipedia](https://en.wikipedia.org/wiki/Mariani%E2%80%93Silver_algorithm)
- [DeepDrill — Perturbation theory docs](https://dirkwhoffmann.github.io/DeepDrill/docs/Theory/Perturbation.html)
- [Fractals/perturbation — WikiBooks](https://en.wikibooks.org/wiki/Fractals/perturbation) *(omits the mandatory `+c` term — use with care)*

**Practitioner blogs**
- Phil Thompson — [Perturbation theory (2022)](https://philthompson.me/2022/Perturbation-Theory-and-the-Mandelbrot-set.html) · [BLA (2023)](https://philthompson.me/2023/Faster-Mandelbrot-Set-Rendering-with-BLA-Bivariate-Linear-Approximation.html)
- [Ambrose Cavalier — GPU deep zoom](https://ambrosecavalier.com/projects/gpu-deep-zoom/about/)
- [Mike Kohn — Mandelbrot SIMD](https://www.mikekohn.net/software/mandelbrots_simd.php) · [bumbershootsoft — Optimizing Mandelbrot with SIMD (2024)](https://bumbershootsoft.wordpress.com/2024/01/27/optimizing-mandelbrot-generation-with-simd/)

*Fact-check stats — Pass 1: 5 angles · 18 sources · 80 claims · 25 verified · 24 confirmed, 1 killed. Pass 2 (verification + expansion): 5 angles · 17 sources · 60 claims · 25 verified · 24 confirmed, 1 killed; all 8 core claims re-confirmed with corrections folded in above.*
