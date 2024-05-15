#![allow(unused_parens)]

use std::sync::{Arc, Mutex};

use bytemuck::Zeroable;
use glam::{DVec2, IVec2, UVec2};
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::mandel_texture::MandelTexture;
use crate::math::DRect;
use crate::{RenderContext, WindowContext};

enum ManipulateState {
    Idle,
    Drag,
}

pub struct TiledFractalApp {
    window_size: UVec2,
    event_loop_proxy: Arc<Mutex<EventLoopProxy<UserEvent>>>,
    runtime: Runtime,

    manipulate_state: ManipulateState,

    frame_rect: DRect,
    aspect: DVec2,

    mandel_texture: MandelTexture,
}

#[derive(Debug)]
pub enum UserEvent {
    Redraw,
    TileReady { tile_index: usize },
}

impl TiledFractalApp {
    pub fn new(
        window_state: &WindowContext,
        event_loop_proxy: EventLoopProxy<UserEvent>,
    ) -> TiledFractalApp {
        let window_size = UVec2::new(
            window_state.surface_config.width,
            window_state.surface_config.height,
        );

        let mandel_texture = MandelTexture::new(
            &window_state.device,
            &window_state.queue,
            &window_state.surface_config,
            window_size,
        );

        let aspect = DVec2::new(window_size.x as f64 / window_size.y as f64, 1.0);
        let frame_rect = DRect::from_center_size(DVec2::zeroed(), aspect * 2.5);

        let mut result = Self {
            window_size,
            event_loop_proxy: Arc::new(Mutex::new(event_loop_proxy)),
            runtime: Runtime::new().unwrap(),

            manipulate_state: ManipulateState::Idle,

            frame_rect,
            aspect,

            mandel_texture,
        };
        result.update_fractal(result.frame_rect.center());
        return result;
    }

    pub fn update(&mut self, event: Event<UserEvent>) -> EventResult {
        match event {
            Event::WindowClose => EventResult::Exit,
            Event::Resized(window_size) => {
                if self.window_size == window_size {
                    return EventResult::Continue;
                }

                self.frame_rect = DRect::from_center_size(
                    self.frame_rect.center(),
                    self.frame_rect.size * DVec2::from(window_size) / DVec2::from(self.window_size),
                );
                self.window_size = window_size;
                self.mandel_texture.resize_window(window_size);

                self.update_fractal(self.frame_rect.center());

                EventResult::Redraw
            }

            Event::MouseWheel(position, delta) => {
                self.move_scale(position, IVec2::zeroed(), 3.0 * delta);

                EventResult::Redraw
            }
            Event::MouseMove { position, delta } => match self.manipulate_state {
                ManipulateState::Idle => EventResult::Continue,
                ManipulateState::Drag => {
                    self.move_scale(position, delta, 0.0);

                    EventResult::Redraw
                }
            },
            Event::MouseButton(btn, state, _position) => match (btn, state) {
                (MouseButtons::Left, ElementState::Pressed) => {
                    self.manipulate_state = ManipulateState::Drag;
                    EventResult::Continue
                }
                _ => {
                    self.manipulate_state = ManipulateState::Idle;
                    EventResult::Continue
                }
            },

            Event::Custom(event) => self.update_user_event(event),

            _ => EventResult::Continue,
        }
    }

    pub fn render(&mut self, render_info: &RenderContext) {
        self.mandel_texture.render(render_info);
    }

    fn move_scale(&mut self, mouse_pos: UVec2, mouse_delta: IVec2, scroll_delta: f32) {
        let mouse_pos = IVec2::new(
            mouse_pos.x as i32,
            self.window_size.y as i32 - mouse_pos.y as i32,
        );
        let mouse_pos = DVec2::from(mouse_pos) / DVec2::from(self.window_size);
        let mouse_pos = mouse_pos - 0.5f64;

        let mouse_delta = DVec2::from(mouse_delta) / DVec2::from(self.window_size);
        let mouse_delta = DVec2::new(mouse_delta.x, -mouse_delta.y);

        let zoom = 1.15f64.powf(scroll_delta as f64 / 5.0f64);

        let old_size = self.frame_rect.size;
        let new_size = old_size * zoom;

        let old_offset = self.frame_rect.center();
        let new_offset = old_offset - mouse_delta * new_size - mouse_pos * (new_size - old_size);

        self.frame_rect = DRect::from_center_size(new_offset, new_size);

        let focus = self.frame_rect.center() + self.frame_rect.size * mouse_pos;

        self.update_fractal(focus);
    }

    fn update_user_event(&mut self, event: UserEvent) -> EventResult {
        match event {
            UserEvent::Redraw => EventResult::Redraw,
            UserEvent::TileReady {
                tile_index: _tile_index,
            } => EventResult::Redraw,
        }
    }

    fn update_fractal(&mut self, focus: DVec2) {
        let event_loop_proxy = self.event_loop_proxy.clone();

        self.mandel_texture
            .update(self.frame_rect, focus, move |index| {
                event_loop_proxy
                    .lock()
                    .unwrap()
                    .send_event(UserEvent::TileReady { tile_index: index })
                    .unwrap();
            });
    }
}
