//! 狀態變更的單一出口。
//!
//! foreign trait（`with_foreign`）而不是已被取代的 `callback_interface`。
//! 只有一個 listener、後設覆蓋前設；廣播給多個訂閱者是 Bridge 用 Combine 做的事。

use crate::ffi::UiState;

/// 契約 C1：在**呼叫端 thread** 同步觸發，hop 到 main queue 由 Bridge 負責。
/// 契約 C2：`RustEngine` 保證發送前已釋放內部鎖，所以回呼中可以安全地再呼叫 `RustEngine`。
#[uniffi::export(with_foreign)]
pub trait StateListener: Send + Sync {
    fn on_state(&self, state: UiState);
}
