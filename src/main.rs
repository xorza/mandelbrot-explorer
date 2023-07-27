#![allow(dead_code)]
// #![allow(unused_imports)]

use crate::app_base::run;
use crate::fractal_app::FractalApp;

mod app_base;
mod event;
mod math;
mod fractal_app;
mod wgpu_renderer;
mod render_pods;
mod mandelbrot;


fn main() {
    run::<FractalApp>("Mandelbrot explorer");
}


