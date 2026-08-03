//! `torture-01` 是**合格**壓力素材（`specs/baker-core-design.md §5.1`），不是 negative
//! fixture。這裡是那句宣稱唯一的自動化證據——沒有它，「所有特徵最短邊 ≥8px 且對齊偶數
//! 邊界，降採樣＋膨脹後仍存活」只是 `TORTURE_NOTES` 裡的一段話。
//!
//! 素材由生成器現場產生，不讀 `assets/source/torture-01/*.png`——那三張走 LFS，
//! CI 以 `lfs: false` checkout 只拿得到 pointer（`build-infra.md §5`）。

use std::path::{Path, PathBuf};

use baker::{BakeOptions, bake, synth};
use colorpack::{Aspect, ColorPack, Difficulty};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 母帶 3072×4096、無 shade、上千區。烘得出來、且只帶得到預期中的警告。
#[test]
fn torture_01_is_a_passing_stress_asset() {
    let tmp = tempfile::tempdir().unwrap();
    let mut asset = synth::torture_01();
    // 內容與 committed 的三張完全相同，只降 PNG 壓縮等級——測試在乎像素不在乎檔案
    // 大小，而 High 在 4096 級尺寸上要多花數十秒。
    asset.compression = png::Compression::Fast;

    let src = asset.write(tmp.path()).expect("寫出素材失敗");
    let report =
        bake(&src, &BakeOptions::to(tmp.path().join("packs"))).expect("baker 不該自身故障");
    assert!(
        !report.has_error(),
        "torture-01 必須通過——它是合格素材，不是 fixture：\n{}",
        report.to_text()
    );

    // 唯一容許的警告是「區域數超出建議範圍」：上千區正是它存在的目的。
    // 碎片區域或輸出斷裂出現，就代表 ≥8px／偶數對齊的設計沒守住。
    for d in &report.diagnostics {
        assert_eq!(
            d.code,
            "region-count-range",
            "非預期的診斷：\n{}",
            report.to_text()
        );
    }

    let path = tmp
        .path()
        .join("packs")
        .join(format!("{}.colorpack", asset.id));
    let pack = ColorPack::open(std::fs::File::open(&path).unwrap()).expect("reader 開不回來");

    assert_eq!(pack.manifest.canvas_size, [1536, 2048]);
    assert_eq!(pack.manifest.aspect, Aspect::Portrait);
    assert_eq!(pack.manifest.difficulty, Difficulty::Focused);
    assert!(!pack.manifest.has_shade);
    assert!(pack.shade_png.is_none());
    assert!(
        pack.manifest.region_count > 2000,
        "區域數 {} 撐不起「壓力素材」這個定位",
        pack.manifest.region_count
    );
    assert_eq!(pack.regions.len(), pack.manifest.region_count as usize);
    let total: u32 = pack.regions.iter().map(|r| r.area).sum();
    assert_eq!(total, 1536 * 2048);
}

#[test]
fn committed_torture_matches_the_generator() {
    let path = repo_root()
        .join("assets/source/torture-01")
        .join(synth::LOCK_FILE);
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "讀不到 {}：{e}。跑 `cargo xtask gen-torture`",
                path.display()
            )
        }))
        .expect("synth-lock.json 不是合法 JSON");

    assert_eq!(
        lock["content_hash"].as_str().expect("缺 content_hash"),
        synth::torture_content_hash(),
        "assets/source/torture-01/ 與 baker::synth 已分家——跑 `cargo xtask gen-torture`"
    );
}
