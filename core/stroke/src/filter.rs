//! One-Euro filter（`E1-stroke.md §4.1`）。
//!
//! 位置與 radius **各一組參數**：接觸半徑本身就抖，而它驅動的是筆寬，
//! 抖動在視覺上比位置抖動更明顯。

use std::f32::consts::TAU;

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OneEuroParams {
    pub min_cutoff: f32,
    /// 速度愈快、cutoff 愈高（＝愈跟手、愈不平滑）。
    pub beta: f32,
    pub d_cutoff: f32,
}

impl OneEuroParams {
    pub const POSITION: Self = Self {
        min_cutoff: 1.0,
        beta: 0.05,
        d_cutoff: 1.0,
    };

    /// `beta = 0` ＝不隨速度放寬——radius 沒有「快速移動時要更跟手」的需求。
    pub const RADIUS: Self = Self {
        min_cutoff: 0.5,
        beta: 0.0,
        d_cutoff: 1.0,
    };
}

/// `alpha = dt / (dt + tau)`，`tau = 1 / (2π · cutoff)`。
fn alpha(cutoff: f32, dt: f32) -> f32 {
    let tau = 1.0 / (TAU * cutoff);
    dt / (dt + tau)
}

/// 純量版。radius 用。
#[derive(Debug, Clone, Copy)]
pub struct OneEuro {
    params: OneEuroParams,
    state: Option<State>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    x: f32,
    dx: f32,
    t: f32,
}

impl OneEuro {
    pub fn new(params: OneEuroParams) -> Self {
        Self {
            params,
            state: None,
        }
    }

    /// `dt <= 0` 時原樣回傳上一次的輸出、**不動狀態**：coalesced touch 可能帶重複或
    /// 倒退的時戳，而 `dt` 進了分母。丟掉這種樣本比讓它炸出 inf 好。
    pub fn filter(&mut self, x: f32, t: f32) -> f32 {
        let Some(prev) = self.state else {
            self.state = Some(State { x, dx: 0.0, t });
            return x;
        };

        let dt = t - prev.t;
        if dt <= 0.0 {
            return prev.x;
        }

        let dx = (x - prev.x) / dt;
        let dx_hat = prev.dx + (dx - prev.dx) * alpha(self.params.d_cutoff, dt);
        let cutoff = self.params.min_cutoff + self.params.beta * dx_hat.abs();
        let x_hat = prev.x + (x - prev.x) * alpha(cutoff, dt);

        self.state = Some(State {
            x: x_hat,
            dx: dx_hat,
            t,
        });
        x_hat
    }
}

/// 2D 版。位置用。
///
/// **adaptive cutoff 取速度向量的長度，不是逐軸各算一個**：逐軸版在斜向快速移動時
/// 兩軸的 cutoff 不同，平滑量因此隨方向而異——直線會被拉成弧線。
#[derive(Debug, Clone, Copy)]
pub struct OneEuroVec2 {
    params: OneEuroParams,
    state: Option<State2>,
}

#[derive(Debug, Clone, Copy)]
struct State2 {
    x: Vec2,
    dx: Vec2,
    t: f32,
}

impl OneEuroVec2 {
    pub fn new(params: OneEuroParams) -> Self {
        Self {
            params,
            state: None,
        }
    }

    pub fn filter(&mut self, x: Vec2, t: f32) -> Vec2 {
        let Some(prev) = self.state else {
            self.state = Some(State2 {
                x,
                dx: Vec2::ZERO,
                t,
            });
            return x;
        };

        let dt = t - prev.t;
        if dt <= 0.0 {
            return prev.x;
        }

        let dx = (x - prev.x) * (1.0 / dt);
        let dx_hat = prev.dx.lerp(dx, alpha(self.params.d_cutoff, dt));
        let cutoff = self.params.min_cutoff + self.params.beta * dx_hat.length();
        let x_hat = prev.x.lerp(x, alpha(cutoff, dt));

        self.state = Some(State2 {
            x: x_hat,
            dx: dx_hat,
            t,
        });
        x_hat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_passes_through() {
        let mut f = OneEuro::new(OneEuroParams::RADIUS);
        assert_eq!(f.filter(7.0, 0.0), 7.0, "起筆不得有拖尾");
    }

    #[test]
    fn converges_to_a_constant_input() {
        let mut f = OneEuro::new(OneEuroParams::POSITION);
        let mut out = f.filter(0.0, 0.0);
        for i in 1..200 {
            out = f.filter(10.0, i as f32 / 120.0);
        }
        assert!((out - 10.0).abs() < 0.01, "穩定輸入下應收斂，實得 {out}");
    }

    #[test]
    fn suppresses_jitter_around_a_constant() {
        let mut f = OneEuro::new(OneEuroParams::POSITION);
        let mut worst: f32 = 0.0;
        for i in 0..120 {
            let noisy = 100.0 + if i % 2 == 0 { 2.0 } else { -2.0 };
            let out = f.filter(noisy, i as f32 / 120.0);
            if i > 20 {
                worst = worst.max((out - 100.0).abs());
            }
        }
        assert!(worst < 2.0, "±2 的抖動應被壓小，實得 {worst}");
    }

    #[test]
    fn non_advancing_time_is_ignored() {
        let mut f = OneEuro::new(OneEuroParams::POSITION);
        f.filter(0.0, 0.0);
        let a = f.filter(10.0, 0.1);
        let b = f.filter(99.0, 0.1);
        let c = f.filter(99.0, 0.05);
        assert_eq!(a, b, "相同時戳的樣本不得改變狀態");
        assert_eq!(a, c, "倒退的時戳不得改變狀態");
        assert!(a.is_finite());
    }

    #[test]
    fn vec2_smoothing_is_isotropic() {
        // 同一條軌跡轉 90 度，平滑量必須一樣——逐軸各算 cutoff 的版本會在這裡失敗。
        let mut fx = OneEuroVec2::new(OneEuroParams::POSITION);
        let mut fy = OneEuroVec2::new(OneEuroParams::POSITION);
        let (mut last_x, mut last_y) = (Vec2::ZERO, Vec2::ZERO);
        for i in 0..60 {
            let (t, d) = (i as f32 / 120.0, i as f32 * 3.0);
            last_x = fx.filter(Vec2::new(d, 0.0), t);
            last_y = fy.filter(Vec2::new(0.0, d), t);
        }
        assert!((last_x.x - last_y.y).abs() < 1e-5);
    }
}
