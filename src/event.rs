use crate::math::{Vec2i32, Vec2u32};

#[derive(PartialEq, Debug, Clone)]
pub enum MouseButtons {
    Left,
    Right,
    Middle,
    Other(u8),
}

#[derive(PartialEq, Debug, Clone)]
pub enum ElementState {
    Pressed,
    Released,
}

#[derive(PartialEq, Debug, Clone)]
pub enum Event<UserEvent> {
    Init,
    Resized(Vec2u32),
    WindowClose,
    RedrawFinished,
    MouseWheel(Vec2u32, f32),
    MouseMove {
        position: Vec2u32,
        delta: Vec2i32,
    },
    MouseButton(MouseButtons, ElementState, Vec2u32),
    Custom(UserEvent),
    Unknown,
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum EventResult {
    Continue,
    Redraw,
    Exit,
}


impl From<winit::event::ElementState> for ElementState {
    fn from(value: winit::event::ElementState) -> Self {
        match value {
            winit::event::ElementState::Pressed => ElementState::Pressed,
            winit::event::ElementState::Released => ElementState::Released,
        }
    }
}
impl From<winit::event::MouseButton> for MouseButtons {
    fn from(value: winit::event::MouseButton) -> Self {
        match value {
            winit::event::MouseButton::Left => MouseButtons::Left,
            winit::event::MouseButton::Right => MouseButtons::Right,
            winit::event::MouseButton::Middle => MouseButtons::Middle,
            winit::event::MouseButton::Other(other) => MouseButtons::Other(other as u8),
        }
    }
}