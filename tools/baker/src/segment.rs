//! 唯一色掃描 → connected components → RegionMap（`specs/baker-core-design.md §2.1`）。

use std::collections::HashMap;

use crate::seeds::Seed;

pub const MAX_UNIQUE_COLORS: usize = 1024;
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

/// `labels` 裡代表「還沒有 region id」的值。
pub const UNASSIGNED: u32 = u32::MAX;

#[derive(Debug, Clone)]
pub struct Grown {
    /// 每像素的 region id，`UNASSIGNED` 表示未指派。索引即 seed 在輸入切片的位置。
    pub labels: Vec<u32>,
    /// `(先佔住該封閉區的 seed id, 撞進來的 seed id)`。線稿有缺口的證據。
    pub collisions: Vec<(u32, u32)>,
    /// anchor 落在線像素上的 seed id。
    pub on_line: Vec<u32>,
}

/// 逐 seed 4-連通 flood fill，只走非線像素。
///
/// 用逐 seed 而非多源同步 BFS：線稿封閉時兩者等價，不封閉時只有逐 seed
/// 能指出「哪兩個 seed 連在一起」。同步 BFS 會在中間切一條任意分界線然後
/// 靜默通過——那正是要避免的失敗模式（`baker-seeds §3.1 ①`）。
pub fn grow(seeds: &[Seed], line: &[bool], width: u32, height: u32) -> Grown {
    let (w, h) = (width as usize, height as usize);
    let mut labels = vec![UNASSIGNED; w * h];
    let mut collisions = Vec::new();
    let mut on_line = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for (id, s) in seeds.iter().enumerate() {
        let id = id as u32;
        let start = s.anchor.1 as usize * w + s.anchor.0 as usize;
        if line[start] {
            on_line.push(id);
            continue;
        }
        if labels[start] != UNASSIGNED {
            collisions.push((labels[start], id));
            continue;
        }

        labels[start] = id;
        stack.push(start);
        while let Some(p) = stack.pop() {
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<usize>| {
                if !line[n] && labels[n] == UNASSIGNED {
                    labels[n] = id;
                    stack.push(n);
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

    Grown {
        labels,
        collisions,
        on_line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeds::Seed;

    fn seed(x: u32, y: u32) -> Seed {
        Seed {
            anchor: (x, y),
            color: [0, 0, 0],
            solid_area: 100,
        }
    }

    /// 一條垂直線把 3x1 切成左右兩半，兩個 seed 各自佔一半。
    #[test]
    fn two_seeds_separated_by_a_line_get_their_own_regions() {
        let line = vec![false, true, false];
        let g = grow(&[seed(0, 0), seed(2, 0)], &line, 3, 1);
        assert_eq!(g.labels, vec![0, UNASSIGNED, 1]);
        assert!(g.collisions.is_empty());
        assert!(g.on_line.is_empty());
    }

    /// **線稿缺口的症狀**：線沒把兩邊隔開，第二個 seed 撞進第一個的區域。
    /// 必須報 collision 而不是靜默切一條分界線——那是繪師唯一需要知道的事。
    #[test]
    fn a_gap_in_the_line_is_reported_as_a_collision() {
        let line = vec![false, false, false];
        let g = grow(&[seed(0, 0), seed(2, 0)], &line, 3, 1);
        assert_eq!(g.collisions, vec![(0, 1)], "先佔住的是 seed 0");
        assert_eq!(g.labels, vec![0, 0, 0], "整條仍歸 seed 0，preview 才畫得出來");
    }

    /// anchor 落在線上 → flood fill 起不來，回報而非 panic。
    #[test]
    fn a_seed_sitting_on_the_line_is_reported_and_skipped() {
        let line = vec![false, true, false];
        let g = grow(&[seed(1, 0)], &line, 3, 1);
        assert_eq!(g.on_line, vec![0]);
        assert!(g.labels.iter().all(|&v| v == UNASSIGNED));
    }

    /// 沒有 seed 的封閉區維持未指派——`find_orphans` 之後才判定它是不是漏點。
    #[test]
    fn a_closed_area_with_no_seed_stays_unassigned() {
        // 5x1：左半有 seed，右半被線隔開且沒有 seed
        let line = vec![false, false, true, false, false];
        let g = grow(&[seed(0, 0)], &line, 5, 1);
        assert_eq!(g.labels, vec![0, 0, UNASSIGNED, UNASSIGNED, UNASSIGNED]);
    }

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
