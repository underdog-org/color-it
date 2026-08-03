//! Dab 的包絡矩形。Pass 1 的增量 scissor 與 Pass 2 的整筆 scissor 都用它
//! （`docs/specs/E1-stroke.md §7`／`§8`）。
//!
//! 住在 `render` 而不是 `stroke`：`stroke` 不知道 scissor 是什麼（Boundary 2），
//! 而「哪些像素會被寫到」是 pass 的性質。

use stroke::Dab;

/// 畫布像素座標的包絡矩形，閉開區間 `[min, max)`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Bounds {
    /// 每個 dab 是直徑 `size` 的圓，所以半徑要除 2。空切片回 `None`——
    /// 「沒有 dab」與「面積為 0 的矩形」是兩件事，混在一起會讓呼叫端
    /// 誤把空筆畫當成有效 scissor。
    pub fn of_dabs(dabs: &[Dab]) -> Option<Self> {
        let mut it = dabs.iter().map(|d| {
            let r = d.size / 2.0;
            Self {
                min: [d.pos.x - r, d.pos.y - r],
                max: [d.pos.x + r, d.pos.y + r],
            }
        });
        let first = it.next()?;
        Some(it.fold(first, Self::union))
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: [self.min[0].min(other.min[0]), self.min[1].min(other.min[1])],
            max: [self.max[0].max(other.max[0]), self.max[1].max(other.max[1])],
        }
    }

    /// `[x, y, w, h]`，夾到畫布內並向外取整。
    ///
    /// **向外取整**：`set_scissor_rect` 會硬性裁掉框外的片段，少一個像素就是
    /// 筆跡邊緣被切掉一條。整個矩形落在畫布外時回 `None`。
    pub fn to_scissor(self, canvas: [u32; 2]) -> Option<[u32; 4]> {
        let [cw, ch] = [canvas[0] as f32, canvas[1] as f32];
        // NaN 走這條：任何與 NaN 的比較都是 false，夾不住的話 `as u32` 會給 0
        // 而看起來像個合法矩形。
        if !self.min[0].is_finite()
            || !self.min[1].is_finite()
            || !self.max[0].is_finite()
            || !self.max[1].is_finite()
        {
            return None;
        }

        let x0 = self.min[0].floor().clamp(0.0, cw);
        let y0 = self.min[1].floor().clamp(0.0, ch);
        let x1 = self.max[0].ceil().clamp(0.0, cw);
        let y1 = self.max[1].ceil().clamp(0.0, ch);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
    }
}
