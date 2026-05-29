use bytemuck::Zeroable;
use glam::{DVec2, IVec2, UVec2};
use winit::event_loop::EventLoopProxy;

use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::mandel_texture::MandelTexture;
use crate::math::DRect;
use crate::{RenderContext, WindowContext};

#[derive(Debug)]
pub struct TiledFractalApp {
    window_size: UVec2,
    event_loop_proxy: EventLoopProxy<UserEvent>,

    dragging: bool,

    frame_rect: DRect,

    mandel_texture: MandelTexture,
}

#[derive(Debug)]
pub enum UserEvent {
    TileReady,
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
        let frame_rect = DRect::from_center_size(DVec2::new(-0.74, 0.0), aspect * 2.5);

        let mut result = Self {
            window_size,
            event_loop_proxy,

            dragging: false,

            frame_rect,

            mandel_texture,
        };
        result.update_fractal(result.frame_rect.center());
        result
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
            Event::MouseMove { position, delta } => {
                if self.dragging {
                    self.move_scale(position, delta, 0.0);
                    EventResult::Redraw
                } else {
                    EventResult::Continue
                }
            }
            Event::MouseButton(btn, state, _position) => {
                self.dragging = matches!((btn, state), (MouseButtons::Left, ElementState::Pressed));
                EventResult::Continue
            }

            Event::Custom(event) => self.update_user_event(event),

            _ => EventResult::Continue,
        }
    }

    pub fn render(&mut self, render_info: &RenderContext) {
        self.mandel_texture.render(render_info);
    }

    fn move_scale(&mut self, mouse_pos: UVec2, mouse_delta: IVec2, scroll_delta: f32) {
        let result = frame_after_manipulation(
            self.frame_rect,
            self.window_size,
            mouse_pos,
            mouse_delta,
            scroll_delta,
        );
        self.frame_rect = result.frame_rect;
        self.update_fractal(result.focus);
    }

    fn update_user_event(&mut self, event: UserEvent) -> EventResult {
        match event {
            UserEvent::TileReady => EventResult::Redraw,
        }
    }

    fn update_fractal(&mut self, focus: DVec2) {
        let event_loop_proxy = self.event_loop_proxy.clone();

        self.mandel_texture.update(self.frame_rect, focus, move || {
            // A worker can finish after the event loop has closed (shutdown);
            // the redraw it wants is moot then, so a closed loop is not an error.
            let _ = event_loop_proxy.send_event(UserEvent::TileReady);
        });
    }
}

#[derive(Debug)]
struct ManipulateResult {
    frame_rect: DRect,
    focus: DVec2,
}

/// Pure pan/zoom math: maps the current viewport plus a pointer interaction to
/// the new viewport. Zoom is anchored at the cursor — the fractal point under
/// the pointer stays under the pointer — and `focus` is exactly that point, fed
/// to the tiler as compute priority. `mouse_delta` pans; `scroll_delta` zooms.
fn frame_after_manipulation(
    frame_rect: DRect,
    window_size: UVec2,
    mouse_pos: UVec2,
    mouse_delta: IVec2,
    scroll_delta: f32,
) -> ManipulateResult {
    // Pointer in normalized, y-up, origin-at-center coordinates ([-0.5, 0.5]).
    let mouse_pos = IVec2::new(
        mouse_pos.x as i32,
        window_size.y as i32 - mouse_pos.y as i32,
    );
    let mouse_pos = DVec2::from(mouse_pos) / DVec2::from(window_size) - 0.5;

    let mouse_delta = DVec2::from(mouse_delta) / DVec2::from(window_size);
    let mouse_delta = DVec2::new(mouse_delta.x, -mouse_delta.y);

    let zoom = 1.15f64.powf(scroll_delta as f64 / 5.0f64);

    let old_size = frame_rect.size;
    let new_size = old_size * zoom;

    let old_center = frame_rect.center();
    let new_center = old_center - mouse_delta * new_size - mouse_pos * (new_size - old_size);

    let frame_rect = DRect::from_center_size(new_center, new_size);
    let focus = frame_rect.center() + frame_rect.size * mouse_pos;

    ManipulateResult { frame_rect, focus }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: UVec2 = UVec2::new(100, 100);

    fn rect(cx: f64, cy: f64, sx: f64, sy: f64) -> DRect {
        DRect::from_center_size(DVec2::new(cx, cy), DVec2::new(sx, sy))
    }

    fn assert_close(a: DVec2, b: DVec2, what: &str) {
        assert!((a - b).length() < 1e-9, "{what}: {a:?} != {b:?}");
    }

    #[test]
    fn drag_pans_opposite_to_cursor_motion_without_resizing() {
        // Cursor at center, drag right by 10px. No scroll => size unchanged.
        let r = frame_after_manipulation(
            rect(0.0, 0.0, 2.0, 2.0),
            WIN,
            UVec2::new(50, 50),
            IVec2::new(10, 0),
            0.0,
        );
        // new_center = (0,0) - (10/100, 0)*(2,2) = (-0.2, 0); focus at cursor (mp=0).
        assert_close(r.frame_rect.size, DVec2::new(2.0, 2.0), "size");
        assert_close(r.frame_rect.center(), DVec2::new(-0.2, 0.0), "center");
        assert_close(r.focus, DVec2::new(-0.2, 0.0), "focus");
    }

    #[test]
    fn zoom_at_center_keeps_center_and_scales_by_1_15() {
        let r = frame_after_manipulation(
            rect(0.0, 0.0, 2.0, 2.0),
            WIN,
            UVec2::new(50, 50),
            IVec2::ZERO,
            5.0, // 1.15^(5/5) = 1.15
        );
        assert_close(r.frame_rect.size, DVec2::new(2.3, 2.3), "size");
        assert_close(r.frame_rect.center(), DVec2::new(0.0, 0.0), "center");
        assert_close(r.focus, DVec2::new(0.0, 0.0), "focus");
    }

    #[test]
    fn zoom_is_anchored_at_the_cursor() {
        // Cursor 1/4 to the right of center; the fractal point under it must not move.
        let frame = rect(0.0, 0.0, 2.0, 2.0);
        let mouse = UVec2::new(75, 50);

        // Fractal point under the cursor before the zoom.
        // mp = (75/100, 50/100) flipped-y - 0.5 = (0.25, 0.0); point = center + size*mp.
        let mp = DVec2::new(0.25, 0.0);
        let point_before = frame.center() + frame.size * mp;
        assert_close(point_before, DVec2::new(0.5, 0.0), "point under cursor");

        let r = frame_after_manipulation(frame, WIN, mouse, IVec2::ZERO, 5.0);

        // After zooming, the same screen position maps to the same fractal point.
        let point_after = r.frame_rect.center() + r.frame_rect.size * mp;
        assert_close(point_after, point_before, "cursor anchor invariant");
        assert_close(r.frame_rect.size, DVec2::new(2.3, 2.3), "size");
        // focus is exactly the anchored point.
        assert_close(r.focus, point_before, "focus");
    }
}
