//! 輸入平滑、樣條插值、Dab 生成。純 CPU，零 GPU 依賴。
//!
//! 職責與邊界見 `docs/architecture.md §5.1`，實作契約見 `docs/specs/E1-stroke.md`
//! （動工前先讀該文 §14 執行期決議）。
//!
//! ```text
//! 真實樣本 → One-Euro（位置、radius 各一組）→ 向心 Catmull-Rom → 弧長取樣 → Dab
//! ```
//!
//! **本 crate 不知道 `T_wet` 存在**（Boundary 2）。它吃資料、吐資料，
//! 所以筆刷邏輯能在 CI 上跑 golden test，不需要 GPU 也不需要模擬器——
//! 這是唯一能長期防止手感回歸的機制（`architecture.md §5.2`）。

mod builder;
mod dab;
mod filter;
mod math;
mod preset;
mod sample;
mod spline;

pub use builder::{MAX_DABS_PER_DRAW, R_EPS, REFERENCE_SPEED, StrokeBuilder};
pub use dab::Dab;
pub use filter::OneEuroParams;
pub use math::Vec2;
pub use preset::{BlendMode, BrushPreset, Curve, TipId};
pub use sample::InputSample;

/// 無狀態純函式（`architecture.md §5.2`）。golden test 與 E3 的 oplog 重播用。
///
/// **`samples` 必須已經濾除 `predicted: true`**——純函式不知道預測是什麼（`§3`）。
///
/// `size` 是筆刷直徑 px（`Tool::Brush.size`）。它不在 `§5.2` 的原簽章裡，但沒有它
/// 弧長門檻 `spacing × dab_size` 算不出 px（`E1-stroke.md §14` 決議 E）。
///
pub fn generate_dabs(
    samples: &[InputSample],
    preset: &BrushPreset,
    size: f32,
    seed: u32,
) -> Vec<Dab> {
    let mut builder = StrokeBuilder::new(*preset, size, seed);
    builder.extend(samples);
    builder.finish()
}
