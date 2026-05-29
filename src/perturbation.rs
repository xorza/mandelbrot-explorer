//! Perturbation rendering — the gateway past the `f64` zoom wall (~1e-15).
//!
//! Instead of iterating `z → z² + c` per pixel (which loses all precision when
//! pixels differ by less than `f64` epsilon), we compute one high-precision
//! **reference orbit** `Z_n` and iterate every pixel as a small `f64` **delta**
//! `δ_n` off it. The exact delta recurrence (from expanding `(Z+δ)² + (C+dc)`):
//!
//! ```text
//! δ_{n+1} = 2·Z_n·δ_n + δ_n² + dc      (δ_0 = 0, dc = c − C)
//! ```
//!
//! Each pixel's true orbit is `Z_n + δ_n`; the delta stays small enough for
//! `f64` even when `c − C` is far below `f64` epsilon — **provided `dc` is built
//! from the small per-pixel offset, never by subtracting two near-equal absolute
//! coordinates** (that would cancel). So this kernel takes a *delta rect* of
//! offsets relative to the reference centre, not absolute fractal coordinates.
//!
//! This module is Step 1: the recurrence with an `f64` reference orbit, which is
//! enough to validate against the direct kernel at shallow zoom. A bignum
//! reference (for actual deep zoom) slots in behind `ReferenceOrbit`.

use glam::{DVec2, UVec2};

use crate::mandelbrot_simd::{Pixel, axis_scalar};
use crate::math::DRect;

const ESCAPE_R2: f64 = 4.0;

/// A reference orbit `Z_0, Z_1, …` stored as `f64`. The high precision needed to
/// *compute* it (at deep zoom) lives in the constructor; the stored values are
/// only ever used as `f64` in the delta recurrence.
#[derive(Debug)]
pub struct ReferenceOrbit {
    /// `z[n]` is `Z_n`. Length is `≤ max_iterations + 1`; shorter if the
    /// reference point itself escaped (then later pixels need rebasing — Step 2).
    pub z: Vec<DVec2>,
}

impl ReferenceOrbit {
    /// Reference orbit for an absolute `f64` centre — the shallow/mid-zoom path,
    /// used to validate the recurrence. Deep zoom uses [`Self::from_center_decimal`].
    pub fn from_center_f64(center: DVec2, max_iterations: u32) -> Self {
        let mut z = Vec::with_capacity(max_iterations as usize + 1);
        let mut p = DVec2::ZERO;
        for _ in 0..=max_iterations {
            z.push(p);
            if p.length_squared() > ESCAPE_R2 {
                break; // reference escaped; orbit ends here
            }
            // p = p² + center
            p = DVec2::new(p.x * p.x - p.y * p.y + center.x, 2.0 * p.x * p.y + center.y);
        }
        Self { z }
    }

    /// Reference orbit for a centre given as decimal strings, iterated in
    /// arbitrary precision (`digits` significant decimal digits) so it stays
    /// accurate far below `f64` epsilon. Only the per-iteration `Z_n` values are
    /// downcast to `f64` for the delta recurrence — this is the deep-zoom path.
    pub fn from_center_decimal(re: &str, im: &str, max_iterations: u32, digits: usize) -> Self {
        use dashu::float::DBig;

        let cr = re.parse::<DBig>().expect("valid decimal");
        let ci = im.parse::<DBig>().expect("valid decimal");
        Self {
            z: orbit_from_dbig(cr, ci, max_iterations, digits),
        }
    }
}

/// Iterates `Z_{n+1} = Z_n² + C` in arbitrary precision (`digits` decimal
/// digits), downcasting each `Z_n` to `f64`. Shared by the decimal constructor
/// and `HpCenter`.
fn orbit_from_dbig(
    cr: dashu::float::DBig,
    ci: dashu::float::DBig,
    max_iterations: u32,
    digits: usize,
) -> Vec<DVec2> {
    use dashu::float::DBig;

    let at_prec = |x: DBig| x.with_precision(digits).value();
    let cr = at_prec(cr);
    let ci = at_prec(ci);
    let two = DBig::from(2u8);

    let mut zr = DBig::ZERO;
    let mut zi = DBig::ZERO;
    let mut z = Vec::with_capacity(max_iterations as usize + 1);
    for _ in 0..=max_iterations {
        let fx = zr.to_f64().value();
        let fy = zi.to_f64().value();
        z.push(DVec2::new(fx, fy));
        if fx * fx + fy * fy > ESCAPE_R2 {
            break;
        }
        // Z' = (zr² − zi² + cr,  2·zr·zi + ci), each rounded back to `digits`.
        // `nzi` reads the old `zr`, so compute both before reassigning.
        let zr2 = at_prec(&zr * &zr);
        let zi2 = at_prec(&zi * &zi);
        let nzr = at_prec(&zr2 - &zi2 + &cr);
        let nzi = at_prec(&two * &zr * &zi + &ci);
        zr = nzr;
        zi = nzi;
    }
    z
}

/// `f64` → decimal `DBig` via its round-trip decimal string (Rust's `Display`
/// never uses scientific notation, so this parses cleanly).
fn dbig_from_f64(v: f64, digits: usize) -> dashu::float::DBig {
    format!("{v}")
        .parse::<dashu::float::DBig>()
        .expect("finite f64 is valid decimal")
        .with_precision(digits)
        .value()
}

/// A view centre held in arbitrary precision so it stays exact far below `f64`
/// epsilon. Pan/zoom accumulate small `f64` deltas into it (which would vanish
/// if added to an `f64` centre at deep zoom), and it produces the reference
/// orbit for the perturbation kernel.
#[derive(Debug, Clone)]
pub struct HpCenter {
    re: dashu::float::DBig,
    im: dashu::float::DBig,
    digits: usize,
}

impl HpCenter {
    pub fn new(center: DVec2, digits: usize) -> Self {
        Self {
            re: dbig_from_f64(center.x, digits),
            im: dbig_from_f64(center.y, digits),
            digits,
        }
    }

    /// Shifts the centre by a small `f64` offset, preserving precision.
    pub fn translate(&mut self, delta: DVec2) {
        use dashu::float::DBig;
        let at = |x: DBig| x.with_precision(self.digits).value();
        self.re = at(&self.re + dbig_from_f64(delta.x, self.digits));
        self.im = at(&self.im + dbig_from_f64(delta.y, self.digits));
    }

    pub fn to_dvec2(&self) -> DVec2 {
        DVec2::new(self.re.to_f64().value(), self.im.to_f64().value())
    }

    pub fn reference_orbit(&self, max_iterations: u32) -> ReferenceOrbit {
        ReferenceOrbit {
            z: orbit_from_dbig(
                self.re.clone(),
                self.im.clone(),
                max_iterations,
                self.digits,
            ),
        }
    }
}

/// Renders `delta_rect` (per-pixel offsets `dc` *relative to the reference
/// centre*, in `f64`) into `buffer` via the perturbation recurrence against
/// `orbit`. Output matches `mandelbrot_simd`'s `Pixel`: `count = escape iter + 1`
/// (0.0 for in-set), `mag = |z|²` at escape.
pub fn mandelbrot_perturbation(
    orbit: &ReferenceOrbit,
    delta_rect: DRect,
    tile_size: UVec2,
    max_iterations: u32,
    buffer: &mut [Pixel],
) {
    assert_eq!(buffer.len(), (tile_size.x * tile_size.y) as usize);

    for py in 0..tile_size.y {
        let dcy = axis_scalar(delta_rect.pos.y, delta_rect.size.y, tile_size.y, py);
        for px in 0..tile_size.x {
            let dcx = axis_scalar(delta_rect.pos.x, delta_rect.size.x, tile_size.x, px);
            buffer[(py * tile_size.x + px) as usize] =
                perturb_pixel(orbit, dcx, dcy, max_iterations);
        }
    }
}

/// One pixel's delta iteration. `dc = (dcx, dcy)` is the offset from the
/// reference centre.
fn perturb_pixel(orbit: &ReferenceOrbit, dcx: f64, dcy: f64, max_iterations: u32) -> Pixel {
    // δ_0 = 0 (z_0 = Z_0 = 0); δ_1 = dc.
    let mut dx = 0.0;
    let mut dy = 0.0;

    let last = max_iterations.min(orbit.z.len() as u32 - 1);
    for n in 0..last {
        let zr = orbit.z[n as usize];
        // δ_{n+1} = 2·Z_n·δ_n + δ_n² + dc
        let ndx = 2.0 * (zr.x * dx - zr.y * dy) + (dx * dx - dy * dy) + dcx;
        let ndy = 2.0 * (zr.x * dy + zr.y * dx) + 2.0 * dx * dy + dcy;
        dx = ndx;
        dy = ndy;

        // z_{n+1} = Z_{n+1} + δ_{n+1}
        let zn1 = orbit.z[(n + 1) as usize];
        let wx = zn1.x + dx;
        let wy = zn1.y + dy;
        let mag = wx * wx + wy * wy;
        if mag >= ESCAPE_R2 {
            return Pixel {
                count: (n + 1) as f32,
                mag: mag as f32,
            };
        }
    }

    // Never escaped within range: in-set sentinel (zero == Pixel::default()).
    Pixel::default()
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use glam::UVec2;

    use super::*;
    use crate::mandelbrot_simd::mandelbrot_simd;

    /// Perturbation with an f64 reference centred in the tile must reproduce the
    /// direct kernel's escape counts at shallow zoom, where both are valid.
    #[test]
    fn matches_direct_kernel_at_shallow_zoom() {
        let tile_size = UVec2::splat(64);
        let max_iterations = 300;
        // A tile straddling the western boundary: interior, exterior, and edge.
        let tile_rect = DRect::from_pos_size(DVec2::new(-1.0, -0.6), DVec2::splat(1.2));

        let n = (tile_size.x * tile_size.y) as usize;
        let mut direct = vec![Pixel::default(); n];
        assert!(mandelbrot_simd(
            tile_rect,
            tile_size,
            max_iterations,
            Arc::new(AtomicBool::new(false)),
            &mut direct,
        ));

        let center = tile_rect.center();
        let orbit = ReferenceOrbit::from_center_f64(center, max_iterations);
        // Same pixels, expressed as offsets from the reference centre.
        let delta_rect = DRect::from_pos_size(tile_rect.pos - center, tile_rect.size);
        let mut pert = vec![Pixel::default(); n];
        mandelbrot_perturbation(&orbit, delta_rect, tile_size, max_iterations, &mut pert);

        // The two arithmetic formulations can round differently within ±1 count
        // for pixels right on the escape boundary; everything else must match
        // exactly. Assert near-perfect agreement and zero large disagreements.
        let mut off_by_one = 0;
        for (d, p) in direct.iter().zip(&pert) {
            let diff = (d.count - p.count).abs();
            assert!(
                diff <= 1.0,
                "perturbation disagrees by {diff}: direct {} vs pert {}",
                d.count,
                p.count
            );
            if diff != 0.0 {
                off_by_one += 1;
            }
        }
        // Boundary rounding should affect only a sliver of the 4096 pixels.
        assert!(off_by_one < n / 50, "{off_by_one} pixels differ (of {n})");
    }

    /// `HpCenter` must accumulate pans far below `f64` epsilon — the whole point
    /// of high-precision centre tracking for deep zoom.
    #[test]
    fn hp_center_keeps_sub_epsilon_pans() {
        let mut hp = HpCenter::new(DVec2::new(-0.75, 0.0), 40);
        for _ in 0..1000 {
            hp.translate(DVec2::new(1e-18, 0.0)); // ULP(-0.75) ≈ 1.1e-16 ≫ 1e-18
        }
        // 1000 × 1e-18 = 1e-15 of accumulated motion, preserved here…
        assert!(
            (hp.to_dvec2().x + 0.75).abs() > 5e-16,
            "pan was lost: {}",
            hp.to_dvec2().x
        );

        // …while a plain f64 accumulator loses every step (stays exactly -0.75).
        let mut f = -0.75f64;
        for _ in 0..1000 {
            f += 1e-18;
        }
        assert_eq!(f, -0.75, "f64 cannot represent the sub-epsilon pan");
    }

    /// The bignum reference orbit must agree with the f64 one for a centre that
    /// `f64` represents exactly, confirming the high-precision arithmetic reduces
    /// correctly. (Deep-zoom correctness is validated visually via the renderer.)
    #[test]
    fn bignum_reference_matches_f64_at_representable_center() {
        let max_iterations = 100;
        let f64_orbit = ReferenceOrbit::from_center_f64(DVec2::new(-0.5, 0.25), max_iterations);
        let big_orbit = ReferenceOrbit::from_center_decimal("-0.5", "0.25", max_iterations, 40);

        assert_eq!(f64_orbit.z.len(), big_orbit.z.len());
        for (a, b) in f64_orbit.z.iter().zip(&big_orbit.z) {
            assert!(
                (a.x - b.x).abs() < 1e-12 && (a.y - b.y).abs() < 1e-12,
                "{a:?} vs {b:?}"
            );
        }
    }

    /// A deep-interior pixel (origin, in the cardioid) never escapes ⇒ sentinel,
    /// and a far-exterior pixel escapes immediately ⇒ count 1.
    #[test]
    fn interior_and_exterior_pixels() {
        let max_iterations = 200;
        let orbit = ReferenceOrbit::from_center_f64(DVec2::ZERO, max_iterations);
        // Origin (dc = 0): stays at the reference, never escapes.
        let origin = perturb_pixel(&orbit, 0.0, 0.0, max_iterations);
        assert_eq!(origin.count, 0.0, "origin is in the set");
        // c = (2, 0) i.e. dc = (2, 0): z_1 = 2, |z_1|² = 4 ≥ 4 escapes at iter 1.
        let outside = perturb_pixel(&orbit, 2.0, 0.0, max_iterations);
        assert_eq!(outside.count, 1.0, "far exterior escapes immediately");
    }
}
