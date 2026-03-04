#![allow(non_camel_case_types)]

use std::simd::prelude::*;
use std::simd::Select;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};
use glam::DVec2;

use crate::math::{DRect, URect};

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
pub fn mandelbrot_simd(
    image_size: u32,
    tex_rect: URect,
    fractal_offset: DVec2,
    fractal_scale: f64,
    max_iterations: u32,
    cancel_token: Arc<AtomicBool>,
    buffer: &mut [Pixel],
) -> anyhow::Result<()> {
    assert_eq!(buffer.len(), (tex_rect.size.x * tex_rect.size.y) as usize);

    #[cfg(test)]
    let now = std::time::Instant::now();

    let buffer_frame = {
        let image_size = image_size as f64;

        DRect::from_pos_size(
            (DVec2::from(tex_rect.pos) / image_size - 0.5) / fractal_scale - fractal_offset,
            (DVec2::from(tex_rect.size) / image_size) / fractal_scale,
        )
    };

    for y in 0..tex_rect.size.y {
        if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow!("Cancelled"));
        }
        for x in 0..tex_rect.size.x / SIMD_LANE_COUNT as u32 {
            let cx =
                f64simd::from_array(CX_INIT) + f64simd::splat((x * SIMD_LANE_COUNT as u32) as f64);
            let cx = cx * f64simd::splat(buffer_frame.size.x / tex_rect.size.x as f64);
            let cx = cx + f64simd::splat(buffer_frame.pos.x);

            let cy = f64simd::splat(
                buffer_frame.pos.y + buffer_frame.size.y * (y as f64 / tex_rect.size.y as f64),
            );

            let values_simd = pixel(max_iterations, cx, cy);
            let idx = (y * tex_rect.size.x + x * SIMD_LANE_COUNT as u32) as usize;
            buffer[idx..idx + SIMD_LANE_COUNT].copy_from_slice(values_simd.as_slice());
        }
    }

    #[cfg(test)]
    {
        let elapsed = now.elapsed();
        println!("Elapsed: {}ms", elapsed.as_millis());
        println!("Total pixels: {}", tex_rect.size.x * tex_rect.size.y);
    }

    Ok(())
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

    use glam::UVec2;

    use super::*;

    #[test]
    fn draw_mandelbrot() {
        let image_size = 2048;
        let tile_rect = URect::from_pos_size(UVec2::new(0, 0), UVec2::new(image_size, image_size));
        let fractal_offset = DVec2::new(0.10486747136388758, 0.9244368813525663);
        let fractal_scale = 32.0;
        let max_iterations = 1024;
        let cancel_token = Arc::new(AtomicBool::new(false));
        let mut buffer = vec![Pixel::default(); (image_size * image_size) as usize];

        let new = Instant::now();
        let retry = 5;

        for _ in 0..retry {
            mandelbrot_simd(
                image_size,
                tile_rect,
                fractal_offset,
                fractal_scale,
                max_iterations,
                cancel_token.clone(),
                &mut buffer,
            )
            .unwrap();
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
