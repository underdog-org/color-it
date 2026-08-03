//! 文件模型、palette、單一 apply 入口（`docs/specs/E1-bucket.md §2`／`§3`）。
//!
//! **純 CPU、無時鐘、不依賴 `render`**（`xtask/deps-policy.toml` 寫死）。
//! `apply` 不畫任何東西，它回傳描述性的 [`Effect`]，由 `engine` 翻譯成 GPU 動作。
//!
//! E1 的 `document` 不接 oplog、不接 history、沒有 undo——E3 只是在 `apply` 裡
//! 多接那兩條線，`Effect` 這一層不動。

use colorpack::ColorPack;

/// 狀態變更的唯一詞彙（鐵律 #3）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Fill {
        region_id: u32,
        /// `a` 被忽略——填色即不透明（`E1-bucket §5`）。
        color: [u8; 4],
    },
    /// E1 只定型不使用，內容歸 `E1-stroke`。
    BrushStroke { color: [u8; 4], opacity: f32 },
}

/// `apply` 的產物：**描述狀態變了什麼**，不是 GPU 指令。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    Filled {
        region_id: u32,
        /// 正規化後的顏色（`a == 255`）。
        color: [u8; 4],
        /// 這次填色之前的顏色，直接餵給 `FillAnim.prev_color`（`E1-bucket §7`）。
        prev: [u8; 4],
        /// `[x, y, w, h]`，供清 `T_erase` 的 scissor 用（§6）。
        bbox: [u32; 4],
    },
    /// 同色重填、region ID 不存在（座標落在畫布外）——狀態沒變，engine 什麼都不做。
    None,
}

pub struct Document {
    palette: Vec<[u8; 4]>,
    /// 與 `palette` 同長同索引。只讀，載入後不變。
    bboxes: Vec<[u32; 4]>,
    /// `palette` 中 `a != 0` 的筆數，隨 `apply` 遞增。
    colored: u32,
}

impl Document {
    /// `bboxes[id]` 即 `regions[id].bbox`。長度決定 region 數。
    pub fn new(bboxes: Vec<[u32; 4]>) -> Self {
        Self {
            palette: vec![[0; 4]; bboxes.len()],
            bboxes,
            colored: 0,
        }
    }

    pub fn from_pack(pack: &ColorPack) -> Self {
        Self::new(pack.regions.iter().map(|r| r.bbox).collect())
    }

    /// **唯一的狀態變更入口**（鐵律 #3）。
    pub fn apply(&mut self, op: Op) -> Effect {
        match op {
            Op::Fill { region_id, color } => self.fill(region_id, color),
            // E1 的筆刷不經過 document——`T_paint` 是 GPU 側的真相，
            // 進 oplog 是 E3 的事。
            Op::BrushStroke { .. } => Effect::None,
        }
    }

    fn fill(&mut self, region_id: u32, color: [u8; 4]) -> Effect {
        // 不存在的 ID 直接落空——畫布外的 tap 由呼叫端轉成這條路徑（§4.3），
        // 不 clamp，clamp 會讓誤觸填到邊緣區域。
        let Some(slot) = self.palette.get_mut(region_id as usize) else {
            return Effect::None;
        };

        // `a` 恆為 255：`a == 0` 是「從未填過」的狀態旗標，不是使用者可調的透明度（§5）。
        let next = [color[0], color[1], color[2], 255];
        if *slot == next {
            return Effect::None;
        }

        let prev = *slot;
        if prev[3] == 0 {
            self.colored += 1;
        }
        *slot = next;

        Effect::Filled {
            region_id,
            color: next,
            prev,
            bbox: self.bboxes[region_id as usize],
        }
    }

    pub fn palette(&self) -> &[[u8; 4]] {
        &self.palette
    }

    pub fn colored_regions(&self) -> u32 {
        self.colored
    }

    pub fn total_regions(&self) -> u32 {
        self.palette.len() as u32
    }
}
