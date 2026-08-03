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
        for coord in &diagnostic.coords {
            assert!(
                self.planted.contains(coord),
                "{} 回報的座標 {coord:?} 不在植入點裡，失敗原因不是我們想測的那個。\n{}",
                self.expect,
                self.report.to_text()
            );
        }
    }
}

/// 縫隙：`flats` 的 alpha < 255（§2.3 對「未指派像素」的唯一定義）。
#[test]
fn gap_is_rejected_with_the_planted_coordinates() {
    let case = run(Negative::Gap);
    assert_eq!(case.expect, "unassigned-pixel");
    case.assert_rejected_at_planted_coords();
    assert_eq!(case.report.find(case.expect).unwrap().coord_total, 3);
}

/// `reference` 在某一塊裡塗了第二個顏色——不是相鄰區同色，那是合法的。
#[test]
fn reference_with_a_second_color_inside_one_region_is_rejected() {
    let case = run(Negative::RefMismatch);
    assert_eq!(case.expect, "ref-mismatch");
    case.assert_rejected_at_planted_coords();
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
    assert!(d.message.contains("flats.png"), "{}", d.message);
    // 只有 flats 帶 P3，其餘三張不該被連坐
    let hits = case
        .report
        .diagnostics
        .iter()
        .filter(|d| d.code == "color-space")
        .count();
    assert_eq!(hits, 1);
}

/// 開了抗鋸齒的 `flats`。撞到的必須是**實判**（`tiny-color-area`）而不是快篩——
/// 這張圖只有三百多個顏色，遠低於 `MAX_UNIQUE_COLORS`（§2.6）。
#[test]
fn antialiased_flats_is_rejected_by_the_area_rule_not_the_screen() {
    let case = run(Negative::Antialiased);
    assert_eq!(case.expect, "tiny-color-area");
    case.assert_rejected_at_planted_coords();
    assert!(
        case.report.find("unique-color-overflow").is_none(),
        "唯一色數快篩不該命中——它不能取代面積判準"
    );
}

/// 1px 特徵：母帶全數通過，降採樣才整批消失。
#[test]
fn vanishing_1px_features_are_rejected_at_the_output_stage() {
    let case = run(Negative::Vanishing1px);
    assert_eq!(case.expect, "region-count-drift");
    case.assert_rejected_at_planted_coords();

    let d = case.report.find(case.expect).unwrap();
    assert_eq!(d.stage, baker::report::Stage::Output);
    assert_eq!(d.coord_total, 300, "300 個植入點應該全數消失");
    assert!(
        !case
            .report
            .diagnostics
            .iter()
            .any(|d| d.stage == baker::report::Stage::Master && d.severity == Severity::Error),
        "母帶階段應該全數通過：\n{}",
        case.report.to_text()
    );
}
