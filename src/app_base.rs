use std::default::Default;
use std::sync::Arc;
use std::time::Instant;

use bytemuck::Zeroable;
use pollster::FutureExt;
use wgpu::Limits;
use winit::event_loop::{ EventLoop, EventLoopBuilder, EventLoopProxy};

use crate::event::{ElementState, Event, EventResult, MouseButtons};
use crate::math::{Vec2i32, Vec2u32};

pub struct RenderInfo<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub time: f64,
}

pub trait App: 'static + Sized {
    type UserEventType;

    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_config: &wgpu::SurfaceConfiguration,
        event_loop_proxy: EventLoopProxy<Self::UserEventType>,
    ) -> Self;
    fn update(&mut self, event: Event<Self::UserEventType>) -> EventResult;
    fn render(&mut self, render: &RenderInfo);
    fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, window_size: Vec2u32);
}

struct Setup<'a, UserEventType: 'static> {
    window: Arc<winit::window::Window>,
    event_loop: EventLoop<UserEventType>,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface<'a>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn setup<UserEventType: 'static>(title: &str) -> Setup<UserEventType> {
    let event_loop: EventLoop<UserEventType> =
        EventLoopBuilder::<UserEventType>::with_user_event()
            .build()
            .unwrap();
    let window =
        winit::window::WindowBuilder::new()
            .with_title(title)
            // .with_inner_size(LogicalSize::new(1024, 512))
            .build(&event_loop)
            .expect("Failed to create window.");
    let window = Arc::new(window);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: Default::default(),
        dx12_shader_compiler: wgpu::Dx12Compiler::Dxc { dxil_path: None, dxc_path: None },
        gles_minor_version: Default::default(),
    });
    let size = window.inner_size();
    let surface =
        instance.create_surface(Arc::clone(&window)).unwrap();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .block_on()
        .expect("No suitable GPU adapters found on the system.");

    // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the surface.
    let limits = Limits {
        max_push_constant_size: 1024,
        ..Default::default()
    }.using_resolution(adapter.limits());

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::PUSH_CONSTANTS,
                required_limits: limits,
            },
            None,
        )
        .block_on()
        .expect("Unable to find a suitable GPU adapter.");

    Setup {
        window: window.clone(),
        event_loop,
        size,
        surface,
        adapter,
        device,
        queue,
    }
}

fn start<AppType: App>(
    Setup {
        window,
        event_loop,
        size,
        surface,
        adapter,
        device,
        queue,
    }: Setup<AppType::UserEventType>,
) {
    let mut config = surface
        .get_default_config(&adapter, size.width, size.height)
        .expect("Surface isn't supported by the adapter.");
    let surface_view_format = config.format.add_srgb_suffix();
    config.view_formats.push(surface_view_format);
    surface.configure(&device, &config);

    let event_loop_proxy = event_loop.create_proxy();
    let mut app = AppType::new(&device, &queue, &config, event_loop_proxy);

    let start = Instant::now();
    let mut has_error_scope = false;
    let mut mouse_position: Vec2u32 = Vec2u32::zeroed();

    match app.update(Event::Init) {
        EventResult::Continue => {}
        EventResult::Redraw => window.request_redraw(),
        EventResult::Exit => return,
    }

    event_loop.run(move |event, target| {
        let mut result: EventResult = EventResult::Continue;

        if matches!(event, winit::event::Event::AboutToWait) {
            if has_error_scope {
                if let Some(error) = device.pop_error_scope().block_on() {
                    panic!("Device error: {:?}", error);
                }
                has_error_scope = false;
            }

            result = app.update(Event::RedrawFinished);
            return;
        }


        match event {
            winit::event::Event::WindowEvent {
                event:
                winit::event::WindowEvent::Resized(_)
                | winit::event::WindowEvent::ScaleFactorChanged { .. }
                , ..
            } => {
                config.width = size.width.max(1);
                config.height = size.height.max(1);
                surface.configure(&device, &config);

                let window_size = Vec2u32::new(size.width, size.height);

                app.resize(&device, &queue, window_size);
                result = app.update(Event::Resized(window_size));
            }

            winit::event::Event::WindowEvent {
                event: winit::event::WindowEvent::RedrawRequested,
                ..
            } => {
                let surface_texture = match surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(_) => {
                        surface.configure(&device, &config);
                        surface
                            .get_current_texture()
                            .expect("Failed to acquire next surface texture.")
                    }
                };
                let surface_texture_view = surface_texture.texture.create_view(
                    &wgpu::TextureViewDescriptor {
                        format: Some(surface_view_format),
                        ..wgpu::TextureViewDescriptor::default()
                    });

                assert!(!has_error_scope);
                device.push_error_scope(wgpu::ErrorFilter::Validation);
                has_error_scope = true;

                app.render(&RenderInfo {
                    device: &device,
                    queue: &queue,
                    view: &surface_texture_view,
                    time: start.elapsed().as_secs_f64(),
                });

                surface_texture.present();
            }

            winit::event::Event::WindowEvent { event, .. } => {
                let event = process_window_event(event, &mut mouse_position);
                result = app.update(event);
            }

            winit::event::Event::UserEvent(event) => {
                result = app.update(Event::Custom(event));
            }

            _ => {}
        }

        match result {
            EventResult::Continue => {}
            EventResult::Redraw => window.request_redraw(),
            EventResult::Exit => target.exit()
        }
        
    }).unwrap();
}

fn process_window_event<UserEvent>(event: winit::event::WindowEvent, mouse_position: &mut Vec2u32) -> Event<UserEvent> {
    match event {
        winit::event::WindowEvent::Resized(size) =>
            Event::Resized(
                Vec2u32::new(size.width.max(1), size.height.max(1)),
            ),
        winit::event::WindowEvent::Focused(_is_focused) => {
            Event::Unknown
        }
        winit::event::WindowEvent::CursorEntered { .. } => {
            Event::Unknown
        }
        winit::event::WindowEvent::CursorLeft { .. } => {
            Event::Unknown
        }
        winit::event::WindowEvent::CursorMoved { position: _position, .. } => {
            let prev_pos = *mouse_position;
            let new_pos = Vec2u32::new(_position.x as u32, _position.y as u32);
            *mouse_position = new_pos;

            Event::MouseMove {
                position: new_pos,
                delta: Vec2i32::from(new_pos) - Vec2i32::from(prev_pos),
            }
        }
        winit::event::WindowEvent::Occluded(_is_occluded) => {
            Event::Unknown
        }
        winit::event::WindowEvent::MouseInput { state, button, .. } => {
            Event::MouseButton(
                MouseButtons::from(button),
                ElementState::from(state),
                mouse_position.clone(),
            )
        }
        winit::event::WindowEvent::MouseWheel { delta, phase: _phase, .. } => {
            match delta {
                winit::event::MouseScrollDelta::LineDelta(_l1, l2) => {
                    Event::MouseWheel(mouse_position.clone(), l2)
                }
                winit::event::MouseScrollDelta::PixelDelta(pix) => {
                    println!("PIXEL DELTA: {:?}", pix);
                    Event::Unknown
                }
            }
        }
        winit::event::WindowEvent::CloseRequested => {
            Event::WindowClose
        }
        winit::event::WindowEvent::Moved(_position) => {
            Event::Unknown
        }
        _ => Event::Unknown,
    }
}

pub fn run<AppType: App>(title: &str) {
    let setup = setup::<AppType::UserEventType>(title);
    start::<AppType>(setup);
}
