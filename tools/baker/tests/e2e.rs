//! E2E：素材 → pack → 用 colorpack reader 開回來（`specs/baker-core-design.md §6`）。
//!
//! **CI 跑的是 `synth` 生成的素材，不是 LFS 的三張。** `build-infra.md §5` 明訂 CI 以
//! `lfs: false` checkout，真實素材在 CI 裡只拿得到 pointer；而且把 golden 值綁在手繪素材上，
//! 素材一改就要改測試。真實素材的烘焙留給本地的 `cargo xtask bake`。

use std::path::Path;

use baker::{BakeOptions, bake, synth};
use colorpack::{Aspect, ColorPack, Difficulty};

fn bake_synth(dir: &Path, asset: &synth::Asset) -> (baker::report::Report, ColorPack) {
    let src = asset.write(dir).expect("寫出素材失敗");
    let opts = BakeOptions::to(dir.join("packs"));
    let report = bake(&src, &opts).expect("baker 不該自身故障");
    assert!(
        !report.has_error(),
        "合成素材應該通過：\n{}",
        report.to_text()
    );
    let path = dir.join("packs").join(format!("{}.colorpack", asset.id));
    let file = std::fs::File::open(&path).expect("找不到產出的 .colorpack");
    (report, ColorPack::open(file).expect("reader 開不回來"))
}

/// 1:1 ＋ 有 shade。4096² 母帶、1024 格 → 4×4 = 16 區。
#[test]
fn square_with_shade_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let asset = synth::valid("synth-square", 4096, 4096, 1024, true);
    let (_, pack) = bake_synth(tmp.path(), &asset);

    assert_eq!(pack.manifest.canvas_size, [2048, 2048]);
    assert_eq!(pack.manifest.aspect, Aspect::Square);
    assert_eq!(pack.manifest.region_count, 16);
    assert_eq!(pack.manifest.difficulty, Difficulty::Easy);
    assert!(pack.manifest.has_shade);
    assert!(pack.shade_png.is_some());

    assert_eq!(pack.regions.len(), 16);
    assert_eq!(pack.region_ids.len(), 2048 * 2048);
    // 每區的 area 加總必須等於整張畫布——`architecture §4.7` 拿它當進度分母。
    let total: u32 = pack.regions.iter().map(|r| r.area).sum();
    assert_eq!(total, 2048 * 2048);
    assert!(pack.thumb_jpg.starts_with(&[0xff, 0xd8]));
    assert!(pack.manifest.content_hash.starts_with("sha256:"));
}

/// 3:4 ＋ 無 shade。`has_shade = false` 那條路徑。
#[test]
fn portrait_without_shade_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let asset = synth::valid("synth-portrait", 3072, 4096, 256, false);
    let (_, pack) = bake_synth(tmp.path(), &asset);

    assert_eq!(pack.manifest.canvas_size, [1536, 2048]);
    assert_eq!(pack.manifest.aspect, Aspect::Portrait);
    assert_eq!(pack.manifest.region_count, 12 * 16);
    assert_eq!(pack.manifest.difficulty, Difficulty::Medium);
    assert!(!pack.manifest.has_shade);
    assert!(pack.shade_png.is_none());
    assert_eq!(pack.region_ids.len(), 1536 * 2048);

    // regions.json 的索引即 id，且 bbox 落在畫布內
    for (i, region) in pack.regions.iter().enumerate() {
        assert_eq!(region.id, i as u32);
        assert!(region.bbox[0] + region.bbox[2] <= 1536);
        assert!(region.bbox[1] + region.bbox[3] <= 2048);
        assert!(region.suggested_color.starts_with('#'));
    }
    // palette 去重：synth 的 reference 只有 15 個顏色在用
    assert!(pack.manifest.palette.len() <= 15);
}

/// 同輸入重跑 → `.colorpack` 位元相同（§3.2）。
#[test]
fn baking_twice_is_bit_identical() {
    let tmp = tempfile::tempdir().unwrap();
    let asset = synth::valid("synth-determinism", 3072, 4096, 512, true);
    let src = asset.write(tmp.path()).unwrap();

    let mut bytes = Vec::new();
    for run in 0..2 {
        let out = tmp.path().join(format!("packs-{run}"));
        let report = bake(&src, &BakeOptions::to(&out)).unwrap();
        assert!(!report.has_error(), "{}", report.to_text());
        bytes.push(std::fs::read(out.join("synth-determinism.colorpack")).unwrap());
    }
    assert_eq!(bytes[0], bytes[1], ".colorpack 兩次烘焙位元不同");
}
