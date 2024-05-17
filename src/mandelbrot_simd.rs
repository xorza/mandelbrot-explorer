#![allow(non_camel_case_types)]

use std::simd::prelude::*;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Instant;
use std::usize;

use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};
use glam::DVec2;

use crate::env::is_test_build;
use crate::math::{DRect, URect};

const MULTISAMPLE_THRESHOLD: u16 = 64;
const SIMD_LANE_COUNT: usize = 8;
pub const MAX_ITER: u32 = 4500;
const MULTISAMPLE_ENABLED: bool = false;

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
pub async fn mandelbrot_simd(
    image_size: u32,
    tile_rect: URect,
    fractal_offset: DVec2,
    fractal_scale: f64,
    max_iterations: u32,
    cancel_token: Arc<AtomicU32>,
    cancel_token_value: u32,
) -> anyhow::Result<Vec<Pixel>> {
    let now = Instant::now();

    let mut buffer: Vec<Pixel> =
        vec![Pixel::default(); (2 * tile_rect.size.x * tile_rect.size.y) as usize];

    let buffer_frame = {
        let image_size = image_size as f64;
        let fractal_offset = DVec2::new(fractal_offset.x + 0.74, fractal_offset.y);

        DRect::from_pos_size(
            (DVec2::from(tile_rect.pos) / image_size - 0.5) / fractal_scale - fractal_offset,
            (DVec2::from(tile_rect.size) / image_size) / fractal_scale,
        )
    };

    let sample_offsets: [DVec2; 4] = {
        let pixel_size = 1.0 / (fractal_scale * image_size as f64);
        let sample_offset = 0.5 * pixel_size;

        [
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, sample_offset),
            DVec2::new(sample_offset, 0.0),
            DVec2::new(sample_offset, sample_offset),
        ]
    };

    {
        // main buffer

        for y in 0..tile_rect.size.y {
            for x in 0..tile_rect.size.x / SIMD_LANE_COUNT as u32 {
                if (x * SIMD_LANE_COUNT as u32) % 32 == 0 {
                    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value
                    {
                        return Err(anyhow!("Cancelled"));
                    }
                }

                let cx = f64simd::from_slice(CX_INIT.as_slice())
                    + f64simd::splat((x * SIMD_LANE_COUNT as u32) as f64);
                let cx = cx * f64simd::splat(buffer_frame.size.x / tile_rect.size.x as f64);
                let cx = cx + f64simd::splat(buffer_frame.pos.x);

                let cy = f64simd::splat(
                    buffer_frame.pos.y + buffer_frame.size.y * (y as f64 / tile_rect.size.y as f64),
                );

                let values_simd = pixel(max_iterations, cx, cy);
                let start_index = (y * tile_rect.size.x + x * SIMD_LANE_COUNT as u32) as usize;
                buffer[start_index..start_index + SIMD_LANE_COUNT]
                    .copy_from_slice(values_simd.as_slice());
            }
        }
    }

    let mut multisampled_pixels_count: usize = 0;

    if MULTISAMPLE_ENABLED {
        // multisample
        let mut cx_load: Vec<f64> = Vec::with_capacity(SIMD_LANE_COUNT);
        let mut cy_load: Vec<f64> = Vec::with_capacity(SIMD_LANE_COUNT);
        // let mut loaded_cnt = 0;
        let mut loaded_indexes: Vec<usize> = Vec::with_capacity(SIMD_LANE_COUNT);

        let mut acc_index: usize = usize::MAX;
        let mut acc_value: u16 = 0;

        for y in 0..tile_rect.size.y {
            for x in 0..tile_rect.size.x {
                let index = (y * tile_rect.size.x + x) as usize;
                let should_multisample = {
                    let value = buffer[index].r;

                    (x != tile_rect.size.x - 1
                        && value.abs_diff(buffer[(y * tile_rect.size.x + x + 1) as usize].r)
                            > MULTISAMPLE_THRESHOLD)
                        || (x != 0
                            && value.abs_diff(buffer[(y * tile_rect.size.x + x - 1) as usize].r)
                                > MULTISAMPLE_THRESHOLD)
                        || (y != tile_rect.size.y - 1
                            && value.abs_diff(buffer[((y + 1) * tile_rect.size.x + x) as usize].r)
                                > MULTISAMPLE_THRESHOLD)
                        || (y != 0
                            && value.abs_diff(buffer[((y - 1) * tile_rect.size.x + x) as usize].r)
                                > MULTISAMPLE_THRESHOLD)
                };

                if should_multisample {
                    multisampled_pixels_count += 1;

                    let xy = buffer_frame.pos
                        + buffer_frame.size * DVec2::new(x as f64, y as f64)
                            / DVec2::from(tile_rect.size);

                    for sample_offset in &sample_offsets[1..3] {
                        let xy = xy + *sample_offset;

                        cx_load.push(xy.x);
                        cy_load.push(xy.y);
                        loaded_indexes.push(index);
                        // loaded_cnt += 1;

                        if cx_load.len() == SIMD_LANE_COUNT {
                            let cx = f64simd::from_slice(cx_load.as_slice());
                            let cy = f64simd::from_slice(cy_load.as_slice());

                            let values_simd = pixel(max_iterations, cx, cy);
                            for (simd_index, &buffer_index) in loaded_indexes.iter().enumerate() {
                                if buffer_index != acc_index {
                                    if acc_index != usize::MAX {
                                        buffer[acc_index].r = acc_value / 4;
                                    }

                                    acc_index = buffer_index;
                                    acc_value = buffer[acc_index].r;
                                }

                                acc_value += values_simd[simd_index].r;
                            }

                            cx_load.clear();
                            cy_load.clear();
                            loaded_indexes.clear();
                        }
                    }
                }
            }
        }
    }

    if is_test_build() {
        let elapsed = now.elapsed();
        println!("Elapsed: {}ms", elapsed.as_millis());
        println!("Total pixels: {}", tile_rect.size.x * tile_rect.size.y);
        println!("Multisampled pixels count: {}", multisampled_pixels_count);

        // let target = Duration::from_millis(100);
        // if elapsed < target {
        //     tokio::time::sleep(target - elapsed).await;
        //     thread::sleep(target - elapsed);
        // }
    }

    Ok(buffer)
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

fn is_in_main_cardioid(xy: DVec2) -> bool {
    let q = (xy.x - 0.25).powi(2) + xy.y.powi(2);
    let result = q * (q + (xy.x - 0.25)) < 0.25 * xy.y.powi(2);
    result
}

fn is_in_main_circle(xy: DVec2) -> bool {
    let q = (xy.x + 1.0).powi(2) + xy.y.powi(2);
    let result = q < 0.25f64.powi(2);
    result
}

#[cfg(test)]
mod test {
    use glam::UVec2;
    use pollster::FutureExt;

    use crate::env::is_debug_build;

    use super::*;

    #[test]
    fn draw_mandelbrot() {
        use std::sync::atomic::AtomicU32;
        use std::sync::Arc;

        let image_size = 2048;
        let tile_rect = URect::from_pos_size(UVec2::new(0, 0), UVec2::new(image_size, image_size));
        let fractal_offset = DVec2::new(-0.080669055533625203, -0.4499300190992746);
        let fractal_scale = 75.475169471081102;
        let max_iterations = 1024;
        let cancel_token = Arc::new(AtomicU32::new(0));
        let cancel_token_value = 0;

        if !is_debug_build() {
            let cancel_token = cancel_token.clone();
            mandelbrot_simd(
                image_size,
                tile_rect,
                fractal_offset,
                fractal_scale,
                max_iterations,
                cancel_token,
                cancel_token_value,
            )
            .block_on()
            .unwrap();
        }

        let now = std::time::Instant::now();

        let buffer = {
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
                        cancel_token_value,
                    )
                    .block_on()
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
                cancel_token_value,
            )
            .block_on()
            .unwrap()
        };

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
