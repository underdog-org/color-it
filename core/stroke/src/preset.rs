//! 筆刷參數（`architecture.md §4.6`）。五支筆刷共用同一條渲染路徑，差異只在常數。
//!
//! **與 `app_state::BrushPreset` 同名不同物**（`E1-stroke.md §14` 決議 A）：那邊是五選一的
//! enum、＝筆刷 ID；這邊是十四欄的參數 struct。`stroke` 不依賴 `app-state`（同層 crate，
//! 不在下游），所以 ID → 參數的對應寫在 `engine`。

/// tip 貼圖在 `texture_2d_array<f32>` 裡的 layer index
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipId {
    SoftRound,
    HardRound,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
}

/// 不用 LUT 或貝茲：`prd.md` 的 Don't Have 禁止使用者編輯筆刷參數，所以曲線只需要
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

/// preset 名 ＋ 建構子。名字進 golden fixture，失敗訊息才指得出是哪一支。
pub type Named = (&'static str, fn() -> BrushPreset);

impl BrushPreset {
    /// 五支的對照組：乾淨、無 jitter、壓感同時驅動 size 與 opacity。
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

    // ── 其餘四支：每支只靠一到兩個軸與另外四支區分 ─────────────────────────
    //
    // **這裡的數值全是初值，D5 盲測會調動其中一半。** 方向（哪幾欄是差異軸）
    // 才是定案的部分——若某支的軸被推翻，那才是要回頭改設計的事。
    // 只寫 `..soft_round()` 沒覆蓋的欄位，差異因此在 diff 上一眼看得見。

    /// 兩軸：**硬圓 tip ＋ Multiply**。硬邊、疊色變深。
    ///
    /// `pressure_to_opacity` 刻意窄到接近恆定——真實麥克筆的墨水濃度均勻，
    /// 壓感該走 size 而不是濃度。`spacing` 比軟圓筆更小：硬邊在轉彎處的
    /// 扇貝狀鋸齒是 dab 間距露出來的，軟邊藏得住，硬邊藏不住。
    pub const fn marker() -> Self {
        Self {
            tip: TipId::HardRound,
            spacing: 0.03,
            pressure_to_size: Curve::new(0.55, 1.0, 1.0),
            pressure_to_opacity: Curve::new(0.90, 1.0, 1.0),
            flow: 1.0,
            opacity: 0.95,
            blend: BlendMode::Multiply,
            ..Self::soft_round()
        }
    }

    /// 一軸主導：**顆粒 tip**。但貼圖本身不夠——同一張 noise 沿路徑重複貼會
    /// 看出規律條紋，那看起來像貼圖 bug 而不是蠟筆。
    ///
    /// 所以兩個 jitter 是必要的、不是裝飾：`jitter_angle` 把每個 dab 的顆粒轉開，
    /// `jitter_pos` 錯開，重複因此消失。大 `spacing` ＋ 低 `flow` 讓 dab 之間留白——
    /// 「蠟筆沒塗滿」的觀感來自這裡，不是來自貼圖。
    ///
    /// `jitter_size` 留 0：留白已經夠明顯時，size 的抖動只會讓邊界變髒。
    pub const fn crayon() -> Self {
        Self {
            tip: TipId::Grain,
            spacing: 0.30,
            pressure_to_size: Curve::new(0.45, 1.0, 1.0),
            pressure_to_opacity: Curve::new(0.35, 1.0, 1.2),
            jitter_pos: 0.12,
            jitter_angle: 1.0,
            flow: 0.55,
            opacity: 0.90,
            ..Self::soft_round()
        }
    }

    pub const fn airbrush() -> Self {
        Self {
            spacing: 0.015,
            pressure_to_size: Curve::new(0.50, 1.0, 1.0),
            pressure_to_opacity: Curve::new(0.30, 1.0, 1.0),
            velocity_to_size: 0.35,
            flow: 0.08,
            build_up: true,
            ..Self::soft_round()
        }
    }

    pub const fn watercolor() -> Self {
        Self {
            spacing: 0.05,
            pressure_to_size: Curve::new(0.40, 1.0, 1.0),
            pressure_to_opacity: Curve::new(0.35, 1.0, 1.0),
            velocity_to_size: 0.45,
            blend: BlendMode::Multiply,
            flow: 0.15,
            opacity: 0.80,
            build_up: true,
            edge_boost: 0.60,
            ..Self::soft_round()
        }
    }

    /// 五支，順序＝ `app_state::BrushPreset` 的宣告順序。測試與 golden fixture 用。
    pub const ALL: [Named; 5] = [
        ("soft_round", Self::soft_round),
        ("marker", Self::marker),
        ("crayon", Self::crayon),
        ("airbrush", Self::airbrush),
        ("watercolor", Self::watercolor),
    ];
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

    /// 定性的差異軸（`E2-brush.md §3` 的表）。**D5 調不到這四欄**——
    /// 調得到的是幅度，這裡守的是方向。某一格被改動就是設計被推翻，
    /// 那該是一次自覺的決定，不該是調參數的副作用。
    #[test]
    fn the_five_differ_on_the_axes_the_design_claims() {
        let axes = |p: BrushPreset| (p.tip, p.blend, p.build_up, p.edge_boost > 0.0);

        assert_eq!(
            axes(BrushPreset::soft_round()),
            (TipId::SoftRound, BlendMode::Normal, false, false),
            "軟圓筆是對照組，不得有任何特色"
        );
        assert_eq!(
            axes(BrushPreset::marker()),
            (TipId::HardRound, BlendMode::Multiply, false, false),
            "麥克筆＝硬圓 ＋ Multiply"
        );
        assert_eq!(
            axes(BrushPreset::crayon()),
            (TipId::Grain, BlendMode::Normal, false, false),
            "蠟筆的軸是顆粒 tip"
        );
        assert_eq!(
            axes(BrushPreset::airbrush()),
            (TipId::SoftRound, BlendMode::Normal, true, false),
            "噴槍的軸是 build_up，tip 與軟圓筆同一張"
        );
        assert_eq!(
            axes(BrushPreset::watercolor()),
            (TipId::SoftRound, BlendMode::Multiply, true, true),
            "水彩的獨佔軸是 edge_boost"
        );
    }

    #[test]
    fn crayon_needs_jitter_or_the_grain_repeats() {
        // §3.3：少了這兩欄，同一張 noise 沿路徑重複貼會看出規律條紋——
        // 那看起來像貼圖 bug，不像蠟筆。
        let c = BrushPreset::crayon();
        assert!(c.jitter_angle > 0.0, "顆粒不轉開就會出現重複條紋");
        assert!(c.jitter_pos > 0.0, "顆粒不錯開就會出現重複條紋");
        assert!(
            c.spacing > BrushPreset::soft_round().spacing * 2.0,
            "留白靠大 spacing"
        );
    }

    #[test]
    fn only_the_two_with_real_world_taper_react_to_velocity() {
        // §4.2：噴槍與水彩的真實對應物都有「快掃留下較細筆觸」的性質；
        // 軟圓筆要乾淨、麥克筆墨水均勻、蠟筆的稀疏該由 spacing 與 jitter 表達。
        for (name, make) in BrushPreset::ALL {
            let want_nonzero = matches!(name, "airbrush" | "watercolor");
            assert_eq!(
                make().velocity_to_size > 0.0,
                want_nonzero,
                "{name} 的 velocity_to_size"
            );
        }
    }

    #[test]
    fn tilt_is_wired_to_nothing() {
        // §4.1：`InputSample.tilt` 只有 Apple Pencil 有值，而 Pencil 進階是 v1 不做。
        // 五支全 0，路徑不實作——一條沒有輸入來源的耦合只會多一組測不到的分支。
        for (name, make) in BrushPreset::ALL {
            assert_eq!(make().tilt_to_size, 0.0, "{name} 不得依賴 tilt");
        }
    }
}
