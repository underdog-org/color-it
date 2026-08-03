//! 進行中的筆畫（`docs/specs/E1-stroke.md §2`）。
//!
//! ```text
//! FFI InputSample (扁平) ──engine──▶ StrokeBuilder ──▶ Vec<Dab> ──▶ render Pass 1
//! ```
//!
//! 本檔就是那個 `engine`：**扁平 → `Vec2` 的機械轉換，加上筆刷 ID → 參數的對應**。
//! 不做平滑、不做正規化——那些全在 `stroke`（純 CPU、可測）。
//!
//! 檔名是 `brush` 而不是 `stroke`：`engine` 依賴 `stroke` crate，同名模組會讓
//! `use stroke::...` 變成歧義。

use app_state::BrushPreset as BrushId;
use render::{Bounds, Transform};
use stroke::{BrushPreset, Dab, InputSample as Sample, StrokeBuilder, Vec2};

use crate::ffi::InputSample;

/// 筆刷 ID → 十四欄參數（`E1-stroke.md §14` 決議 A）。
///
/// 對應寫在 `engine` 是因為 `stroke` **不依賴 `app-state`**（同層 crate，不在下游），
/// 而 `app_state::BrushPreset`（五選一的 enum）與 `stroke::BrushPreset`（參數 struct）
/// 同名不同物。E1 只有軟圓筆走得通，其餘四支的參數已登記但未調校。
pub(crate) fn preset(id: BrushId) -> BrushPreset {
    match id {
        BrushId::SoftRound => BrushPreset::soft_round(),
        BrushId::Marker => BrushPreset::marker(),
        BrushId::Crayon => BrushPreset::crayon(),
        BrushId::Airbrush => BrushPreset::airbrush(),
        BrushId::Watercolor => BrushPreset::watercolor(),
    }
}

/// 一筆進行中的筆畫。**不經過 `document`**——鐵律 #3 管的是持久狀態，
/// 而 `T_wet` 依定義不是（`§2`）。`document.apply` 只在抬筆時被呼叫一次。
pub(crate) struct ActiveStroke {
    builder: StrokeBuilder,
    /// 整筆已經畫到 `T_wet` 上的範圍，**含預測點**——`§9` 的重建要清得掉它們，
    /// 少算了預測點的話筆尾會留下一截洗不掉的痕跡。
    bounds: Option<Bounds>,
    /// 起筆處的 region。Mode A 的 `active_region_id` 就是它（`E1-bucket §4.4`）。
    pub(crate) region_id: u32,
    /// 整筆的顏色，編碼值 straight alpha。
    pub(crate) color: [f32; 4],
    /// `Tool::Brush.opacity` 覆寫值，`None` 時取 `preset.opacity`（Boundary 1）。
    pub(crate) opacity: f32,
    pub(crate) build_up: bool,
}

impl ActiveStroke {
    pub(crate) fn new(
        preset: BrushPreset,
        size: f32,
        seed: u32,
        color: [f32; 4],
        opacity: Option<f32>,
        region_id: u32,
    ) -> Self {
        Self {
            builder: StrokeBuilder::new(preset, size, seed),
            bounds: None,
            region_id,
            color,
            opacity: opacity.unwrap_or(preset.opacity),
            build_up: preset.build_up,
        }
    }

    /// 收真實樣本，回傳本次新增的 dab。**預測點不得走這裡**（決議 H）：
    /// 讓它更新濾波器狀態，下一個真實樣本的濾波就建立在猜測上，而誤差會留在筆畫裡。
    pub(crate) fn push_real(&mut self, samples: &[Sample]) -> &[Dab] {
        self.builder.extend(samples);
        self.builder.take_new()
    }

    /// 預測點：複製一份 builder 狀態算完就丟（決議 H）。
    pub(crate) fn predicted(&self, samples: &[Sample]) -> Vec<Dab> {
        self.builder.predicted_dabs(samples)
    }

    /// 抬筆時的重建內容：**整筆的真實 dab**（`§9` 第 2 步）。
    pub(crate) fn finish(self) -> (Vec<Dab>, Option<Bounds>) {
        let bounds = self.bounds;
        (self.builder.finish(), bounds)
    }

    pub(crate) fn build_up(&self) -> bool {
        self.build_up
    }

    pub(crate) fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// 累積已經畫到 `T_wet` 上的範圍。
    pub(crate) fn grow(&mut self, drawn: Option<Bounds>) {
        self.bounds = match (self.bounds, drawn) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, None) => a,
            (None, b) => b,
        };
    }
}

/// 扁平 DTO → `stroke` 的樣本。**只有座標換算，沒有別的**。
///
/// 螢幕像素 → 畫布像素：`Dab.pos` 是畫布座標，而弧長門檻 `spacing × size` 也是
/// 畫布 px。在螢幕空間濾波會讓手感隨畫布縮放浮動（E2 的縮放平移落地時尤其明顯）。
pub(crate) fn to_sample(transform: &Transform, s: &InputSample) -> Sample {
    let pos = transform.canvas_pos([s.x, s.y]);
    Sample {
        pos: Vec2::new(pos[0], pos[1]),
        t: s.t,
        pressure: s.pressure,
        // `radius > 0` 表示手指、`== 0` 表示觸控筆（契約 C9）。把 Pencil 的
        // `majorRadius` 歸零是 Bridge 的責任，這裡照收。
        radius: s.radius,
        tilt: Vec2::new(s.tilt_x, s.tilt_y),
        predicted: s.predicted,
    }
}
