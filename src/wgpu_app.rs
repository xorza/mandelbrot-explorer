use std::rc::Rc;

use wgpu::*;

use crate::app_base::{App, RenderInfo};
use crate::event::{Event, EventResult};
use crate::math::UVec2;
use crate::wgpu_renderer::{ WgpuRenderer};

pub struct WgpuApp {
    window_size: UVec2,
    renderer: WgpuRenderer,
}

impl App for WgpuApp {
    fn init(device: &Device,
            queue: &Queue,
            surface_config: &SurfaceConfiguration) -> Self {
        let window_size = UVec2::new(surface_config.width, surface_config.height);
        let renderer = WgpuRenderer::new(device, queue, surface_config, window_size);

        let mut result = Self {
            window_size: UVec2::new(0, 0),
            renderer,
        };
        result.resize(device, queue, window_size);

        result
    }

    fn update(&mut self, event: Event) -> EventResult {
        match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resize(_size) => EventResult::Redraw,

            _ => EventResult::Continue
        }
    }

    fn render(&self, render_info: RenderInfo) {
        self.renderer.go(&render_info);
    }

    fn resize(&mut self, device: &Device, queue: &Queue, window_size: UVec2) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
        self.renderer.resize(device, queue, window_size);
    }
}

impl WgpuApp {

}
