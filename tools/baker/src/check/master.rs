//! 母帶解析度的檢查。

use colorpack::Aspect;

use crate::compose::luma;
use crate::image::Image;
use crate::report::{Coords, Diagnostic, Stage, code};
use crate::seeds::Seed;
use crate::segment::{Grown, Orphan};

/// 母帶長邊。
pub const MASTER_LONG_EDGE: u32 = 4096;

/// 三張圖尺寸與對齊一致、長邊 4096、比例 1:1 或 3:4。
pub fn geometry(images: &[(&str, &Image)]) -> (Vec<Diagnostic>, Option<Aspect>) {
    let mut out = Vec::new();
    let (_, first) = images[0];
    let (w, h) = (first.width, first.height);

    let odd: Vec<String> = images
        .iter()
        .filter(|(_, img)| (img.width, img.height) != (w, h))
        .map(|(name, img)| format!("{name} {}×{}", img.width, img.height))
        .collect();
    if !odd.is_empty() {
        out.push(Diagnostic::error(
            code::SIZE_MISMATCH,
            Stage::Master,
            format!(
                "圖層尺寸不一致：{} {w}×{h}，但 {}。\
                 必須從同一個 .clip 的不同圖層導出",
                images[0].0,
                odd.join("、")
            ),
        ));
    }

    let aspect = Aspect::from_master_size(w, h);
    if aspect.is_none() {
        out.push(Diagnostic::error(
            code::CANVAS_SIZE,
            Stage::Master,
            format!(
                "畫布 {w}×{h} 不合規：長邊必須是 {MASTER_LONG_EDGE}，比例 1:1（4096×4096）\
                 或 3:4（3072×4096）"
            ),
        ));
    }
    (out, aspect)
}

pub fn color_space(images: &[(&str, &Image)]) -> Vec<Diagnostic> {
    images
        .iter()
        .filter(|(_, img)| !img.color_space.is_srgb)
        .map(|(name, img)| {
            Diagnostic::error(
                code::COLOR_SPACE,
                Stage::Master,
                format!("{name} 的色彩描述檔不是 sRGB：{}", img.color_space.basis),
            )
        })
        .collect()
}

/// 線像素佔比過高的警告（§4.1）。獨立成一條：它必須在 `grow` 之前就報出來，
/// 因為門檻不對時後面每一條診斷都會是雜訊。
pub fn line_coverage(ratio: f32, max_ratio: f32) -> Vec<Diagnostic> {
    if ratio <= max_ratio {
        return Vec::new();
    }
    vec![Diagnostic::warning(
        code::LINE_COVERAGE,
        Stage::Master,
        format!(
            "線像素佔畫布 {:.1}%，超過 {:.0}%。\
             lineart 應該是透明底、只有線本身不透明——白底交付會讓整張都判成線",
            ratio * 100.0,
            max_ratio * 100.0
        ),
    )]
}

/// 色標的四條（§4.1）：太小、落在線上、撞進同一封閉區、漏點。
///
/// **一次全報**（§4.4）：繪師補一條線交一次、補一個點又交一次是不能接受的，
/// 所以這裡沒有任何提早退出。
pub fn seeds(
    seeds: &[Seed],
    grown: &Grown,
    orphans: &[Orphan],
    min_seed_area: u32,
    min_orphan_area: u32,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let anchor = |id: u32| seeds[id as usize].anchor;

    let mut small = Coords::master();
    let mut smallest = u32::MAX;
    for s in seeds.iter().filter(|s| s.solid_area < min_seed_area) {
        smallest = smallest.min(s.solid_area);
        small.push(s.anchor.0, s.anchor.1);
    }
    if !small.is_empty() {
        out.push(
            Diagnostic::error(
                code::SEED_TOO_SMALL,
                Stage::Master,
                format!(
                    "{} 個色標的不透明面積不足 {min_seed_area}px（最小的只有 {smallest}px），\
                     取不出可靠的建議色。把點畫大一點，直徑約 16px 以上",
                    small.total()
                ),
            )
            .with_coords(small),
        );
    }

    if !grown.on_line.is_empty() {
        let mut coords = Coords::master();
        for &id in &grown.on_line {
            let (x, y) = anchor(id);
            coords.push(x, y);
        }
        out.push(
            Diagnostic::error(
                code::SEED_ON_LINE,
                Stage::Master,
                format!(
                    "{} 個色標壓在線上，填不出區域。把點移進封閉區裡面",
                    coords.total()
                ),
            )
            .with_coords(coords),
        );
    }

    if !grown.collisions.is_empty() {
        let mut coords = Coords::master();
        for &(first, second) in &grown.collisions {
            for (x, y) in [anchor(first), anchor(second)] {
                coords.push(x, y);
            }
        }
        out.push(
            Diagnostic::error(
                code::SEED_COLLISION,
                Stage::Master,
                format!(
                    "{} 組色標落進同一個封閉區——線稿在它們之間有缺口。\
                     請在每一組的兩點之間補線（座標成對列出）",
                    grown.collisions.len()
                ),
            )
            .with_coords(coords),
        );
    }

    // 已由 `merge_small_orphans` 依面積遞減排序，座標順序即「該先看哪個」（§5）。
    if !orphans.is_empty() {
        let mut coords = Coords::master();
        for o in orphans {
            coords.push(o.anchor.0, o.anchor.1);
        }
        out.push(
            Diagnostic::error(
                code::ORPHAN_AREA,
                Stage::Master,
                format!(
                    "{} 個封閉區沒有色標（最大一處 {}px），漏點了。\
                     **一個封閉區一個點，不是一個顏色一個點**——\
                     線稿把一片顏色切成幾塊，就要點幾個點。門檻 {min_orphan_area}px",
                    coords.total(),
                    orphans[0].area
                ),
            )
            .with_coords(coords),
        );
    }

    out
}

/// `shade` 不得有 luma < 60 的像素——那個暗度只可能是線稿（`assets-spec §4.4`）。
/// 判定跑在**合成到白底之後**的值：透明處會被補成白色，那是它最終進 pack 的樣子。
pub const SHADE_MIN_LUMA: u8 = 60;

pub fn shade(shade_white: &[u8], width: u32) -> Vec<Diagnostic> {
    // 逐像素的症狀，聚類（§5）——整片過暗會是幾十萬個座標。
    let mut coords = Coords::clustered();
    let mut darkest = 255u8;
    for (i, px) in shade_white.chunks_exact(4).enumerate() {
        let y = luma([px[0], px[1], px[2]]);
        if y < SHADE_MIN_LUMA {
            darkest = darkest.min(y);
            let i = i as u32;
            coords.push(i % width, i / width);
        }
    }
    if coords.is_empty() {
        return Vec::new();
    }
    vec![
        Diagnostic::error(
            code::SHADE_TOO_DARK,
            Stage::Master,
            format!(
                "shade 有 {} 個像素的 luma < {SHADE_MIN_LUMA}（最暗 {darkest}）。\
                 線稿或底色被畫進 shade 了，只留陰影本身",
                coords.total()
            ),
        )
        .with_coords(coords),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binarize::MAX_LINE_RATIO;
    use crate::seeds::MIN_SEED_AREA;
    use crate::segment::MIN_ORPHAN_AREA;

    fn image(w: u32, h: u32, rgba: Vec<u8>) -> Image {
        Image {
            width: w,
            height: h,
            rgba,
            color_space: crate::image::ColorSpace {
                is_srgb: true,
                basis: "測試".into(),
            },
        }
    }

    fn seed_at(x: u32, y: u32, solid_area: u32) -> Seed {
        Seed {
            anchor: (x, y),
            color: [0, 0, 0],
            solid_area,
        }
    }

    fn grown(collisions: Vec<(u32, u32)>, on_line: Vec<u32>) -> Grown {
        Grown {
            labels: Vec::new(),
            collisions,
            on_line,
        }
    }

    /// §4.4：四條互相獨立，一次全報。少報一條就等於多一次退件往返。
    #[test]
    fn all_four_seed_problems_are_reported_at_once() {
        let list = [
            seed_at(10, 10, 4), // 太小
            seed_at(20, 20, 999),
            seed_at(30, 30, 999),
            seed_at(40, 40, 999), // 壓線
        ];
        let orphans = [Orphan {
            area: 900,
            anchor: (50, 50),
            bbox: [50, 50, 30, 30],
        }];
        let out = seeds(
            &list,
            &grown(vec![(1, 2)], vec![3]),
            &orphans,
            MIN_SEED_AREA,
            MIN_ORPHAN_AREA,
        );

        let codes: Vec<&str> = out.iter().map(|d| d.code).collect();
        assert!(codes.contains(&code::SEED_TOO_SMALL), "{codes:?}");
        assert!(codes.contains(&code::SEED_ON_LINE), "{codes:?}");
        assert!(codes.contains(&code::SEED_COLLISION), "{codes:?}");
        assert!(codes.contains(&code::ORPHAN_AREA), "{codes:?}");
    }

    /// collision 的座標**成對**列出——繪師要知道在哪兩點之間補線，
    /// 只給一個點他無從下手。
    #[test]
    fn a_collision_reports_both_anchors_as_a_pair() {
        let list = [seed_at(1, 1, 999), seed_at(7, 3, 999)];
        let out = seeds(
            &list,
            &grown(vec![(0, 1)], Vec::new()),
            &[],
            MIN_SEED_AREA,
            MIN_ORPHAN_AREA,
        );
        let d = out.iter().find(|d| d.code == code::SEED_COLLISION).unwrap();
        assert_eq!(d.points(), vec![(1, 1), (7, 3)]);
    }

    /// orphan 依面積遞減——「該先看哪個」是報告的排序依據（§5）。
    #[test]
    fn orphan_coords_keep_the_largest_first_ordering() {
        let orphans = [
            Orphan {
                area: 9000,
                anchor: (9, 9),
                bbox: [9, 9, 90, 100],
            },
            Orphan {
                area: 600,
                anchor: (1, 1),
                bbox: [1, 1, 20, 30],
            },
        ];
        let out = seeds(
            &[],
            &grown(Vec::new(), Vec::new()),
            &orphans,
            MIN_SEED_AREA,
            MIN_ORPHAN_AREA,
        );
        let d = out.iter().find(|d| d.code == code::ORPHAN_AREA).unwrap();
        assert_eq!(d.points(), vec![(9, 9), (1, 1)]);
        assert!(d.message.contains("9000px"), "{}", d.message);
    }

    /// 一張乾淨的素材不該產生任何色標診斷。
    #[test]
    fn a_clean_asset_produces_no_seed_diagnostics() {
        let list = [seed_at(1, 1, 999)];
        assert!(
            seeds(
                &list,
                &grown(Vec::new(), Vec::new()),
                &[],
                MIN_SEED_AREA,
                MIN_ORPHAN_AREA
            )
            .is_empty()
        );
    }

    /// 白底交付的線稿：alpha 全滿 → 整張都判成線。這是 `line-coverage` 的唯一用途。
    #[test]
    fn an_opaque_lineart_is_a_coverage_warning_not_an_error() {
        let out = line_coverage(1.0, MAX_LINE_RATIO);
        assert_eq!(out[0].code, code::LINE_COVERAGE);
        assert_eq!(out[0].severity, crate::report::Severity::Warning);
        assert!(
            line_coverage(MAX_LINE_RATIO, MAX_LINE_RATIO).is_empty(),
            "門檻上不報"
        );
    }

    #[test]
    fn canvas_size_rejects_anything_but_the_two_shapes() {
        let img = image(2048, 2048, vec![255; 2048 * 2048 * 4]);
        let (out, aspect) = geometry(&[("flats.png", &img)]);
        assert!(aspect.is_none());
        assert_eq!(out[0].code, code::CANVAS_SIZE);
    }

    #[test]
    fn shade_luma_floor_is_checked_after_compositing() {
        let dark = vec![10u8, 10, 10, 255, 255, 255, 255, 255];
        let out = shade(&dark, 2);
        assert_eq!(out[0].code, code::SHADE_TOO_DARK);
        assert_eq!(out[0].points(), vec![(0, 0)]);
    }
}
