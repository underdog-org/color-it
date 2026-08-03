//! 輸入樣本（`architecture.md §5.2`）。
//!
//! 與 `engine::ffi::InputSample` **刻意是兩組型別**：那邊是 uniffi 的扁平 DTO，
//! 這邊用 `Vec2`。轉換歸 `engine`（`E1-stroke.md §2`），本 crate 不知道 uniffi 存在。

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputSample {
    pub pos: Vec2,
    /// 秒。**濾波的 `dt` 一律從這裡取**——coalesced touch 的間隔不均勻，
    /// 假設固定 dt 會讓濾波強度隨取樣率浮動（`E1-stroke.md §4.1`）。
    pub t: f32,
    /// 觸控筆的真實壓感。手指模式下這個欄位無意義，壓感由 `radius` 正規化而來。
    pub pressure: f32,
    /// 接觸半徑（點）。**`> 0` 表示手指、`== 0` 表示觸控筆**（`E1-stroke.md §2.2`）。
    /// 把 `radius` 歸零是 iOS Bridge 的責任，即使 `UITouch.majorRadius` 對筆也有值。
    pub radius: f32,
    /// E1 不用（`velocity_to_size` / `tilt_to_size` 都是 0.0），欄位先留著。
    pub tilt: Vec2,
    /// 預測點只影響當前 frame，不進 oplog（`contracts.md` C4）。
    pub predicted: bool,
}

impl InputSample {
    /// 手指樣本：`radius` 有值、`pressure` 由 `stroke` 自己正規化。
    pub fn finger(pos: Vec2, t: f32, radius: f32) -> Self {
        Self {
            pos,
            t,
            pressure: 0.0,
            radius,
            tilt: Vec2::ZERO,
            predicted: false,
        }
    }

    /// 觸控筆樣本：`radius == 0`，直接用 `pressure`。
    pub fn stylus(pos: Vec2, t: f32, pressure: f32) -> Self {
        Self {
            pos,
            t,
            pressure,
            radius: 0.0,
            tilt: Vec2::ZERO,
            predicted: false,
        }
    }

    /// `radius > 0` 就是手指（`E1-stroke.md §2.2`）。
    pub fn is_finger(&self) -> bool {
        self.radius > 0.0
    }
}
