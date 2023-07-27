#![allow(dead_code)]
// #![allow(unused_imports)]

mod app_base;
mod event;
mod math;
mod fractal_app;
mod wgpu_renderer;
mod custom_math;


use crate::app_base::run;
use crate::fractal_app::FractalApp;

fn main() {
    run::<FractalApp>("UI App");
}


