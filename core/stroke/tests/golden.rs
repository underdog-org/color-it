//! Golden test（`E1-stroke.md §10`）。`stroke` 是全專案唯一能在 CI 防手感回歸的地方。
//!
//! # 為什麼全部標 `#[ignore]`
//!
//! **E1 不設為 CI gate**：One-Euro 與 `R_EPS` 的參數還在實機調校，每調一次
//! fixture 就會全紅——一組永遠是紅的測試等於沒有測試。E2 參數定案後拿掉
//! `#[ignore]` 即為 gate（`§14` 決議 D）。
//!
//! 真正現在就守著的是 `equivalence.rs`（串流／批次等價、同 seed 逐位元相同）
//! 與 `properties.rs`（不 overshoot、不打結）——它們與參數值無關。
//!
//! ```sh
//! cargo test -p colorlull-stroke --test golden -- --ignored              # 比對
//! UPDATE_GOLDEN=1 cargo test -p colorlull-stroke --test golden -- --ignored  # 調校後重產
//! ```

mod support;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use stroke::{BrushPreset, Dab, InputSample, Vec2, generate_dabs};
use support::{dwell, fast_turn, slow_line};

const SIZE: f32 = 24.0;
const SEED: u32 = 0x5eed;

/// 比對容差。刻意不用逐位元：`sqrt` / `powf` 的最後一位在不同 target 上會差，
/// 而那不是手感回歸。「逐位元相同」由 `equivalence.rs` 在同一台機器上守。
const TOL: f32 = 1e-4;

#[derive(Serialize, Deserialize)]
struct Fixture {
    name: String,
    preset: String,
    size: f32,
    seed: u32,
    samples: Vec<S>,
    dabs: Vec<D>,
}

#[derive(Serialize, Deserialize)]
struct S {
    x: f32,
    y: f32,
    t: f32,
    pressure: f32,
    radius: f32,
}

#[derive(Serialize, Deserialize, Debug)]
struct D {
    x: f32,
    y: f32,
    size: f32,
    angle: f32,
    alpha: f32,
}

impl From<&InputSample> for S {
    fn from(s: &InputSample) -> Self {
        Self {
            x: s.pos.x,
            y: s.pos.y,
            t: s.t,
            pressure: s.pressure,
            radius: s.radius,
        }
    }
}

impl From<&S> for InputSample {
    fn from(s: &S) -> Self {
        Self {
            pos: Vec2::new(s.x, s.y),
            t: s.t,
            pressure: s.pressure,
            radius: s.radius,
            tilt: Vec2::ZERO,
            predicted: false,
        }
    }
}

impl From<&Dab> for D {
    fn from(d: &Dab) -> Self {
        Self {
            x: d.pos.x,
            y: d.pos.y,
            size: d.size,
            angle: d.angle,
            alpha: d.alpha,
        }
    }
}

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"))
}

/// fixture 的輸入軌跡是這裡的 SSOT，期望輸出才存在 JSON 裡——
/// 兩邊都存會讓「改了軌跡忘了重產」變成靜默通過。
fn check(name: &str, samples: Vec<InputSample>) {
    let dabs = generate_dabs(&samples, &BrushPreset::soft_round(), SIZE, SEED);
    let path = path(name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let fixture = Fixture {
            name: name.to_owned(),
            preset: "soft_round".to_owned(),
            size: SIZE,
            seed: SEED,
            samples: samples.iter().map(S::from).collect(),
            dabs: dabs.iter().map(D::from).collect(),
        };
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir tests/golden");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");
        eprintln!("更新 {}：{} 個 dab", path.display(), dabs.len());
        return;
    }

    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("讀不到 {}：{e}。用 UPDATE_GOLDEN=1 產生", path.display()));
    let want: Fixture = serde_json::from_str(&raw).expect("parse fixture");

    assert_eq!(want.size, SIZE, "{name}：fixture 的 size 與測試不符");
    assert_eq!(want.seed, SEED, "{name}：fixture 的 seed 與測試不符");
    assert_eq!(
        want.samples.len(),
        samples.len(),
        "{name}：輸入軌跡改了，需要 UPDATE_GOLDEN=1 重產"
    );
    assert_eq!(
        dabs.len(),
        want.dabs.len(),
        "{name}：dab 數量 {} → {}",
        want.dabs.len(),
        dabs.len()
    );

    for (i, (got, want)) in dabs.iter().zip(&want.dabs).enumerate() {
        let got = D::from(got);
        for (field, a, b) in [
            ("x", got.x, want.x),
            ("y", got.y, want.y),
            ("size", got.size, want.size),
            ("angle", got.angle, want.angle),
            ("alpha", got.alpha, want.alpha),
        ] {
            assert!(
                (a - b).abs() <= TOL,
                "{name} dab#{i} 的 {field}：{a} != {b}"
            );
        }
    }
}

#[test]
#[ignore = "E1 參數調校中，不設為 CI gate（§10）"]
fn golden_slow_line() {
    check("slow_line", slow_line());
}

#[test]
#[ignore = "E1 參數調校中，不設為 CI gate（§10）"]
fn golden_fast_turn() {
    check("fast_turn", fast_turn());
}

#[test]
#[ignore = "E1 參數調校中，不設為 CI gate（§10）"]
fn golden_dwell() {
    check("dwell", dwell());
}
