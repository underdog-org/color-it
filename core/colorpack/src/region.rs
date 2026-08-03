//! `regions.json` 的逐區記錄（`specs/baker-core-design.md §3.5`）。

use serde::{Deserialize, Serialize};

/// 所有幾何量都在**輸出解析度**：`area` 是 `architecture §4.7` 進度計算的分母，
/// 必須與 runtime 的 `T_region` 同一個空間。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionEntry {
    pub id: u32,
    /// `[x, y]`，整數像素（面積加權平均後向下取整）。
    pub centroid: [u32; 2],
    pub area: u32,
    /// `[x, y, w, h]`。
    pub bbox: [u32; 4],
    /// `#RRGGBB`（大寫十六進位）。
    pub suggested_color: String,
}

/// `#RRGGBB`。`palette[]` 與 `suggested_color` 共用同一種寫法。
pub fn hex_color(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_uppercase_and_zero_padded() {
        assert_eq!(hex_color([0, 10, 255]), "#000AFF");
    }
}
