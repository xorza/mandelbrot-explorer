use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fractal::mandelbrot_simd::{Pixel, mandelbrot_simd};
use fractal::math::DRect;
use glam::{DVec2, UVec2};

/// A representative tile workload: where it sits in fractal space, how deep it
/// iterates, and how big the tile is. These mirror the three regimes the kernel
/// actually hits at runtime.
struct Scenario {
    name: &'static str,
    rect: DRect,
    max_iterations: u32,
    tile: u32,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // Whole-set view: a large fraction of pixels stay in the set and run the
        // full iteration budget — the worst case for the early-bailout path.
        Scenario {
            name: "full_view",
            rect: DRect::from_pos_size(DVec2::new(-2.5, -1.75), DVec2::new(3.5, 3.5)),
            max_iterations: 256,
            tile: 128,
        },
        // Deep zoom on a filament (coords from the draw_mandelbrot test): a mix
        // of fast-escaping and deep pixels, with a high iteration cap.
        Scenario {
            name: "deep_zoom",
            rect: full_image_rect(DVec2::new(0.10486747136388758, 0.9244368813525663), 32.0),
            max_iterations: 1024,
            tile: 128,
        },
        // Exterior region far from the set: almost everything escapes in a few
        // iterations, so the cardioid/bulb fast-paths and early bailout dominate.
        Scenario {
            name: "exterior",
            rect: DRect::from_pos_size(DVec2::new(-3.0, -3.0), DVec2::new(1.0, 1.0)),
            max_iterations: 256,
            tile: 128,
        },
    ]
}

/// Rebuilds the fractal rect the old `(offset, scale)` full-image API produced,
/// matching the `draw_mandelbrot` test fixture.
fn full_image_rect(offset: DVec2, scale: f64) -> DRect {
    DRect::from_pos_size(
        DVec2::splat(-0.5) / scale - offset,
        DVec2::splat(1.0 / scale),
    )
}

fn bench_kernel(c: &mut Criterion) {
    let mut group = c.benchmark_group("mandelbrot_simd");

    for s in scenarios() {
        let tile_size = UVec2::splat(s.tile);
        let pixels = (s.tile * s.tile) as u64;
        let mut buffer = vec![Pixel::default(); pixels as usize];
        let cancel = Arc::new(AtomicBool::new(false));

        group.throughput(Throughput::Elements(pixels));
        group.bench_with_input(BenchmarkId::from_parameter(s.name), &s, |b, s| {
            b.iter(|| {
                let ok = mandelbrot_simd(
                    s.rect,
                    tile_size,
                    s.max_iterations,
                    cancel.clone(),
                    &mut buffer,
                );
                assert!(ok);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_kernel);
criterion_main!(benches);
