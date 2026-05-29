//! Tight single-threaded loop over the SIMD kernel, for `perf`-style profiling
//! without criterion's statistics polluting the samples.
//!
//! Usage: `cargo build --release --example profile_kernel && \
//!         perf record -g target/release/examples/profile_kernel [seconds]`

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use fractal::mandelbrot_simd::{Pixel, mandelbrot_simd};
use fractal::math::DRect;
use glam::{DVec2, UVec2};

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    // The `deep_zoom` bench scenario: a filament with a high iteration cap, the
    // compute-bound case where the iteration loop dominates.
    let tile_size = UVec2::splat(128);
    let rect = DRect::from_pos_size(
        DVec2::splat(-0.5) / 32.0 - DVec2::new(0.10486747136388758, 0.9244368813525663),
        DVec2::splat(1.0 / 32.0),
    );
    let max_iterations = 1024;

    let mut buffer = vec![Pixel::default(); (tile_size.x * tile_size.y) as usize];
    let cancel = Arc::new(AtomicBool::new(false));

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut runs: u64 = 0;
    while Instant::now() < deadline {
        for _ in 0..256 {
            mandelbrot_simd(
                black_box(rect),
                tile_size,
                max_iterations,
                cancel.clone(),
                &mut buffer,
            );
            black_box(&buffer);
            runs += 1;
        }
    }
    println!("{runs} renders");
}
