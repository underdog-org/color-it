//! Render graph、wgpu、WGSL。唯一 import wgpu 的 crate。
//!
//! 職責與邊界見 `docs/architecture.md §5.1`，E1 的實作契約見 `docs/specs/E1-wgpu.md`。

mod context;
mod error;
mod gpu;
mod mask;
mod resources;

pub use context::{RenderContext, SURFACE_FORMAT, SurfaceHandle};
pub use error::RenderError;
pub use gpu::{Gpu, MIN_TEXTURE_DIMENSION_2D};
pub use mask::{MaskBinding, MaskMode, MaskUniform};
pub use resources::DocumentResources;
