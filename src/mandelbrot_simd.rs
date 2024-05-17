#![allow(non_camel_case_types)]

use std::simd::prelude::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use std::usize;

use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};
use glam::DVec2;

use crate::env::is_test_build;
use crate::math::{DRect, URect};

const SIMD_LANE_COUNT: usize = 8;
pub const MAX_ITER: u32 = 4500;

type f64simd = Simd<f64, SIMD_LANE_COUNT>;
type i64simd = Simd<i64, SIMD_LANE_COUNT>;
type mask64simd = Mask<i64, SIMD_LANE_COUNT>;
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

    let now = Instant::now();
    let buffer_frame = {
        let image_size = image_size as f64;
        let fractal_offset = DVec2::new(fractal_offset.x, fractal_offset.y);

        DRect::from_pos_size(
            (DVec2::from(tex_rect.pos) / image_size - 0.5) / fractal_scale - fractal_offset,
            (DVec2::from(tex_rect.size) / image_size) / fractal_scale,
        )
    };

    {
        for y in 0..tex_rect.size.y {
            for x in 0..tex_rect.size.x / SIMD_LANE_COUNT as u32 {
                if (x * SIMD_LANE_COUNT as u32) % 32 == 0 {
                    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(anyhow!("Cancelled"));
                    }
                }

                let cx = f64simd::from_slice(CX_INIT.as_slice())
                    + f64simd::splat((x * SIMD_LANE_COUNT as u32) as f64);
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
    }

    if is_test_build() {
        let elapsed = now.elapsed();
        println!("Elapsed: {}ms", elapsed.as_millis());
        println!("Total pixels: {}", tex_rect.size.x * tex_rect.size.y);

        // let target = Duration::from_millis(100);
        // if elapsed < target {
        //     tokio::time::sleep(target - elapsed).await;
        //     thread::sleep(target - elapsed);
        // }
    }

    Ok(())
}

fn pixel(max_iterations: u32, cx: f64simd, cy: f64simd) -> CountSimd {
    let mut zx = f64simd::splat(0.0);
    let mut zy = f64simd::splat(0.0);
    let mut cnt = i64simd::splat(0);
    let mut escaped = mask64simd::splat(false);

    let f64_4_0 = f64simd::splat(5.0);
    let i64_0 = i64simd::splat(0);
    let i64_1 = i64simd::splat(1);

    for _ in 0..max_iterations {
        (zx, zy) = (zx * zx - zy * zy + cx, zx * zy + zx * zy + cy);
        escaped |= (zx * zx + zy * zy).simd_ge(f64_4_0);

        if escaped.all() {
            break;
        }

        cnt += escaped.select(i64_0, i64_1);
    }

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
    use glam::UVec2;

    use crate::env::is_debug_build;

    use super::*;

    #[test]
    fn draw_mandelbrot() {
        use std::sync::Arc;

        let image_size = 2048;
        let tile_rect = URect::from_pos_size(UVec2::new(0, 0), UVec2::new(image_size, image_size));
        let fractal_offset = DVec2::new(-0.080669055533625203, -0.4499300190992746);
        let fractal_scale = 75.475169471081102;
        let fractal_offset = DVec2::new(0.10486747136388758, 0.9244368813525663);
        let fractal_scale = 32.0;
        let max_iterations = 1024;
        let cancel_token = Arc::new(AtomicBool::new(false));
        let mut buffer = vec![Pixel::default(); (image_size * image_size) as usize];

        let now = Instant::now();

        if !is_debug_build() {
            for _i in 0..4 {
                let cancel_token = cancel_token.clone();
                mandelbrot_simd(
                    image_size,
                    tile_rect,
                    fractal_offset,
                    fractal_scale,
                    max_iterations,
                    cancel_token,
                    &mut buffer,
                )
                .unwrap();
            }
        }

        mandelbrot_simd(
            image_size,
            tile_rect,
            fractal_offset,
            fractal_scale,
            max_iterations,
            cancel_token,
            &mut buffer,
        )
        .unwrap();

        if is_debug_build() {
            let elapsed = now.elapsed().as_millis();
            println!("DEBUG Avg elapsed: {}ms", elapsed);
        } else {
            let elapsed = now.elapsed().as_millis() / 5;
            println!("Avg elapsed: {}ms", elapsed);
        }

        let mut image = image::ImageBuffer::new(image_size, image_size);
        for y in 0..image_size {
            for x in 0..image_size {
                let index = (y * image_size + x) as usize;
                let pixel = (buffer[index].r % 256) as u8;
                let color = image::Rgb([pixel, pixel, pixel]);
                image.put_pixel(x, y, color);
            }
        }
        image.save("test_output/mandelbrot.png").unwrap();
    }
}
