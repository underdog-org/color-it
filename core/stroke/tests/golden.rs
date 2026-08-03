//! Golden test（`E1-stroke.md §10`）。`stroke` 是全專案唯一能在 CI 防手感回歸的地方。
//!
//! 參數尚未定案：D5 盲測必然會調動五支裡一半的欄位，每調一次 fixture 就會
//! 全紅——一組永遠是紅的測試等於沒有測試。**解 `#[ignore]` 排在所有參數定案
//! 之後**（`E2-brush.md §6.4`），提前解除只會再標回去。
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

/// 一條軌跡對五支 preset。**五支共用同一份輸入**，所以 fixture 之間的差異
/// 完全來自 preset——差異軸沒實作的話這裡會看得出來。
fn check_all(trajectory: &str, samples: Vec<InputSample>) {
    for (preset, make) in BrushPreset::ALL {
        check(trajectory, preset, make(), &samples);
    }
}

fn check(trajectory: &str, preset_name: &str, preset: BrushPreset, samples: &[InputSample]) {
    let name = format!("{trajectory}_{preset_name}");
    let name = name.as_str();
    let dabs = generate_dabs(samples, &preset, SIZE, SEED);
    let path = path(name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let fixture = Fixture {
            name: name.to_owned(),
            preset: preset_name.to_owned(),
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
    assert_eq!(want.preset, preset_name, "{name}：fixture 記的是另一支筆刷");
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
#[ignore = "參數定案（D5）後解除，見 E2-brush.md §6.4"]
fn golden_slow_line() {
    check_all("slow_line", slow_line());
}

#[test]
#[ignore = "參數定案（D5）後解除，見 E2-brush.md §6.4"]
fn golden_fast_turn() {
    check_all("fast_turn", fast_turn());
}

#[test]
#[ignore = "參數定案（D5）後解除，見 E2-brush.md §6.4"]
fn golden_dwell() {
    check_all("dwell", dwell());
}

/// **這條不是 `#[ignore]`。** 它不比對數值，只確認 15 個 fixture 都在——
/// 少一個檔案代表某支 preset 或某條軌跡被悄悄漏掉了，而那是上面三條在解除
/// ignore 之前唯一看得見的失敗方式。
#[test]
fn all_fifteen_fixtures_exist() {
    let mut missing = Vec::new();
    for trajectory in ["slow_line", "fast_turn", "dwell"] {
        for (preset, _) in BrushPreset::ALL {
            let p = path(&format!("{trajectory}_{preset}"));
            if !p.exists() {
                missing.push(format!("{trajectory}_{preset}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "缺少 {} 個 fixture：{missing:?}。用 UPDATE_GOLDEN=1 產生",
        missing.len()
    );
}
