//! 擴散動畫的 buffer 佈局（`docs/specs/E1-composite.md §5`）。
//!
//! **per-tap 而非 per-region**：`origin` 與 `max_radius` 每次 tap 都重寫。
//! 動畫推進的時間軸與 `prev_color` 的寫入歸 `E1-bucket`，本檔只定佈局。

use bytemuck::{Pod, Zeroable};

/// 32 bytes／筆。65535 區上限 = 2 MB。
///
/// 對齊：`prev_color` 是 `vec4<f32>`，要 16-byte 對齊，所以它落在 offset 16，
/// 前面剛好塞得下 `origin`(8) ＋ `max_radius`(4) ＋ `progress`(4)。
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct FillAnim {
    /// 點擊處，畫布像素座標。
    pub origin: [f32; 2],
    /// region bbox 對角線（保守略大，寧可早結束）。
    pub max_radius: f32,
    /// 0..1，CPU 每 frame 更新。到 1 之後停止寫入。
    pub progress: f32,
    /// 這次填色之前該區域的顏色（編碼值 RGBA，straight alpha）。
    pub prev_color: [f32; 4],
}

pub const FILL_ANIM_SIZE: u64 = size_of::<FillAnim>() as u64;

impl Default for FillAnim {
    fn default() -> Self {
        Self::zeroed()
    }
}

const _: () = assert!(FILL_ANIM_SIZE == 32);
