use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::time::Instant;

use anyhow::anyhow;
use num_complex::Complex;

use crate::math::{RectU32, Vec2f64};

//noinspection RsConstantConditionIf
pub async fn mandelbrot(
    image_size: u32,
    tile_rect: RectU32,
    fractal_offset: Vec2f64,
    fractal_scale: f64,
    max_iterations: u32,
    cancel_token: Arc<AtomicU32>,
    cancel_token_value: u32,
) -> anyhow::Result<Vec<u8>>
{
    let now = Instant::now();

    let mut buffer: Vec<u8> = vec![128; (tile_rect.size.x * tile_rect.size.y) as usize];
    let mut samples: Vec<u8> = vec![1; (tile_rect.size.x * tile_rect.size.y) as usize];

    let image_size = image_size as f64;
    let fractal_offset = Vec2f64::new(fractal_offset.x + 0.74, fractal_offset.y);
    let scale = fractal_scale;
    let tile_offset = Vec2f64::from(tile_rect.pos);

    let pixel_offset = 1.0 / (scale * image_size);
    let sample_offset = pixel_offset * 0.25;
    let sample_offsets = [
        Vec2f64::new(-sample_offset, -sample_offset),
        Vec2f64::new(-sample_offset, sample_offset),
        Vec2f64::new(sample_offset, -sample_offset),
        Vec2f64::new(sample_offset, sample_offset),
    ];

    let pixel_position = |x: u32, y: u32| -> Vec2f64{
        let xy =
            ((Vec2f64::new(x as f64, y as f64)
                + tile_offset) / image_size
                - 0.5)
                / scale
                - fractal_offset;
        xy
    };
    let pixel_index = |x: u32, y: u32| -> usize{
        (y * tile_rect.size.x + x) as usize
    };

    const MULTISAMPLE: bool = true;

    for y in 0..tile_rect.size.y {
        for x in 0..tile_rect.size.x {
            if x % 32 == 0 {
                if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value {
                    return Err(anyhow!("Cancelled"));
                }
            }

            let index = pixel_index(x, y);
            let xy = pixel_position(x, y);

            let mut result0 = pixel(max_iterations, xy + sample_offsets[0]);

            if MULTISAMPLE
                && (1..tile_rect.size.x - 1).contains(&x)
                && (1..tile_rect.size.y - 1).contains(&y)
            {
                let mut should_multisample = false;

                {
                    let x_prev_index = pixel_index(x - 1, y);
                    let x_prev_color0 = buffer[x_prev_index] as u16;

                    if result0.abs_diff(x_prev_color0) > 128 {
                        if samples[x_prev_index] == 1 {
                            let x_prev_pixel_pos = pixel_position(x - 1, y);
                            let x_prev_color1 = pixel(max_iterations, x_prev_pixel_pos + sample_offsets[1]);
                            let x_prev_color2 = pixel(max_iterations, x_prev_pixel_pos + sample_offsets[2]);
                            let x_prev_color3 = pixel(max_iterations, x_prev_pixel_pos + sample_offsets[3]);
                            buffer[x_prev_index] = ((x_prev_color0 + x_prev_color1 + x_prev_color2 + x_prev_color3) / 4) as u8;
                            samples[x_prev_index] = 4;
                        }
                        should_multisample = true;
                    }
                }
                {
                    let y_prev_index = pixel_index(x, y - 1);
                    let y_prev_color0 = buffer[y_prev_index] as u16;

                    if result0.abs_diff(y_prev_color0) > 128 {
                        if samples[y_prev_index] == 1 {
                            let y_prev_pixel_pos = pixel_position(x, y - 1);
                            let y_prev_color1 = pixel(max_iterations, y_prev_pixel_pos + sample_offsets[1]);
                            let y_prev_color2 = pixel(max_iterations, y_prev_pixel_pos + sample_offsets[2]);
                            let y_prev_color3 = pixel(max_iterations, y_prev_pixel_pos + sample_offsets[3]);
                            buffer[y_prev_index] = ((y_prev_color0 + y_prev_color1 + y_prev_color2 + y_prev_color3) / 4) as u8;
                            samples[y_prev_index] = 4;
                        }
                        should_multisample = true;
                    }
                }

                if should_multisample {
                    let result1 = pixel(max_iterations, xy + sample_offsets[1]);
                    let result2 = pixel(max_iterations, xy + sample_offsets[2]);
                    let result3 = pixel(max_iterations, xy + sample_offsets[3]);

                    result0 = (result0 + result1 + result2 + result3) / 4;
                    samples[index] = 4;
                }
            }

            buffer[index] = result0 as u8;
        }
    }

    if false {
        let elapsed = now.elapsed();
        println!("Elapsed: {}ms", elapsed.as_millis());
        // let target = Duration::from_millis(100);
        // if elapsed < target {
        //     tokio::time::sleep(target - elapsed).await;
        //     thread::sleep(target - elapsed);
        // }
    }

    Ok(buffer)
}

fn pixel(max_iterations: u32, xy: Vec2f64) -> u16 {
    if is_in_main_cardioid(xy) || is_in_main_circle(xy) {
        return 0u16;
    }
    let c: Complex<f64> = Complex::new(xy.x, xy.y);
    let mut z: Complex<f64> = Complex::new(0.0, 0.0);

    let mut i: u32 = 0;

    while z.norm() <= 4.0 && i < max_iterations {
        z = z * z + c;
        i += 1;
    }

    if i == max_iterations {
        0u16
    } else {
        let i = (i as f32 / max_iterations as f32).powf(0.7);
        let color = 1.0 - i;

        (255.0 * color) as u16
    }
}


fn is_in_main_cardioid(xy: Vec2f64) -> bool {
    let q = (xy.x - 0.25).powi(2) + xy.y.powi(2);
    let result = q * (q + (xy.x - 0.25)) < 0.25 * xy.y.powi(2);
    result
}
fn is_in_main_circle(xy: Vec2f64) -> bool {
    let q = (xy.x + 1.0).powi(2) + xy.y.powi(2);
    let result = q < 0.25f64.powi(2);
    result
}

#[cfg(test)]
mod test {
    use pollster::FutureExt;

    use crate::math::Vec2u32;

    #[test]
    fn draw_mandelbrot() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicU32;

        use crate::math::{RectU32, Vec2f64};

        let image_size = 2048;
        let tile_rect = RectU32::new(
            Vec2u32::new(0, 0),
            Vec2u32::new(image_size, image_size),
        );
        let fractal_offset = Vec2f64::new(-0.080669055533625203, -0.4499300190992746);
        let fractal_scale = 75.475169471081102;
        let max_iterations = 350;
        let cancel_token = Arc::new(AtomicU32::new(0));
        let cancel_token_value = 0;

        let now = std::time::Instant::now();

        let buffer = {
            for _i in 0..9 {
                let cancel_token = cancel_token.clone();
                crate::mandelbrot::mandelbrot(
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

            crate::mandelbrot::mandelbrot(
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

        let elapsed = now.elapsed().as_millis() / 10;
        println!("Elapsed: {}ms", elapsed);


        let mut image = image::ImageBuffer::new(image_size, image_size);
        for y in 0..image_size {
            for x in 0..image_size {
                let index = (y * image_size + x) as usize;
                let color = buffer[index];
                let color = image::Rgb([color, color, color]);
                image.put_pixel(x, y, color);
            }
        }
        image.save("test_output/mandelbrot.png").unwrap();
    }
}

