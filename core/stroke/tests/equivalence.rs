//! `E1-stroke.md §2.1` 的不變式。**這是 gate**（§10）：它與參數值無關，
//! 只與實作一致性有關，所以不像 golden fixture 那樣會被調校弄紅。
//!
//! 目前 `generate_dabs` 委派給 `StrokeBuilder`，所以這幾條測試看起來像廢話。
//! 它們守的是未來：任何人把批次版拆成第二份實作，這裡會先紅。

mod support;

use stroke::{BrushPreset, StrokeBuilder, generate_dabs};
use support::{bits, dwell, fast_turn, slow_line, stylus_ramp};

const SIZE: f32 = 24.0;
const SEED: u32 = 0x5eed;

fn cases() -> Vec<(&'static str, Vec<stroke::InputSample>)> {
    vec![
        ("slow_line", slow_line()),
        ("fast_turn", fast_turn()),
        ("dwell", dwell()),
        ("stylus_ramp", stylus_ramp()),
    ]
}

#[test]
fn streaming_equals_batch() {
    let preset = BrushPreset::soft_round();
    for (name, samples) in cases() {
        let mut b = StrokeBuilder::new(preset, SIZE, SEED);
        for s in &samples {
            b.push(s);
        }
        let streamed = b.finish();
        let batched = generate_dabs(&samples, &preset, SIZE, SEED);

        assert!(!streamed.is_empty(), "{name}：不得產不出 dab");
        assert_eq!(bits(&streamed), bits(&batched), "{name}：串流與批次不等價");
    }
}

#[test]
fn take_new_does_not_change_the_result() {
    // Pass 1 每 frame 取一次新 dab。取用節奏是 frame 邊界決定的，
    // 不得影響整筆的輸出——否則 §2.1 會隨著 frame rate 飄。
    let preset = BrushPreset::soft_round();
    for (name, samples) in cases() {
        let mut drained = StrokeBuilder::new(preset, SIZE, SEED);
        let mut seen = 0;
        for (i, s) in samples.iter().enumerate() {
            drained.push(s);
            if i % 3 == 0 {
                seen += drained.take_new().len();
            }
        }
        let with_takes = drained.finish();
        let untouched = generate_dabs(&samples, &preset, SIZE, SEED);

        assert_eq!(
            bits(&with_takes),
            bits(&untouched),
            "{name}：取用節奏改變了輸出"
        );
        assert!(
            seen <= with_takes.len(),
            "{name}：take_new 吐出了不存在的 dab"
        );
    }
}

#[test]
fn push_granularity_does_not_matter() {
    let preset = BrushPreset::soft_round();
    for (name, samples) in cases() {
        let mut chunked = StrokeBuilder::new(preset, SIZE, SEED);
        for chunk in samples.chunks(7) {
            chunked.extend(chunk);
        }
        assert_eq!(
            bits(&chunked.finish()),
            bits(&generate_dabs(&samples, &preset, SIZE, SEED)),
            "{name}：批次大小改變了輸出"
        );
    }
}

#[test]
fn same_seed_is_bit_identical() {
    // §12：「同 seed 兩次執行逐位元相同」。jitter 全 0 時這條很鬆，
    // 但 E2 開了 jitter 之後它才是 oplog 重播能對得上的根據。
    let mut preset = BrushPreset::soft_round();
    preset.jitter_pos = 0.3;
    preset.jitter_size = 0.2;
    preset.jitter_angle = 1.0;

    for (name, samples) in cases() {
        let a = generate_dabs(&samples, &preset, SIZE, SEED);
        let b = generate_dabs(&samples, &preset, SIZE, SEED);
        assert_eq!(bits(&a), bits(&b), "{name}：同 seed 兩次執行不同");

        let c = generate_dabs(&samples, &preset, SIZE, SEED ^ 0xffff);
        assert_ne!(bits(&a), bits(&c), "{name}：換 seed 卻沒換 jitter");
    }
}

#[test]
fn predicted_dabs_do_not_touch_committed_state() {
    // §9：預測點只影響當前 frame。抬筆時的重建之所以便宜且正確，
    // 就是因為 committed 狀態從頭到尾沒被預測點碰過。
    let preset = BrushPreset::soft_round();
    let samples = slow_line();
    let (real, tail) = samples.split_at(40);

    let mut b = StrokeBuilder::new(preset, SIZE, SEED);
    b.extend(real);

    let predicted: Vec<_> = tail
        .iter()
        .map(|s| {
            let mut s = *s;
            s.predicted = true;
            s
        })
        .collect();

    let ghost = b.predicted_dabs(&predicted);
    assert!(!ghost.is_empty(), "預測點應該要產得出 dab");

    // 算完預測之後再繼續餵真實樣本，結果必須與從未算過預測完全相同。
    b.extend(tail);
    assert_eq!(
        bits(&b.finish()),
        bits(&generate_dabs(&samples, &preset, SIZE, SEED)),
        "predicted_dabs 污染了 committed 狀態"
    );
}
