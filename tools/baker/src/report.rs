//! 診斷與報告（`specs/baker-core-design.md §4`）。
//!
//! 文字輸出與 `--report x.json` 從同一個 `Vec<Diagnostic>` 渲染，不是兩份真相。

use std::cmp::Reverse;
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

/// 一張全錯的圖不該吐四百萬行。聚類之後這是**叢集**數上限，不是座標數上限。
pub const MAX_COORDS: usize = 16;

/// 座標聚類的格寬（母帶像素，§5）。同一格內的座標視為「同一處」。
///
/// 128px 是繪師在 CSP 裡縮到全圖時大約一個筆刷點的尺度：報 16 個散落座標他要
/// 逐一跳過去看，報「(1204,880) 附近 500 處」他一眼知道那是同一個地方的同一件事。
///
/// 取整分格不是真的連通分量：橫跨格線的一片問題會報成相鄰的兩處。這是刻意的
/// ——真連通分量要再掃一次整張圖，而「兩處在隔壁」對繪師的動作沒有差別。
pub const CLUSTER_GRID: u32 = 128;

/// 一處問題。`count` 是聚進這一叢的原始座標數。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Coord {
    pub x: u32,
    pub y: u32,
    pub count: usize,
}

/// 診斷之間的閱讀順序（§5「可疑度排序」）。**不是嚴重度順序**：
/// `line-coverage` 是 Warning 卻排在所有色標錯誤前面——門檻判錯時，後面每一條
/// 診斷都是它的衍生雜訊，先看它才不會白補 50 個點。
const SUSPICION: [&str; 18] = [
    code::SOURCE_INCOMPLETE,
    code::SIZE_MISMATCH,
    code::CANVAS_SIZE,
    code::COLOR_SPACE,
    code::META_ID_MISMATCH,
    code::META_BAD_CATEGORY,
    code::LINE_COVERAGE,
    code::SEED_ON_LINE,
    code::SEED_TOO_SMALL,
    code::SEED_COLLISION,
    code::ORPHAN_AREA,
    code::SHADE_TOO_DARK,
    code::REGION_COUNT_OVERFLOW,
    code::REGION_COUNT_DRIFT,
    code::REGION_SPLIT,
    code::TINY_REGION,
    code::REGION_COUNT_RANGE,
    // 認不得的 code 排在最後。
    "",
];

fn suspicion_rank(code: &str) -> usize {
    SUSPICION
        .iter()
        .position(|&c| c == code)
        .unwrap_or(SUSPICION.len())
}

/// 依「該先看哪個」重排。同一 code 之內的順序由各檢查自己決定（`orphan-area`
/// 是面積遞減），這裡只排 code 與 code 之間。
pub fn sort_by_suspicion(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by_key(|d| suspicion_rank(d.code));
}

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
    /// 聚類後的「處」，依叢集大小遞減。**一律是母帶座標系**（繪師在 CSP 裡看到的
    /// 那個）。輸出階段發現的問題已經 ×2 換算。只留前 `MAX_COORDS` 叢。
    pub coords: Vec<Coord>,
    /// 聚類**前**的原始座標總數。
    pub coord_total: usize,
    /// 聚類後的總叢集數。`coords.len()` 只是它的前 `MAX_COORDS` 個。
    pub cluster_total: usize,
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
            cluster_total: 0,
            region: None,
        }
    }

    pub fn with_coords(mut self, coords: Coords) -> Self {
        let mut clusters = coords.clusters;
        // 大的一叢先看（§5）。stable sort：同大小時保留各檢查自己的排序，
        // `orphan-area` 的面積遞減因此不會被打亂。
        clusters.sort_by_key(|c| Reverse(c.count));
        self.coord_total = coords.total;
        self.cluster_total = clusters.len();
        clusters.truncate(MAX_COORDS);
        self.coords = clusters;
        self
    }

    /// 叢集代表座標。測試與 `--debug-out` 用。
    pub fn points(&self) -> Vec<(u32, u32)> {
        self.coords.iter().map(|c| (c.x, c.y)).collect()
    }

    pub fn with_region(mut self, region: u32) -> Self {
        self.region = Some(region);
        self
    }
}

/// 收集座標，**邊收邊聚類**並自動換算回母帶座標系。
///
/// 聚類必須在 push 當下做，不能先收完再聚：`shade-too-dark` 最壞情況是整張 12.6M
/// 個像素，全部留下來再聚類要 100MB。以 `CLUSTER_GRID` 格取整當 key 之後，叢集數
/// 的上限是「畫布格數」（3072×4096 → 768），與問題像素數無關。
#[derive(Debug, Default, Clone)]
pub struct Coords {
    clusters: Vec<Coord>,
    /// 格座標 → `clusters` 的索引。
    index: std::collections::HashMap<(u32, u32), usize>,
    total: usize,
    /// 輸出階段收集時設 2：座標一律換算回母帶座標系（§4.2）。
    scale: u32,
    /// 聚類格寬。**1 = 不聚類**，只去重完全相同的座標。
    grid: u32,
}

impl Coords {
    /// **不聚類**。座標本身就是可執行動作的那種診斷用它：色標的四條裡，
    /// 每個 anchor 都是繪師要動手的一個點，兩個相距 20px 的色標是兩件事不是一件事。
    /// `seed-collision` 更是絕對不能聚——聚掉就毀了「在這兩點之間補線」的語意。
    pub fn master() -> Self {
        Self {
            scale: 1,
            grid: 1,
            ..Default::default()
        }
    }

    /// 聚類。座標是「症狀出現的地方」而非「要動手的地方」的診斷用它：
    /// `shade-too-dark` 動輒上萬個像素，逐一列出對誰都沒用。
    pub fn clustered() -> Self {
        Self {
            scale: 1,
            grid: CLUSTER_GRID,
            ..Default::default()
        }
    }

    /// 輸出解析度收集的座標，push 時自動 ×2 換算回母帶。一律聚類——
    /// 輸出階段的診斷（`tiny-region` 等）動輒上千個區域。
    pub fn output() -> Self {
        Self {
            scale: 2,
            grid: CLUSTER_GRID,
            ..Default::default()
        }
    }

    /// 叢集的代表座標是**第一個**落進該格的座標，不是格心——回報的座標必須是
    /// 真的有問題的那個像素，繪師跳過去才看得到東西。
    pub fn push(&mut self, x: u32, y: u32) {
        self.total += 1;
        let (x, y) = (x * self.scale, y * self.scale);
        let cell = (x / self.grid, y / self.grid);
        match self.index.get(&cell) {
            Some(&i) => self.clusters[i].count += 1,
            None => {
                self.index.insert(cell, self.clusters.len());
                self.clusters.push(Coord { x, y, count: 1 });
            }
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
                    .map(|c| {
                        if c.count > 1 {
                            format!("({}, {})×{}", c.x, c.y, c.count)
                        } else {
                            format!("({}, {})", c.x, c.y)
                        }
                    })
                    .collect();
                let suffix = if d.cluster_total > d.coords.len() {
                    format!("，另有 {} 處", d.cluster_total - d.coords.len())
                } else {
                    String::new()
                };
                let note = if d.stage == Stage::Output {
                    "（母帶座標，於輸出解析度發現）"
                } else {
                    "（母帶座標）"
                };
                // 聚類過的才報「共 N px」——沒聚到東西時那個數字只是雜訊。
                let scope = if d.coord_total > d.cluster_total {
                    format!("{} 處／共 {} px", d.cluster_total, d.coord_total)
                } else {
                    format!("{} 處", d.cluster_total)
                };
                let _ = writeln!(out, "      {note} {scope}：{}{suffix}", list.join(" "));
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

    fn report_of(diagnostics: Vec<Diagnostic>) -> Report {
        Report {
            id: "x".into(),
            source: "x".into(),
            diagnostics,
            seeds: None,
            summary: None,
        }
    }

    #[test]
    fn output_coords_are_converted_back_to_master_space() {
        let mut coords = Coords::output();
        coords.push(10, 20);
        let d = Diagnostic::warning(code::TINY_REGION, Stage::Output, "x").with_coords(coords);
        assert_eq!(d.points(), vec![(20, 40)]);
    }

    /// 上限套在**叢集**上：100 個彼此隔開的問題點 → 100 叢，只留 16。
    #[test]
    fn clusters_are_capped_but_totals_are_kept() {
        let mut coords = Coords::master();
        for i in 0..100 {
            coords.push(i * CLUSTER_GRID, 0);
        }
        let d = Diagnostic::error(code::ORPHAN_AREA, Stage::Master, "x").with_coords(coords);
        assert_eq!(d.coords.len(), MAX_COORDS);
        assert_eq!((d.coord_total, d.cluster_total), (100, 100));
        assert!(report_of(vec![d]).to_text().contains("另有 84 處"));
    }

    /// §5 座標聚類：擠在一起的座標算**一處**，不是 16 個散落座標。
    /// 這是 `shade-too-dark` 動輒上萬個像素時，報告還讀得懂的唯一理由。
    #[test]
    fn neighbouring_coords_collapse_into_one_place() {
        let mut coords = Coords::clustered();
        // 全部壓在同一格內（1200/128 = 9、880/128 = 6）——跨格線的情形見
        // `CLUSTER_GRID` 的說明，那會報成相鄰的兩處。
        for i in 0..500 {
            coords.push(1200 + i % 16, 880 + i % 16);
        }
        coords.push(3000, 100); // 遠處另一叢
        let d = Diagnostic::error(code::SHADE_TOO_DARK, Stage::Master, "x").with_coords(coords);

        assert_eq!(d.cluster_total, 2, "同一格內的 500 個座標是一處");
        assert_eq!(d.coord_total, 501);
        assert_eq!(d.coords[0].count, 500, "大的一叢排前面");
        assert_eq!(
            (d.coords[0].x, d.coords[0].y),
            (1200, 880),
            "代表座標取第一個落進該格的，不是格心"
        );
        let text = report_of(vec![d]).to_text();
        assert!(text.contains("2 處／共 501 px"), "{text}");
        assert!(text.contains("(1200, 880)×500"), "{text}");
    }

    /// §5 可疑度排序：`line-coverage` 是 Warning，卻要排在色標錯誤前面——
    /// 二值化門檻不對時，後面每一條都是它的衍生雜訊。
    #[test]
    fn line_coverage_is_read_before_the_seed_errors_it_would_explain() {
        let mut list = vec![
            Diagnostic::error(code::ORPHAN_AREA, Stage::Master, "x"),
            Diagnostic::error(code::SEED_ON_LINE, Stage::Master, "x"),
            Diagnostic::warning(code::LINE_COVERAGE, Stage::Master, "x"),
            Diagnostic::error(code::CANVAS_SIZE, Stage::Master, "x"),
        ];
        sort_by_suspicion(&mut list);
        let codes: Vec<&str> = list.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![
                code::CANVAS_SIZE,
                code::LINE_COVERAGE,
                code::SEED_ON_LINE,
                code::ORPHAN_AREA
            ]
        );
    }

    /// 排序是 stable 的：同一 code 之內的順序由各檢查自己決定
    /// （`orphan-area` 是面積遞減），重排不得打亂它。
    #[test]
    fn sorting_keeps_the_order_within_a_code() {
        let mut list = vec![
            Diagnostic::error(code::ORPHAN_AREA, Stage::Master, "大"),
            Diagnostic::error(code::ORPHAN_AREA, Stage::Master, "小"),
        ];
        sort_by_suspicion(&mut list);
        assert_eq!(list[0].message, "大");
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
