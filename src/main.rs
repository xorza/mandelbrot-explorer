#![allow(dead_code)]
// #![allow(unused_imports)]

use crate::app_base::run;
use crate::tiled_fractal_app::TiledFractalApp;

mod app_base;
mod event;
mod math;
mod wgpu_renderer;
mod render_pods;
mod mandelbrot;
mod mandel_texture;
mod tiled_fractal_app;


fn main() {
    run::<TiledFractalApp>("Mandelbrot explorer");
}


