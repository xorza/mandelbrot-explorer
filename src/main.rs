#![allow(dead_code)]
// #![allow(unused_imports)]

mod app_base;
mod event;
mod math;
mod wgpu_app;
mod wgpu_renderer;
mod custom_math;


use crate::app_base::run;
use crate::wgpu_app::WgpuApp;

fn main() {
    run::<WgpuApp>("UI App");
}


