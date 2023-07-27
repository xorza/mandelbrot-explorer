#![allow(unused_parens)]

use bytemuck::Zeroable;
use num_complex::Complex;
use rayon::iter::IndexedParallelIterator;
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;

use crate::app_base::{App, RenderInfo};
use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::math::{Vec2f64, Vec2i32, Vec2u32};
use crate::wgpu_renderer::WgpuRenderer;

enum ManipulateState {
    Idle,
    Drag,
}

pub struct FractalApp {
    window_size: Vec2u32,
    renderer: WgpuRenderer,
    manipulate_state: ManipulateState,
    is_dirty: bool,

    offset: Vec2f64,
    scale: f64,
}


fn mandelbrot(size: Vec2u32, offset: Vec2f64, scale: f64) -> Vec<u8> {
    let mut buffer: Vec<u8> = vec![0; (size.x * size.y) as usize];
    let width = size.x as f64;
    let height = size.y as f64;
    let aspect = width / height;

    let start = std::time::Instant::now();

    buffer
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, pixel)| {
            let x = (i as f64 % width) / width;
            let y = (i as f64 / height) / (aspect * height);

            let cx = (x - 0.5) * scale - offset.x;
            let cy = (y - 0.5) * scale - offset.y;

            let cx = cx * aspect;

            let c: Complex<f64> = Complex::new(cx, cy);
            let mut z: Complex<f64> = Complex::new(0.0, 0.0);

            let mut it: u32 = 0;
            const MAX_IT: u32 = 256;

            while z.norm() <= 8.0 && it <= MAX_IT {
                z = z * z + c;
                it += 1;
            }

            *pixel = it as u8;
        });

    let elapsed = start.elapsed();
    println!("Mandelbrot rendered in {}ms", elapsed.as_millis());

    buffer
}


impl App for FractalApp {
    fn init(device: &wgpu::Device,
            queue: &wgpu::Queue,
            surface_config: &wgpu::SurfaceConfiguration) -> Self {
        let window_size = Vec2u32::new(surface_config.width, surface_config.height);
        let renderer = WgpuRenderer::new(device, queue, surface_config, window_size);

        let offset = Vec2f64::zeroed();
        let scale = 1.0f64;

        Self {
            window_size,
            renderer,
            manipulate_state: ManipulateState::Idle,
            is_dirty: true,
            offset,
            scale,
        }
    }

    fn update(&mut self, event: Event) -> EventResult {
        let result = match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resized(_size) => EventResult::Redraw,

            Event::MouseWheel(position, delta) => {
                let zoom = 1.15f64.powf(delta as f64 / 5.0);
                self.move_scale(position, Vec2i32::zeroed(), zoom);

                EventResult::Redraw
            }
            Event::MouseMove { position, delta } => {
                match self.manipulate_state {
                    ManipulateState::Idle => EventResult::Continue,
                    ManipulateState::Drag => {
                        self.move_scale(position, delta, 1.0);

                        EventResult::Redraw
                    }
                }
            }
            Event::MouseButton(btn, state, _position) => {
                match (btn, state) {
                    (MouseButtons::Left, ElementState::Pressed) => {
                        self.manipulate_state = ManipulateState::Drag;
                        EventResult::Continue
                    }
                    _ => {
                        self.manipulate_state = ManipulateState::Idle;
                        EventResult::Continue
                    }
                }
            }

            _ => EventResult::Continue
        };

        if matches!(result, EventResult::Redraw) {
            self.is_dirty = true;
        }

        result
    }

    fn render(&mut self, render_info: RenderInfo) {
        if self.is_dirty {
            let tex_scale = 3u32;
            let tex_size = Vec2u32::new(self.window_size.x / tex_scale, self.window_size.y / tex_scale);
            let texels = mandelbrot(tex_size, self.offset, self.scale);
            self.renderer.update_texture(&render_info, tex_size, texels.as_slice());
        }

        self.renderer.go(&render_info);
    }

    fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, window_size: Vec2u32) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
        self.renderer.resize(device, queue, window_size);
        self.is_dirty = true;
    }
}

impl FractalApp {
    fn move_scale(&mut self, mouse_pos: Vec2u32, mouse_delta: Vec2i32, zoom: f64) {
        let mouse_pos = Vec2f64::from(mouse_pos)
            / Vec2f64::from(self.window_size);
        let mouse_pos = Vec2f64::new(mouse_pos.x, 1.0 - mouse_pos.y);

        let mouse_delta = Vec2f64::from(mouse_delta)
            / Vec2f64::from(self.window_size);
        let mouse_delta = Vec2f64::new(mouse_delta.x, -mouse_delta.y);

        let old_scale = self.scale;
        let new_scale = old_scale / zoom;

        let old_offset = self.offset;
        let new_offset = mouse_delta * new_scale + old_offset + (mouse_pos - 0.5) * (new_scale - old_scale);

        self.scale = new_scale;
        self.offset = new_offset;
    }
}
