//! Golden test（`specs/baker-seeds.md §6` 第 1 條）。

use baker::report::code;
use baker::{BakeOptions, bake, synth};
use colorpack::{Aspect, ColorPack, Difficulty};

/// `region_ids` 的正規化 hash。**單獨凍結**：`content_hash` 一起涵蓋六個 entry，
/// 只看它的話「哪一段漂移了」還要再查一次。這條一動就是 labels 本身變了。
const REGION_IDS_HASH: &str =
    "sha256:f127c47e847bf3c905be843b91611c140772eaaddb2718a2972682a84d3b8a93";

/// 整包的 `content_hash`（regions.json / regions.bin / lineart / shade / thumb）。
const CONTENT_HASH: &str =
    "sha256:ccd6dbe445de132631c7578b9449452fb0c0c839a5682195775a41c83b38fbc4";

/// 6×8 格。埋在 cell(2,3) 的碎片被 `merge_small_orphans` 併掉，不另成一區。
const REGIONS: u32 = 48;

#[test]
fn the_golden_asset_bakes_to_frozen_bytes() {
    let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
    let asset = synth::golden();
    let src = asset.write(tmp.path()).expect("寫出素材失敗");
    let report =
        bake(&src, &BakeOptions::to(tmp.path().join("packs"))).expect("baker 不該自身故障");
    assert!(
        !report.has_error(),
        "golden 素材必須通過：\n{}",
        report.to_text()
    );
    // 碎片是**靜默**併入的。這條先斷言，否則「碎片改成報錯」會退化成
    // 下面那句看不懂的 hash 不符。
    assert!(
        report.find(code::ORPHAN_AREA).is_none(),
        "324px 的碎片該被靜默併入鄰居（§3.1 ④）：\n{}",
        report.to_text()
    );

    let path = tmp.path().join("packs").join("synth-golden.colorpack");
    let pack = ColorPack::open(std::fs::File::open(&path).expect("找不到產出的 .colorpack"))
        .expect("reader 開不回來");

    // 結構先斷言：hash 不符時，這幾條能立刻分辨「區域數變了」與「只有邊界像素挪動」。
    assert_eq!(pack.manifest.canvas_size, [1536, 2048]);
    assert_eq!(pack.manifest.aspect, Aspect::Portrait);
    assert_eq!(pack.manifest.region_count, REGIONS);
    assert_eq!(pack.manifest.difficulty, Difficulty::Easy);
    assert!(pack.manifest.has_shade);
    assert_eq!(pack.region_ids.len(), 1536 * 2048);
    let total: u32 = pack.regions.iter().map(|r| r.area).sum();
    assert_eq!(total, 1536 * 2048, "全覆蓋不變式（§3.1 ②）");

    let ids: Vec<u8> = pack
        .region_ids
        .iter()
        .flat_map(|id| id.to_le_bytes())
        .collect();
    assert_eq!(
        colorpack::hash::content_hash(&[("region_ids", &ids)]),
        REGION_IDS_HASH,
        "逐像素 region id 漂移了——grow / merge_small_orphans / close / resample / dilate \
         有一條改了行為。確認那是刻意的契約變更（＝全量重烘）再更新凍結值"
    );
    assert_eq!(
        pack.manifest.content_hash, CONTENT_HASH,
        "整包內容漂移了。region_ids 那條若是綠的，問題在 lineart / shade / thumb / regions.json"
    );
}
