#![allow(non_camel_case_types)]

use std::simd::Select;
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

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub(crate) struct Pixel {
    r: u16,
}

const CX_INIT: [f64; SIMD_LANE_COUNT] = {
    let mut r = [0.0; SIMD_LANE_COUNT];
    let mut i = 0;
    while i < SIMD_LANE_COUNT {
        r[i] = i as f64;
        i += 1;
    }
    r
};

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

    for y in 0..tile_size.y {
        if cancel_token.load(Ordering::Relaxed) {
            return false;
        }
        for x in 0..tile_size.x / SIMD_LANE_COUNT as u32 {
            let cx =
                f64simd::from_array(CX_INIT) + f64simd::splat((x * SIMD_LANE_COUNT as u32) as f64);
            let cx = cx * f64simd::splat(tile_rect.size.x / tile_size.x as f64);
            let cx = cx + f64simd::splat(tile_rect.pos.x);

            let cy = f64simd::splat(
                tile_rect.pos.y + tile_rect.size.y * (y as f64 / tile_size.y as f64),
            );

            let values_simd = pixel(max_iterations, cx, cy);
            let idx = (y * tile_size.x + x * SIMD_LANE_COUNT as u32) as usize;
            buffer[idx..idx + SIMD_LANE_COUNT].copy_from_slice(values_simd.as_slice());
        }
    }

    true
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
        return [Pixel { r: 0 }; SIMD_LANE_COUNT];
    }

    let mut zx = f64simd::splat(0.0);
    let mut zy = f64simd::splat(0.0);
    let mut zx2 = f64simd::splat(0.0);
    let mut zy2 = f64simd::splat(0.0);
    let mut cnt = i64simd::splat(0);
    let mut escaped = in_set;

    let f64_4_0 = f64simd::splat(4.0);
    let i64_0 = i64simd::splat(0);
    let i64_1 = i64simd::splat(1);

    let mut i = 0u32;
    while i < max_iterations {
        let batch = (max_iterations - i).min(8);
        for _ in 0..batch {
            zy = (zx + zx) * zy + cy;
            zx = zx2 - zy2 + cx;
            zx2 = zx * zx;
            zy2 = zy * zy;
            escaped |= (zx2 + zy2).simd_ge(f64_4_0);
            cnt += escaped.select(i64_0, i64_1);
        }
        i += batch;

        if escaped.all() {
            break;
        }
    }

    let max_iter_simd = i64simd::splat(max_iterations as i64);
    cnt = in_set.select(max_iter_simd, cnt);

    cnt.as_array().map(|iters| {
        if iters as u32 == max_iterations {
            Pixel { r: 0 }
        } else {
            Pixel {
                r: 1 + (iters % u16::MAX as i64) as u16,
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

        // Pixel (x, y) samples c = (-2 + x*0.5, -2 + y*0.5).
        let at = |x: u32, y: u32| buffer[(y * 8 + x) as usize].r;

        // Origin c = (0, 0): inside the set => never escapes => stored as 0.
        // x=4 -> -2+2=0, y=4 -> 0.
        assert_eq!(at(4, 4), 0, "origin is in the set");

        // c = (-2, -2) (corner): z1 = c, |z1|^2 = 8 >= 4 escapes on iter 1, which
        // does not increment the counter (escape iteration counts 0) => cnt 0,
        // stored as 0 + 1 offset => 1.
        assert_eq!(at(0, 0), 1, "far corner escapes immediately");

        // c = (0.5, 0): zx progresses 0.5, 0.75, 1.0625, 1.6289 (4 non-escaping
        // iterations), then 3.1533 whose square 9.94 >= 4 escapes on the 5th
        // (uncounted) => cnt 4, stored as 4 + 1 offset => 5.
        assert_eq!(at(5, 4), 5, "c=0.5 escapes after 4 counted iterations");
    }

    #[test]
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
                let pixel = (buffer[index].r % 256) as u8;
                let color = image::Rgb([pixel, pixel, pixel]);
                image.put_pixel(x, y, color);
            }
        }
        std::fs::create_dir_all("test_output").unwrap();
        image.save("test_output/mandelbrot.png").unwrap();
    }
}
