//! `RustEngine` 鎖起來的那一坨狀態。
//!
//! 業務狀態一律住 `app_state::AppState`（`architecture.md §5.1`），這裡只放
//! 「engine 自己的生命週期」——surface handle、listener、stroke 狀態機。

use std::sync::Arc;

use app_state::AppState;

use crate::ffi::{SurfaceHandle, UiState};
use crate::listener::StateListener;

/// S0 的 mock 值。M1 有 `.colorpack` 格式之後改成從資產包讀。
pub(crate) const MOCK_TOTAL_REGIONS: u32 = 24;

pub(crate) struct Inner {
    pub app: AppState,
    pub listener: Option<Arc<dyn StateListener>>,
    /// 記下來但不碰 GPU；S0 沒有 wgpu device。
    pub surface: Option<SurfaceHandle>,
    /// stroke 狀態機只活在這裡，**不在 `UiState`**——所以 `append_samples` 不會 emit。
    pub stroke_active: bool,
    pub last_emitted: Option<UiState>,
}

impl Inner {
    pub(crate) fn new(app: AppState) -> Self {
        let last_emitted = Some(UiState::from(&app));
        Self {
            app,
            listener: None,
            surface: None,
            stroke_active: false,
            last_emitted,
        }
    }
}
