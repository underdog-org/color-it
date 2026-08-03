//! 三條測試軌跡（`E1-stroke.md §10`）。
//!
//! 每個 test binary 各自編譯本模組，用不到的那幾個一定會被判 dead code。
#![allow(dead_code)]

use stroke::{Dab, InputSample, Vec2};

/// 120 Hz。`t` 一律由這個間隔推，好讓軌跡的「速度」是可控的。
pub const DT: f32 = 1.0 / 120.0;

/// 直線慢速：每 frame 前進 2 px。
pub fn slow_line() -> Vec<InputSample> {
    (0..60)
        .map(|i| {
            let f = i as f32;
            InputSample::finger(Vec2::new(20.0 + f * 2.0, 50.0), f * DT, 9.0)
        })
        .collect()
}

/// 快速轉向：兩段幾乎反向的長直線，接縫處只有一個樣本。
///
/// 這是均勻參數化 Catmull-Rom 會 overshoot 與打結的那個形狀（`§4.2`）——
/// 前半每 frame 40 px，轉角處驟降到 3 px，樣本間距差了一個數量級。
///
/// 長度剛好夠讓打結測試有意義（>30 個 dab）就停：spacing 是 1.2 px，
/// 再長只是讓 golden fixture 的 JSON 變肥，測不到新東西。
pub fn fast_turn() -> Vec<InputSample> {
    const LEG: usize = 7;
    const STEP: f32 = 40.0;
    let turn = 10.0 + (LEG - 1) as f32 * STEP;

    let mut out = Vec::new();
    let mut t = 0.0;
    for i in 0..LEG {
        out.push(InputSample::finger(
            Vec2::new(10.0 + i as f32 * STEP, 100.0),
            t,
            11.0,
        ));
        t += DT;
    }
    out.push(InputSample::finger(Vec2::new(turn + 3.0, 103.0), t, 11.0));
    t += DT;
    for i in 1..LEG {
        out.push(InputSample::finger(
            Vec2::new(turn - i as f32 * STEP, 106.0),
            t,
            11.0,
        ));
        t += DT;
    }
    out
}

/// 原地停留：位置完全不動，只有 `radius` 有一點雜訊（真手指必然如此）。
pub fn dwell() -> Vec<InputSample> {
    (0..40)
        .map(|i| {
            let f = i as f32;
            let r = 8.0 + if i % 2 == 0 { 0.15 } else { -0.15 };
            InputSample::finger(Vec2::new(200.0, 200.0), f * DT, r)
        })
        .collect()
}

/// 觸控筆版的直線，壓感由輕到重再到輕。`radius == 0`（`§2.2`）。
pub fn stylus_ramp() -> Vec<InputSample> {
    (0..60)
        .map(|i| {
            let f = i as f32;
            let p = 1.0 - (f / 59.0 * 2.0 - 1.0).abs();
            InputSample::stylus(Vec2::new(20.0 + f * 4.0, 80.0), f * DT, p)
        })
        .collect()
}

/// 逐位元比較。`§12` 要的是「逐位元相同」，不是「差不多」。
pub fn bits(dabs: &[Dab]) -> Vec<[u32; 5]> {
    dabs.iter()
        .map(|d| {
            [
                d.pos.x.to_bits(),
                d.pos.y.to_bits(),
                d.size.to_bits(),
                d.angle.to_bits(),
                d.alpha.to_bits(),
            ]
        })
        .collect()
}
