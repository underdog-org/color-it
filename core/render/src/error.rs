//! `render` 的錯誤型別。
//!
//! `attach_surface` 從 E1 起真的會失敗（`docs/specs/E1-wgpu.md §2.2`），
//! 這裡的每一個變體都對應那條路徑上的一個真實失敗點。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("取不到 Metal adapter：{0}")]
    NoAdapter(wgpu::RequestAdapterError),

    #[error("建不出 device：{0}")]
    NoDevice(wgpu::RequestDeviceError),

    /// `E1-wgpu.md §2.1`：唯一的 required limit。
    #[error("adapter 的 max_texture_dimension_2d = {found}，低於需求 {required}")]
    TextureDimensionTooSmall { found: u32, required: u32 },

    #[error("建不出 surface：{0}")]
    CreateSurface(wgpu::CreateSurfaceError),

    #[error("解不開資產包裡的 PNG：{0}")]
    Png(#[from] png::DecodingError),

    /// 容器過了 `ColorPack::open` 的檢查，但內容與 manifest 對不起來。
    #[error("資產包內容不一致：{0}")]
    AssetMismatch(&'static str),
}
