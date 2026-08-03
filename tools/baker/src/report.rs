//! 診斷與報告（`specs/baker-core-design.md §4`）。
//!
//! 文字輸出與 `--report x.json` 從同一個 `Vec<Diagnostic>` 渲染，不是兩份真相。

use std::fmt::Write as _;

use serde::Serialize;

/// `code` 是固定字彙表——測試斷言的是特定 `code`，繪師收到的退件也照它分類。
/// 新增檢查必須同時進 `baker-seeds.md §4.1` 的那張表。
pub mod code {
    pub const SOURCE_INCOMPLETE: &str = "source-incomplete";
    pub const SIZE_MISMATCH: &str = "size-mismatch";
    pub const CANVAS_SIZE: &str = "canvas-size";
    pub const COLOR_SPACE: &str = "color-space";
    /// 兩個以上色標落進同一封閉區 → 線稿有缺口。
    pub const SEED_COLLISION: &str = "seed-collision";
    /// ≥`MIN_ORPHAN_AREA` 的自由區沒有色標 → 漏點了。
    pub const ORPHAN_AREA: &str = "orphan-area";
    /// 色標的 `alpha == 255` 面積不足，取不出可靠眾數色。
    pub const SEED_TOO_SMALL: &str = "seed-too-small";
    /// 色標重心落在線像素上，flood fill 起不來。
    pub const SEED_ON_LINE: &str = "seed-on-line";
    /// 線像素佔比過高——二值化門檻不對，或線稿是白底交付的。
    pub const LINE_COVERAGE: &str = "line-coverage";
    pub const SHADE_TOO_DARK: &str = "shade-too-dark";
    pub const META_ID_MISMATCH: &str = "meta-id-mismatch";
    pub const META_BAD_CATEGORY: &str = "meta-bad-category";
    pub const REGION_COUNT_DRIFT: &str = "region-count-drift";
    pub const REGION_COUNT_OVERFLOW: &str = "region-count-overflow";
    pub const TINY_REGION: &str = "tiny-region";
    pub const REGION_COUNT_RANGE: &str = "region-count-range";
    pub const REGION_SPLIT: &str = "region-split";
}

/// 一張全錯的圖不該吐四百萬行。
pub const MAX_COORDS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Master,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub stage: Stage,
    pub message: String,
    /// **一律是母帶座標系**（繪師在 CSP 裡看到的那個）。輸出階段發現的問題已經 ×2 換算。
    pub coords: Vec<(u32, u32)>,
    /// 座標總數。`coords` 只留前 `MAX_COORDS` 個。
    pub coord_total: usize,
    pub region: Option<u32>,
}

impl Diagnostic {
    pub fn error(code: &'static str, stage: Stage, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, stage, message)
    }

    pub fn warning(code: &'static str, stage: Stage, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, stage, message)
    }

    fn new(
        severity: Severity,
        code: &'static str,
        stage: Stage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code,
            stage,
            message: message.into(),
            coords: Vec::new(),
            coord_total: 0,
            region: None,
        }
    }

    pub fn with_coords(mut self, coords: Coords) -> Self {
        self.coord_total = coords.total;
        self.coords = coords.kept;
        self
    }

    pub fn with_region(mut self, region: u32) -> Self {
        self.region = Some(region);
        self
    }
}

/// 收集座標並自動套用 `MAX_COORDS` 上限與換算。
#[derive(Debug, Default, Clone)]
pub struct Coords {
    kept: Vec<(u32, u32)>,
    total: usize,
    /// 輸出階段收集時設 2：座標一律換算回母帶座標系（§4.2）。
    scale: u32,
}

impl Coords {
    pub fn master() -> Self {
        Self {
            scale: 1,
            ..Default::default()
        }
    }

    /// 輸出解析度收集的座標，push 時自動 ×2 換算回母帶。
    pub fn output() -> Self {
        Self {
            scale: 2,
            ..Default::default()
        }
    }

    pub fn push(&mut self, x: u32, y: u32) {
        self.total += 1;
        if self.kept.len() < MAX_COORDS {
            self.kept.push((x * self.scale, y * self.scale));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    pub fn total(&self) -> usize {
        self.total
    }
}

/// 色標統計。取代 `flats` 時代的唯一色數（那條隨 `flats.png` 一起消失）。
///
/// 掛在 `Report` 而不是 `Summary`：拒收時最需要它——「繪師到底點了幾個點」
/// 是判斷 `orphan-area` 是漏點還是整層交錯的第一個依據。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SeedStats {
    /// `seeds.png` 裡讀到的色標數。
    pub seeds: usize,
    /// 線像素佔全畫布的比例。
    pub line_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub id: String,
    pub canvas_size: [u32; 2],
    pub aspect: &'static str,
    pub region_count: u32,
    pub difficulty: &'static str,
    pub category: &'static str,
    pub has_shade: bool,
    pub content_hash: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub id: String,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
    /// 一讀到 `seeds` 就有，與是否通過無關。
    pub seeds: Option<SeedStats>,
    /// 只有真的打包出來才有。
    pub summary: Option<Summary>,
}

impl Report {
    pub fn has_error(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    }

    pub fn find(&self, code: &str) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| d.code == code)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Report 序列化不會失敗")
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "baker：{} （{}）", self.id, self.source);
        for d in &self.diagnostics {
            let mark = match d.severity {
                Severity::Error => "✗",
                Severity::Warning => "⚠",
            };
            let stage = match d.stage {
                Stage::Master => "母帶",
                Stage::Output => "輸出",
            };
            let _ = writeln!(out, "  {mark} [{}] {stage}：{}", d.code, d.message);
            if let Some(region) = d.region {
                let _ = writeln!(out, "      區域 #{region}");
            }
            if !d.coords.is_empty() {
                let list: Vec<String> = d
                    .coords
                    .iter()
                    .map(|(x, y)| format!("({x}, {y})"))
                    .collect();
                let suffix = if d.coord_total > d.coords.len() {
                    format!("，另有 {} 處", d.coord_total - d.coords.len())
                } else {
                    String::new()
                };
                let note = if d.stage == Stage::Output {
                    "（母帶座標，於輸出解析度發現）"
                } else {
                    "（母帶座標）"
                };
                let _ = writeln!(out, "      {note} {}{suffix}", list.join(" "));
            }
        }
        if let Some(s) = &self.seeds {
            let _ = writeln!(
                out,
                "  色標 {} 個／線像素 {:.2}%",
                s.seeds,
                s.line_ratio * 100.0
            );
        }
        if let Some(s) = &self.summary {
            let _ = writeln!(
                out,
                "  ✓ {}×{} {} ／ {} 區 ／ {} ／ shade {}",
                s.canvas_size[0],
                s.canvas_size[1],
                s.aspect,
                s.region_count,
                s.difficulty,
                if s.has_shade { "有" } else { "無" },
            );
            let _ = writeln!(out, "  → {}", s.output);
            let _ = writeln!(out, "  {}", s.content_hash);
        }
        let (errors, warnings) = (self.count(Severity::Error), self.count(Severity::Warning));
        let _ = writeln!(out, "  {errors} 個錯誤、{warnings} 個警告");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_coords_are_converted_back_to_master_space() {
        let mut coords = Coords::output();
        coords.push(10, 20);
        let d = Diagnostic::warning(code::TINY_REGION, Stage::Output, "x").with_coords(coords);
        assert_eq!(d.coords, vec![(20, 40)]);
    }

    #[test]
    fn coords_are_capped_but_total_is_kept() {
        let mut coords = Coords::master();
        for i in 0..100 {
            coords.push(i, i);
        }
        let d = Diagnostic::error(code::ORPHAN_AREA, Stage::Master, "x").with_coords(coords);
        assert_eq!(d.coords.len(), MAX_COORDS);
        assert_eq!(d.coord_total, 100);
        let report = Report {
            id: "x".into(),
            source: "x".into(),
            diagnostics: vec![d],
            seeds: None,
            summary: None,
        };
        assert!(report.to_text().contains("另有 84 處"));
    }

    /// 色標統計一律列出——含被拒收的素材。「點了幾個點」是判讀 `orphan-area`
    /// 的第一個依據，正好是被退件時最需要的資訊。
    #[test]
    fn seed_stats_are_printed_even_when_the_asset_is_rejected() {
        let report = Report {
            id: "x".into(),
            source: "x".into(),
            diagnostics: vec![Diagnostic::error(code::ORPHAN_AREA, Stage::Master, "x")],
            seeds: Some(SeedStats {
                seeds: 7,
                line_ratio: 0.0498,
            }),
            summary: None,
        };
        let text = report.to_text();
        assert!(text.contains("色標 7 個"), "{text}");
        assert!(text.contains("4.98%"), "{text}");
    }
}
