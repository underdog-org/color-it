//! 向心 Catmull-Rom，`alpha = 0.5`（`E1-stroke.md §4.2`）。
//!
//! 不是均勻參數化。均勻版在樣本間距差異大時會 overshoot 與打結——手指快速轉向時
//! 必然發生。向心版沒有 cusp 與自交，代價只是每個 knot 多一個 `sqrt`。

use crate::math::Vec2;

/// knot 間距的下限。重合的控制點（手指停在原地）會讓 `t_{i+1} - t_i == 0`，
/// 而那個差進了 Barry–Goldman 的分母——沒有這個下限，「原地停留」直接產出 NaN。
const KNOT_EPS: f32 = 1e-4;

/// `p1`–`p2` 之間的一段。knot 只算一次，弧長取樣會沿著它取幾十個點。
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    p: [Vec2; 4],
    t: [f32; 4],
}

impl Segment {
    pub fn new(p: [Vec2; 4]) -> Self {
        let mut t = [0.0f32; 4];
        for i in 1..4 {
            // alpha = 0.5 ⇒ 間距開根號。這就是「向心」的全部。
            t[i] = t[i - 1] + p[i].distance(p[i - 1]).max(KNOT_EPS).sqrt();
        }
        Self { p, t }
    }

    /// `u ∈ [0, 1]` 橫跨 `p1`–`p2`。
    pub fn at(&self, u: f32) -> Vec2 {
        let (t, p) = (&self.t, &self.p);
        let tt = t[1] + (t[2] - t[1]) * u;

        // Barry–Goldman 三層遞推。每個分母都被 KNOT_EPS 撐開了，不會是 0。
        let a1 = lerp_t(p[0], p[1], t[0], t[1], tt);
        let a2 = lerp_t(p[1], p[2], t[1], t[2], tt);
        let a3 = lerp_t(p[2], p[3], t[2], t[3], tt);
        let b1 = lerp_t(a1, a2, t[0], t[2], tt);
        let b2 = lerp_t(a2, a3, t[1], t[3], tt);
        lerp_t(b1, b2, t[1], t[2], tt)
    }
}

fn lerp_t(a: Vec2, b: Vec2, ta: f32, tb: f32, t: f32) -> Vec2 {
    a.lerp(b, (t - ta) / (tb - ta))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(pts: [(f32, f32); 4]) -> Segment {
        Segment::new(pts.map(|(x, y)| Vec2::new(x, y)))
    }

    #[test]
    fn interpolates_the_inner_control_points() {
        let s = seg([(0.0, 0.0), (1.0, 0.0), (2.0, 1.0), (3.0, 1.0)]);
        let (a, b) = (s.at(0.0), s.at(1.0));
        assert!(
            a.distance(Vec2::new(1.0, 0.0)) < 1e-4,
            "u=0 應落在 p1，實得 {a:?}"
        );
        assert!(
            b.distance(Vec2::new(2.0, 1.0)) < 1e-4,
            "u=1 應落在 p2，實得 {b:?}"
        );
    }

    #[test]
    fn collinear_points_stay_collinear() {
        let s = seg([(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)]);
        for i in 0..=10 {
            let p = s.at(i as f32 / 10.0);
            assert!(p.y.abs() < 1e-4, "直線不得彎曲，u={i} 得 y={}", p.y);
        }
    }

    #[test]
    fn coincident_points_do_not_produce_nan() {
        // 手指停在原地：四個控制點完全重合。
        let s = seg([(5.0, 5.0); 4]);
        for i in 0..=10 {
            let p = s.at(i as f32 / 10.0);
            assert!(p.x.is_finite() && p.y.is_finite(), "得 {p:?}");
        }
    }

    #[test]
    fn sharp_turn_does_not_overshoot_the_hull() {
        // 極端的間距落差 ＋ 急轉。均勻參數化在這裡會衝出控制點的 bbox。
        let s = seg([(0.0, 0.0), (1.0, 0.0), (1.2, 0.2), (1.2, 60.0)]);
        let (lo, hi) = (Vec2::new(0.0, 0.0), Vec2::new(1.2, 60.0));
        for i in 0..=40 {
            let p = s.at(i as f32 / 40.0);
            assert!(
                p.x >= lo.x - 0.05 && p.x <= hi.x + 0.05 && p.y >= lo.y - 0.05,
                "衝出控制點範圍：{p:?}"
            );
        }
    }
}
