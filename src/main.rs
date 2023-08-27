#![feature(portable_simd)]
#![allow(dead_code)]

use crate::app_base::run;
use crate::tiled_fractal_app::TiledFractalApp;

mod app_base;
mod event;
mod math;
mod render_pods;
mod mandel_texture;
mod tiled_fractal_app;
mod env;
mod mandelbrot_simd;


fn main() {
    run::<TiledFractalApp>("Mandelbrot explorer");
}


