//! 輸出解析度的檢查。母帶通過不代表輸出通過（`architecture §9.2` 陷阱 3）。

use colorpack::manifest::MAX_REGION_COUNT;

use crate::report::{Coords, Diagnostic, Stage, code};
use crate::segment::{RegionMap, count_components_per_id};

/// 碎片區域門檻，**輸出**解析度。繪師端對應的母帶數字是 800px（`assets-spec §7` 註）。
pub const MIN_REGION_AREA: u32 = 200;

/// `region-count-range` 的門檻。兩者都是警告，不是錯誤。
pub const REGION_COUNT_MIN: u32 = 8;
pub const REGION_COUNT_MAX: u32 = 2000;

/// 輸出解析度的逐區統計。索引即 region id；`area == 0` 代表該區在輸出被吃掉了。
pub struct OutputStats {
    pub area: Vec<u32>,
    pub bbox: Vec<[u32; 4]>,
    pub centroid: Vec<[u32; 2]>,
}

pub fn stats(ids: &[u32], width: u32, height: u32, count: u32) -> OutputStats {
    let n = count as usize;
    let mut area = vec![0u32; n];
    let mut sum = vec![(0u64, 0u64); n];
    let mut min = vec![(u32::MAX, u32::MAX); n];
    let mut max = vec![(0u32, 0u32); n];

    for (i, &id) in ids.iter().enumerate() {
        let id = id as usize;
        let (x, y) = (i as u32 % width, i as u32 / width);
        area[id] += 1;
        sum[id].0 += x as u64;
        sum[id].1 += y as u64;
        min[id] = (min[id].0.min(x), min[id].1.min(y));
        max[id] = (max[id].0.max(x), max[id].1.max(y));
    }
    debug_assert_eq!(area.iter().sum::<u32>() as usize, (width * height) as usize);

    let mut bbox = vec![[0u32; 4]; n];
    let mut centroid = vec![[0u32; 2]; n];
    for id in 0..n {
        if area[id] == 0 {
            continue;
        }
        bbox[id] = [
            min[id].0,
            min[id].1,
            max[id].0 - min[id].0 + 1,
            max[id].1 - min[id].1 + 1,
        ];
        centroid[id] = [
            (sum[id].0 / area[id] as u64) as u32,
            (sum[id].1 / area[id] as u64) as u32,
        ];
    }
    OutputStats {
        area,
        bbox,
        centroid,
    }
}

pub fn check(
    master: &RegionMap,
    ids: &[u32],
    width: u32,
    height: u32,
    stats: &OutputStats,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    debug_assert!(
        ids.iter().all(|&id| id < master.count),
        "majority 產出了不存在的 region ID——輸出階段不可能有未指派像素"
    );

    // 降採樣後區域數與母帶一致。座標取被吃掉那一區在母帶的第一個像素。
    let vanished: Vec<u32> = (0..master.count)
        .filter(|&id| stats.area[id as usize] == 0)
        .collect();
    if !vanished.is_empty() {
        let mut coords = Coords::master();
        for &id in &vanished {
            let (x, y) = master.coord(master.first_pixel[id as usize]);
            coords.push(x, y);
        }
        out.push(
            Diagnostic::error(
                code::REGION_COUNT_DRIFT,
                Stage::Output,
                format!(
                    "{} 個區域在降採樣後消失（母帶 {} 區 → 輸出 {} 區）。\
                     那些區域細到撐不過縮圖，請加寬或併入鄰區",
                    vanished.len(),
                    master.count,
                    master.count - vanished.len() as u32
                ),
            )
            .with_coords(coords),
        );
    }

    if master.count > MAX_REGION_COUNT {
        out.push(Diagnostic::error(
            code::REGION_COUNT_OVERFLOW,
            Stage::Output,
            format!(
                "區域數 {} 超過 R16 上限 {MAX_REGION_COUNT}，regions.bin 裝不下",
                master.count
            ),
        ));
    }

    let mut tiny = Coords::output();
    for id in 0..master.count as usize {
        let area = stats.area[id];
        if area > 0 && area < MIN_REGION_AREA {
            tiny.push(stats.centroid[id][0], stats.centroid[id][1]);
        }
    }
    if !tiny.is_empty() {
        out.push(
            Diagnostic::warning(
                code::TINY_REGION,
                Stage::Output,
                format!(
                    "{} 個碎片區域（輸出面積 < {MIN_REGION_AREA}px，母帶約 {}px）。\
                     確認那些小塊是刻意的（例如亮點），不是雜點",
                    tiny.total(),
                    MIN_REGION_AREA * 4
                ),
            )
            .with_coords(tiny),
        );
    }

    if master.count < REGION_COUNT_MIN || master.count > REGION_COUNT_MAX {
        out.push(Diagnostic::warning(
            code::REGION_COUNT_RANGE,
            Stage::Output,
            format!(
                "區域數 {} 落在建議範圍 {REGION_COUNT_MIN}–{REGION_COUNT_MAX} 之外",
                master.count
            ),
        ));
    }

    // §2.7：母帶連通的區域，1px 頸部被降採樣吃掉後可能裂成兩塊。ID 還在，但 runtime
    // 的 Mode A 遮罩是 `id == active_region_id`——使用者點一塊，另一塊也會被填。
    let pieces = count_components_per_id(ids, width, height, master.count);
    let mut split = Coords::output();
    for (id, &count) in pieces.iter().enumerate() {
        if count > 1 {
            split.push(stats.centroid[id][0], stats.centroid[id][1]);
        }
    }
    if !split.is_empty() {
        out.push(
            Diagnostic::warning(
                code::REGION_SPLIT,
                Stage::Output,
                format!(
                    "{} 個區域在輸出解析度下斷成多塊。使用者點一塊時另一塊也會被填——\
                     被線稿切斷的髮束是合理交付，但無意的細頸不是",
                    split.total()
                ),
            )
            .with_coords(split),
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master(labels: &[u32], w: u32, h: u32, count: u32) -> RegionMap {
        RegionMap::from_labels(labels.to_vec(), w, h, count)
    }

    #[test]
    fn stats_are_in_output_space() {
        let ids = vec![0, 0, 1, 1];
        let s = stats(&ids, 2, 2, 2);
        assert_eq!(s.area, vec![2, 2]);
        assert_eq!(s.bbox[0], [0, 0, 2, 1]);
        assert_eq!(s.centroid[1], [0, 1]);
    }

    #[test]
    fn vanished_region_is_an_error_with_master_coordinates() {
        // 母帶 4×2：兩區，其中 id 1 是 (1,0) 的單一像素
        let m = master(&[0, 1, 0, 0, 0, 0, 0, 0], 4, 2, 2);
        let ids = vec![0, 0]; // majority 把 id 1 吃掉
        let s = stats(&ids, 2, 1, m.count);
        let out = check(&m, &ids, 2, 1, &s);
        let d = out
            .iter()
            .find(|d| d.code == code::REGION_COUNT_DRIFT)
            .unwrap();
        assert!(d.coords.contains(&(1, 0)));
    }

    #[test]
    fn split_region_is_a_warning() {
        // id 0 在輸出被 id 1 從中間切開 → 兩塊
        let m2 = master(&[0, 1, 0], 3, 1, 2);
        let split_ids = vec![0, 1, 0];
        let s = stats(&split_ids, 3, 1, m2.count);
        let out = check(&m2, &split_ids, 3, 1, &s);
        assert!(out.iter().any(|d| d.code == code::REGION_SPLIT));

        // 反例：連成一塊的不報
        let m1 = master(&[0, 0, 0, 0], 4, 1, 1);
        let ids = vec![0, 0, 0, 0];
        let s = stats(&ids, 4, 1, m1.count);
        assert!(
            !check(&m1, &ids, 4, 1, &s)
                .iter()
                .any(|d| d.code == code::REGION_SPLIT)
        );
    }
}
