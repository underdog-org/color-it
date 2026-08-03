//! 串流版的筆畫狀態機（`E1-stroke.md §2`、`§4`、`§5`）。
//!
//! **這是全 crate 唯一一份管線實作。** `generate_dabs` 是它的批次外殼，
//! 所以 `§2.1` 的「串流 == 批次」由建構保證，不是靠兩份程式碼互相追平——
//! spec 擔心的漂移在這個結構下沒有發生的餘地。測試仍然照設，它守的是
//! 「後人不要把它拆成兩份」。

use crate::dab::Dab;
use crate::filter::{OneEuro, OneEuroParams, OneEuroVec2};
use crate::math::Vec2;
use crate::preset::BrushPreset;
use crate::sample::InputSample;
use crate::spline::Segment;

/// `majorRadius` 正規化的分母下限，點（`E1-stroke.md §5`）。實機調校。
pub const R_EPS: f32 = 4.0;

/// 單次 draw 的 instance 上限（`E1-stroke.md §7`）。超過由 `render` 分批，
/// **不是靜默截斷**——那會變成「畫太快就斷線」。放在這裡是因為它是 dab 的性質，
/// 而 `stroke` 是 dab 的出處。
pub const MAX_DABS_PER_DRAW: usize = 4096;

/// 每個 segment 切幾段折線去逼近弧長。
///
/// 32 是精度與成本的折衷：一個 segment 通常只跨兩個樣本（快速移動時 20–40 px），
/// 折線誤差因此在 0.01 px 量級，遠小於 dab 半徑。
const SUBSTEPS: u32 = 32;

/// dab 間距的下限，px。`spacing × size` 理論上不會是 0（`pressure_to_size.min` 有底），
/// 但它是 preset 給的數字——一個 0 會讓弧長取樣原地無限迴圈。
const MIN_STEP: f32 = 0.05;

#[derive(Debug, Clone, Copy)]
struct Ctrl {
    pos: Vec2,
    pressure: f32,
}

/// per-stroke running baseline（`E1-stroke.md §5`）。
///
/// **不用固定 min/max**：`majorRadius` 的絕對值因手指大小而異（`architecture.md §10.2`）。
/// 已知限制：一筆之內若力道單調遞增，`r_min` 永遠是起筆值，壓感範圍會被壓縮。
/// 使用者層級的長期基線是更好的解，但它需要跨 session 的狀態——E1 只做 per-stroke。
#[derive(Debug, Clone, Copy)]
struct Baseline {
    min: f32,
    max: f32,
}

impl Baseline {
    /// 初值是 `r₀` **±`R_EPS/2` 的帶狀**，不是 `r₀` 本身（`E1-stroke.md §14` 決議 F）。
    /// spec 的公式原樣照抄，但若 min/max 都從 `r₀` 起算，起筆的分子恆為 0——
    /// 每一筆都從最細最淡開始，與同段「此時應為中值」矛盾。帶狀初值讓起筆得 0.5，
    /// 而 min/max 照樣單調外擴。
    fn new(r: f32) -> Self {
        Self {
            min: r - R_EPS / 2.0,
            max: r + R_EPS / 2.0,
        }
    }

    fn normalize(&mut self, r: f32) -> f32 {
        self.min = self.min.min(r);
        self.max = self.max.max(r);
        ((r - self.min) / (self.max - self.min).max(R_EPS)).clamp(0.0, 1.0)
    }
}

/// jitter 專用。`seed` 讓它可重現，否則 E3 的縮時重播會與原作不同。
///
/// 自己寫 xorshift 而不是拉 `rand`：需求只有「決定性、跨平台逐位元相同」，
/// 而 `rand` 的演算法會隨版本變——那會讓 golden fixture 在升版時整批變紅。
#[derive(Debug, Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        // xorshift 的狀態不能是 0。
        Self(seed | 1)
    }

    fn next_unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        // 取高 24 bit：f32 的尾數就這麼寬，用滿即可，除法是精確的 2 的冪。
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }

    /// `[-1, 1)`。
    fn next_signed(&mut self) -> f32 {
        self.next_unit() * 2.0 - 1.0
    }
}

/// 有狀態：One-Euro 濾波器、radius baseline、spacing 累積量、已收樣本（`E1-stroke.md §2`）。
///
/// **進行中的筆畫不經過 `document`**（鐵律 #3 管的是持久狀態，而 `T_wet` 依定義不是）。
#[derive(Debug, Clone)]
pub struct StrokeBuilder {
    preset: BrushPreset,
    /// 筆刷直徑，px。來自 `Tool::Brush.size`（`E1-stroke.md §14` 決議 E）。
    size: f32,
    pos_filter: OneEuroVec2,
    radius_filter: OneEuro,
    baseline: Option<Baseline>,
    rng: Rng,
    pts: Vec<Ctrl>,
    dabs: Vec<Dab>,
    /// `take_new` 的游標。**不影響 `dabs` 的內容**——取用節奏不得改變輸出，
    /// 否則 `§2.1` 的不變式會隨 frame 邊界飄。
    taken: usize,
    /// 弧長累積量。**跨 segment 保留**，否則每段接縫處都會多一個或少一個 dab，
    /// 在慢速筆畫上看得出串珠（`E1-stroke.md §4.3`）。
    dist_acc: f32,
    /// 下一個 dab 還差多少弧長。隨壓感變化——`spacing` 的單位是筆尖直徑比。
    threshold: f32,
}

impl StrokeBuilder {
    pub fn new(preset: BrushPreset, size: f32, seed: u32) -> Self {
        Self {
            preset,
            size,
            pos_filter: OneEuroVec2::new(OneEuroParams::POSITION),
            radius_filter: OneEuro::new(OneEuroParams::RADIUS),
            baseline: None,
            rng: Rng::new(seed),
            pts: Vec::new(),
            dabs: Vec::new(),
            taken: 0,
            dist_acc: 0.0,
            threshold: MIN_STEP,
        }
    }

    /// **只吃真實樣本**（`E1-stroke.md §14` 決議 H）。預測點走 [`predicted_dabs`]。
    ///
    /// [`predicted_dabs`]: Self::predicted_dabs
    pub fn push(&mut self, sample: &InputSample) {
        debug_assert!(
            !sample.predicted,
            "predicted 樣本不得進 committed 狀態，用 predicted_dabs"
        );

        let pos = self.pos_filter.filter(sample.pos, sample.t);
        let pressure = if sample.is_finger() {
            // 先濾再正規化：baseline 吃的是濾過的 r，否則單一個雜訊尖峰就永久
            // 撐開 r_max，整筆的壓感範圍從此被壓扁。
            let r = self.radius_filter.filter(sample.radius, sample.t);
            self.baseline
                .get_or_insert_with(|| Baseline::new(r))
                .normalize(r)
        } else {
            sample.pressure.clamp(0.0, 1.0)
        };

        self.pts.push(Ctrl { pos, pressure });
        let k = self.pts.len() - 1;

        if k == 0 {
            // 起筆一定有一個 dab：點一下就該留下一個點，而「原地停留」也因此
            // 恰好只有這一個——濃度不隨停留時間變深。
            self.emit(pos, pressure);
            return;
        }
        if k >= 2 {
            // 收到 P[k] 才畫得出 segment k-2（需要它當第四個控制點）。
            // 這就是 §4.2 說的「樣條天生落後一個樣本」，由 predictedTouches 補。
            self.emit_segment(k - 2);
        }
    }

    pub fn extend(&mut self, samples: &[InputSample]) {
        for s in samples {
            self.push(s);
        }
    }

    /// 本 frame 新增的 dab。Pass 1 拿它去畫，並據此算增量 bbox。
    pub fn take_new(&mut self) -> &[Dab] {
        let from = self.taken;
        self.taken = self.dabs.len();
        &self.dabs[from..]
    }

    /// 預測點只影響當前 frame（`contracts.md` C4）。
    ///
    /// 複製一份狀態算完就丟——committed 的 `pts` / `dabs` / 濾波器狀態都不被污染。
    /// `§9` 的 `end_stroke` 重建因此不必特別處理預測點：`finish()` 本來就只含真實樣本。
    pub fn predicted_dabs(&self, predicted: &[InputSample]) -> Vec<Dab> {
        let mut probe = self.clone();
        let from = probe.dabs.len();
        for s in predicted {
            let mut s = *s;
            s.predicted = false;
            probe.push(&s);
        }
        probe.dabs[from..].to_vec()
    }

    /// 收尾：補畫最後一段，回傳整筆的 dab。
    ///
    /// 這個回傳值就是 `§2.1` 不變式的左邊，也是 `§9` 重建 `T_wet` 時 Pass 1 要重跑的內容。
    pub fn finish(mut self) -> Vec<Dab> {
        let m = self.pts.len();
        if m >= 2 {
            // 串流時已畫到 segment m-3，最後一段的第四個控制點永遠不會到。
            self.emit_segment(m - 2);
        }
        self.dabs
    }

    /// 目前為止（不含收尾那一段）的全部 dab。
    pub fn dabs(&self) -> &[Dab] {
        &self.dabs
    }

    pub fn preset(&self) -> &BrushPreset {
        &self.preset
    }

    /// segment `j` ＝ `pts[j]` 到 `pts[j+1]`。端點的控制點重複自己。
    fn emit_segment(&mut self, j: usize) {
        let last = self.pts.len() - 1;
        let seg = Segment::new([
            self.pts[j.saturating_sub(1)].pos,
            self.pts[j].pos,
            self.pts[j + 1].pos,
            self.pts[(j + 2).min(last)].pos,
        ]);
        let (pa, pb) = (self.pts[j].pressure, self.pts[j + 1].pressure);

        let mut prev = seg.at(0.0);
        let mut prev_u = 0.0;
        for i in 1..=SUBSTEPS {
            let u = i as f32 / SUBSTEPS as f32;
            let cur = seg.at(u);
            self.walk(prev, prev_u, cur, u, pa, pb);
            prev = cur;
            prev_u = u;
        }
    }

    /// 沿一小段折線前進，每滿一個 `threshold` 就放一個 dab。
    fn walk(&mut self, from: Vec2, u_from: f32, to: Vec2, u_to: f32, pa: f32, pb: f32) {
        let (mut a, mut ua) = (from, u_from);
        loop {
            let len = a.distance(to);
            // NaN 也走這條：`NaN <= 0.0` 是 false，所以要靠 `is_finite` 攔。
            // 沒攔住的話下面的 `need / len` 會讓迴圈永遠不收斂。
            if len <= 0.0 || !len.is_finite() {
                return;
            }

            let need = self.threshold - self.dist_acc;
            if need > len {
                self.dist_acc += len;
                return;
            }

            let f = need / len;
            let pos = a.lerp(to, f);
            let u = ua + (u_to - ua) * f;
            self.emit(pos, pa + (pb - pa) * u);

            a = pos;
            ua = u;
            self.dist_acc = 0.0;
        }
    }

    fn emit(&mut self, pos: Vec2, pressure: f32) {
        let p = pressure.clamp(0.0, 1.0);
        let size = self.size * self.preset.pressure_to_size.eval(p);

        // 三個 jitter 一律抽、一律照同一順序——即使參數是 0.0。抽或不抽若取決於參數，
        // 調一次 jitter 就會連帶改變後面每個 dab 的隨機序列。
        let (jx, jy, jsize, jangle) = (
            self.rng.next_signed(),
            self.rng.next_signed(),
            self.rng.next_signed(),
            self.rng.next_signed(),
        );

        let preset = &self.preset;
        self.dabs.push(Dab {
            pos: pos + Vec2::new(jx, jy) * (preset.jitter_pos * size),
            size: size * (1.0 + preset.jitter_size * jsize),
            angle: preset.jitter_angle * jangle * std::f32::consts::PI,
            alpha: (preset.flow * preset.pressure_to_opacity.eval(p)).clamp(0.0, 1.0),
            tip: preset.tip,
        });

        self.threshold = (preset.spacing * size).max(MIN_STEP);
    }
}
