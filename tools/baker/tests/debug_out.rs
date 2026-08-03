//! `--debug-out` 是**退件附件**（`specs/baker-seeds.md §5`）。
//!
//! 它唯一的使用場景就是素材被拒收的那一刻——「因為有錯所以什麼都不給你」對繪師
//! 毫無用處。所以這裡測的是拒收路徑，不是通過路徑。

use std::path::Path;

use baker::synth::{self, Negative};
use baker::{BakeOptions, bake, debug_out};

fn size_of(dir: &Path, name: &str) -> u64 {
    std::fs::metadata(dir.join(name))
        .unwrap_or_else(|e| panic!("{name} 沒有產出：{e}"))
        .len()
}

/// `seed-collision` 被拒收，四件產物仍然齊備。
#[test]
fn the_four_attachments_are_produced_even_when_the_asset_is_rejected() {
    let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
    let fixture = synth::negative(Negative::SeedCollision);
    let src = fixture.asset.write(tmp.path()).expect("寫出 fixture 失敗");
    let debug = tmp.path().join("debug");

    let mut opts = BakeOptions::to(tmp.path().join("packs"));
    opts.debug_out = Some(debug.clone());
    let report = bake(&src, &opts).expect("baker 不該自身故障");

    assert!(report.has_error(), "這張 fixture 本來就該被拒收");
    for name in [
        debug_out::PREVIEW,
        debug_out::SEEDS_OVERLAY,
        debug_out::REFERENCE_PREVIEW,
        debug_out::REGIONS_JSON,
    ] {
        assert!(size_of(&debug, name) > 0, "{name} 是空的");
    }

    // 附件要對得上診斷：撞在一起的那兩個色標必須在 regions.json 裡查得到。
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(debug.join(debug_out::REGIONS_JSON)).unwrap(),
    )
    .expect("regions.json 不是合法 JSON");
    let anchors: Vec<(u32, u32)> = json["regions"]
        .as_array()
        .expect("regions 是陣列")
        .iter()
        .map(|r| {
            (
                r["seed_anchor"][0].as_u64().unwrap() as u32,
                r["seed_anchor"][1].as_u64().unwrap() as u32,
            )
        })
        .collect();
    for planted in &fixture.planted {
        assert!(
            anchors.contains(planted),
            "植入的色標 {planted:?} 不在 regions.json 裡"
        );
    }
}

/// 沒給 `--debug-out` 就一個檔都不寫。
#[test]
fn nothing_is_written_without_the_flag() {
    let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
    let asset = synth::valid("synth-no-debug", 3072, 4096, 1024, false);
    let src = asset.write(tmp.path()).expect("寫出素材失敗");

    let opts = BakeOptions::to(tmp.path().join("packs"));
    assert!(opts.debug_out.is_none());
    bake(&src, &opts).expect("baker 不該自身故障");

    let stray = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.file_name() == "debug");
    assert!(!stray, "沒給旗標卻寫了東西出來");
}
