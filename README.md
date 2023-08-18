# Mandelbrot explorer

## Description
Desktop UI application for exploring the Mandelbrot set. Draggable and zoomable.
Calculation is done on CPU, no Simd yet.
Multithreaded.
Draft drag and zoom done on GPU.

Written on Rust. Uses winit, wgpu, rayon and tokio.
Runs pretty smooth on my Macbook Air M2 2022.

![Screen Recording 2023-08-18 at 5.35.27 PM.webm](doc%2FScreen%20Recording%202023-08-18%20at%205.35.27%20PM.mov)