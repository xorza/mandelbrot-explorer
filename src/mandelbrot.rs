use num_complex::Complex;
use rayon::iter::IndexedParallelIterator;
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;

use crate::math::{Vec2f64, Vec2u32};

pub fn mandelbrot(
    size: Vec2u32,
    offset: Vec2f64,
    scale: f64,
) -> Vec<u8>
{
    let mut buffer: Vec<u8> = vec![0; (size.x * size.y) as usize];
    let width = size.x as f64;
    let height = size.y as f64;
    let aspect = width / height;

    let start = std::time::Instant::now();

    // center
    let offset = Vec2f64::new(offset.x + 0.4, offset.y) * 2.3;
    let scale = scale * 2.3;

    buffer
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, pixel)| {
            let x = (i as f64 % width) / width;
            let y = (i as f64 / height) / (aspect * height);

            let cx = (x - 0.5) * scale - offset.x;
            let cy = (y - 0.5) * scale - offset.y;

            let cx = cx * aspect;

            let c: Complex<f64> = Complex::new(cx, cy);
            let mut z: Complex<f64> = Complex::new(0.0, 0.0);

            let mut it: u32 = 0;
            const MAX_IT: u32 = 256;

            while z.norm() <= 8.0 && it <= MAX_IT {
                z = z * z + c;
                it += 1;
            }

            *pixel = it as u8;
        });

    let elapsed = start.elapsed();
    println!("Mandelbrot rendered in {}ms", elapsed.as_millis());

    buffer
}
