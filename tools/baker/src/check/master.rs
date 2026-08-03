//! 母帶解析度的檢查。

use colorpack::Aspect;

use crate::compose::luma;
use crate::image::Image;
use crate::report::{Coords, Diagnostic, Stage, UniqueColors, code};
use crate::segment::{self, MIN_COLOR_AREA, RESERVED_COLOR};

/// 母帶長邊。
pub const MASTER_LONG_EDGE: u32 = 4096;

/// 四張圖尺寸與對齊一致、長邊 4096、比例 1:1 或 3:4。
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
                "四張圖尺寸不一致：{} {w}×{h}，但 {}。\
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

/// `flats` 的四條：未指派像素、抗鋸齒快篩、抗鋸齒實判、保留色。
pub fn flats(flats: &Image) -> (Vec<Diagnostic>, UniqueColors) {
    let mut out = Vec::new();
    let width = flats.width;

    // §2.3「未指派像素」的唯一定義：flats 的 alpha < 255。
    let mut gaps = Coords::master();
    for (i, px) in flats.rgba.chunks_exact(4).enumerate() {
        if px[3] != 255 {
            let i = i as u32;
            gaps.push(i % width, i / width);
        }
    }
    if !gaps.is_empty() {
        out.push(
            Diagnostic::error(
                code::UNASSIGNED_PIXEL,
                Stage::Master,
                format!(
                    "{} 個像素的 alpha < 255。flats 必須整張不透明、每個像素都有顏色\
                     （含線稿底下）。做 assets-spec §6.1 的洋紅檢查",
                    gaps.total()
                ),
            )
            .with_coords(gaps),
        );
    }

    let unique_colors = match segment::color_histogram(&flats.rgba, width) {
        // 快篩命中：圖徹底壞掉（例如色彩空間被轉換過），在算面積直方圖之前就停。
        Err(seen) => {
            out.push(Diagnostic::error(
                code::UNIQUE_COLOR_OVERFLOW,
                Stage::Master,
                format!(
                    "flats 的唯一色數超過 {}（掃到第 {seen} 個就停）。\
                     這通常代表 flats 被色彩管理轉換過，或抗鋸齒完全沒關",
                    segment::MAX_UNIQUE_COLORS
                ),
            ));
            // 掃到就停，所以這是下界不是實際值。
            UniqueColors {
                count: seen,
                exact: false,
            }
        }
        Ok(hist) => {
            // 實判：這才是 assets-spec §4.2 / §7 對繪師承諾的那條。
            let mut tiny: Vec<(&[u8; 3], u32, (u32, u32))> = hist
                .iter()
                .filter(|(_, s)| s.area < MIN_COLOR_AREA)
                .map(|(c, s)| (c, s.area, s.first))
                .collect();
            tiny.sort_by_key(|(c, _, _)| **c);
            if !tiny.is_empty() {
                let mut coords = Coords::master();
                for (_, _, (x, y)) in &tiny {
                    coords.push(*x, *y);
                }
                let sample: Vec<String> = tiny
                    .iter()
                    .take(4)
                    .map(|(c, area, _)| {
                        format!("#{:02X}{:02X}{:02X}（{area}px）", c[0], c[1], c[2])
                    })
                    .collect();
                out.push(
                    Diagnostic::error(
                        code::TINY_COLOR_AREA,
                        Stage::Master,
                        format!(
                            "{} 個顏色的總面積 < {MIN_COLOR_AREA}px（母帶），例如 {}。\
                             這是抗鋸齒沒關的判準——關掉抗鋸齒重新填色",
                            tiny.len(),
                            sample.join("、")
                        ),
                    )
                    .with_coords(coords),
                );
            }

            if let Some(stat) = hist.get(&RESERVED_COLOR) {
                let mut coords = Coords::master();
                coords.push(stat.first.0, stat.first.1);
                out.push(
                    Diagnostic::error(
                        code::RESERVED_COLOR,
                        Stage::Master,
                        format!(
                            "flats 使用了保留色 #FF00FF（{}px）。它是 assets-spec §6.1 縫隙檢查\
                             的底色，請換一個顏色",
                            stat.area
                        ),
                    )
                    .with_coords(coords),
                );
            }
            UniqueColors {
                count: hist.len(),
                exact: true,
            }
        }
    };

    (out, unique_colors)
}

/// `shade` 不得有 luma < 60 的像素——那個暗度只可能是線稿（`assets-spec §4.4`）。
/// 判定跑在**合成到白底之後**的值：透明處會被補成白色，那是它最終進 pack 的樣子。
pub const SHADE_MIN_LUMA: u8 = 60;

pub fn shade(shade_white: &[u8], width: u32) -> Vec<Diagnostic> {
    let mut coords = Coords::master();
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

    #[test]
    fn alpha_below_255_is_an_unassigned_pixel() {
        let mut rgba = vec![255u8; 4 * 4];
        rgba[4 * 2 + 3] = 254;
        let (out, _) = flats(&image(2, 2, rgba));
        let d = out
            .iter()
            .find(|d| d.code == code::UNASSIGNED_PIXEL)
            .unwrap();
        assert_eq!(d.coords, vec![(0, 1)]);
    }

    #[test]
    fn reserved_magenta_is_reported_with_its_first_coordinate() {
        let mut rgba: Vec<u8> = std::iter::repeat_n([9u8, 9, 9, 255], 4).flatten().collect();
        rgba[4..8].copy_from_slice(&[255, 0, 255, 255]);
        let (out, _) = flats(&image(2, 2, rgba));
        let d = out.iter().find(|d| d.code == code::RESERVED_COLOR).unwrap();
        assert_eq!(d.coords, vec![(1, 0)]);
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
        assert_eq!(out[0].coords, vec![(0, 0)]);
    }
}
