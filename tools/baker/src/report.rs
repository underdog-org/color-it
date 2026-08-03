//! 診斷與報告（`specs/baker-core-design.md §4`）。
//!
//! 文字輸出與 `--report x.json` 從同一個 `Vec<Diagnostic>` 渲染，不是兩份真相。

use std::fmt::Write as _;

use serde::Serialize;

/// `code` 是固定字彙表——測試斷言的是特定 `code`，繪師收到的退件也照它分類。
/// 新增檢查必須同時進 `baker-core-design.md §4.1` 的那張表。
pub mod code {
    pub const SOURCE_INCOMPLETE: &str = "source-incomplete";
    pub const SIZE_MISMATCH: &str = "size-mismatch";
    pub const CANVAS_SIZE: &str = "canvas-size";
    pub const COLOR_SPACE: &str = "color-space";
    pub const UNIQUE_COLOR_OVERFLOW: &str = "unique-color-overflow";
    pub const TINY_COLOR_AREA: &str = "tiny-color-area";
    pub const RESERVED_COLOR: &str = "reserved-color";
    pub const UNASSIGNED_PIXEL: &str = "unassigned-pixel";
    pub const REF_MISMATCH: &str = "ref-mismatch";
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub id: String,
    pub canvas_size: [u32; 2],
    pub aspect: &'static str,
    pub region_count: u32,
    pub difficulty: &'static str,
    pub category: &'static str,
    pub has_shade: bool,
    /// `flats` 的實際唯一色數。一律列出，之後要調 `MAX_UNIQUE_COLORS` 才有依據（§2.6）。
    pub unique_colors: usize,
    pub content_hash: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub id: String,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
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

    /// 是否出現過某個 `code`——拒收測試斷言用。
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
        if let Some(s) = &self.summary {
            let _ = writeln!(
                out,
                "  ✓ {}×{} {} ／ {} 區 ／ {} ／ shade {} ／ 唯一色 {}",
                s.canvas_size[0],
                s.canvas_size[1],
                s.aspect,
                s.region_count,
                s.difficulty,
                if s.has_shade { "有" } else { "無" },
                s.unique_colors
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
        let d = Diagnostic::error(code::UNASSIGNED_PIXEL, Stage::Master, "x").with_coords(coords);
        assert_eq!(d.coords.len(), MAX_COORDS);
        assert_eq!(d.coord_total, 100);
        let report = Report {
            id: "x".into(),
            source: "x".into(),
            diagnostics: vec![d],
            summary: None,
        };
        assert!(report.to_text().contains("另有 84 處"));
    }
}
