//! 筆刷參數（`architecture.md §4.6`）。五支筆刷共用同一條渲染路徑，差異只在常數。
//!
//! **與 `app_state::BrushPreset` 同名不同物**（`E1-stroke.md §14` 決議 A）：那邊是五選一的
//! enum、＝筆刷 ID；這邊是十四欄的參數 struct。`stroke` 不依賴 `app-state`（同層 crate，
//! 不在下游），所以 ID → 參數的對應寫在 `engine`。

/// tip 貼圖在 `texture_2d_array<f32>` 裡的 layer index（`E1-stroke.md §6.1`）。
///
/// 從第一天就用 array 而非單張，是為了讓 E2 加 tip 不動 bind group layout。
/// **E1 只有 `SoftRound` 真的畫得出來**，而且是程序生成的解析式徑向衰減，不進資產。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipId {
    SoftRound,
    HardRound,
    /// 顆粒紋理（蠟筆）。E2 才有真的貼圖。
    Grain,
}

impl TipId {
    pub fn layer(self) -> u32 {
        match self {
            Self::SoftRound => 0,
            Self::HardRound => 1,
            Self::Grain => 2,
        }
    }

    /// E1 只實作軟圓。`render` 依此決定要不要 fallback 並記一次 log（`E1-stroke.md §6`）。
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::SoftRound)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
}

/// 三個參數、無編輯器、完全決定性（`E1-stroke.md §6`）。
///
/// 不用 LUT 或貝茲：`prd.md` 的 Don't Have 禁止使用者編輯筆刷參數，所以曲線只需要
/// 「表達得出五支 preset 的差異」，不需要可編輯性。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub min: f32,
    pub max: f32,
    pub gamma: f32,
}

impl Curve {
    pub const fn new(min: f32, max: f32, gamma: f32) -> Self {
        Self { min, max, gamma }
    }

    /// `out = min + (max - min) × p^gamma`。
    ///
    /// `p` 先夾到 `[0, 1]`：`powf` 對負數會給 NaN，而一個 NaN 的 `size` 會讓
    /// 弧長取樣的比較全部變 false，整筆靜默消失——這是最難查的那種 bug。
    pub fn eval(&self, p: f32) -> f32 {
        let p = p.clamp(0.0, 1.0);
        self.min + (self.max - self.min) * p.powf(self.gamma)
    }
}

/// 十四個欄位逐字照 `architecture.md §4.6`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushPreset {
    pub tip: TipId,
    /// dab 間距，單位是**筆尖直徑比**——所以間距隨壓感變化是預期行為（`E1-stroke.md §4.3`）。
    pub spacing: f32,
    pub pressure_to_size: Curve,
    pub pressure_to_opacity: Curve,
    /// E2。E1 恆為 0.0。
    pub velocity_to_size: f32,
    /// E2。E1 恆為 0.0。
    pub tilt_to_size: f32,
    pub jitter_pos: f32,
    pub jitter_size: f32,
    pub jitter_angle: f32,
    pub blend: BlendMode,
    /// 單 dab 濃度。
    pub flow: f32,
    /// 整筆上限的預設值，可被 `Tool::Brush.opacity` 覆寫（`architecture.md §6` Boundary 1）。
    /// **`stroke` 不碰它**——它是 Pass 2 commit 時才乘的，不進 `Dab`。
    pub opacity: f32,
    /// 同筆內是否疊加。`render` 用它切 Pass 1 的 blend（`E1-stroke.md §7`）。
    pub build_up: bool,
    /// commit 時的邊緣加成（`architecture.md §4.2 (d)`）。0 = 不套用。
    pub edge_boost: f32,
}

impl BrushPreset {
    /// E1 唯一實作得完整的一支（`E1-stroke.md §6`）。
    pub const fn soft_round() -> Self {
        Self {
            tip: TipId::SoftRound,
            spacing: 0.05,
            pressure_to_size: Curve::new(0.35, 1.0, 1.0),
            pressure_to_opacity: Curve::new(0.40, 1.0, 1.0),
            velocity_to_size: 0.0,
            tilt_to_size: 0.0,
            jitter_pos: 0.0,
            jitter_size: 0.0,
            jitter_angle: 0.0,
            blend: BlendMode::Normal,
            flow: 1.0,
            opacity: 0.85,
            build_up: false,
            edge_boost: 0.0,
        }
    }

    // ── 其餘四支照 `architecture.md §4.6` 的表登記 ─────────────────────────
    // **E1 都不實作**：曲線一律沿用軟圓筆的初值，等 E2 調校。登記在這裡是為了讓
    // 「哪些欄位已定案、哪些還沒」看得見——散在別處會變成沒人知道的待辦。

    pub const fn marker() -> Self {
        Self {
            tip: TipId::HardRound,
            spacing: 0.04,
            blend: BlendMode::Multiply,
            ..Self::soft_round()
        }
    }

    pub const fn crayon() -> Self {
        Self {
            tip: TipId::Grain,
            spacing: 0.08,
            ..Self::soft_round()
        }
    }

    /// 「大軟圓」＝同一張 tip、由 `Tool::Brush.size` 放大，不是另一個 `TipId`。
    pub const fn airbrush() -> Self {
        Self {
            spacing: 0.02,
            build_up: true,
            ..Self::soft_round()
        }
    }

    /// `edge_boost` 的初值待 E2 調校，先留 0.0（＝不套用）而不是猜一個數字。
    pub const fn watercolor() -> Self {
        Self {
            spacing: 0.06,
            blend: BlendMode::Multiply,
            build_up: true,
            ..Self::soft_round()
        }
    }
}

impl Default for BrushPreset {
    fn default() -> Self {
        Self::soft_round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_spans_min_to_max() {
        let c = Curve::new(0.35, 1.0, 1.0);
        assert_eq!(c.eval(0.0), 0.35);
        assert_eq!(c.eval(1.0), 1.0);
        assert!((c.eval(0.5) - 0.675).abs() < 1e-6);
    }

    #[test]
    fn curve_clamps_out_of_range_pressure() {
        let c = Curve::new(0.35, 1.0, 2.0);
        assert_eq!(c.eval(-1.0), 0.35, "負壓感不得產生 NaN");
        assert_eq!(c.eval(2.0), 1.0);
    }

    #[test]
    fn tip_layers_are_distinct() {
        let layers: Vec<u32> = [TipId::SoftRound, TipId::HardRound, TipId::Grain]
            .iter()
            .map(|t| t.layer())
            .collect();
        assert_eq!(layers, vec![0, 1, 2]);
    }

    #[test]
    fn only_soft_round_is_implemented_in_e1() {
        assert!(BrushPreset::soft_round().tip.is_implemented());
        for p in [
            BrushPreset::marker(),
            BrushPreset::crayon(),
            BrushPreset::watercolor(),
        ] {
            let _ = p;
        }
        assert!(!TipId::HardRound.is_implemented());
        assert!(!TipId::Grain.is_implemented());
        // 噴槍用的也是軟圓 tip，所以它「畫得出來」，只是 build_up 的 blend 是 E2。
        assert!(BrushPreset::airbrush().tip.is_implemented());
    }
}
