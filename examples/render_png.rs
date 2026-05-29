//! Offscreen render of the Mandelbrot view to a PNG — no window or display.
//!
//! Drives the real CPU kernel, uploads its output to a float texture, and runs
//! the real screen shader (smooth μ coloring + embedded palette, or `--de`
//! distance-estimation shading) into an offscreen sRGB target, then reads it
//! back and writes a PNG. Useful for eyeballing coloring changes in headless
//! environments where the winit app can't open a window.
//!
//! Usage:
//!   cargo run --release --example render_png -- [out.png] [size] [cx cy half] [max_iter] [--de]
//! Defaults: mandelbrot.png 1024  (full set: -0.5 0 1.5)  auto max_iter.

use std::sync::mpsc;

use encase::{ShaderType, UniformBuffer};
use fractal::mandelbrot_simd::{DePixel, MAX_ITER, Pixel, mandelbrot_simd, mandelbrot_simd_de};
use fractal::math::DRect;
use glam::{DVec2, Mat4, UVec2, Vec2};

/// Immediate block for both shaders (layout via `encase`). `pixel_spacing` is
/// read by the DE shader; the smooth shader treats it as trailing padding.
#[derive(ShaderType)]
struct DrawParams {
    proj_mat: mint::ColumnMatrix4<f32>,
    texture_size: mint::Vector2<f32>,
    pixel_spacing: f32,
}

impl DrawParams {
    fn as_bytes(&self) -> Vec<u8> {
        let mut buffer = UniformBuffer::new(Vec::new());
        buffer.write(self).unwrap();
        buffer.into_inner()
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let de_mode = raw.iter().any(|a| a == "--de");
    let mut pos = raw.into_iter().filter(|a| a != "--de");

    let out_path = pos.next().unwrap_or_else(|| "mandelbrot.png".to_string());
    let size = pos
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1024)
        .next_multiple_of(8);

    let parse = |a: &mut dyn Iterator<Item = String>| a.next().and_then(|s| s.parse::<f64>().ok());
    let cx = parse(&mut pos).unwrap_or(-0.5);
    let cy = parse(&mut pos).unwrap_or(0.0);
    let half = parse(&mut pos).unwrap_or(1.5);
    let rect = DRect::from_pos_size(DVec2::new(cx - half, cy - half), DVec2::splat(2.0 * half));

    let max_iter = pos
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(|| {
            (1000.0 + (1.0 / rect.size.length_squared()).log2().max(0.0) * 50.0) as u32
        })
        .min(MAX_ITER);

    let tile_size = UVec2::splat(size);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let pixel_spacing = (rect.size.x / size as f64) as f32;
    eprintln!(
        "rendering {size}×{size} @ ({cx}, {cy}) half {half}, max_iter {max_iter}{}",
        if de_mode { ", distance estimation" } else { "" }
    );

    // CPU render, then GPU color + readback. Both paths share one render helper;
    // only the texel format and shader differ.
    let params = DrawParams {
        proj_mat: Mat4::IDENTITY.into(),
        texture_size: Vec2::splat(size as f32).into(),
        pixel_spacing,
    };
    if de_mode {
        let mut buf = vec![DePixel::default(); (size * size) as usize];
        assert!(mandelbrot_simd_de(
            rect, tile_size, max_iter, cancel, &mut buf
        ));
        pollster::block_on(render(
            bytemuck::cast_slice(&buf),
            wgpu::TextureFormat::Rgba32Float,
            include_str!("../src/de_shader.wgsl"),
            params,
            size,
            &out_path,
        ));
    } else {
        let mut buf = vec![Pixel::default(); (size * size) as usize];
        assert!(mandelbrot_simd(rect, tile_size, max_iter, cancel, &mut buf));
        pollster::block_on(render(
            bytemuck::cast_slice(&buf),
            wgpu::TextureFormat::Rg32Float,
            include_str!("../src/screen_shader.wgsl"),
            params,
            size,
            &out_path,
        ));
    }
    eprintln!("wrote {out_path}");
}

/// Uploads `data_bytes` (a `size×size` texture in `data_format`), runs `shader`
/// into an offscreen sRGB target with the embedded palette, and writes a PNG.
async fn render(
    data_bytes: &[u8],
    data_format: wgpu::TextureFormat,
    shader_src: &str,
    params: DrawParams,
    size: u32,
    out_path: &str,
) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: Default::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .expect("no GPU adapter");

    let limits = wgpu::Limits {
        max_immediate_size: 256,
        ..Default::default()
    }
    .using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::IMMEDIATES,
            required_limits: limits,
            experimental_features: Default::default(),
            memory_hints: Default::default(),
            trace: Default::default(),
        })
        .await
        .expect("no device");

    let extent = wgpu::Extent3d {
        width: size,
        height: size,
        depth_or_array_layers: 1,
    };
    let texel_size = data_format.target_pixel_byte_cost().unwrap();

    let data_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: data_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
        label: None,
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &data_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * texel_size),
            rows_per_image: Some(size),
        },
        extent,
    );
    let data_view = data_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let palette_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D1,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
        label: None,
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &palette_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        include_bytes!("../palette.rgba"),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256 * 4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let palette_view = palette_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D1,
                },
                count: None,
            },
        ],
        label: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&data_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&palette_view),
            },
        ],
        label: None,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: <DrawParams as encase::ShaderSize>::SHADER_SIZE.get() as u32,
        label: None,
    });

    let target_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(target_format.into())],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            front_face: wgpu::FrontFace::Cw,
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let target = device.create_texture(&wgpu::TextureDescriptor {
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
        label: None,
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded_bpr = size * 4;
    let padded_bpr = unpadded_bpr.next_multiple_of(align);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded_bpr * size) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_immediates(0, &params.as_bytes());
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bpr),
                rows_per_image: Some(size),
            },
        },
        extent,
    );
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    rx.recv().unwrap().unwrap();

    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size {
        let start = (row * padded_bpr) as usize;
        pixels.extend_from_slice(&mapped[start..start + unpadded_bpr as usize]);
    }
    drop(mapped);
    readback.unmap();

    let image: image::RgbaImage =
        image::ImageBuffer::from_raw(size, size, pixels).expect("buffer size matches");
    image.save(out_path).expect("write png");
}
