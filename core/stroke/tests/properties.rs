//! `E1-stroke.md §12` 裡不需要 GPU 就驗得到的那幾條。
//!
//! **這些是 gate，不像 golden fixture 標 `#[ignore]`**：它們驗的是性質
//! （不 overshoot、不打結、停留不重複下 dab），不是具體數值——調參數不會弄紅它們。

mod support;

use stroke::{BrushPreset, StrokeBuilder, Vec2, generate_dabs};
use support::{DT, bits, dwell, fast_turn, slow_line, stylus_ramp};

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

// ── RNG 契約（`E2-brush.md §6.2`）─────────────────────────────────────────
//
// **這組與參數值無關，所以現在就是 gate，不必等 D5 定案。** golden fixture 標
// `#[ignore]` 是因為它比對具體數值；這幾條比對的是不變式，調參數不會弄紅它們。

#[test]
fn the_same_seed_replays_bit_for_bit() {
    // 契約第一條：一筆一個 seed。少了它，E3 的縮時重播會與原作不同。
    for (name, make) in BrushPreset::ALL {
        let preset = make();
        let a = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
        let b = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
        assert_eq!(bits(&a), bits(&b), "{name} 同 seed 兩次執行不同");
    }

    // 有 jitter 的那支，換 seed 就該換一組顆粒——否則 seed 根本沒接上。
    let crayon = BrushPreset::crayon();
    let a = generate_dabs(&fast_turn(), &crayon, SIZE, SEED);
    let b = generate_dabs(&fast_turn(), &crayon, SIZE, SEED ^ 0xffff);
    assert_ne!(bits(&a), bits(&b), "換 seed 沒有換掉 jitter");
}

#[test]
fn zero_jitter_makes_the_seed_irrelevant() {
    let mut preset = BrushPreset::crayon();
    preset.jitter_pos = 0.0;
    preset.jitter_size = 0.0;
    preset.jitter_angle = 0.0;

    let a = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
    let b = generate_dabs(&fast_turn(), &preset, SIZE, SEED ^ 0xffff);
    assert_eq!(bits(&a), bits(&b), "jitter 全 0 時 seed 仍然影響輸出");
}

#[test]
fn jitter_does_not_feed_back_into_sampling() {
    let base = BrushPreset::crayon();
    let n = generate_dabs(&fast_turn(), &base, SIZE, SEED).len();

    for (label, mutate) in [
        (
            "jitter_pos",
            (|p: &mut BrushPreset| p.jitter_pos *= 3.0) as fn(&mut BrushPreset),
        ),
        ("jitter_size", |p: &mut BrushPreset| p.jitter_size = 0.4),
        ("jitter_angle", |p: &mut BrushPreset| p.jitter_angle *= 0.5),
    ] {
        let mut preset = base;
        mutate(&mut preset);
        let dabs = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
        assert_eq!(dabs.len(), n, "改 {label} 改變了 dab 數量");
    }

    // 位置只有 `jitter_pos` 動得到：改另外兩欄，擾動前的取樣位置必須一動不動。
    let mut preset = base;
    preset.jitter_size = 0.4;
    preset.jitter_angle *= 0.5;
    let moved = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
    let base_dabs = generate_dabs(&fast_turn(), &base, SIZE, SEED);
    assert!(
        moved
            .iter()
            .zip(&base_dabs)
            .all(|(a, b)| a.pos.x.to_bits() == b.pos.x.to_bits()
                && a.pos.y.to_bits() == b.pos.y.to_bits()),
        "改 jitter_size／jitter_angle 動到了取樣位置"
    );
}

#[test]
fn only_the_presets_that_ask_for_it_react_to_speed() {
    let mut checked_zero = 0;
    for (name, make) in BrushPreset::ALL {
        let mut preset = make();
        // jitter_size 也會動到 size，先關掉才量得到速度那一項。
        preset.jitter_size = 0.0;
        let dabs = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
        let sizes: Vec<u32> = dabs.iter().map(|d| d.size.to_bits()).collect();
        let uniform = sizes.windows(2).all(|w| w[0] == w[1]);

        if preset.velocity_to_size == 0.0 {
            assert!(
                uniform,
                "{name} 的 velocity_to_size 是 0，size 卻跟著速度變了"
            );
            checked_zero += 1;
        } else {
            assert!(!uniform, "{name} 的 velocity_to_size 非 0，size 卻沒反應");
        }
    }
    assert_eq!(checked_zero, 3, "§4.2 說五支裡有三支不吃速度");
}

#[test]
fn speed_makes_the_stroke_thinner_never_thicker() {
    // 語意方向：越快越細。反過來會讓快掃變成粗線——那是「手滑一下就一大坨」。
    let preset = BrushPreset::airbrush();
    let dabs = generate_dabs(&fast_turn(), &preset, SIZE, SEED);
    let at_rest = SIZE * preset.pressure_to_size.eval(0.5);
    for d in &dabs {
        assert!(
            d.size <= at_rest + 1e-4,
            "速度把 size 放大了：{} > {at_rest}",
            d.size
        );
    }
    // 而且真的細到看得出來，不是四捨五入等級的差異。
    let min = dabs.iter().map(|d| d.size).fold(f32::MAX, f32::min);
    assert!(
        min < at_rest * 0.95,
        "快掃只細了 {}%",
        100.0 - min / at_rest * 100.0
    );
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
