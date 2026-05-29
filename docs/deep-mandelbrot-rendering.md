# Deep & Optimal Mandelbrot Rendering

*A researched survey of the algorithms behind fast, deep-zoom Mandelbrot rendering — perturbation theory, series approximation, bilinear approximation (BLA), glitch handling, precision tradeoffs, SIMD/GPU compute, and coloring. Each section is tied back to where it fits this crate (`fractal`).*

Compiled 2026-05-29. Findings were fan-out web-searched, then adversarially fact-checked (3-vote, 2/3-to-kill). Confidence and sources are noted per section. The dominant primary authority in this niche is Claude Heiland-Allen (`mathr.co.uk`, Kalles Fraktaler) — see [Caveats](#caveats).

---

## TL;DR

- **Deep zoom is bounded by precision, not iteration count.** Past ~10⁻¹⁵ (the limit of `f64`), every pixel would need arbitrary-precision arithmetic — far too slow. **Perturbation theory** sidesteps this: compute *one* high-precision reference orbit, then iterate every pixel as a small `f64`/`f32` *delta* off that reference. ~100× speedup. This is the single most important technique and the foundation everything else builds on.
- **Two acceleration layers stack on top of perturbation:** *series approximation* (skip early iterations via a Taylor polynomial in `c`) and the newer, now-favored *bilinear approximation / BLA* (skip many iterations at once via a precomputed hierarchical table). BLA is simpler, more parallelizable, and generalizes to other formulas.
- **Robustness** is handled either reactively (Pauldelbrot's glitch criterion + retry) or proactively (*rebasing*, which needs only one reference for the whole Mandelbrot set).
- **This crate currently targets shallow-to-mid zoom** (`f64`, SIMD, Mariani–Silver tile fill). None of the deep-zoom machinery is needed until you cross the `f64` precision floor — but when you do, perturbation is the gateway. See [Where this fits the `fractal` crate](#where-this-fits-the-fractal-crate).

---

## 1. Perturbation theory — the foundation

**Confidence: high.** Sources: [mathr 2021](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html), [DeepDrill](https://dirkwhoffmann.github.io/DeepDrill/docs/Theory/Perturbation.html), [Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set).

The standard iteration `z → z² + c` loses all meaning at deep zoom because the pixel-to-pixel difference in `c` is smaller than `f64` epsilon relative to `z`. Perturbation rewrites the problem in terms of deviations from a single reference.

Let `Z` be a **high-precision reference orbit** (uppercase) for a reference point `C`, and let each pixel be `C + c` with a per-pixel **delta orbit** `z` (lowercase), so the true orbit is `Z + z`. Expanding `(Z + z)² + (C + c)` and subtracting `Z² + C` gives the **exact** delta recurrence:

```
z_{n+1} = 2·Z_n·z_n + z_n² + c
```

(equivalently `Δ_{n+1} = 2·X_n·Δ_n + Δ_n² + Δ_0`, with `Δ_0 = c`, the per-pixel offset in `c`).

**Why it's fast:** the reference orbit is computed *once* in arbitrary precision (e.g. MPFR/MPIR). Every pixel's delta `z` stays small enough that ~16 significant figures — hardware `f64`, or even `f32` — suffice. You replace millions of arbitrary-precision pixel iterations with one arbitrary-precision reference plus millions of cheap `f64` delta iterations. Speedup is roughly two orders of magnitude.

- **The `+ c` term is mandatory.** A variant circulating on WikiBooks dropping the constant perturbation term (`z_{n+1} = 2·Z_n·z_n + z_n²`) was **refuted 0-3** in fact-checking. Do not use it.
- Popularized by **K.I. Martin's 2013 SuperFractalThing**; now universal in deep-zoom renderers.
- The reference point should ideally be *inside* the set or near the region of interest so its orbit stays bounded and representative; poor reference choice is the root cause of glitches (§4).

---

## 2. Series approximation — skip the early iterations

**Confidence: high.** Sources: [mathr 2021](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html), [WikiBooks](https://en.wikibooks.org/wiki/Fractals/perturbation), [Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set).

For the first many iterations, the delta orbit `z` behaves almost linearly in `c`. Series approximation represents it as a **truncated Taylor polynomial in `c`**:

```
z_n ≈ Σ_k  A_{n,k} · c^k        (A_{n+1,1} = 2·Z_n·A_{n,1} + 1, etc.)
```

The coefficients `A_{n,k}` depend **only on the reference orbit**, so they're computed *once per image*. To render a pixel, you evaluate the polynomial at that pixel's `c` to jump straight to a later iteration `n`, skipping all the per-pixel work up to that point. Iteration then proceeds with the normal perturbation recurrence (§1).

- Typical **~10× speedup** at depth ~10¹⁰⁰; **~100× combined** with perturbation. (Figures are "typical" and scene-dependent.)
- The risk is over-skipping: the polynomial is only valid while higher-order terms stay negligible. You pick the skip iteration conservatively, or validate it.
- **Biseries variants** (`NanoMB1` / `NanoMB2` in Kalles Fraktaler, by knighty) skip *whole periods* near minibrots for extreme zooms.

---

## 3. Bilinear approximation (BLA) — the current state of the art

**Confidence: high.** Source: [mathr 2022](https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html) (originated by Zhuoran on fractalforums); corroborated by [philthompson.me 2023](https://philthompson.me/2023/Faster-Mandelbrot-Set-Rendering-with-BLA-Bivariate-Linear-Approximation.html).

BLA is the modern alternative to series approximation. The insight: when the delta `z` is small, the **z² term is negligible**, so the recurrence is approximately *linear*:

```
z_{n+1} ≈ 2·Z_n·z_n + c          (valid when z² ≪ 2·Z_n·z + c, within a validity radius r)
```

A single linear step is `z → A·z + B·c`. Two consecutive linear steps compose into another linear step — so you can **merge** them. BLA precomputes a table of these merged steps:

- A reference orbit of `M` iterations yields a **2M-entry table** built by hierarchically merging neighbouring 1-step approximations (`M + M/2 + M/4 + … = 2M`), a binary merge tree.
- At render time, for each pixel you pick the **largest valid merged step** (whose validity radius `r` contains the current `z`) and skip that many iterations *at once*, falling back to single perturbation steps when no merged step is valid.

**Why BLA is favored over series approximation:**

1. Conceptually simpler.
2. Easier to implement and parallelize (the table is shared; per-pixel lookups are independent — GPU-friendly).
3. Better-understood stopping/validity conditions.
4. Generalizes to other formulas (Burning Ship, hybrids), where series approximation is awkward.

> **Caveat:** the authoritative source explicitly *withholds* a definitive speed verdict ("need benchmarks before declaring an overall winner"). BLA's edge is simplicity / generality / parallelizability, not a *proven* universal speedup. Treat "BLA is faster" as plausible-but-unsettled. See [Open questions](#open-questions).

---

## 4. Glitch handling & reference selection

**Confidence: high.** Sources: [mathr 2021](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html), [mathr 2022](https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html).

When a pixel's true orbit diverges too far from the reference, the small-delta assumption breaks and you get a **glitch** (visibly wrong pixels, often blobs). Two strategies:

### Reactive — Pauldelbrot's criterion (2014)

Flag a pixel as glitched when:

```
|Z_n + z_n|²  <  G · |Z_n|²        (G typically 1e-2 … 1e-8)
```

i.e. the full value has shrunk so close to the reference that precision is lost. It's **nearly free**: `|Z + z|²` is already computed for the escape test, and `G·|Z|²` can be precomputed per reference iteration. Glitched pixels are **retried** with a new or rebased reference (often one chosen near the glitch cluster).

### Proactive — rebasing (Zhuoran)

Instead of detecting and patching, **rebasing** resets which reference iteration a pixel is measured against when its delta grows too large, keeping deltas small by construction. Crucially, rebasing needs only **as many reference orbits as the formula has critical points** — for the Mandelbrot set (and Burning Ship), that's **one**. This largely *avoids* glitches rather than chasing them, and pairs naturally with BLA in modern renderers.

> **Open problem (per the primary source):** how to choose `G` optimally is unsettled — which is part of why rebasing is attractive.

---

## 5. Precision & arithmetic representations

**Confidence: high (FractalShark specifics).** Source: [FractalShark README](https://github.com/mattsaccount364/FractalShark).

The reference orbit needs *real* arbitrary precision; the deltas don't. The interesting engineering is in choosing the cheapest representation that survives each zoom depth:

| Representation | Use | Notes |
|---|---|---|
| `f64` (hardware) | deltas at shallow/mid zoom | ~15–16 sig figs; the per-pixel workhorse |
| `f32` (hardware) | deltas where precision allows | doubles SIMD/GPU lane count |
| **double-double / "2×32"** | extended-precision deltas | two floats + shared exponent. FractalShark's GPU `2x32` gives ~**48-bit mantissa** — fills the gap between `f32` and full bignum on the GPU |
| floats with extended exponent | very deep deltas | `f64` runs out of *exponent* range (~10⁻³⁰⁸) before mantissa at extreme depth; a separate `i64` exponent fixes this |
| arbitrary precision (MPFR/MPIR/NTT) | reference orbit only | computed once per image |

**FractalShark** (GPU, CUDA, GPLv3) is a notable data point on the high end:
- Two CUDA implementations of (bi)linear approximation, ported from FractalZoomer.
- The custom `2x32` float for GPU perturbation arithmetic (~48-bit mantissa).
- An **NTT-based high-precision GPU multiply** whose reference-orbit computation at 16384 32-bit limbs reportedly **outperforms multithreaded MPIR + AVX-2 by ~10×** on an RTX 4090.

> **Caveat:** that 10× is a self-reported, single-config benchmark with no disclosed methodology. Don't treat it as guaranteed.

---

## 6. SIMD & CPU vectorization (shallow/mid zoom — what this crate does now)

**Confidence: medium** (practitioner blogs). Sources: [Mike Kohn](https://www.mikekohn.net/software/mandelbrots_simd.php), [bumbershootsoft 2024](https://bumbershootsoft.wordpress.com/2024/01/27/optimizing-mandelbrot-generation-with-simd/).

Before precision forces perturbation, the game is raw throughput of `z² + c`:

- **Lane-parallel iteration.** Pack N pixels (8× `f64` with AVX-512, or `Simd<f64, 8>` via `portable_simd` — exactly what `mandelbrot_simd.rs` does) and iterate them together. **This crate already does this.**
- **The escape-divergence problem.** Lanes escape at different iteration counts, but SIMD must step in lockstep. You keep iterating until *all* lanes escape (or hit the cap), masking out escaped lanes so they stop accumulating. A per-lane active mask updates the counts only where still iterating.
- **Periodicity checking.** Points inside the set never escape, costing the full `MAX_ITER`. Detecting that an orbit has entered a cycle (or using cardioid/period-2-bulb membership tests) lets you bail early. Trades branchy scalar logic against clean SIMD lockstep — often *not* worth it inside a tight SIMD kernel, but big for interior-heavy views.
- **Unrolling the inner loop** (this crate does chunked unrolling) amortizes loop overhead and the periodic cancel-token check.

### Mariani–Silver / boundary tracing (this crate's tile fill)

**Confidence: medium.** Source: [Wikipedia](https://en.wikipedia.org/wiki/Mariani%E2%80%93Silver_algorithm).

The Mandelbrot set is **connected**, so if the entire *boundary* of a rectangle maps to the same iteration count, the whole interior must too — fill it without computing a single interior pixel. Mariani–Silver:

1. Compute the border of a tile/rectangle.
2. If the border is uniform → flood-fill the interior with that value.
3. Otherwise subdivide into quadrants and recurse.

This is a huge win for the large solid regions (interior, far exterior) and is what your recent "Mariani–Silver tile fill optimization" commit added. **Important nuance the verified research flagged as an open question:** how Mariani–Silver interacts with perturbation/BLA at deep zoom is *not* well-documented. The uniform-border guarantee rests on exact iteration counts; with smooth coloring or perturbation glitches, "uniform border" needs care (use exact integer counts for the fill test, color separately). Keep the fill decision on integer escape counts only.

---

## 7. GPU compute

**Confidence: medium.** Sources: [FractalShark](https://github.com/mattsaccount364/FractalShark), [Ambrose Cavalier](https://ambrosecavalier.com/projects/gpu-deep-zoom/about/).

- **Shallow/mid zoom on GPU** is embarrassingly parallel: one thread per pixel, `f32`/`f64` iteration. This crate uses the GPU only for the *texture→palette→reprojection* path, not iteration — a reasonable split given the CPU SIMD kernel.
- **Deep zoom on GPU** means porting perturbation + BLA: upload the reference orbit and the BLA table once, then run per-pixel delta iteration in a compute shader. BLA's independent per-pixel table lookups make it GPU-friendly; series approximation's per-image polynomial setup is more awkward.
- The hard part is **precision on the GPU** (no native bignum) — hence FractalShark's `2x32` and NTT-multiply work, and extended-exponent float types.

---

## 8. Coloring

### Smooth (continuous) coloring

**Confidence: high.** Source: [Linas Vepstas](https://linas.org/art-gallery/escape/escape.html).

Integer escape counts produce visible **banding** (a stair-step function). The **renormalized iteration count** removes it:

```
μ = n + 1 − log( log|Z(n)| ) / log 2
```

where `n` is the escape iteration and `|Z(n)|` the modulus just after escape. The `log 2` is the map's degree (general form: `… / log P` for `z^P`). Key property: **μ is approximately independent of the escape radius** — squaring the radius adds 1 to `n` while the `−log₂(log|Z|)` term drops by 1, cancelling. So you can use a large escape radius (better smoothness) without recalibrating colors.

> This crate currently does a shader-side `pow(norm, 0.4)` curve on the raw `u16` count. Switching to μ-based smoothing would need either a `f32` texture (instead of `u16`) or computing μ in the kernel and storing a fixed-point fractional part. The CLAUDE.md note about keeping the texture as a single `u16` iteration count is the relevant constraint — smooth coloring is the main reason you'd revisit it.

### Exterior distance estimation (DE)

**Confidence: high.** Source: [Inigo Quilez](https://iquilezles.org/articles/mandelbrot/).

For crisp boundaries / anti-aliasing-free filament rendering, estimate the distance to the set:

```
d = G / |∇G|            (G = Hubbard–Douady potential / Green's function)
practical estimator:  d ≈ |z| · log|z| / |z'|
```

This requires iterating the **derivative** `z'` alongside `z`. For `z² + c`:

```
z'_{n+1} = 2·z_n·z'_n + 1       (general z^p:  z'_{n+1} = p·z_n^{p-1}·z'_n + 1)
```

Cheap (one extra complex multiply-add per iteration) and gives a scalar distance per pixel you can threshold or shade. The rigorous tighter bound is `sinh(G)/(2|∇G|)`; `G/|∇G|` is its correct leading-order estimate and the universally used one.

---

## Where this fits the `fractal` crate

Your current architecture (CPU `portable_simd` kernel, `f64` `DRect` coordinates, `MAX_ITER = 4500`, GPU only for palette/reprojection, Mariani–Silver tile fill) is a well-built **shallow-to-mid-zoom** engine. The research maps onto a clear upgrade ladder:

| If you want… | Add | Where |
|---|---|---|
| Smooth, band-free color | μ renormalized count (§8) | kernel output + `screen_shader.wgsl`; needs richer than `u16` texel or packed fractional |
| Crisp filaments / DE shading | derivative iteration `z' → 2zz' + 1` (§8) | `mandelbrot_simd.rs` inner loop |
| Faster interior/exterior fill | already have Mariani–Silver — keep fill test on integer counts (§6) | done |
| **Zoom past ~10⁻¹⁵** (the `f64` wall) | **perturbation** (§1) — the gateway; everything else depends on it | new high-precision reference path + delta kernel |
| Deep-zoom speed on top of perturbation | **BLA** (§3) — simpler/more parallel than series approx | reference-orbit table + per-tile delta lookup |
| Glitch-free deep zoom | **rebasing** (§4) — one reference for the whole set | reference management |
| Extended depth past `f64` exponent | double-double / extended-exponent deltas (§5) | delta arithmetic types |

**The single biggest leap** is perturbation: it's the difference between a renderer that dies at ~`1e-15` zoom and one that reaches `1e-100`+. Everything in §2–§5 is an optimization *of* perturbation, not a substitute for it.

---

## Caveats

- **Source concentration.** Roughly half the corpus traces to one primary author, **Claude Heiland-Allen** (`mathr.co.uk`, Kalles Fraktaler) — the recognized domain authority, but cross-author primary corroboration for BLA specifics is thin (one mathr article + secondary echoes). *(This "Claude" is the fractal mathematician, unrelated to Anthropic.)*
- **Performance numbers are self-reported, single-config.** The ~10× (FractalShark NTT) and ~10×/~100× (series approximation) figures are scene/hardware/depth-dependent — not guarantees.
- **Fast-moving niche.** BLA (2021–2022, Zhuoran) has largely superseded series approximation in *newer* renderers, while older-but-shipping tools (Kalles Fraktaler 2+) remain on series approximation — so "state of the art" depends on which implementation you mean.
- **The math is the robust part.** Perturbation recurrence, smooth coloring, and distance estimation are timeless and independently re-derivable; the engineering benchmarks are the soft part.
- **One claim was refuted (0-3) and excluded:** the z²-and-`+c`-omitting delta recurrence from WikiBooks.

## Open questions

1. **Optimal glitch threshold `G`** (1e-2…1e-8) — or is it made moot by rebasing? The primary source leaves this open.
2. **Is BLA actually faster than series approximation/NanoMB**, or only simpler/more general/more parallel? The authority withholds a verdict; no neutral head-to-head exists.
3. **Double-double / arbitrary-precision vs GPU NTT/2×32 tradeoffs across zoom depths** — when does each representation become *necessary*? Not covered by a general precision-vs-depth analysis.
4. **How Mariani–Silver interacts with perturbation/BLA at deep zoom** — appears in this codebase but the verified sources didn't address it. Likely fine if the fill test stays on exact integer counts, but unvalidated.

---

## Sources

**Primary / authoritative**
- Claude Heiland-Allen — [Deep zoom theory and practice (2021)](https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html)
- Claude Heiland-Allen — [Deep zoom theory and practice again — BLA (2022)](https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html)
- Claude Heiland-Allen — [Kalles Fraktaler](https://mathr.co.uk/kf/kf.html)
- [FractalShark (GitHub, CUDA, GPLv3)](https://github.com/mattsaccount364/FractalShark)
- Inigo Quilez — [Mandelbrot set / distance estimation](https://iquilezles.org/articles/mandelbrot/)
- Linas Vepstas — [Renormalizing the Mandelbrot escape (smooth coloring)](https://linas.org/art-gallery/escape/escape.html)

**Secondary / educational**
- [Plotting algorithms for the Mandelbrot set — Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set)
- [Mariani–Silver algorithm — Wikipedia](https://en.wikipedia.org/wiki/Mariani%E2%80%93Silver_algorithm)
- [DeepDrill — Perturbation theory docs](https://dirkwhoffmann.github.io/DeepDrill/docs/Theory/Perturbation.html)
- [Fractals/perturbation — WikiBooks](https://en.wikibooks.org/wiki/Fractals/perturbation) *(one variant here was refuted; use with care)*

**Practitioner blogs**
- Phil Thompson — [Perturbation theory (2022)](https://philthompson.me/2022/Perturbation-Theory-and-the-Mandelbrot-set.html) · [BLA (2023)](https://philthompson.me/2023/Faster-Mandelbrot-Set-Rendering-with-BLA-Bivariate-Linear-Approximation.html)
- [Ambrose Cavalier — GPU deep zoom](https://ambrosecavalier.com/projects/gpu-deep-zoom/about/)
- [Mike Kohn — Mandelbrot SIMD](https://www.mikekohn.net/software/mandelbrots_simd.php)
- [bumbershootsoft — Optimizing Mandelbrot with SIMD (2024)](https://bumbershootsoft.wordpress.com/2024/01/27/optimizing-mandelbrot-generation-with-simd/)

*Fact-check stats: 5 search angles · 18 sources fetched · 80 claims extracted · 25 verified (3-vote adversarial) · 24 confirmed, 1 killed · 8 findings after synthesis.*
