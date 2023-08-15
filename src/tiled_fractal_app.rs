#![allow(unused_parens)]

use std::sync::{Arc, Mutex};

use bytemuck::Zeroable;
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

use crate::app_base::{App, RenderInfo};
use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::mandel_texture::MandelTexture;
use crate::math::{RectF64, Vec2f64, Vec2i32, Vec2u32};
use crate::wgpu_renderer::{ScreenTexBindGroup, WgpuRenderer};

enum ManipulateState {
    Idle,
    Drag,
}

pub struct TiledFractalApp {
    window_size: Vec2u32,
    renderer: WgpuRenderer,
    event_loop: EventLoopProxy<UserEvent>,
    runtime: Runtime,

    manipulate_state: ManipulateState,

    offset: Vec2f64,
    scale: f64,

    mandel_texture: MandelTexture,
    screen_tex_bind_group: ScreenTexBindGroup,

    has_update_tiles: bool,
}


#[derive(Debug)]
pub enum UserEvent {
    Redraw,
    TileReady {
        tile_index: usize,
    },
}

impl App for TiledFractalApp {
    type UserEventType = UserEvent;

    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_config: &wgpu::SurfaceConfiguration,
        event_loop_proxy: EventLoopProxy<UserEvent>,
    ) -> TiledFractalApp
    {
        let window_size = Vec2u32::new(surface_config.width, surface_config.height);
        let renderer = WgpuRenderer::new(device, queue, surface_config, window_size);

        let mut mandel_texture = MandelTexture::new(device);
        mandel_texture.fractal_scale = window_size.x as f64 / mandel_texture.tex_size.x as f64;

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &renderer.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mandel_texture.texture_view1),
                },
            ],
            label: None,
        });

        let screen_tex_bind_group = ScreenTexBindGroup {
            bind_group,
            texture_size: mandel_texture.tex_size,
        };

        Self {
            window_size,
            renderer,
            event_loop: event_loop_proxy,
            runtime: Runtime::new().unwrap(),

            manipulate_state: ManipulateState::Idle,

            offset: Vec2f64::zeroed(),
            scale: 0.25f64,

            mandel_texture,
            screen_tex_bind_group,

            has_update_tiles: false,
        }
    }

    fn update(&mut self, event: Event<UserEvent>) -> EventResult {
        let result = match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resized(_size) => EventResult::Redraw,

            Event::MouseWheel(position, delta) => {
                self.move_scale(position, Vec2i32::zeroed(), delta);

                EventResult::Redraw
            }
            Event::MouseMove { position, delta } => {
                match self.manipulate_state {
                    ManipulateState::Idle => EventResult::Continue,
                    ManipulateState::Drag => {
                        self.move_scale(position, delta, 0.0);

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
                self.update_user_event(event)
            }

            Event::Init => {
                self.update_fractal();

                EventResult::Continue
            }

            _ => EventResult::Continue
        };

        result
    }

    fn render(&mut self, render_info: RenderInfo) {
        if self.has_update_tiles {
            self.has_update_tiles = false;
            self.update_tiles(&render_info);
        }

        // let offset = self.offset / Vec2f64::from(self.window_size);
        let offset = Vec2f64::zeroed();

        self.renderer.go(
            &render_info,
            &self.screen_tex_bind_group,
            offset,
        );
    }

    fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, window_size: Vec2u32) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
        self.renderer.resize(device, queue, window_size);
    }
}

impl TiledFractalApp {
    fn move_scale(&mut self, mouse_pos: Vec2u32, mouse_delta: Vec2i32, scroll_delta: f32) {
        let mouse_pos = Vec2i32::new(mouse_pos.x as i32, self.window_size.y as i32 - mouse_pos.y as i32);
        let mouse_pos = Vec2f64::from(mouse_pos) / Vec2f64::from(self.window_size);
        let mouse_pos = mouse_pos * 2.0f64 - 1.0f64;

        let mouse_delta = 2.0f64 * Vec2f64::from(mouse_delta) / Vec2f64::from(self.window_size);
        let mouse_delta = Vec2f64::new(mouse_delta.x, -mouse_delta.y);

        let zoom = 1.15f64.powf(scroll_delta as f64 / 5.0f64);

        let old_scale = self.scale;
        let new_scale = old_scale * zoom;

        let old_offset = self.offset;
        let new_offset =
            old_offset
                + mouse_delta * new_scale
                - mouse_pos * (new_scale - old_scale);

        self.scale = new_scale;
        self.offset = new_offset;

        self.update_fractal();
    }

    fn update_user_event(&mut self, event: UserEvent) -> EventResult {
        match event {
            UserEvent::Redraw => EventResult::Redraw,
            UserEvent::TileReady { tile_index: _tile_index } => {
                self.has_update_tiles = true;
                EventResult::Redraw
            }
        }
    }

    fn update_fractal(&mut self) {
        let size = self.scale * Vec2f64::new(self.window_size.x as f64 / self.window_size.y as f64, 1.0);
        let frame_rect = RectF64::new(
            self.offset - size / 2.0f64,
            size,
        );

        println!("frame_rect: {:?}", frame_rect);

        let event_loop_proxy =
            Arc::new(Mutex::new(self.event_loop.clone()));

        self.mandel_texture.update(
            frame_rect,
            move |index| {
                event_loop_proxy.lock().unwrap().send_event(
                    UserEvent::TileReady {
                        tile_index: index,
                    }
                ).unwrap();
            },
        );
    }

    fn update_tiles(&self, render_info: &RenderInfo) {
        self.mandel_texture.upload_tiles(render_info);
    }
}
