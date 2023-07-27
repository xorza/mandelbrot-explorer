use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use num_complex::Complex;
use rayon::iter::IndexedParallelIterator;
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;

use crate::math::{Vec2f64, Vec2u32};

pub fn mandelbrot1(
    size: Vec2u32,
    offset: Vec2f64,
    scale: f64,
    cancel_token: Arc<AtomicU32>,
) -> anyhow::Result<Vec<u8>>
{
    let mut buffer: Vec<u8> = vec![0; (size.x * size.y) as usize];
    let width = size.x as f64;
    let height = size.y as f64;
    let aspect = width / height;

    let cancel_token_value = cancel_token.load(std::sync::atomic::Ordering::Relaxed);

    let start = std::time::Instant::now();

    // center
    let offset = Vec2f64::new(offset.x + 0.2, offset.y) * 2.3;
    let scale = scale * 2.3;

    // let mut buffers = (0..size.y)
    //     .map(|_| Vec::with_capacity(size.x as usize))
    //     .collect::<Vec<Vec<u8>>>();
    buffer
        .par_iter_mut()
        .enumerate()
        .try_for_each(|(i, pix)| {
            if i % 100 == 0 {
                if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value {
                    return Err(());
                }
            }

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

            *pix = it as u8;

            Ok(())
        })
        .map_err(|_| anyhow::anyhow!("Cancelled"))?;

    let elapsed = start.elapsed();
    println!("Mandelbrot1 rendered in {}ms", elapsed.as_millis());

    // if elapsed.as_millis() < 500 {
    //     let ms = 500 - elapsed.as_millis() as u64;
    //     thread::sleep(std::time::Duration::from_millis(ms));
    // }

    Ok(buffer)
}


pub fn mandelbrot2(
    size: Vec2u32,
    offset: Vec2f64,
    scale: f64,
    cancel_token: Arc<AtomicU32>,
) -> anyhow::Result<Vec<u8>>
{
    let width = size.x as f64;
    let height = size.y as f64;
    let aspect = width / height;

    let cancel_token_value = cancel_token.load(std::sync::atomic::Ordering::Relaxed);

    let start = std::time::Instant::now();

    // center
    let offset = Vec2f64::new(offset.x + 0.2, offset.y) * 2.3;
    let scale = scale * 2.3;

    let mut buffers = (0..size.y)
        .map(|_| Vec::with_capacity(size.x as usize))
        .collect::<Vec<Vec<u8>>>();
    buffers
        .par_iter_mut()
        .enumerate()
        .try_for_each(|(y, row)| {
            if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value {
                return Err(());
            }

            for x in 0..size.x {
                let x = x as f64 / width;
                let y = y as f64 / height;

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

                row.push(it as u8);
            }

            Ok(())
        })
        .map_err(|_| anyhow::anyhow!("Cancelled"))?;

    let buffer: Vec<u8> = buffers
        .into_iter()
        .flatten()
        .collect::<Vec<u8>>();

    let elapsed = start.elapsed();
    println!("Mandelbrot2 rendered in {}ms", elapsed.as_millis());

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mandelbrot1() {
        for _ in 0..3 {
            let size = Vec2u32::new(4000, 4000);
            let offset = Vec2f64::new(0.0, 0.0);
            let scale = 1.0;

            let cancel_token = Arc::new(AtomicU32::new(0));

            let buffer = mandelbrot1(size, offset, scale, cancel_token).unwrap();

            assert_eq!(buffer.len(), (size.x * size.y) as usize);
        }
    }

    #[test]
    fn test_mandelbrot2() {
        for _ in 0..3 {
            let size = Vec2u32::new(4000, 4000);
            let offset = Vec2f64::new(0.0, 0.0);
            let scale = 1.0;

            let cancel_token = Arc::new(AtomicU32::new(0));

            let buffer = mandelbrot2(size, offset, scale, cancel_token).unwrap();

            assert_eq!(buffer.len(), (size.x * size.y) as usize);
        }
    }
}