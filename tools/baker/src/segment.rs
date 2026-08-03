//! 唯一色掃描 → connected components → RegionMap（`specs/baker-core-design.md §2.1`）。

use std::collections::HashMap;

/// 快篩門檻（§2.6）。命中代表圖徹底壞掉（例如色彩空間被轉換過）。
pub const MAX_UNIQUE_COLORS: usize = 1024;
/// 實判門檻（§2.6、`assets-spec §4.2`），**母帶**解析度。
pub const MIN_COLOR_AREA: u32 = 100;

/// `#FF00FF` 是 `assets-spec §6.1` 縫隙檢查的保留色。
pub const RESERVED_COLOR: [u8; 3] = [255, 0, 255];

#[derive(Debug, Clone, Copy)]
pub struct ColorStat {
    pub area: u32,
    /// raster order 第一次出現的位置。
    pub first: (u32, u32),
}

/// `Err(n)` 代表唯一色數超過 `MAX_UNIQUE_COLORS`，直方圖在第 n 個顏色就放棄了——
/// 這時面積資訊沒有意義，也不該讓 HashMap 長到幾百萬筆。
pub fn color_histogram(rgba: &[u8], width: u32) -> Result<HashMap<[u8; 3], ColorStat>, usize> {
    let mut map: HashMap<[u8; 3], ColorStat> = HashMap::new();
    for (i, px) in rgba.chunks_exact(4).enumerate() {
        let key = [px[0], px[1], px[2]];
        match map.get_mut(&key) {
            Some(stat) => stat.area += 1,
            None => {
                if map.len() >= MAX_UNIQUE_COLORS {
                    return Err(map.len() + 1);
                }
                let i = i as u32;
                map.insert(
                    key,
                    ColorStat {
                        area: 1,
                        first: (i % width, i / width),
                    },
                );
            }
        }
    }
    Ok(map)
}

#[derive(Debug, Clone)]
pub struct RegionMap {
    pub width: u32,
    pub height: u32,
    /// 每像素的 region id。長度 = `width * height`。
    pub labels: Vec<u32>,
    pub count: u32,
    /// 每個 region 的像素數。索引即 id。
    pub areas: Vec<u32>,
    /// 每個 region 在 raster order 的第一個像素（線性 index）。
    pub first_pixel: Vec<u32>,
}

impl RegionMap {
    pub fn coord(&self, index: u32) -> (u32, u32) {
        (index % self.width, index / self.width)
    }
}

/// **4-連通**（§2.1）。8-連通會把只在對角相觸的兩塊同色區域併成一塊，而
/// `assets-spec §4.2 ④` 給繪師的心智模型是「相連才會合併」——對角相觸算不算相連
/// 在繪師端是模稜兩可的，取保守的 4-連通。
///
/// id 依「第一個像素的 raster order」配發，完全決定性。
pub fn label_regions(rgba: &[u8], width: u32, height: u32) -> RegionMap {
    let (w, h) = (width as usize, height as usize);
    let mut labels = vec![u32::MAX; w * h];
    let mut areas = Vec::new();
    let mut first_pixel = Vec::new();
    let mut stack: Vec<u32> = Vec::new();

    let color_at = |i: usize| -> [u8; 3] { [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]] };

    for seed in 0..w * h {
        if labels[seed] != u32::MAX {
            continue;
        }
        let id = areas.len() as u32;
        let target = color_at(seed);
        let mut area = 0u32;

        labels[seed] = id;
        stack.push(seed as u32);
        while let Some(p) = stack.pop() {
            area += 1;
            let p = p as usize;
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<u32>| {
                if labels[n] == u32::MAX && color_at(n) == target {
                    labels[n] = id;
                    stack.push(n as u32);
                }
            };
            if x > 0 {
                visit(p - 1, &mut stack);
            }
            if x + 1 < w {
                visit(p + 1, &mut stack);
            }
            if y > 0 {
                visit(p - w, &mut stack);
            }
            if y + 1 < h {
                visit(p + w, &mut stack);
            }
        }
        areas.push(area);
        first_pixel.push(seed as u32);
    }

    RegionMap {
        width,
        height,
        count: areas.len() as u32,
        labels,
        areas,
        first_pixel,
    }
}

/// 在既有的 ID map 上重跑 4-連通，回報每個 id 被切成幾塊（§2.7 的 `region-split`）。
/// 回傳 `(塊數, 每個 id 的某一塊之外的代表座標)`。
pub fn count_components_per_id(ids: &[u32], width: u32, height: u32, count: u32) -> Vec<u32> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut pieces = vec![0u32; count as usize];
    let mut stack: Vec<u32> = Vec::new();

    for seed in 0..w * h {
        if seen[seed] {
            continue;
        }
        let id = ids[seed];
        pieces[id as usize] += 1;
        seen[seed] = true;
        stack.push(seed as u32);
        while let Some(p) = stack.pop() {
            let p = p as usize;
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<u32>| {
                if !seen[n] && ids[n] == id {
                    seen[n] = true;
                    stack.push(n as u32);
                }
            };
            if x > 0 {
                visit(p - 1, &mut stack);
            }
            if x + 1 < w {
                visit(p + 1, &mut stack);
            }
            if y > 0 {
                visit(p - w, &mut stack);
            }
            if y + 1 < h {
                visit(p + w, &mut stack);
            }
        }
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(pixels: &[[u8; 3]]) -> Vec<u8> {
        pixels
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect()
    }

    const R: [u8; 3] = [255, 0, 0];
    const G: [u8; 3] = [0, 255, 0];

    /// §2.1：對角相觸的同色塊在 4-連通下是**兩個**區域。
    /// 這個 2×2 的四格兩兩對角同色，8-連通會併成 2 區，4-連通是 4 區。
    #[test]
    fn diagonal_touch_is_not_connected() {
        let map = label_regions(&rgba(&[R, G, G, R]), 2, 2);
        assert_eq!(map.count, 4);
        assert_eq!(map.labels, vec![0, 1, 2, 3]);
        assert_eq!(map.areas, vec![1, 1, 1, 1]);
    }

    /// 反例：橫向相連的同色確實會被併成一塊（`assets-spec §4.2 ④`）。
    #[test]
    fn orthogonally_adjacent_same_color_merges() {
        let map = label_regions(&rgba(&[R, R, G, G]), 2, 2);
        assert_eq!(map.count, 2);
        assert_eq!(map.areas, vec![2, 2]);
    }

    #[test]
    fn ids_follow_raster_order_of_first_pixel() {
        let map = label_regions(&rgba(&[G, G, R, R]), 2, 2);
        assert_eq!(map.labels, vec![0, 0, 1, 1]);
        assert_eq!(map.first_pixel, vec![0, 2]);
    }

    #[test]
    fn histogram_bails_out_instead_of_growing_unbounded() {
        // 每個像素一個顏色，遠超 1024
        let pixels: Vec<[u8; 3]> = (0..2000u32)
            .map(|i| [(i >> 8) as u8, (i & 0xff) as u8, 0])
            .collect();
        assert!(color_histogram(&rgba(&pixels), 2000).is_err());
    }

    #[test]
    fn histogram_records_area_and_first_occurrence() {
        let hist = color_histogram(&rgba(&[R, G, G, R]), 2).unwrap();
        assert_eq!(hist[&R].area, 2);
        assert_eq!(hist[&R].first, (0, 0));
        assert_eq!(hist[&G].first, (1, 0));
    }

    #[test]
    fn split_detection_counts_pieces() {
        // id 0 被 id 1 從中間切開 → 兩塊
        let ids = vec![0, 1, 0, 0, 1, 0];
        assert_eq!(count_components_per_id(&ids, 3, 2, 2), vec![2, 1]);
    }
}
