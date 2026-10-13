//! Universal GPU vector & batch rasterizer on wgpu render pipelines.

pub mod atlas;
pub mod backend;
pub mod batch;
pub mod oracle;
pub mod path;
pub mod pipeline;
pub mod vertex;

pub use backend::{gpu_caps, GpuBackend, PresentReport};
pub use oracle::{diff_count_exact, diff_count_tol, max_channel_diff, RgbaImage};
