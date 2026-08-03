//! 拒收測試（`specs/baker-core-design.md §6`）。
//!
//! 每條都斷言 **特定 `code` 出現 ＋ 座標落在生成器刻意植入的位置**——只斷言「失敗」
//! 會讓任何理由的失敗都變綠燈。
//!
//! fixture 不進 git（§5.2）：每條測試自己生一組完整 4096 級尺寸的素材到 tempdir，
//! 跑完即丟。repo 零增重，fixture 隨規格演進不會膠死。

use baker::report::{Report, Severity};
use baker::synth::{self, Negative};
use baker::{BakeOptions, bake};

struct Case {
    report: Report,
    expect: &'static str,
    planted: Vec<(u32, u32)>,
    _tmp: tempfile::TempDir,
}

fn run(kind: Negative) -> Case {
    let tmp = tempfile::tempdir().expect("建立 tempdir 失敗");
    let fixture = synth::negative(kind);
    let dir = fixture.asset.write(tmp.path()).expect("寫出 fixture 失敗");
    let opts = BakeOptions::to(tmp.path().join("packs"));
    let report = bake(&dir, &opts).expect("baker 不該自身故障");
    Case {
        report,
        expect: fixture.expect,
        planted: fixture.planted,
        _tmp: tmp,
    }
}

impl Case {
    /// 拒收 ＋ 命中預期的 `code` ＋ 每個回報座標都落在植入點上。
    fn assert_rejected_at_planted_coords(&self) {
        assert!(
            self.report.has_error(),
            "應該拒收，實際報告：\n{}",
            self.report.to_text()
        );
        let diagnostic = self.report.find(self.expect).unwrap_or_else(|| {
            panic!(
                "沒有出現 {} ，實際報告：\n{}",
                self.expect,
                self.report.to_text()
            )
        });
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(
            !diagnostic.coords.is_empty(),
            "{} 必須附座標——`assets-spec §7` 承諾退件附失敗座標",
            self.expect
        );
        for coord in &diagnostic.points() {
            assert!(
                self.planted.contains(coord),
                "{} 回報的座標 {coord:?} 不在植入點裡，失敗原因不是我們想測的那個。\n{}",
                self.expect,
                self.report.to_text()
            );
        }
    }
}

/// **線稿缺口**：兩個色標落進同一封閉區。座標成對列出，繪師才知道在哪兩點之間補線。
#[test]
fn a_gap_in_the_lineart_is_rejected_as_a_seed_collision() {
    let case = run(Negative::SeedCollision);
    assert_eq!(case.expect, "seed-collision");
    case.assert_rejected_at_planted_coords();
    let d = case.report.find(case.expect).unwrap();
    assert_eq!(d.coords.len(), 2, "一組缺口 = 兩個座標：{:?}", d.coords);
}

/// **漏點**：整個封閉區沒有色標。附面積，且面積要落在該 cell 的量級。
#[test]
fn an_unseeded_closed_area_is_rejected_as_an_orphan() {
    let case = run(Negative::OrphanArea);
    assert_eq!(case.expect, "orphan-area");
    case.assert_rejected_at_planted_coords();
    let d = case.report.find(case.expect).unwrap();
    assert_eq!(d.coord_total, 1, "只漏點一個 cell：{}", d.message);
}

/// 色標太小 → 取不出可靠的眾數色。
#[test]
fn a_tiny_seed_is_rejected() {
    let case = run(Negative::SeedTooSmall);
    assert_eq!(case.expect, "seed-too-small");
    case.assert_rejected_at_planted_coords();
}

/// 色標壓在線上 → flood fill 起不來。
#[test]
fn a_seed_on_the_line_is_rejected() {
    let case = run(Negative::SeedOnLine);
    assert_eq!(case.expect, "seed-on-line");
    case.assert_rejected_at_planted_coords();
}

/// 白底交付的線稿：alpha 全滿 → 整張都判成線。這條是**警告**不是錯誤，
/// 它的用途是解釋「為什麼一個區域都認不出來」，不是自己去擋。
#[test]
fn an_opaque_lineart_raises_the_coverage_warning() {
    let case = run(Negative::LineCoverage);
    let d = case
        .report
        .find("line-coverage")
        .unwrap_or_else(|| panic!("沒有出現 line-coverage：\n{}", case.report.to_text()));
    assert_eq!(d.severity, Severity::Warning);
    assert!(
        case.report.has_error(),
        "整張都是線 → 色標全部壓線，仍然要拒收：\n{}",
        case.report.to_text()
    );
}

/// Display P3 描述檔。這條沒有座標可報（問題在 chunk 不在像素），改斷言訊息指名是哪張圖。
#[test]
fn display_p3_profile_is_rejected() {
    let case = run(Negative::DisplayP3);
    assert!(case.report.has_error());
    let d = case
        .report
        .find("color-space")
        .unwrap_or_else(|| panic!("沒有出現 color-space：\n{}", case.report.to_text()));
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("seeds.png"), "{}", d.message);
    // 只有 seeds 帶 P3，lineart 不該被連坐
    let hits = case
        .report
        .diagnostics
        .iter()
        .filter(|d| d.code == "color-space")
        .count();
    assert_eq!(hits, 1);
}
