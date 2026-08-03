//! `reference` 一致性驗證 ＋ 逐區取建議色（`specs/baker-core-design.md §2.4`）。
//!
//! **只有一條檢查**：對每個 flats region，`reference` 在其內部像素同色。
//! 「`reference` 不引入 `flats` 沒有的邊界」是它的直接推論，不是另一條要檢查的事。
//! 相鄰區 `reference` 同色是合法的（`assets-spec §4.3`），所以 baker 完全不需要建鄰接圖。

use colorpack::region::hex_color;

use crate::report::{Coords, Diagnostic, Stage, code};
use crate::segment::RegionMap;

pub struct Suggested {
    /// 每個 region 的建議色，索引即 region id。
    pub colors: Vec<[u8; 3]>,
}

/// 一次線性掃描同時做兩件事：驗一致性、取建議色。
pub fn read(reference_rgba: &[u8], regions: &RegionMap) -> (Suggested, Option<Diagnostic>) {
    let mut colors = vec![[0u8; 3]; regions.count as usize];
    let mut seen = vec![false; regions.count as usize];
    let mut reported = vec![false; regions.count as usize];
    let mut coords = Coords::master();
    let mut first_region = None;

    for (i, px) in reference_rgba.chunks_exact(4).enumerate() {
        let id = regions.labels[i] as usize;
        let rgb = [px[0], px[1], px[2]];
        if !seen[id] {
            seen[id] = true;
            colors[id] = rgb;
        } else if colors[id] != rgb && !reported[id] {
            reported[id] = true;
            let (x, y) = regions.coord(i as u32);
            coords.push(x, y);
            first_region.get_or_insert(id as u32);
        }
    }

    let diagnostic = (!coords.is_empty()).then(|| {
        let mut d = Diagnostic::error(
            code::REF_MISMATCH,
            Stage::Master,
            format!(
                "{} 個 flats 區域的 reference 內部不是單一顏色（座標是每個區域的首個相異像素）。\
                 複製 flats 圖層重做，每區只塗一個顏色",
                coords.total()
            ),
        )
        .with_coords(coords);
        if let Some(region) = first_region {
            d = d.with_region(region);
        }
        d
    });

    (Suggested { colors }, diagnostic)
}

/// `manifest.palette[]`：**去重後**的建議色票，依總面積遞減排序，平手取較小 region id。
/// 不設上限，UI 自行取前 N 個（§3.5）。面積用**輸出**解析度，與 `regions.json` 同一個空間。
pub fn palette(colors: &[[u8; 3]], output_areas: &[u32]) -> Vec<String> {
    let mut acc: Vec<([u8; 3], u64, u32)> = Vec::new();
    for (id, color) in colors.iter().enumerate() {
        let area = output_areas.get(id).copied().unwrap_or(0) as u64;
        match acc.iter_mut().find(|(c, _, _)| c == color) {
            Some(entry) => entry.1 += area,
            None => acc.push((*color, area, id as u32)),
        }
    }
    acc.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    acc.into_iter().map(|(c, _, _)| hex_color(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::label_regions;

    fn rgba(pixels: &[[u8; 3]]) -> Vec<u8> {
        pixels
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect()
    }
    const R: [u8; 3] = [255, 0, 0];
    const G: [u8; 3] = [0, 255, 0];
    const B: [u8; 3] = [0, 0, 255];

    #[test]
    fn uniform_reference_passes_and_yields_colors() {
        let regions = label_regions(&rgba(&[R, R, G, G]), 2, 2);
        let (s, d) = read(&rgba(&[B, B, R, R]), &regions);
        assert!(d.is_none());
        assert_eq!(s.colors, vec![B, R]);
    }

    /// 相鄰區同色是**合法**的——任何「相鄰區顏色必須相異」的檢查都會退掉合格素材。
    #[test]
    fn adjacent_regions_may_share_a_reference_color() {
        let regions = label_regions(&rgba(&[R, R, G, G]), 2, 2);
        let (_, d) = read(&rgba(&[B, B, B, B]), &regions);
        assert!(d.is_none());
    }

    #[test]
    fn second_color_inside_one_region_is_reported_with_its_coordinate() {
        let regions = label_regions(&rgba(&[R, R, R, R]), 2, 2);
        let (_, d) = read(&rgba(&[B, B, B, G]), &regions);
        let d = d.expect("應該要拒收");
        assert_eq!(d.code, code::REF_MISMATCH);
        assert_eq!(d.coords, vec![(1, 1)]);
        assert_eq!(d.region, Some(0));
    }

    #[test]
    fn palette_is_deduped_and_sorted_by_total_area() {
        let colors = [R, G, R];
        // R 合計 3 + 1 = 4，G 是 10 → G 在前
        assert_eq!(
            palette(&colors, &[3, 10, 1]),
            vec!["#00FF00".to_owned(), "#FF0000".to_owned()]
        );
    }

    #[test]
    fn palette_ties_go_to_the_smaller_region_id() {
        assert_eq!(
            palette(&[G, R], &[5, 5]),
            vec!["#00FF00".to_owned(), "#FF0000".to_owned()]
        );
    }
}
