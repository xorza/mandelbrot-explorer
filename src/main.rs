#![allow(dead_code)]
#![allow(unused_imports)]

mod app_base;
mod event;
mod math;
mod wgpu_app;
mod wgpu_renderer;

use num_complex::Complex;
use rayon::prelude::*;

const WIDTH: usize = 4096;
const HEIGHT: usize = 4096;

fn sample_grad(alpha: f64) -> [u8; 4] {
    // @formatter:off
    const COLORS:[(f64, [u8; 4]); 5]=[
        (    0.0f64, [  0,   7, 100, 255]),
        (   0.16f64, [ 32, 107, 203, 255]),
        (   0.42f64, [237, 255, 255, 255]),
        ( 0.6425f64, [255, 170,   0, 255]),
        // ( 0.8575f64, [  0,   2,   0, 255]),
        (    1.0f64, [  0,   0,   0, 255]),
    ];
    // @formatter:on

    let mut low = COLORS[0].1;
    let mut high = COLORS[0].1;
    let mut low_a = 0.0;
    let mut high_a = 0.0;

    for &(pos, color) in COLORS.iter() {
        if alpha < pos {
            high = color;
            high_a = pos;
            break;
        } else {
            low = color;
            low_a = pos;
        }
    }

    let alpha = (alpha - low_a) / (high_a - low_a);

    let mut result = [0; 4];
    for i in 0..4 {
        let a = low[i] as f64;
        let b = high[i] as f64;
        result[i] = ((1.0 - alpha) * a + alpha * b) as u8;
    }

    result
}

use crate::app_base::run;
use crate::wgpu_app::WgpuApp;

fn main() {
    run::<WgpuApp>("UI App");
}



type Float = f64;
fn mandelbrot(buffer: &mut Vec<u32>) -> u128 {
    let start = std::time::Instant::now();
    buffer.par_iter_mut().enumerate().for_each(|(i, pixel)| {
        let x = i % WIDTH;
        let y = i / WIDTH;

        let cx = (x as Float - (0.75 * WIDTH as Float)) / (0.5 * WIDTH as Float);
        let cy = (y as Float - (0.5 * HEIGHT as Float)) / (0.5 * HEIGHT as Float);

        let c: Complex<Float> = Complex::new(cx, cy);
        let mut z: Complex<Float> = Complex::new(0.0, 0.0);

        let mut it: u32 = 0;
        const MAX_IT: u32 = 512;
        while z.norm() <= 2.0 && it <= MAX_IT {
            z = z * z + c;
            it += 1;
        }

        if it > MAX_IT - 1 {
            return;
        }

        let alpha = it as f64 / MAX_IT as f64;
        let alpha = alpha.powf(0.3);
        let color = sample_grad(alpha);

        *pixel = *bytemuck::from_bytes(&color)
    });
    let elapsed = start.elapsed();
    println!("Elapsed: {}ms", elapsed.as_millis());

    elapsed.as_millis()
}
