use glam::{IVec2, UVec2};

#[derive(PartialEq, Debug, Clone)]
pub enum MouseButtons {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u8),
}

#[derive(PartialEq, Debug, Clone)]
pub enum ElementState {
    Pressed,
    Released,
}

#[derive(PartialEq, Debug, Clone)]
pub enum Event<UserEvent> {
    Resized(UVec2),
    WindowClose,
    RedrawFinished,
    MouseWheel(UVec2, f32),
    MouseMove { position: UVec2, delta: IVec2 },
    MouseButton(MouseButtons, ElementState, UVec2),
    Custom(UserEvent),
    TouchpadMagnify(UVec2, f32),
    KeyboardInput(winit::event::KeyEvent),
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
            winit::event::MouseButton::Back => MouseButtons::Back,
            winit::event::MouseButton::Forward => MouseButtons::Forward,
        }
    }
}
