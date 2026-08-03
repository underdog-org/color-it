//! Render graph、wgpu、WGSL。唯一 import wgpu 的 crate。
//!
//! 職責與邊界見 `docs/architecture.md §5.1`，E1 的實作契約見 `docs/specs/E1-wgpu.md`。

mod composite;
mod context;
mod erase;
mod error;
mod fill;
mod gpu;
mod mask;
mod resources;

pub use composite::{CompositePass, Frame, Transform};
pub use context::{RenderContext, SURFACE_FORMAT, SurfaceHandle};
pub use erase::ErasePass;
pub use error::RenderError;
pub use fill::{FILL_ANIM_SIZE, FILL_DURATION, Fill, FillAnim, FillAnimator, ease_out, encode};
pub use gpu::{Gpu, MIN_TEXTURE_DIMENSION_2D};
pub use mask::{MaskBinding, MaskMode, MaskUniform};
pub use resources::DocumentResources;
