# Mandelbrot explorer

## Description
#### MSRV 1.77.2 nightly
Desktop UI application for exploring the Mandelbrot set. Draggable and zoomable.
Calculation is done on CPU with 64 bit precision.
Written on Rust. Uses winit, wgpu, tokio and portable_simd.
Multithreaded, uses SIMD.
Preview drag and zoom done on GPU.

Uses nightly toolchain for SIMD support.

Runs pretty smooth on my Macbook Air M2 2022.
The following single-threaded 2048x2048 image render with 1024 max iterations takes 135ms:

![bench.png](/doc/bench.png)


## Additional images
https://youtu.be/W6jAF17scfc

[![Youtube demo](https://img.youtube.com/vi/W6jAF17scfc/0.jpg)](https://www.youtube.com/watch?v=W6jAF17scfc)

![screenshot1](/doc/Screenshot%202023-08-21%20at%206.23.35%20PM.png)

![scrennshot2](/doc/Screenshot%202024-05-18%20at%208.45.35%E2%80%AFAM.png)
