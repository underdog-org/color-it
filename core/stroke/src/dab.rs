//! 管線的輸出（`architecture.md §5.2`）。
//!
//! **刻意沒有 `repr(C)` 也沒有 `bytemuck`**（`E1-stroke.md §14` 決議 G）：
//! `Dab` 是 GPU instance 資料的來源，但版面配置是 `render` 的事——
//! `render` 自己定 `DabInstance` 與轉換，Boundary 2 才守得住。

use crate::math::Vec2;
use crate::preset::TipId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dab {
    /// 畫布座標，px。
    pub pos: Vec2,
    /// **直徑**，px。已含 `pressure_to_size` 與 `jitter_size`。
    pub size: f32,
    /// 弧度。E1 只有 `jitter_angle` 會動到它，軟圓筆恆為 0。
    pub angle: f32,
    /// 單 dab 濃度 = `preset.flow × pressure_to_opacity(p)`（`E1-stroke.md §7`）。
    /// **整筆上限 `opacity` 不在這裡**，它是 Pass 2 commit 時才乘的。
    pub alpha: f32,
    pub tip: TipId,
}
