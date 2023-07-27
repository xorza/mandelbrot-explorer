use wgpu::*;

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
}

impl App for WgpuApp {
    fn init(device: &Device,
            queue: &Queue,
            surface_config: &SurfaceConfiguration) -> Self {
        let window_size = Vec2u32::new(surface_config.width, surface_config.height);
        let renderer = WgpuRenderer::new(device, queue, surface_config, window_size);


        Self {
            window_size,
            renderer,
            manipulate_state: ManipulateState::Idle,
        }
    }

    fn update(&mut self, event: Event) -> EventResult {
        let result = match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resize(_size) => EventResult::Redraw,

            Event::MouseWheel(delta) => {
                self.renderer.scale *= (1.0 + delta / 20.0) as f64;

                EventResult::Redraw
            }
            Event::MouseMove { position: _position, delta } => {
                match self.manipulate_state {
                    ManipulateState::Idle => EventResult::Continue,
                    ManipulateState::Drag => {
                        let mut delta = Vec2f64::from(delta) / Vec2f64::from(self.window_size);
                        delta.y = -delta.y;
                        self.renderer.offset += delta * self.renderer.scale;

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
            self.renderer.invalidate();
        }

        result
    }

    fn render(&mut self, render_info: RenderInfo) {
        self.renderer.go(&render_info);
    }

    fn resize(&mut self, device: &Device, queue: &Queue, window_size: Vec2u32) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
        self.renderer.resize(device, queue, window_size);
    }
}

impl WgpuApp {}
