//! `E1-stroke.md §12` 裡不需要 GPU 就驗得到的那幾條。
//!
//! **這些是 gate，不像 golden fixture 標 `#[ignore]`**：它們驗的是性質
//! （不 overshoot、不打結、停留不重複下 dab），不是具體數值——調參數不會弄紅它們。

mod support;

use stroke::{BrushPreset, StrokeBuilder, Vec2, generate_dabs};
use support::{DT, dwell, fast_turn, slow_line, stylus_ramp};

const SIZE: f32 = 24.0;
const SEED: u32 = 0x5eed;

fn soft_round(samples: &[stroke::InputSample]) -> Vec<stroke::Dab> {
    generate_dabs(samples, &BrushPreset::soft_round(), SIZE, SEED)
}

#[test]
fn a_single_sample_still_leaves_a_dab() {
    // 點一下就該留下一個點。零 dab 是「點了沒反應」。
    let dabs = soft_round(&slow_line()[..1]);
    assert_eq!(dabs.len(), 1);
}

#[test]
fn dwelling_in_place_emits_exactly_one_dab() {
    // §12：「慢速來回塗抹同一處，濃度不隨次數變深」的 CPU 側前提——
    // 手指不動就不該持續產 dab。真正的濃度上限由 T_wet ＋ Max blend 保證（Pass 1）。
    let dabs = soft_round(&dwell());
    assert_eq!(dabs.len(), 1, "原地停留產出了 {} 個 dab", dabs.len());
}

#[test]
fn fast_turn_stays_inside_the_input_bounds() {
    // §4.2：向心參數化沒有 overshoot。均勻版會在急轉角衝出樣本的 bbox。
    let samples = fast_turn();
    let dabs = soft_round(&samples);

    let (mut lo, mut hi) = (Vec2::new(f32::MAX, f32::MAX), Vec2::new(f32::MIN, f32::MIN));
    for s in &samples {
        lo = Vec2::new(lo.x.min(s.pos.x), lo.y.min(s.pos.y));
        hi = Vec2::new(hi.x.max(s.pos.x), hi.y.max(s.pos.y));
    }

    // 容差只給濾波的平滑量，不給樣條的 overshoot。
    const SLACK: f32 = 1.0;
    for d in &dabs {
        assert!(
            d.pos.x >= lo.x - SLACK
                && d.pos.x <= hi.x + SLACK
                && d.pos.y >= lo.y - SLACK
                && d.pos.y <= hi.y + SLACK,
            "dab 衝出輸入範圍：{:?} 不在 {lo:?}..{hi:?}",
            d.pos
        );
    }
}

#[test]
fn fast_turn_does_not_knot() {
    let dabs = soft_round(&fast_turn());
    let pts: Vec<Vec2> = dabs.iter().map(|d| d.pos).collect();
    assert!(pts.len() > 30, "軌跡太短，測不出打結");

    for i in 0..pts.len() - 1 {
        for j in i + 2..pts.len() - 1 {
            assert!(
                !segments_cross(pts[i], pts[i + 1], pts[j], pts[j + 1]),
                "第 {i} 段與第 {j} 段相交：{:?}-{:?} × {:?}-{:?}",
                pts[i],
                pts[i + 1],
                pts[j],
                pts[j + 1]
            );
        }
    }
}

#[test]
fn spacing_matches_the_preset() {
    let preset = BrushPreset::soft_round();
    let dabs = soft_round(&slow_line());
    assert!(dabs.len() > 10);

    for pair in dabs.windows(2) {
        let step = pair[0].pos.distance(pair[1].pos);
        let want = preset.spacing * pair[0].size;
        assert!(
            (step - want).abs() < want * 0.25,
            "間距 {step} 偏離 {want} 太多"
        );
    }
}

#[test]
fn pressure_drives_size_and_alpha_within_the_curve() {
    let preset = BrushPreset::soft_round();
    let dabs = soft_round(&stylus_ramp());

    let (mut min_size, mut max_size) = (f32::MAX, f32::MIN);
    for d in &dabs {
        assert!(d.size > 0.0 && d.size.is_finite());
        assert!((0.0..=1.0).contains(&d.alpha), "alpha 出界：{}", d.alpha);
        assert!(
            d.size >= SIZE * preset.pressure_to_size.min - 1e-3
                && d.size <= SIZE * preset.pressure_to_size.max + 1e-3,
            "size 出了曲線的值域：{}",
            d.size
        );
        min_size = min_size.min(d.size);
        max_size = max_size.max(d.size);
    }
    // 壓感由輕到重再到輕，筆寬必須真的跟著動。
    assert!(max_size > min_size * 1.5, "壓感沒有驅動筆寬");
}

#[test]
fn stylus_uses_pressure_and_finger_uses_radius() {
    // §2.2：radius > 0 → 手指、走自適應正規化並忽略 pressure；radius == 0 → 觸控筆。
    let preset = BrushPreset::soft_round();

    // 同一條軌跡，pressure 給滿但 radius 也給著 → 應該走 radius，不理 pressure。
    let finger: Vec<_> = slow_line()
        .iter()
        .map(|s| {
            let mut s = *s;
            s.pressure = 1.0;
            s
        })
        .collect();
    let ignored = generate_dabs(&finger, &preset, SIZE, SEED);
    let baseline = soft_round(&slow_line());
    assert_eq!(ignored.len(), baseline.len(), "手指模式沒有忽略 pressure");
    assert!(
        ignored
            .iter()
            .zip(&baseline)
            .all(|(a, b)| (a.size - b.size).abs() < 1e-5),
        "手指模式沒有忽略 pressure"
    );

    // 觸控筆：radius 全 0，壓感必須真的生效。
    let hard: Vec<_> = (0..30)
        .map(|i| stroke::InputSample::stylus(Vec2::new(i as f32 * 5.0, 0.0), i as f32 * DT, 1.0))
        .collect();
    let soft: Vec<_> = hard
        .iter()
        .map(|s| {
            let mut s = *s;
            s.pressure = 0.0;
            s
        })
        .collect();
    let hard_size = generate_dabs(&hard, &preset, SIZE, SEED)[0].size;
    let soft_size = generate_dabs(&soft, &preset, SIZE, SEED)[0].size;
    assert!(hard_size > soft_size, "觸控筆的 pressure 沒有生效");
}

#[test]
fn finger_baseline_starts_mid_range() {
    // §14 決議 F：帶狀初值讓起筆得中值，而不是恆為 0（＝最細最淡）。
    let preset = BrushPreset::soft_round();
    let dabs = soft_round(&slow_line());
    let mid = SIZE * preset.pressure_to_size.eval(0.5);
    assert!(
        (dabs[0].size - mid).abs() < 1e-3,
        "起筆的 size {} 應為中值 {mid}",
        dabs[0].size
    );
}

#[test]
fn opacity_is_not_baked_into_the_dab() {
    // §7／§8：dab.alpha = flow × pressure_to_opacity(p)。整筆上限 opacity 是
    // Pass 2 commit 時才乘的——烘進 dab 會讓「調 opacity 變成每個 dab 變淡」，
    // 那是 §12 明文要避免的行為。
    let mut preset = BrushPreset::soft_round();
    let a = generate_dabs(&slow_line(), &preset, SIZE, SEED);
    preset.opacity = 0.1;
    let b = generate_dabs(&slow_line(), &preset, SIZE, SEED);
    assert!(
        a.iter().zip(&b).all(|(x, y)| x.alpha == y.alpha),
        "opacity 洩進了 dab.alpha"
    );
}

#[test]
fn a_long_fast_stroke_is_not_truncated() {
    let samples: Vec<_> = (0..400)
        .map(|i| stroke::InputSample::finger(Vec2::new(i as f32 * 60.0, 0.0), i as f32 * DT, 10.0))
        .collect();
    let dabs = soft_round(&samples);
    assert!(
        dabs.len() > stroke::MAX_DABS_PER_DRAW,
        "這條測試要跨過分批門檻才有意義，實得 {}",
        dabs.len()
    );
    // 最後一個 dab 要真的走到軌跡尾端，不是停在某個上限。
    let end = samples.last().unwrap().pos;
    assert!(
        dabs.last().unwrap().pos.distance(end) < 50.0,
        "筆畫在中途斷了：尾端 {:?} 應接近 {end:?}",
        dabs.last().unwrap().pos
    );
}

#[test]
fn every_dab_is_finite() {
    for samples in [slow_line(), fast_turn(), dwell(), stylus_ramp()] {
        for d in soft_round(&samples) {
            assert!(
                d.pos.x.is_finite()
                    && d.pos.y.is_finite()
                    && d.size.is_finite()
                    && d.angle.is_finite()
                    && d.alpha.is_finite(),
                "產出了非有限值：{d:?}"
            );
        }
    }
}

#[test]
fn builder_is_usable_frame_by_frame() {
    // Pass 1 的實際用法：每 frame 餵一批、取一批，抬筆時 finish。
    let mut b = StrokeBuilder::new(BrushPreset::soft_round(), SIZE, SEED);
    let samples = fast_turn();
    let mut total = 0;
    for chunk in samples.chunks(4) {
        b.extend(chunk);
        total += b.take_new().len();
    }
    let all = b.finish();
    assert!(total > 0 && total <= all.len());
}

/// 兩條線段是否真的交叉（共端點不算）。
fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    fn cross(o: Vec2, p: Vec2, q: Vec2) -> f32 {
        (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x)
    }
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}
