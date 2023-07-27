#![allow(unused_parens)]

use bytemuck::Zeroable;
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

use crate::app_base::{App, RenderInfo};
use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::mandelbrot::mandelbrot;
use crate::math::{Vec2f32, Vec2f64, Vec2i32, Vec2u32};
use crate::wgpu_renderer::WgpuRenderer;

enum ManipulateState {
    Idle,
    Drag,
}

pub struct FractalApp {
    window_size: Vec2u32,
    renderer: WgpuRenderer,
    event_loop: EventLoopProxy<UserEvent>,
    runtime: Runtime,

    manipulate_state: ManipulateState,

    offset: Vec2f64,
    scale: f64,

    task_handle: Option<tokio::task::JoinHandle<()>>,
    updated_texture: Option<(Vec2u32, Vec<u8>)>,
}

#[derive(Debug)]
pub enum UserEvent {
    Redraw,
    UpdateTexture {
        size: Vec2u32,
        texels: Vec<u8>,
    },
}

impl App for FractalApp {
    type UserEventType = UserEvent;

    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_config: &wgpu::SurfaceConfiguration,
        event_loop_proxy: EventLoopProxy<UserEvent>,
    ) -> FractalApp
    {
        let window_size = Vec2u32::new(surface_config.width, surface_config.height);
        let renderer = WgpuRenderer::new(device, queue, surface_config, window_size);

        let offset = Vec2f64::zeroed();
        let scale = 1.0f64;

        Self {
            window_size,
            renderer,
            event_loop: event_loop_proxy,
            runtime: Runtime::new().unwrap(),

            manipulate_state: ManipulateState::Idle,

            offset,
            scale,

            task_handle: None,
            updated_texture: None,
        }
    }

    fn update(&mut self, event: Event<UserEvent>) -> EventResult {
        let result = match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resized(_size) => EventResult::Redraw,

            Event::MouseWheel(position, delta) => {
                let zoom = 1.15f64.powf(delta as f64 / 5.0);
                self.move_scale(position, Vec2i32::zeroed(), zoom);
                self.rerender();

                EventResult::Continue
            }
            Event::MouseMove { position, delta } => {
                match self.manipulate_state {
                    ManipulateState::Idle => EventResult::Continue,
                    ManipulateState::Drag => {
                        self.move_scale(position, delta, 1.0);
                        self.rerender();

                        EventResult::Continue
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

            Event::Custom(event) => {
                if matches!(event, UserEvent::Redraw) {
                    EventResult::Redraw
                } else {
                    self.update_user_event(event)
                }
            }

            Event::Init => {
                self.rerender();
                EventResult::Continue
            },

            _ => EventResult::Continue
        };

        result
    }

    fn render(&mut self, render_info: RenderInfo) {
        if let Some((tex_size, texels)) = self.updated_texture.take() {
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
        self.rerender();
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

    fn update_user_event(&mut self, event: UserEvent) -> EventResult {
        match event {
            UserEvent::UpdateTexture { size, texels } => {
                self.updated_texture = Some((size, texels));
                self.task_handle = None;

                EventResult::Redraw
            }

            _ => EventResult::Continue
        }
    }

    fn rerender(&mut self) {
        if self.task_handle.is_some() {
            return;
        }

        let event_loop = self.event_loop.clone();
        let window_size = self.window_size;
        let scale = self.scale;
        let offset = self.offset;

        self.task_handle = Some(self.runtime.spawn(async move {
            let tex_scale = 0.3f32;
            let tex_size = tex_scale * Vec2f32::from(window_size);
            let tex_size = Vec2u32::from(tex_size);

            let texels = mandelbrot(tex_size, offset, scale);

            event_loop.send_event(UserEvent::UpdateTexture {
                size: tex_size,
                texels,
            }).unwrap();
        }));
    }
}
