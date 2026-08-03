//! FFI facade、生命週期、把 UI 事件翻成 Op。不含任何業務邏輯。
//!
//! **本 crate 是 FFI 的 SSOT**（`architecture.md §7`）。Swift binding 由 uniffi 從這裡
//! 的 proc-macro 標註生成，`docs/contracts.md` 只放程式碼表達不出來的語意條款。
//!
//! 行為表見 `docs/specs/ffi-contract.md §5`。

mod engine;
mod error;
mod ffi;
mod inner;
mod listener;

pub use engine::RustEngine;
pub use error::EngineError;
pub use ffi::{BrushId, InputSample, Progress, Rgba, SurfaceHandle, Tool, Transform, UiState};
pub use listener::StateListener;

uniffi::setup_scaffolding!("colorlull_engine");
