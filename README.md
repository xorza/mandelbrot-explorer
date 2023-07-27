# Mandelbrot explorer

## Description
Desktop UI application for exploring the Mandelbrot set. Draggable and zoomable.
Written on Rust. Uses winit, wgpu, rayon and tokio. Runs pretty smooth on 
my Macbook Air M2 2022.

Fractal calculation is done on CPU, no Simd yet. Draft drag and zoom
done on GPU.

![Screenshot 2023-07-27 at 11.38.29 PM.png](doc%2FScreenshot%202023-07-27%20at%2011.38.29%20PM.png)