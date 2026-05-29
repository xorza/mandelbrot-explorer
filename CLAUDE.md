# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A desktop Mandelbrot explorer (crate name `fractal`). The set is computed on the **CPU** with `portable_simd` across all physical cores (via a small `ComputePool` of OS threads); the **GPU** (wgpu) only uploads the iteration-count texture, maps it through a palette, and does a cheap reprojection trick to give an instant preview while new tiles compute. Interactive pan/zoom. No async runtime — there is no I/O to wait on.

## Commands

Requires **nightly** (pinned in `rust-toolchain.toml`) for the `portable_simd` feature — there is no stable fallback.

- Run: `cargo run --release` — release is effectively mandatory; the SIMD kernel is unusably slow in debug, and the release profile sets `lto=true`, `codegen-units=1`.
- Test: prefer `cargo test --release` — `mandelbrot_simd::test::draw_mandelbrot` renders a 2048² image and is unusably slow in a debug build. Single test: `cargo test --release escape_counts`.
- Standard verification: `cargo fmt --all && cargo check && cargo clippy --all-targets -- -D warnings`.

The palette is embedded as raw RGBA via `include_bytes!("../palette.rgba")` (256×1, 1024 bytes), decoded ahead of time from `palette.png`. The production build pulls in **no image codec**; the `image` crate is a dev-dependency used only by `draw_mandelbrot`. If you change `palette.png`, regenerate `palette.rgba` (e.g. with PIL: `Image.open('palette.png').convert('RGBA').tobytes()`).

## Architecture

Data flows in two coordinate systems, both `f64` (`DRect` in `math.rs`):
- **`frame_rect`** — the viewport actually visible on screen, in fractal space.
- **`fractal_rect`** — the larger region currently baked into the 4096² texture. It *contains* `frame_rect` with margin. While the frame stays inside it at the same scale, **no recompute happens** — panning just shifts the sampling of the existing texture.

### Module map

- `main.rs` — winit `ApplicationHandler`. `AppState` owns `WindowContext` (wgpu device/queue/surface/config) and the `TiledFractalApp`. Drives the redraw loop in `redraw_if_needed` (acquires surface texture, wraps render in a wgpu validation error scope that **panics** on GPU error), builds the `RenderContext` passed into rendering. `main.rs` also defines `pub struct RenderContext` (borrowed device/queue/view).
- `event.rs` — translates winit events into an internal `Event<UserEvent>` enum, decoupling app logic from winit. Handlers return `EventResult` (`Continue` / `Redraw` / `Exit`) which the main loop acts on.
- `tiled_fractal_app.rs` — application logic. Holds `frame_rect`, the pan/zoom `ManipulateState` (Idle/Drag), and the `MandelTexture`. Translates mouse input into new `frame_rect`s and calls `MandelTexture::update`. Defines `UserEvent::TileReady`.
- `mandel_texture.rs` — **the core**. See below.
- `mandelbrot_simd.rs` — the compute kernel. 8-lane `Simd<f64>` Mandelbrot iteration. `MAX_ITER = 4500`. Output `Pixel` is a single `u16` iteration count. Checks an `AtomicBool` cancel token periodically so abandoned tiles bail early.
- `buffer_pool.rs` — recycling free-list for tile pixel buffers. `BufferHandle` returns its `Vec<u8>` to the pool on `Drop` (via a `Weak` back-ref), avoiding per-tile reallocation; the pool grows on demand and never shrinks.
- `compute_pool.rs` — `ComputePool`, a fixed set of OS worker threads pulling FIFO jobs off an `mpsc` channel. CPU-bound tile renders run here; workers detach and exit when the pool drops. Cancellation is not the pool's concern — jobs poll the `AtomicBool` token themselves.
- `math.rs` — `URect` (texel/integer rects) and `DRect` (fractal-space `f64` rects with intersect/contains/center).
- `render_pods.rs` — `#[repr(C)]` GPU POD structs: `ScreenRect` (full-screen quad verts) and `DrawParams` (proj matrix + texture size), pushed as **immediates** (push constants), not bind-group uniforms.
- `blit_shader.wgsl` / `screen_shader.wgsl` — see rendering below. Texture is `texture_2d<u32>` (raw iteration counts), sampled with `textureLoad`.

### MandelTexture: tiling, async compute, double-buffer preview

The 4096² texture (`TEXTURE_SIZE`) is divided into 128² **tiles** (`TILE_SIZE`). On `update(frame_rect, focus, tile_ready_callback)`:
1. If the frame left the current `fractal_rect` or changed scale, a new `fractal_rect` is chosen and `frame_changed` is set.
2. Tiles are sorted by distance from `focus` so the area the user is looking at computes **first** (this determines `ComputePool` spawn/FIFO order, hence compute priority).
3. Each in-view tile that needs work is handed to `ComputePool` (one job per tile). The `mandelbrot_simd` call takes the tile's fractal-space `DRect` (from `Tile::fractal_rect`) directly. Out-of-view tiles, and tiles being recomputed, have their pending job cancelled by raising the `AtomicBool` token — a job that sees it set bails on its next row and returns `false`. Completed buffers move to `TileState::WaitForUpload`; the job fires `tile_ready_callback`, which posts `UserEvent::TileReady` through the winit `EventLoopProxy` to request a redraw.

`render` = `blit_textures` → `upload_tiles` → `surface_render`:
- **Two texture slots** (`slots: [_; 2]`) act as a double buffer. When the frame changes, `blit_textures` reprojects the *old* texture (`slots[0]`) into `slots[1]` with a scale+translate matrix derived from the ratio of the previous and new `fractal_rect`, giving an immediately-scaled (blurry) preview, then `slots.swap(0, 1)`. This is what makes zoom feel instant before any new tile finishes. `blit_shader.wgsl` does this pass.
- `upload_tiles` writes any `WaitForUpload` tile buffers into `slots[0]` via `queue.write_texture`.
- `surface_render` draws `slots[0]` to the surface, offset/scaled so `fractal_rect` maps onto the visible `frame_rect`, and `screen_shader.wgsl` maps the `u32` iteration count through the 1-D palette (with a smooth-ish `pow(norm, 0.4)` curve and an edge-darkening factor).

`calc_max_iters` raises the iteration cap as you zoom in (`log2(1/size²)`-based, capped at `MAX_ITER`), so deep zooms get more detail.

## Conventions specific to this repo

- GPU validation errors are treated as fatal (`panic!`) — don't add silent error handling around wgpu calls.
- The `frame_rect` ⊇ recompute relationship is the key performance invariant: avoid changes that recompute tiles on every pan within the texture margin.
- Pixel format is a single `u16` iteration count, not color; coloring is entirely shader-side. Keep it that way unless changing both the texture format and `screen_shader.wgsl`.
- `fractal_rect_prev` is written **only** by `blit_textures` (it tracks the rect the current slot's pixels were rendered at). Do not assign it in `update` — several `update`s can coalesce before one render, and reassigning there misaligns the preview reprojection.
