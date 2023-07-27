use num_complex::Complex;
use rayon::iter::IndexedParallelIterator;
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;

use crate::app_base::{App, RenderInfo};
use crate::event::{Event, EventResult};
use crate::math::{Vec2f64, Vec2u32};
use crate::wgpu_renderer::WgpuRenderer;

enum ManipulateState {
    Idle,
    Drag,
}

pub struct WgpuApp {
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
            let x = i as f64 % width;
            let y = i as f64 / height;

            let cx = (x * scale - (offset.x * width)) * aspect / (0.5 * width);
            let cy = (y * scale / aspect - (offset.y * height)) / (0.5 * height);

            let c: Complex<f64> = Complex::new(cx, cy);
            let mut z: Complex<f64> = Complex::new(0.0, 0.0);

            let mut it: u32 = 0;
            const MAX_IT: u32 = 256;

            while z.norm() <= 4.0 && it <= MAX_IT {
                z = z * z + c;
                it += 1;
            }

            *pixel = it as u8;
        });

    let elapsed = start.elapsed();
    println!("Mandelbrot rendered in {}ms", elapsed.as_millis());

    buffer
}


impl App for WgpuApp {
    fn init(device: &wgpu::Device,
            queue: &wgpu::Queue,
            surface_config: &wgpu::SurfaceConfiguration) -> Self {
        let window_size = Vec2u32::new(surface_config.width, surface_config.height);
        let renderer = WgpuRenderer::new(device, queue, surface_config, window_size);

        let offset = Vec2f64::new(0.5, 0.5);
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

            Event::MouseWheel(delta) => {
                self.scale *= (1.0 + delta / 20.0) as f64;

                EventResult::Redraw
            }
            Event::MouseMove { position: _position, delta } => {
                match self.manipulate_state {
                    ManipulateState::Idle => EventResult::Continue,
                    ManipulateState::Drag => {
                        let mut delta = Vec2f64::from(delta) / Vec2f64::from(self.window_size);
                        delta.y = -delta.y;
                        self.offset += delta * self.scale;

                        EventResult::Redraw
                    }
                }
            }
            Event::MouseButton(btn, state, _position) => {
                match (btn, state) {
                    (crate::event::MouseButtons::Left, crate::event::ElementState::Pressed) => {
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
            let tex_scale = 4u32;
            let tex_size = Vec2u32::new(self.window_size.x / tex_scale, self.window_size.y / tex_scale);
            let texels = mandelbrot(tex_size, self.offset, self.scale);
            self.renderer.update_texture(&render_info,tex_size, texels.as_slice());
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

impl WgpuApp {}
