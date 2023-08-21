#![allow(dead_code)]
// #![allow(unused_imports)]

use crate::app_base::run;
use crate::tiled_fractal_app::TiledFractalApp;

mod app_base;
mod event;
mod math;
mod render_pods;
mod mandel_texture;
mod tiled_fractal_app;
mod mandelbrot;
mod env;


fn main() {
    run::<TiledFractalApp>("Mandelbrot explorer");
}


