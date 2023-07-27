#![allow(unused_parens)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bytemuck::Zeroable;
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

use crate::app_base::{App, RenderInfo};
use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::mandelbrot::mandelbrot1;
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

    final_offset: Vec2f64,
    final_scale: f64,

    draft_offset: Vec2f32,
    draft_scale: f32,

    cancel_token: Arc<AtomicU32>,

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

        Self {
            window_size,
            renderer,
            event_loop: event_loop_proxy,
            runtime: Runtime::new().unwrap(),

            manipulate_state: ManipulateState::Idle,

            final_offset: Vec2f64::zeroed(),
            final_scale: 1.0f64,

            draft_offset: Vec2f32::zeroed(),
            draft_scale: 1.0f32,

            cancel_token: Arc::new(AtomicU32::new(0)),
            updated_texture: None,
        }
    }

    fn update(&mut self, event: Event<UserEvent>) -> EventResult {
        let result = match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resized(_size) => EventResult::Redraw,

            Event::MouseWheel(position, delta) => {
                self.move_scale(position, Vec2i32::zeroed(), delta);
                self.rerender();

                EventResult::Redraw
            }
            Event::MouseMove { position, delta } => {
                match self.manipulate_state {
                    ManipulateState::Idle => EventResult::Continue,
                    ManipulateState::Drag => {
                        self.move_scale(position, delta, 0.0);
                        self.rerender();

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
            }

            _ => EventResult::Continue
        };

        result
    }

    fn render(&mut self, render_info: RenderInfo) {
        if let Some((tex_size, texels)) = self.updated_texture.take() {
            self.renderer.update_texture(&render_info, tex_size, texels.as_slice());

            self.draft_scale = 1.0f32;
            self.draft_offset = Vec2f32::zeroed();
        }

        self.renderer.go(&render_info, self.draft_offset, self.draft_scale);
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
    fn move_scale(&mut self, mouse_pos: Vec2u32, mouse_delta: Vec2i32, scroll_delta: f32) {
        let mouse_pos = Vec2f32::from(mouse_pos)
            / Vec2f32::from(self.window_size);
        let mouse_pos = Vec2f32::new(mouse_pos.x, 1.0 - mouse_pos.y);

        let mouse_delta = Vec2f32::from(mouse_delta)
            / Vec2f32::from(self.window_size);
        let mouse_delta = Vec2f32::new(mouse_delta.x, -mouse_delta.y);

        let zoom = 1.15f32.powf(scroll_delta / 5.0);

        {
            let old_final_scale = self.final_scale;
            let new_final_scale = old_final_scale / zoom as f64;

            let old_offset = self.final_offset;
            let new_offset =
                Vec2f64::from(mouse_delta) * new_final_scale
                    + old_offset
                    + (Vec2f64::from(mouse_pos) - 0.5) * (new_final_scale - old_final_scale);

            self.final_scale = new_final_scale;
            self.final_offset = new_offset;
        }

        {
            let mouse_pos = mouse_pos * 2.0f32 - 1.0f32;

            let old_draft_scale = self.draft_scale;
            let new_draft_scale = old_draft_scale * zoom;

            let old_draft_offset = self.draft_offset;
            let new_draft_offset =
                2.0 * mouse_delta * new_draft_scale
                    + old_draft_offset
                    - mouse_pos * (new_draft_scale - old_draft_scale);

            self.draft_scale = new_draft_scale;
            self.draft_offset = new_draft_offset;
        }
    }

    fn update_user_event(&mut self, event: UserEvent) -> EventResult {
        match event {
            UserEvent::UpdateTexture { size, texels } => {
                self.updated_texture = Some((size, texels));

                EventResult::Redraw
            }

            _ => EventResult::Continue
        }
    }

    fn rerender(&mut self) {
        self.cancel_token.fetch_add(1, Ordering::Relaxed);

        let event_loop = self.event_loop.clone();
        let window_size = self.window_size;
        let scale = self.final_scale;
        let offset = self.final_offset;
        let cancel_token = self.cancel_token.clone();

        self.runtime.spawn(async move {
            let tex_scale = 0.5f32;
            let tex_size = tex_scale * Vec2f32::from(window_size);
            let tex_size = Vec2u32::from(tex_size);

            let texels = mandelbrot1(tex_size, offset, scale, cancel_token)
                .ok();

            if let Some(texels) = texels {
                event_loop.send_event(UserEvent::UpdateTexture {
                    size: tex_size,
                    texels,
                }).unwrap();
            }
        });
    }
}
