# Mandelbrot explorer

## Description
Desktop UI application for exploring the Mandelbrot set. Draggable and zoomable.
Calculation is done on CPU with 64 bit precision.
Written on Rust. Uses winit, wgpu, tokio and portable_simd.
Multithreaded, uses SIMD.
Preview drag and zoom done on GPU.

To enable portable_simd nightly toolchain is used.
It sometimes corrupts executable and it crashed on start, to fix it remove `target` and `Cargo.lock` and rebuild. 

Runs pretty smooth on my Macbook Air M2 2022.
Single-threaded 2048x2048 render with 1024 max iterations takes 540ms.

![doc/Screen Recording 2023-08-18 at 5.35.27 PM.gif](doc/Screen%20Recording%202023-08-18%20at%205.35.27%20PM.gif)
![Screenshot 2023-08-21 at 6.23.35 PM.png](doc%2FScreenshot%202023-08-21%20at%206.23.35%20PM.png)
