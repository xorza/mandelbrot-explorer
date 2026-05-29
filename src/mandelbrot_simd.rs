#![allow(non_camel_case_types)]

use std::simd::Select;
use std::simd::StdFloat;
use std::simd::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};
use glam::UVec2;

use crate::math::DRect;

const SIMD_LANE_COUNT: usize = 8;
pub const MAX_ITER: u32 = 4500;

type f64simd = Simd<f64, SIMD_LANE_COUNT>;
type i64simd = Simd<i64, SIMD_LANE_COUNT>;
type CountSimd = [Pixel; SIMD_LANE_COUNT];

/// Raw escape data for one pixel. Smooth (μ) coloring is finished on the GPU,
/// which has hardware `log2` and only pays it for visible pixels: the screen
/// shader computes μ = `count − log₂(½·ln mag)`. `count == 0.0` is the in-set
/// sentinel (never escaped within the iteration cap).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct Pixel {
    /// Escape iteration + 1 (so escaped pixels are ≥ 1), or 0.0 for in-set.
    pub(crate) count: f32,
    /// |z|² captured on the escape iteration (undefined when `count == 0`).
    pub(crate) mag: f32,
}

/// `Pixel` plus the derivative magnitude for exterior **distance estimation**:
/// the GPU computes `d ≈ √mag · ½·ln mag / √dmag` (≈ |z|·ln|z|/|z'|), the
/// distance to the set in fractal units. Produced only by `mandelbrot_simd_de`
/// (a quality pass) — the derivative iteration is too heavy for the interactive
/// kernel, so the fast path stays on `Pixel`. `count == 0.0` is the in-set
/// sentinel; the 4th field pads to a GPU `Rgba32Float` texel.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct DePixel {
    pub(crate) count: f32,
    pub(crate) mag: f32,
    /// |z'|² captured on the escape iteration (undefined when `count == 0`).
    pub(crate) dmag: f32,
    pub(crate) _pad: f32,
}

const LANE_RAMP: [f64; SIMD_LANE_COUNT] = {
    let mut r = [0.0; SIMD_LANE_COUNT];
    let mut i = 0;
    while i < SIMD_LANE_COUNT {
        r[i] = i as f64;
        i += 1;
    }
    r
};

/// Fractal-space coordinate of `SIMD_LANE_COUNT` consecutive pixels starting at
/// pixel `base` along an axis spanning `span` over `n` pixels from `pos`.
fn axis_lanes(pos: f64, span: f64, n: u32, base: u32) -> f64simd {
    (f64simd::from_array(LANE_RAMP) + f64simd::splat(base as f64)) * f64simd::splat(span / n as f64)
        + f64simd::splat(pos)
}

/// Fractal-space coordinate of a single pixel `i` along the same axis. Shared
/// with the perturbation kernel so both map pixels to coordinates identically.
pub(crate) fn axis_scalar(pos: f64, span: f64, n: u32, i: u32) -> f64 {
    pos + span * (i as f64 / n as f64)
}

//noinspection RsConstantConditionIf
/// Renders `tile_rect` (in fractal space) into `buffer` at `tile_size` pixels.
/// Returns `false` if `cancel_token` was raised before completion.
pub fn mandelbrot_simd(
    tile_rect: DRect,
    tile_size: UVec2,
    max_iterations: u32,
    cancel_token: Arc<AtomicBool>,
    buffer: &mut [Pixel],
) -> bool {
    assert_eq!(buffer.len(), (tile_size.x * tile_size.y) as usize);
    assert_eq!(tile_size.x % SIMD_LANE_COUNT as u32, 0);
    // μ ≈ max_iterations is stored as f32; the cap must stay within f32's exact
    // integer range. `MAX_ITER` (4500) is far under this.
    debug_assert!(max_iterations < (1 << 24));

    for y in 0..tile_size.y {
        if cancel_token.load(Ordering::Relaxed) {
            return false;
        }
        let cy = f64simd::splat(axis_scalar(
            tile_rect.pos.y,
            tile_rect.size.y,
            tile_size.y,
            y,
        ));
        for x in 0..tile_size.x / SIMD_LANE_COUNT as u32 {
            let cx = axis_lanes(
                tile_rect.pos.x,
                tile_rect.size.x,
                tile_size.x,
                x * SIMD_LANE_COUNT as u32,
            );

            let values_simd = pixel(max_iterations, cx, cy);
            let idx = (y * tile_size.x + x * SIMD_LANE_COUNT as u32) as usize;
            buffer[idx..idx + SIMD_LANE_COUNT].copy_from_slice(values_simd.as_slice());
        }
    }

    true
}

/// Distance-estimation render of `tile_rect`: like `mandelbrot_simd`, but also
/// iterates the derivative `z'` and emits `dmag` (|z'|² at escape) per pixel for
/// crisp boundary/filament shading. Heavier than the plain kernel (a complex
/// multiply-add per iteration), so it's a quality pass, not the interactive one.
/// Returns `false` if `cancel_token` was raised before completion.
pub fn mandelbrot_simd_de(
    tile_rect: DRect,
    tile_size: UVec2,
    max_iterations: u32,
    cancel_token: Arc<AtomicBool>,
    buffer: &mut [DePixel],
) -> bool {
    assert_eq!(buffer.len(), (tile_size.x * tile_size.y) as usize);
    assert_eq!(tile_size.x % SIMD_LANE_COUNT as u32, 0);
    debug_assert!(max_iterations < (1 << 24));

    for y in 0..tile_size.y {
        if cancel_token.load(Ordering::Relaxed) {
            return false;
        }
        let cy = f64simd::splat(axis_scalar(
            tile_rect.pos.y,
            tile_rect.size.y,
            tile_size.y,
            y,
        ));
        for x in 0..tile_size.x / SIMD_LANE_COUNT as u32 {
            let cx = axis_lanes(
                tile_rect.pos.x,
                tile_rect.size.x,
                tile_size.x,
                x * SIMD_LANE_COUNT as u32,
            );
            let values = pixel_de(max_iterations, cx, cy);
            let idx = (y * tile_size.x + x * SIMD_LANE_COUNT as u32) as usize;
            buffer[idx..idx + SIMD_LANE_COUNT].copy_from_slice(&values);
        }
    }

    true
}

/// Outcome of scanning a tile's one-pixel border.
#[derive(Debug, PartialEq, Eq)]
enum BorderScan {
    /// Every border pixel stayed in the set (never escaped).
    AllInSet,
    /// At least one border pixel escaped.
    HasExterior,
    /// `cancel_token` was raised mid-scan.
    Cancelled,
}

/// Renders `tile_rect` into `buffer`, with a Mariani–Silver shortcut: if the
/// tile's one-pixel border is entirely in the set, the Mandelbrot set's simple
/// connectedness means the interior is too, so the tile is filled with 0 without
/// iterating the ~97% of pixels inside the border. Otherwise it falls back to a
/// full render. Returns `false` if cancelled (matching `mandelbrot_simd`).
///
/// The border is sampled at pixel centres, so a sub-pixel exterior filament that
/// slips between two border samples can be missed — the standard Mariani–Silver
/// discretization error, rare and visually tiny at the interior fill level.
pub fn mandelbrot_tile(
    tile_rect: DRect,
    tile_size: UVec2,
    max_iterations: u32,
    cancel_token: Arc<AtomicBool>,
    buffer: &mut [Pixel],
) -> bool {
    assert_eq!(buffer.len(), (tile_size.x * tile_size.y) as usize);

    match scan_border(tile_rect, tile_size, max_iterations, &cancel_token) {
        BorderScan::AllInSet => {
            buffer.fill(Pixel {
                count: 0.0,
                mag: 0.0,
            });
            true
        }
        BorderScan::HasExterior => {
            mandelbrot_simd(tile_rect, tile_size, max_iterations, cancel_token, buffer)
        }
        BorderScan::Cancelled => false,
    }
}

/// Classifies the tile's one-pixel border. A pixel is "in set" iff `pixel`
/// stores 0 (never escaped within `max_iterations`) — the same classification
/// the full render uses, so the fill is byte-identical to brute force whenever
/// the connectedness argument holds.
fn scan_border(
    tile_rect: DRect,
    tile_size: UVec2,
    max_iterations: u32,
    cancel_token: &AtomicBool,
) -> BorderScan {
    assert_eq!(tile_size.x % SIMD_LANE_COUNT as u32, 0);
    assert_eq!(tile_size.y % SIMD_LANE_COUNT as u32, 0);

    if cancel_token.load(Ordering::Relaxed) {
        return BorderScan::Cancelled;
    }

    let any_escaped = |values: &CountSimd| values.iter().any(|p| p.count != 0.0);

    // Top and bottom rows: full SIMD rows, like the main render.
    for y in [0, tile_size.y - 1] {
        let cy = f64simd::splat(axis_scalar(
            tile_rect.pos.y,
            tile_rect.size.y,
            tile_size.y,
            y,
        ));
        for x in 0..tile_size.x / SIMD_LANE_COUNT as u32 {
            let cx = axis_lanes(
                tile_rect.pos.x,
                tile_rect.size.x,
                tile_size.x,
                x * SIMD_LANE_COUNT as u32,
            );
            if any_escaped(&pixel(max_iterations, cx, cy)) {
                return BorderScan::HasExterior;
            }
        }
    }

    // Left and right columns: walk down each edge eight rows at a time, holding
    // the column's `cx` constant across the lanes.
    for x in [0, tile_size.x - 1] {
        let cx = f64simd::splat(axis_scalar(
            tile_rect.pos.x,
            tile_rect.size.x,
            tile_size.x,
            x,
        ));
        for y in 0..tile_size.y / SIMD_LANE_COUNT as u32 {
            let cy = axis_lanes(
                tile_rect.pos.y,
                tile_rect.size.y,
                tile_size.y,
                y * SIMD_LANE_COUNT as u32,
            );
            if any_escaped(&pixel(max_iterations, cx, cy)) {
                return BorderScan::HasExterior;
            }
        }
    }

    BorderScan::AllInSet
}

fn pixel(max_iterations: u32, cx: f64simd, cy: f64simd) -> CountSimd {
    // Cardioid check: q*(q + (x - 0.25)) <= 0.25*y^2
    let cy2 = cy * cy;
    let xm = cx - f64simd::splat(0.25);
    let q = xm * xm + cy2;
    let in_cardioid = (q * (q + xm)).simd_le(f64simd::splat(0.25) * cy2);

    // Period-2 bulb check: (x+1)^2 + y^2 <= 1/16
    let xp1 = cx + f64simd::splat(1.0);
    let in_bulb = (xp1 * xp1 + cy2).simd_le(f64simd::splat(0.0625));

    let in_set = in_cardioid | in_bulb;

    if in_set.all() {
        return [Pixel {
            count: 0.0,
            mag: 0.0,
        }; SIMD_LANE_COUNT];
    }

    let mut zx = f64simd::splat(0.0);
    let mut zy = f64simd::splat(0.0);
    let mut zx2 = f64simd::splat(0.0);
    let mut zy2 = f64simd::splat(0.0);
    let mut cnt = i64simd::splat(0);
    let mut escaped = in_set;
    // |z|² captured on the iteration each lane first escapes, for smooth (μ)
    // coloring; lanes that never escape keep 0.0 and are handled as in-set.
    let mut mag_escape = f64simd::splat(0.0);

    let f64_4_0 = f64simd::splat(4.0);
    let i64_0 = i64simd::splat(0);
    let i64_1 = i64simd::splat(1);

    // One iteration step. `2*zx*zy + cy` is a fused multiply-add (hardware FMA;
    // see .cargo/config.toml for the target-feature requirement).
    macro_rules! step {
        () => {{
            zy = (zx + zx).mul_add(zy, cy);
            zx = zx2 - zy2 + cx;
            zx2 = zx * zx;
            zy2 = zy * zy;
            let mag = zx2 + zy2;
            // Capture |z|² on the escape step: keep the stored value once escaped,
            // else overwrite with the current magnitude. One select, no extra mask.
            mag_escape = escaped.select(mag_escape, mag);
            escaped |= mag.simd_ge(f64_4_0);
            cnt += escaped.select(i64_0, i64_1);
        }};
    }

    // Full chunks of 8 run a const-bound (unrollable) inner loop and amortise
    // the escape reduction to once per chunk; a short remainder finishes the
    // tail when `max_iterations` isn't a multiple of 8.
    let mut i = 0u32;
    let mut all_escaped = false;
    while i + SIMD_LANE_COUNT as u32 <= max_iterations {
        for _ in 0..SIMD_LANE_COUNT {
            step!();
        }
        i += SIMD_LANE_COUNT as u32;
        if escaped.all() {
            all_escaped = true;
            break;
        }
    }
    if !all_escaped {
        while i < max_iterations {
            step!();
            i += 1;
        }
    }

    let max_iter_simd = i64simd::splat(max_iterations as i64);
    cnt = in_set.select(max_iter_simd, cnt);

    let counts = cnt.to_array();
    let mags = mag_escape.to_array();
    std::array::from_fn(|lane| {
        let n = counts[lane];
        if n as u32 == max_iterations {
            // In the set (or never escaped within the cap): sentinel.
            Pixel {
                count: 0.0,
                mag: 0.0,
            }
        } else {
            // Raw escape data; the GPU finishes the smooth μ. `n + 1` keeps
            // escaped pixels ≥ 1, distinct from the 0.0 in-set sentinel.
            Pixel {
                count: (n + 1) as f32,
                mag: mags[lane] as f32,
            }
        }
    })
}

/// As `pixel`, but also carries the derivative `z'` (`dz`) for distance
/// estimation. `dz_{n+1} = 2·z_n·dz_n + 1` is evaluated with the pre-update
/// `z_n`, so `dz` and `z` stay in step; `|z'|²` at escape goes into `dmag`.
fn pixel_de(max_iterations: u32, cx: f64simd, cy: f64simd) -> [DePixel; SIMD_LANE_COUNT] {
    let cy2 = cy * cy;
    let xm = cx - f64simd::splat(0.25);
    let q = xm * xm + cy2;
    let in_cardioid = (q * (q + xm)).simd_le(f64simd::splat(0.25) * cy2);
    let xp1 = cx + f64simd::splat(1.0);
    let in_bulb = (xp1 * xp1 + cy2).simd_le(f64simd::splat(0.0625));
    let in_set = in_cardioid | in_bulb;

    if in_set.all() {
        return [DePixel::default(); SIMD_LANE_COUNT];
    }

    let mut zx = f64simd::splat(0.0);
    let mut zy = f64simd::splat(0.0);
    let mut zx2 = f64simd::splat(0.0);
    let mut zy2 = f64simd::splat(0.0);
    let mut dzx = f64simd::splat(0.0);
    let mut dzy = f64simd::splat(0.0);
    let mut cnt = i64simd::splat(0);
    let mut escaped = in_set;
    let mut mag_escape = f64simd::splat(0.0);
    let mut dmag_escape = f64simd::splat(0.0);

    let f64_4_0 = f64simd::splat(4.0);
    let two = f64simd::splat(2.0);
    let one = f64simd::splat(1.0);
    let i64_0 = i64simd::splat(0);
    let i64_1 = i64simd::splat(1);

    macro_rules! step {
        () => {{
            // dz_{n+1} = 2·z_n·dz_n + 1, using z_n (the pre-update z).
            let ndzx = two * (zx * dzx - zy * dzy) + one;
            let ndzy = two * (zx * dzy + zy * dzx);
            dzx = ndzx;
            dzy = ndzy;

            zy = (zx + zx).mul_add(zy, cy);
            zx = zx2 - zy2 + cx;
            zx2 = zx * zx;
            zy2 = zy * zy;
            let mag = zx2 + zy2;
            mag_escape = escaped.select(mag_escape, mag);
            dmag_escape = escaped.select(dmag_escape, dzx * dzx + dzy * dzy);
            escaped |= mag.simd_ge(f64_4_0);
            cnt += escaped.select(i64_0, i64_1);
        }};
    }

    let mut i = 0u32;
    let mut all_escaped = false;
    while i + SIMD_LANE_COUNT as u32 <= max_iterations {
        for _ in 0..SIMD_LANE_COUNT {
            step!();
        }
        i += SIMD_LANE_COUNT as u32;
        if escaped.all() {
            all_escaped = true;
            break;
        }
    }
    if !all_escaped {
        while i < max_iterations {
            step!();
            i += 1;
        }
    }

    let max_iter_simd = i64simd::splat(max_iterations as i64);
    cnt = in_set.select(max_iter_simd, cnt);

    let counts = cnt.to_array();
    let mags = mag_escape.to_array();
    let dmags = dmag_escape.to_array();
    std::array::from_fn(|lane| {
        let n = counts[lane];
        if n as u32 == max_iterations {
            DePixel::default()
        } else {
            DePixel {
                count: (n + 1) as f32,
                mag: mags[lane] as f32,
                dmag: dmags[lane] as f32,
                _pad: 0.0,
            }
        }
    })
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use std::time::Instant;

    use glam::{DVec2, UVec2};

    use super::*;

    /// Rebuilds the fractal rect the old `(offset, scale)` API produced for a
    /// full-image render, so the rendered output is byte-identical.
    fn full_image_rect(offset: DVec2, scale: f64) -> DRect {
        DRect::from_pos_size(
            DVec2::splat(-0.5) / scale - offset,
            DVec2::splat(1.0 / scale),
        )
    }

    #[test]
    fn escape_counts_match_hand_computed() {
        // A square 8×8 tile over [-2, 2]² in fractal space.
        let tile_size = UVec2::splat(8);
        let tile_rect = DRect::from_pos_size(DVec2::splat(-2.0), DVec2::splat(4.0));
        let max_iterations = 50;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut buffer = vec![Pixel::default(); 64];

        assert!(mandelbrot_simd(
            tile_rect,
            tile_size,
            max_iterations,
            cancel,
            &mut buffer
        ));

        // Pixel (x, y) samples c = (-2 + x*0.5, -2 + y*0.5). The kernel stores
        // raw escape data (count = n+1, mag = |z|² at escape); the GPU finishes
        // μ = count − log₂(½·ln mag). We reproduce that formula here.
        let at = |x: u32, y: u32| buffer[(y * 8 + x) as usize];
        let mu = |p: Pixel| p.count - (0.5 * (p.mag as f64).ln()).log2() as f32;

        // Origin c = (0, 0): inside the set => never escapes => sentinel 0.
        assert_eq!(at(4, 4).count, 0.0, "origin is in the set");

        // c = (-2, -2): z1 = c, |z1|² = 8 ≥ 4 escapes on iter 1 (n = 0).
        let corner = at(0, 0);
        assert_eq!(corner.count, 1.0, "escapes on iteration 1");
        assert!((corner.mag - 8.0).abs() < 1e-9, "mag = {}", corner.mag);
        // μ = 1 − log₂(½·ln 8) = 1 − log₂(1.03972) = 0.94379.
        assert!(
            (mu(corner) - 0.94379).abs() < 1e-4,
            "corner μ = {}",
            mu(corner)
        );

        // c = (0.5, 0): zx → 0.5, 0.75, 1.0625, 1.6289 (4 counted iters), then
        // 3.15331 on the 5th, |z|² = 9.9434. n = 4.
        let cusp = at(5, 4);
        assert_eq!(cusp.count, 5.0, "escapes after 4 counted iterations");
        assert!((cusp.mag - 9.9434).abs() < 1e-3, "mag = {}", cusp.mag);
        // μ = 5 − log₂(½·ln 9.9434) = 5 − log₂(1.14845) = 4.80030.
        assert!((mu(cusp) - 4.80030).abs() < 1e-4, "c=0.5 μ = {}", mu(cusp));
    }

    #[test]
    fn de_derivative_matches_hand_computed() {
        // Same 8×8 tile, distance-estimation kernel: it must reproduce count/mag
        // and additionally the derivative magnitude dmag = |z'|² at escape.
        let tile_size = UVec2::splat(8);
        let tile_rect = DRect::from_pos_size(DVec2::splat(-2.0), DVec2::splat(4.0));
        let max_iterations = 50;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut buffer = vec![DePixel::default(); 64];

        assert!(mandelbrot_simd_de(
            tile_rect,
            tile_size,
            max_iterations,
            cancel,
            &mut buffer
        ));
        let at = |x: u32, y: u32| buffer[(y * 8 + x) as usize];

        // Origin is in the set: sentinel, no derivative.
        assert_eq!(at(4, 4).count, 0.0);

        // c = (-2, -2): escapes on iter 1, z'_1 = 2·z_0·z'_0 + 1 = 1 ⇒ dmag = 1.
        let corner = at(0, 0);
        assert_eq!(corner.count, 1.0);
        assert!(
            (corner.dmag - 1.0).abs() < 1e-9,
            "corner dmag = {}",
            corner.dmag
        );

        // c = (0.5, 0): z' → 1, 2, 4, 9.5, then 2·1.62890625·9.5 + 1 = 31.94922
        // on the escape (5th) step ⇒ dmag = 31.94922² = 1020.75.
        let cusp = at(5, 4);
        assert_eq!(cusp.count, 5.0);
        assert!(
            (cusp.dmag - 1020.75).abs() < 1e-1,
            "c=0.5 dmag = {}",
            cusp.dmag
        );
        // count/mag must still match the plain kernel.
        assert!((cusp.mag - 9.9434).abs() < 1e-3, "c=0.5 mag = {}", cusp.mag);
    }

    #[test]
    fn border_scan_classifies_tiles() {
        let tile_size = UVec2::splat(64);
        let max_iterations = 200;
        let cancel = AtomicBool::new(false);
        let scan = |rect| scan_border(rect, tile_size, max_iterations, &cancel);

        // Deep inside the main cardioid: every border pixel stays in the set.
        let interior = DRect::from_pos_size(DVec2::splat(-0.1), DVec2::splat(0.2));
        assert_eq!(scan(interior), BorderScan::AllInSet);

        // Straddling the western boundary of the set: border crosses into the
        // exterior.
        let boundary = DRect::from_pos_size(DVec2::new(-2.0, -1.0), DVec2::splat(2.0));
        assert_eq!(scan(boundary), BorderScan::HasExterior);

        // Far exterior: everything escapes immediately.
        let exterior = DRect::from_pos_size(DVec2::splat(2.0), DVec2::splat(0.5));
        assert_eq!(scan(exterior), BorderScan::HasExterior);
    }

    #[test]
    fn tile_fill_is_byte_identical_to_full_render() {
        let tile_size = UVec2::splat(64);
        let max_iterations = 200;
        let n = (tile_size.x * tile_size.y) as usize;

        // The fast-path (interior) tile plus two fall-back tiles. For all three,
        // `mandelbrot_tile` must reproduce `mandelbrot_simd` exactly.
        let interior = DRect::from_pos_size(DVec2::splat(-0.1), DVec2::splat(0.2));
        let boundary = DRect::from_pos_size(DVec2::new(-2.0, -1.0), DVec2::splat(2.0));
        let exterior = DRect::from_pos_size(DVec2::splat(2.0), DVec2::splat(0.5));

        for rect in [interior, boundary, exterior] {
            let mut full = vec![Pixel::default(); n];
            let mut tiled = vec![Pixel::default(); n];
            let cancel = Arc::new(AtomicBool::new(false));

            assert!(mandelbrot_simd(
                rect,
                tile_size,
                max_iterations,
                cancel.clone(),
                &mut full
            ));
            assert!(mandelbrot_tile(
                rect,
                tile_size,
                max_iterations,
                cancel,
                &mut tiled
            ));

            let full: Vec<(f32, f32)> = full.iter().map(|p| (p.count, p.mag)).collect();
            let tiled: Vec<(f32, f32)> = tiled.iter().map(|p| (p.count, p.mag)).collect();
            assert_eq!(tiled, full, "tile vs full render differ for {rect:?}");
        }

        // The interior tile must actually take the fill shortcut: all zeros.
        let mut tiled = vec![Pixel::default(); n];
        assert!(mandelbrot_tile(
            interior,
            tile_size,
            max_iterations,
            Arc::new(AtomicBool::new(false)),
            &mut tiled
        ));
        assert!(
            tiled.iter().all(|p| p.count == 0.0),
            "interior tile should be filled in-set"
        );
    }

    #[test]
    #[ignore = "heavy 2048² render; run with: cargo test --release -- --ignored"]
    fn draw_mandelbrot() {
        let image_size = 2048;
        let fractal_offset = DVec2::new(0.10486747136388758, 0.9244368813525663);
        let fractal_scale = 32.0;
        let tile_rect = full_image_rect(fractal_offset, fractal_scale);
        let tile_size = UVec2::splat(image_size);
        let max_iterations = 1024;
        let cancel_token = Arc::new(AtomicBool::new(false));
        let mut buffer = vec![Pixel::default(); (image_size * image_size) as usize];

        let new = Instant::now();
        let retry = 5;

        for _ in 0..retry {
            assert!(mandelbrot_simd(
                tile_rect,
                tile_size,
                max_iterations,
                cancel_token.clone(),
                &mut buffer,
            ));
        }

        let elapsed = new.elapsed();
        println!("Avg elapsed: {}ms", elapsed.as_millis() / retry);

        let mut image = image::ImageBuffer::new(image_size, image_size);
        for y in 0..image_size {
            for x in 0..image_size {
                let index = (y * image_size + x) as usize;
                let pixel = (buffer[index].count as u32 % 256) as u8;
                let color = image::Rgb([pixel, pixel, pixel]);
                image.put_pixel(x, y, color);
            }
        }
        std::fs::create_dir_all("test_output").unwrap();
        image.save("test_output/mandelbrot.png").unwrap();
    }
}
