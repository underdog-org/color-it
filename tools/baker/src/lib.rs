//! 資產烘焙管線。
//!
//! ```text
//! Source ──▶ Master ──▶ RegionMap(母帶) ──▶ Output ──▶ ColorPack
//! ```
//!
//! 規格見 `docs/specs/baker-core-design.md`；上位描述見 `docs/architecture.md §9`。

pub mod binarize;
pub mod check;
pub mod compose;
pub mod dilate;
pub mod image;
pub mod migrate;
pub mod report;
pub mod resample;
pub mod seeds;
pub mod segment;
pub mod source;
pub mod synth;
pub mod thumb;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colorpack::manifest::Difficulty;
use colorpack::region::hex_color;
use colorpack::{ColorPack, Manifest, RegionEntry};

use crate::image::{Image, PngOptions};
use crate::report::{Report, Summary, code};

/// `baker-seeds.md §3.3` 的四個參數。預設值是契約的一部分——真要調就等於改契約，
/// 全量重烘是應該的。
///
/// **尚未納入 `content_hash`**：`content_hash` 由 pack entries 算出（`colorpack::ColorPack::write_to`），
/// 刻意排除 `manifest.json`，而 §「`.colorpack` 格式一律不動」擋掉了新增 hashed entry 這條路
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub line_threshold: u8,
    pub min_seed_area: u32,
    pub min_orphan_area: u32,
    pub max_line_ratio: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            line_threshold: binarize::DEFAULT_LINE_THRESHOLD,
            min_seed_area: seeds::MIN_SEED_AREA,
            min_orphan_area: segment::MIN_ORPHAN_AREA,
            max_line_ratio: binarize::MAX_LINE_RATIO,
        }
    }
}

impl Params {
    pub const KEYS: [&'static str; 4] = [
        "line_threshold",
        "min_seed_area",
        "min_orphan_area",
        "max_line_ratio",
    ];

    /// `--set key=value` 的解析。認不得的 key 直接報錯並列出四個合法值——
    /// 打錯字卻靜默套用預設是最難查的那種 bug。
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let bad = |what: &str| {
            anyhow::anyhow!("--set {key}={value}：{value:?} 不是合法的 {what}")
        };
        match key {
            "line_threshold" => self.line_threshold = value.parse().map_err(|_| bad("u8"))?,
            "min_seed_area" => self.min_seed_area = value.parse().map_err(|_| bad("u32"))?,
            "min_orphan_area" => self.min_orphan_area = value.parse().map_err(|_| bad("u32"))?,
            "max_line_ratio" => {
                let v: f32 = value.parse().map_err(|_| bad("f32"))?;
                if !(0.0..=1.0).contains(&v) {
                    anyhow::bail!("--set max_line_ratio={value}：必須落在 0.0–1.0");
                }
                self.max_line_ratio = v;
            }
            _ => anyhow::bail!(
                "--set 認不得的參數 {key:?}，可用的是：{}",
                Self::KEYS.join(" / ")
            ),
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BakeOptions {
    /// 預設 `assets/packs/`（gitignore，走 R2）。
    pub out_dir: PathBuf,
    pub report_json: Option<PathBuf>,
    pub params: Params,
}

impl BakeOptions {
    pub fn to(out_dir: impl Into<PathBuf>) -> Self {
        Self {
            out_dir: out_dir.into(),
            report_json: None,
            params: Params::default(),
        }
    }
}

/// 管線編排，一條直線。
///
/// `Err` 只用在 baker 自身故障（讀不到檔、PNG 壞掉、寫不出去）——素材本身的問題
/// 一律走 `Report` 的 diagnostics（§4.2 的 exit code：0 通過／1 有 Error／2 自身故障）。
pub fn bake(dir: &Path, opts: &BakeOptions) -> Result<Report> {
    let (src, mut diagnostics) = source::Source::load(dir)?;
    let mut report = Report {
        id: src.folder_id.clone(),
        source: dir.display().to_string(),
        diagnostics: Vec::new(),
        seeds: None,
        summary: None,
    };

    if diagnostics
        .iter()
        .any(|d| d.code == code::SOURCE_INCOMPLETE)
    {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let lineart = Image::load(&src.lineart)?;
    let seeds_img = Image::load(&src.seeds)?;
    let shade = src.shade.as_ref().map(|p| Image::load(p)).transpose()?;

    let mut named: Vec<(&str, &Image)> = vec![
        (source::LINEART, &lineart),
        (source::SEEDS, &seeds_img),
    ];
    if let Some(shade) = &shade {
        named.push((source::SHADE, shade));
    }

    let (geometry, aspect) = check::master::geometry(&named);

    let size_mismatch = geometry.iter().any(|d| d.code == code::SIZE_MISMATCH);
    diagnostics.extend(geometry);
    diagnostics.extend(check::master::color_space(&named));

    if size_mismatch {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let (master_w, master_h) = (lineart.width, lineart.height);

    // ── 母帶階段（§3 管線）──────────────────────────────────────────
    let p = &opts.params;
    let line = binarize::line_mask(&lineart.rgba, p.line_threshold);
    let line_ratio = binarize::line_ratio(&line);
    diagnostics.extend(check::master::line_coverage(line_ratio, p.max_line_ratio));

    let seed_list = seeds::read(&seeds_img.rgba, master_w, master_h);
    report.seeds = Some(report::SeedStats {
        seeds: seed_list.len(),
        line_ratio,
    });
    drop(seeds_img);

    let mut grown = segment::grow(&seed_list, &line, master_w, master_h);
    // grow 之後才知道每區多大，merge 要靠它挑「面積最大的相鄰區」。
    let grown_areas = region_areas(&grown.labels, seed_list.len());
    let orphans = segment::merge_small_orphans(
        &mut grown.labels,
        &line,
        master_w,
        master_h,
        &grown_areas,
        p.min_orphan_area,
    );
    diagnostics.extend(check::master::seeds(
        &seed_list,
        &grown,
        &orphans,
        p.min_seed_area,
        p.min_orphan_area,
    ));

    let lineart_white = compose::over_white(&lineart.rgba);
    let shade_white = shade.as_ref().map(|s| compose::over_white(&s.rgba));
    if let Some(shade_white) = &shade_white {
        diagnostics.extend(check::master::shade(shade_white, master_w));
    }
    drop(shade);

    // 階段間 fail-fast：母帶有 Error 就不進降採樣。canvas-size 也在這裡被擋下，
    // 所以底下的 aspect 一定推得出來。
    if diagnostics
        .iter()
        .any(|d| d.severity == report::Severity::Error)
    {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }
    let aspect = aspect.expect("canvas-size 通過就一定推得出 aspect");

    // 測地擴張把 id 填進線像素，直到全覆蓋（§3.1 ②）。母帶通過才做——
    // 有錯的話 labels 還有洞，`from_labels` 會 panic。
    let (_, left) = segment::close(&mut grown.labels, master_w, master_h);
    debug_assert_eq!(left, 0, "母帶無錯就不該有碰不到任何 seed 的孤島");

    let suggested: Vec<[u8; 3]> = seed_list.iter().map(|s| s.color).collect();
    let regions = segment::RegionMap::from_labels(
        std::mem::take(&mut grown.labels),
        master_w,
        master_h,
        seed_list.len() as u32,
    );

    // ── 縮圖（母帶解析度，§3.7）────────────────────────────────────
    let thumb_jpg = thumb::render(
        master_w,
        master_h,
        &regions.labels,
        &suggested,
        &lineart_white,
        shade_white.as_deref(),
    )?;

    drop(line);

    // ── 降採樣 ──────────────────────────────────────────────────────
    let (out_w, out_h) = (master_w / 2, master_h / 2);
    let mut ids = resample::majority_ids(&regions.labels, master_w, master_h, &regions.areas);
    let line_alpha = resample::box_alpha(&lineart.rgba, master_w, master_h);
    let lineart_out = resample::box_rgba(&lineart_white, master_w, master_h);
    let shade_out = shade_white
        .as_ref()
        .map(|s| resample::box_rgba(s, master_w, master_h));
    drop(lineart_white);
    drop(shade_white);
    drop(lineart);

    // ── 膨脹（必須在降採樣之後）────────────────────────────────────
    dilate::dilate_under_lineart(&mut ids, out_w, out_h, &line_alpha, &regions.areas);
    drop(line_alpha);

    // ── 輸出階段 ────────────────────────────────────────────────────
    let stats = check::output::stats(&ids, out_w, out_h, regions.count);
    diagnostics.extend(check::output::check(&regions, &ids, out_w, out_h, &stats));

    report.diagnostics = diagnostics;
    if report.has_error() {
        return finish(report, opts);
    }

    let region_count = stats.area.iter().filter(|&&a| a > 0).count() as u32;
    debug_assert_eq!(region_count, regions.count, "region-count-drift 應已擋下");

    // ── 打包 ────────────────────────────────────────────────────────
    let entries: Vec<RegionEntry> = (0..regions.count)
        .map(|id| RegionEntry {
            id,
            centroid: stats.centroid[id as usize],
            area: stats.area[id as usize],
            bbox: stats.bbox[id as usize],
            suggested_color: hex_color(suggested[id as usize]),
        })
        .collect();

    let mut pack = ColorPack {
        manifest: Manifest {
            schema_version: colorpack::manifest::SCHEMA_VERSION.to_owned(),
            id: src.folder_id.clone(),
            content_hash: String::new(),
            canvas_size: [out_w, out_h],
            aspect,
            region_count,
            difficulty: Difficulty::from_region_count(region_count),
            category: src.category().expect("category 已驗過"),
            has_shade: shade_out.is_some(),
            palette: seeds::palette(&suggested, &stats.area),
        },
        regions: entries,
        region_ids: ids.iter().map(|&id| id as u16).collect(),
        lineart_png: image::encode_rgba(&lineart_out, out_w, out_h, PngOptions::default())?,
        shade_png: shade_out
            .map(|s| image::encode_rgba(&s, out_w, out_h, PngOptions::default()))
            .transpose()?,
        thumb_jpg,
    };

    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("建立 {} 失敗", opts.out_dir.display()))?;
    let out_path = opts.out_dir.join(format!("{}.colorpack", src.folder_id));
    let file = std::fs::File::create(&out_path)
        .with_context(|| format!("建立 {} 失敗", out_path.display()))?;
    pack.write_to(std::io::BufWriter::new(file))
        .with_context(|| format!("寫入 {} 失敗", out_path.display()))?;

    report.summary = Some(Summary {
        id: src.folder_id,
        canvas_size: [out_w, out_h],
        aspect: aspect.as_str(),
        region_count,
        difficulty: pack.manifest.difficulty.as_str(),
        category: pack.manifest.category.as_str(),
        has_shade: pack.manifest.has_shade,
        content_hash: pack.manifest.content_hash.clone(),
        output: out_path.display().to_string(),
    });
    finish(report, opts)
}

/// `grow` 之後的逐 seed 面積。未認領像素（`UNASSIGNED`）不計。
fn region_areas(labels: &[u32], count: usize) -> Vec<u32> {
    let mut areas = vec![0u32; count];
    for &id in labels {
        if id != segment::UNASSIGNED {
            areas[id as usize] += 1;
        }
    }
    areas
}

fn finish(report: Report, opts: &BakeOptions) -> Result<Report> {
    if let Some(path) = &opts.report_json {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("建立 {} 失敗", parent.display()))?;
        }
        std::fs::write(path, report.to_json())
            .with_context(|| format!("寫入 {} 失敗", path.display()))?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::Asset;

    const W: u32 = 64;
    const H: u32 = 64;

    /// 一張同時踩到四條**互相獨立**檢查的素材：
    /// `canvas-size`（64×64 不是 4096 級）、`seed-collision`（左半兩個色標之間沒有線）、
    /// `orphan-area`（右半整塊沒有色標）、`seed-too-small`（其中一個色標只有 4px）。
    fn four_independent_problems() -> Asset {
        let at = |x: u32, y: u32| ((y * W + x) * 4) as usize;

        // 線稿：只有一條垂直線把畫布切成左右兩半。左半內部**故意沒有分界**。
        let mut lineart = vec![0u8; (W * H * 4) as usize];
        for y in 0..H {
            lineart[at(W / 2, y) + 3] = 255;
        }

        // 左半兩個色標 → 同一個封閉區 → collision。其中第二個只有 2×2 → too-small。
        // 右半一個色標都沒有 → orphan-area（面積 32×64 = 2048 ≥ MIN_ORPHAN_AREA）。
        let mut seeds = vec![0u8; (W * H * 4) as usize];
        for y in 8..20u32 {
            for x in 4..16u32 {
                seeds[at(x, y)..at(x, y) + 4].copy_from_slice(&[220, 30, 30, 255]);
            }
        }
        for y in 40..42u32 {
            for x in 4..6u32 {
                seeds[at(x, y)..at(x, y) + 4].copy_from_slice(&[30, 30, 220, 255]);
            }
        }

        Asset {
            id: "fixture-four-problems".to_owned(),
            title: "Four problems".to_owned(),
            category: "mandala".to_owned(),
            notes: "階段內不 fail-fast 的回歸測試素材。".to_owned(),
            width: W,
            height: H,
            lineart,
            seeds,
            shade: None,
            seeds_icc: None,
            compression: png::Compression::Fast,
        }
    }

    /// §4.4「階段內不 fail-fast」：繪師一次拿到所有問題，來回從 N 天變 1 天。
    ///
    /// 迴歸的是提早退出又變寬——`canvas-size` 被綁進 `size-mismatch` 的 structural
    /// failure 會讓整批母帶檢查不跑；四條色標診斷之間任何一條先 return 都會讓
    /// 繪師補一條線交一次、補一個點又交一次。
    #[test]
    fn an_early_failure_does_not_hide_the_independent_checks() {
        let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
        let dir = four_independent_problems()
            .write(tmp.path())
            .expect("寫出素材失敗");
        let report =
            bake(&dir, &BakeOptions::to(tmp.path().join("packs"))).expect("baker 不該自身故障");

        for expected in [
            code::CANVAS_SIZE,
            code::SEED_COLLISION,
            code::ORPHAN_AREA,
            code::SEED_TOO_SMALL,
        ] {
            assert!(
                report.find(expected).is_some(),
                "報告缺 {expected}——階段內 fail-fast 又變寬了：\n{}",
                report.to_text()
            );
        }
    }

    /// 預設值是契約的一部分（§3.3），寫死在測試裡當守門。
    #[test]
    fn the_default_params_are_the_contract() {
        let p = Params::default();
        assert_eq!(p.line_threshold, 128);
        assert_eq!(p.min_seed_area, 64);
        assert_eq!(p.min_orphan_area, 500);
        assert_eq!(p.max_line_ratio, 0.35);
    }

    /// 打錯 key 必須報錯——靜默套用預設會讓「我明明調了參數」變成查不到的 bug。
    #[test]
    fn an_unknown_key_is_an_error_that_lists_the_valid_ones() {
        let mut p = Params::default();
        let e = p.set("min_orphan", "10").unwrap_err().to_string();
        assert!(e.contains("min_orphan_area"), "{e}");
        assert_eq!(p, Params::default(), "報錯就不該留下半套修改");
    }

    #[test]
    fn set_parses_each_key_and_rejects_a_ratio_out_of_range() {
        let mut p = Params::default();
        p.set("line_threshold", "200").unwrap();
        p.set("min_seed_area", "16").unwrap();
        p.set("min_orphan_area", "1200").unwrap();
        p.set("max_line_ratio", "0.5").unwrap();
        assert_eq!((p.line_threshold, p.min_seed_area), (200, 16));
        assert_eq!((p.min_orphan_area, p.max_line_ratio), (1200, 0.5));

        assert!(p.set("max_line_ratio", "1.5").is_err());
        assert!(p.set("line_threshold", "300").is_err(), "u8 溢位要報錯");
    }

    /// 參數真的有接進管線：把 `min_orphan_area` 調到碎片之下，
    /// 原本靜默併入的碎片就會變成 `orphan-area`。
    #[test]
    fn lowering_min_orphan_area_turns_a_merged_fragment_into_an_error() {
        let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
        let dir = four_independent_problems()
            .write(tmp.path())
            .expect("寫出素材失敗");

        let mut opts = BakeOptions::to(tmp.path().join("packs"));
        opts.params.min_orphan_area = 100_000; // 遠大於整張畫布
        let report = bake(&dir, &opts).expect("baker 不該自身故障");
        assert!(
            report.find(code::ORPHAN_AREA).is_none(),
            "門檻拉高之後右半該被當成碎片併掉：\n{}",
            report.to_text()
        );
    }

    /// 色標統計一律列出——被拒收時也要有。
    #[test]
    fn seed_stats_survive_a_rejection() {
        let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
        let dir = four_independent_problems()
            .write(tmp.path())
            .expect("寫出素材失敗");
        let report =
            bake(&dir, &BakeOptions::to(tmp.path().join("packs"))).expect("baker 不該自身故障");

        assert!(report.has_error());
        assert!(report.summary.is_none(), "沒打包出來就不該有 summary");
        let stats = report.seeds.expect("拒收也要列出色標統計");
        assert_eq!(stats.seeds, 2);
        assert!(report.to_text().contains("色標 2 個"), "{}", report.to_text());
    }
}
