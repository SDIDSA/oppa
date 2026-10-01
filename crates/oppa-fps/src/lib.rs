//! Cross-platform FPS counter: one `ComponentHost` scene (white
//! root, centered DejaVu text leaf) under a winit event loop on
//! Windows, Linux, Android, and Web. Vello GPU first with a loud
//! softbuffer-CPU fallback; the same bundled font bytes shape and
//! rasterize identically on every target.

#[cfg(target_os = "android")]
pub mod android;
pub mod app;
pub mod clock;
#[cfg(not(target_arch = "wasm32"))]
pub mod driver;
#[cfg(target_arch = "wasm32")]
pub mod web;
