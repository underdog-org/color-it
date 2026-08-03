//! 資產烘焙管線。
//!
//! ```text
//! Source ──▶ Master ──▶ RegionMap(母帶) ──▶ Output ──▶ ColorPack
//! ```
//!
//! 規格見 `docs/specs/baker-core-design.md`；上位描述見 `docs/architecture.md §9`。

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
    let structural_failure = !geometry.is_empty();
    diagnostics.extend(geometry);
    diagnostics.extend(check::master::color_space(&named));

    // 尺寸不一致時逐像素的檢查會失去意義（也會越界），只能先把已知的問題交出去。
    if structural_failure {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let (master_w, master_h) = (flats.width, flats.height);
    let aspect = aspect.expect("geometry 通過就一定推得出 aspect");

    // ── 母帶階段 ────────────────────────────────────────────────────
    let (flats_diags, unique_colors) = check::master::flats(&flats);
    let flats_broken = !flats_diags.is_empty();
    diagnostics.extend(flats_diags);

    let lineart_white = compose::over_white(&lineart.rgba);
    let shade_white = shade.as_ref().map(|s| compose::over_white(&s.rgba));
    if let Some(shade_white) = &shade_white {
        diagnostics.extend(check::master::shade(shade_white, master_w));
    }
    drop(shade);

    // flats 壞掉時 connected components 的結果沒有意義（例如唯一色數爆掉會切出幾百萬區），
    // 但 lineart / shade / meta 的問題仍要一次交出去。
    if flats_broken {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

    let regions = segment::label_regions(&flats.rgba, master_w, master_h);
    drop(flats);

    let (suggested, ref_diag) = reference::read(&reference_img.rgba, &regions);
    diagnostics.extend(ref_diag);
    drop(reference_img);

    // 階段間 fail-fast：母帶有 Error 就不進降採樣。
    if diagnostics
        .iter()
        .any(|d| d.severity == report::Severity::Error)
    {
        report.diagnostics = diagnostics;
        return finish(report, opts);
    }

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
    // line_mask 要的是降採樣後的線稿覆蓋度，必須從**原始 straight-alpha** 取——
    // 合成到白底之後 alpha 全是 255（§2.5）。
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
            region_count: regions.count,
            difficulty: Difficulty::from_region_count(regions.count),
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
        region_count: regions.count,
        difficulty: difficulty_str(pack.manifest.difficulty),
        category: pack.manifest.category.as_str(),
        has_shade: pack.manifest.has_shade,
        unique_colors,
        content_hash: pack.manifest.content_hash.clone(),
        output: out_path.display().to_string(),
    });
    finish(report, opts)
}

fn difficulty_str(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Easy => "easy",
        Difficulty::Medium => "medium",
        Difficulty::Focused => "focused",
    }
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
