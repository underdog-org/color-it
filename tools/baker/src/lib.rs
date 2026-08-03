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
pub mod reference;
pub mod report;
pub mod resample;
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

#[derive(Debug, Clone)]
pub struct BakeOptions {
    /// 預設 `assets/packs/`（gitignore，走 R2）。
    pub out_dir: PathBuf,
    pub report_json: Option<PathBuf>,
}

impl BakeOptions {
    pub fn to(out_dir: impl Into<PathBuf>) -> Self {
        Self {
            out_dir: out_dir.into(),
            report_json: None,
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
        unique_colors: None,
        summary: None,
    };

    if diagnostics
        .iter()
        .any(|d| d.code == code::SOURCE_INCOMPLETE)
    {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let flats = Image::load(&src.flats)?;
    let lineart = Image::load(&src.lineart)?;
    let reference_img = Image::load(&src.reference)?;
    let shade = src.shade.as_ref().map(|p| Image::load(p)).transpose()?;

    let mut named: Vec<(&str, &Image)> = vec![
        (source::FLATS, &flats),
        (source::LINEART, &lineart),
        (source::REFERENCE, &reference_img),
    ];
    if let Some(shade) = &shade {
        named.push((source::SHADE, shade));
    }

    let (geometry, aspect) = check::master::geometry(&named);

    // 只有 size-mismatch 必須提早停——四張圖尺寸不一致時逐像素的檢查會越界。
    // canvas-size 單獨命中時像素檢查完全跑得動，照 §4.2「階段內不 fail-fast」跑完，
    // 否則繪師改完尺寸重交，才會第一次看到縫隙問題。
    let size_mismatch = geometry.iter().any(|d| d.code == code::SIZE_MISMATCH);
    diagnostics.extend(geometry);
    diagnostics.extend(check::master::color_space(&named));

    if size_mismatch {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let (master_w, master_h) = (flats.width, flats.height);

    // ── 母帶階段 ────────────────────────────────────────────────────
    let (flats_diags, unique_colors) = check::master::flats(&flats);
    // 唯一色數爆掉代表圖徹底壞掉，connected components 會切出幾百萬區——只有這一條
    // 必須提早停。tiny-color-area / reserved-color / unassigned-pixel 都不影響
    // label_regions，讓 ref-mismatch 也一次交出去。
    let unique_color_overflow = flats_diags
        .iter()
        .any(|d| d.code == code::UNIQUE_COLOR_OVERFLOW);
    report.unique_colors = Some(unique_colors);
    diagnostics.extend(flats_diags);

    let lineart_white = compose::over_white(&lineart.rgba);
    let shade_white = shade.as_ref().map(|s| compose::over_white(&s.rgba));
    if let Some(shade_white) = &shade_white {
        diagnostics.extend(check::master::shade(shade_white, master_w));
    }
    drop(shade);

    if unique_color_overflow {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let regions = segment::label_regions(&flats.rgba, master_w, master_h);
    drop(flats);

    let (suggested, ref_diag) = reference::read(&reference_img.rgba, &regions);
    diagnostics.extend(ref_diag);
    drop(reference_img);

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

    // ── 縮圖（母帶解析度，§3.7）────────────────────────────────────
    let thumb_jpg = thumb::render(
        master_w,
        master_h,
        &regions.labels,
        &suggested.colors,
        &lineart_white,
        shade_white.as_deref(),
    )?;

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

    // §2：`region_count` 與 `difficulty` 一律依**輸出解析度**判定。走到這裡
    // region-count-drift（Error）已擋掉「有區域在輸出消失」，兩數必然相等——
    // 但取值直接照規格，不靠另一條檢查維持的不變式。
    let region_count = stats.area.iter().filter(|&&a| a > 0).count() as u32;
    debug_assert_eq!(region_count, regions.count, "region-count-drift 應已擋下");

    // ── 打包 ────────────────────────────────────────────────────────
    let entries: Vec<RegionEntry> = (0..regions.count)
        .map(|id| RegionEntry {
            id,
            centroid: stats.centroid[id as usize],
            area: stats.area[id as usize],
            bbox: stats.bbox[id as usize],
            suggested_color: hex_color(suggested.colors[id as usize]),
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
            palette: reference::palette(&suggested.colors, &stats.area),
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
    use crate::segment::RESERVED_COLOR;
    use crate::synth::{Asset, PALETTE, PERM};

    const W: u32 = 64;
    const H: u32 = 64;

    /// 一張同時踩到四條**互相獨立**檢查的素材
    /// 同時踩到四條**互相獨立**檢查的素材：`canvas-size`（64×64）、
    /// `unassigned-pixel`（一個 alpha 0 的洞）、`reserved-color`（右半是 #FF00FF）、
    /// `ref-mismatch`（左區裡塗第二個顏色）。
    ///
    /// 沒有一條會讓別條算不出來，所以報告必須四條全含。
    fn four_independent_problems() -> Asset {
        let mut flats = Vec::with_capacity((W * H * 4) as usize);
        let mut reference = Vec::with_capacity((W * H * 4) as usize);
        for _y in 0..H {
            for x in 0..W {
                let region = if x < W / 2 { 1usize } else { 2usize };
                let c = if region == 2 {
                    RESERVED_COLOR
                } else {
                    PALETTE[1]
                };
                flats.extend_from_slice(&[c[0], c[1], c[2], 255]);
                let r = PALETTE[PERM[region] as usize];
                reference.extend_from_slice(&[r[0], r[1], r[2], 255]);
            }
        }
        let at = |x: u32, y: u32| ((y * W + x) * 4) as usize;
        flats[at(3, 3) + 3] = 0;
        for y in 8..12 {
            for x in 8..12 {
                reference[at(x, y)..at(x, y) + 3].copy_from_slice(&[1, 2, 3]);
            }
        }
        Asset {
            id: "fixture-four-problems".to_owned(),
            title: "Four problems".to_owned(),
            category: "mandala".to_owned(),
            notes: "階段內不 fail-fast 的回歸測試素材。".to_owned(),
            width: W,
            height: H,
            flats,
            lineart: vec![0u8; (W * H * 4) as usize],
            reference,
            shade: None,
            flats_icc: None,
            compression: png::Compression::Fast,
        }
    }

    /// §4.2「階段內不 fail-fast」：繪師一次拿到所有問題，來回從 N 天變 1 天。
    ///
    /// 迴歸的是兩個曾經過寬的提早退出——`canvas-size` 被綁進 `size-mismatch` 的
    /// structural failure、`reserved-color` 被綁進 flats 的 broken 判定。前者會讓
    /// 逐像素檢查整批不跑，後者會讓 `ref-mismatch` 永遠看不到。
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
            code::UNASSIGNED_PIXEL,
            code::RESERVED_COLOR,
            code::REF_MISMATCH,
        ] {
            assert!(
                report.find(expected).is_some(),
                "報告缺 {expected}——階段內 fail-fast 又變寬了：\n{}",
                report.to_text()
            );
        }
    }

    /// §2.6「報告中一律列出實際唯一色數」——被拒收時也要有。
    #[test]
    fn unique_colors_survive_a_rejection() {
        let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
        let dir = four_independent_problems()
            .write(tmp.path())
            .expect("寫出素材失敗");
        let report =
            bake(&dir, &BakeOptions::to(tmp.path().join("packs"))).expect("baker 不該自身故障");

        assert!(report.has_error());
        assert!(report.summary.is_none(), "沒打包出來就不該有 summary");
        let unique = report.unique_colors.expect("拒收也要列出唯一色數");
        assert_eq!((unique.count, unique.exact), (2, true));
        assert!(
            report.to_text().contains("唯一色 2"),
            "{}",
            report.to_text()
        );
    }
}
