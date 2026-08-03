//! 色標 flood fill → 測地擴張 → RegionMap（`specs/baker-seeds.md §3`）。
//!
//! 區域由**線稿**決定，不由顏色決定。`grow` 讓每個色標各自吃下自己的封閉區，
//! `close` 再把線像素本身瓜分掉，讓 `region_ids` 全覆蓋。

use std::cmp::Reverse;

use crate::seeds::Seed;

/// 未認領自由區的報錯門檻（母帶）。低於此值視為線稿的小洞，併進鄰居。
pub const MIN_ORPHAN_AREA: u32 = 500;

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

    /// 從已全覆蓋的 label map 統計面積與 raster order 首像素。
    ///
    /// **前提是 `close` 已跑完**：留著 `UNASSIGNED` 會 panic，因為那代表
    /// `region_ids` 有洞，而 App 端不容許沒有 id 的像素（§3.1 ②）。
    pub fn from_labels(labels: Vec<u32>, width: u32, height: u32, count: u32) -> Self {
        let n = count as usize;
        let mut areas = vec![0u32; n];
        let mut first_pixel = vec![u32::MAX; n];
        for (i, &id) in labels.iter().enumerate() {
            assert_ne!(id, UNASSIGNED, "close 之後不該還有未指派像素");
            let id = id as usize;
            areas[id] += 1;
            if first_pixel[id] == u32::MAX {
                first_pixel[id] = i as u32;
            }
        }
        Self {
            width,
            height,
            count,
            labels,
            areas,
            first_pixel,
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orphan {
    pub area: u32,
    /// raster order 的第一個像素。診斷座標用，不需要是重心。
    pub anchor: (u32, u32),
}

/// 沒有任何 seed 認領的自由區（非線像素且未指派）的 4-連通塊。
///
/// **必須在 `close` 之前呼叫**——`close` 會把 id 擴散進這些區域，之後就找不到了。
/// 回傳依面積遞減排序，平手取 anchor 的 raster order（§5「可疑度排序」）。
pub fn find_orphans(labels: &[u32], line: &[bool], width: u32, height: u32) -> Vec<Orphan> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    let free = |i: usize| !line[i] && labels[i] == UNASSIGNED;

    for start in 0..w * h {
        if seen[start] || !free(start) {
            continue;
        }
        let mut area = 0u32;
        seen[start] = true;
        stack.push(start);
        while let Some(p) = stack.pop() {
            area += 1;
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<usize>| {
                if !seen[n] && free(n) {
                    seen[n] = true;
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
        out.push(Orphan {
            area,
            anchor: ((start % w) as u32, (start / w) as u32),
        });
    }

    out.sort_by_key(|o| (Reverse(o.area), o.anchor.1, o.anchor.0));
    out
}

/// 面積 < `min_area` 的未認領自由區就地併進「面積最大的相鄰區」（§3.1 ④），
/// 回傳**剩下的**大塊——那些才是 `orphan-area`，是繪師真的漏點了。
///
/// 自由區永遠被線像素圍住（否則 `grow` 早就把它吃進去了），所以「相鄰」必須**穿過線**
/// 去找：由碎片邊界逐環向外走線像素，第一個碰到已標記像素的環決定候選集合，
/// 其中取 `region_areas` 最大者，平手取較小 id。
///
/// 必須在 `close` 之前呼叫——`close` 會把 id 擴散進這些區域，之後就找不到了。
pub fn merge_small_orphans(
    labels: &mut [u32],
    line: &[bool],
    width: u32,
    height: u32,
    region_areas: &[u32],
    min_area: u32,
) -> Vec<Orphan> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut kept = Vec::new();
    let mut block: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    // `labels` 會在迴圈中被寫入，自由與否必須以「非線 ＋ 當下未指派」判定。
    for start in 0..w * h {
        if seen[start] || line[start] || labels[start] != UNASSIGNED {
            continue;
        }
        block.clear();
        seen[start] = true;
        stack.push(start);
        while let Some(p) = stack.pop() {
            block.push(p);
            for_each_neighbor(p, w, h, |n| {
                if !seen[n] && !line[n] && labels[n] == UNASSIGNED {
                    seen[n] = true;
                    stack.push(n);
                }
            });
        }

        let anchor = ((start % w) as u32, (start / w) as u32);
        let area = block.len() as u32;
        if area >= min_area {
            kept.push(Orphan { area, anchor });
            continue;
        }
        if let Some(id) = nearest_region(&block, labels, line, w, h, region_areas) {
            for &p in &block {
                labels[p] = id;
            }
        }
    }

    kept.sort_by_key(|o| (Reverse(o.area), o.anchor.1, o.anchor.0));
    kept
}

/// 由 `block` 穿過線像素向外 BFS，回傳最近一環上「面積最大」的已標記 region id。
/// 走不到任何已標記像素（整張圖沒有 seed）時回傳 `None`。
fn nearest_region(
    block: &[usize],
    labels: &[u32],
    line: &[bool],
    w: usize,
    h: usize,
    region_areas: &[u32],
) -> Option<u32> {
    let mut visited: std::collections::HashSet<usize> = block.iter().copied().collect();
    let mut frontier: Vec<usize> = block.to_vec();
    let mut next: Vec<usize> = Vec::new();

    while !frontier.is_empty() {
        let mut candidates: Vec<u32> = Vec::new();
        next.clear();
        for &p in &frontier {
            for_each_neighbor(p, w, h, |n| {
                if !visited.insert(n) {
                    return;
                }
                if labels[n] != UNASSIGNED {
                    candidates.push(labels[n]);
                } else if line[n] {
                    next.push(n);
                }
                // 非線又未指派：另一個自由區塊，不穿過。
            });
        }
        if !candidates.is_empty() {
            return candidates
                .into_iter()
                .min_by_key(|&id| (Reverse(region_areas[id as usize]), id));
        }
        std::mem::swap(&mut frontier, &mut next);
    }
    None
}

fn for_each_neighbor(p: usize, w: usize, h: usize, mut f: impl FnMut(usize)) {
    let (x, y) = (p % w, p / w);
    if x > 0 {
        f(p - 1);
    }
    if x + 1 < w {
        f(p + 1);
    }
    if y > 0 {
        f(p - w);
    }
    if y + 1 < h {
        f(p + w);
    }
}

/// 同步 BFS 波前：每輪把所有「與已標記像素相鄰的未指派像素」一次指派完，
/// 取鄰居中**最小的 id**。
///
/// 同步是為了確定性——逐像素推進的話結果依賴掃描順序，golden test 守不住。
/// 取最小 id 讓等距處的分界線有唯一解，視覺上落在線的中軸。
///
/// 回傳 `(輪數, 收斂後仍未指派的像素數)`。剩餘量非 0 代表整張圖有連一個 seed
/// 都碰不到的孤島。
pub fn close(labels: &mut [u32], width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width as usize, height as usize);
    let mut rounds = 0u32;
    let mut pending: Vec<(usize, u32)> = Vec::new();

    loop {
        pending.clear();
        for p in 0..w * h {
            if labels[p] != UNASSIGNED {
                continue;
            }
            let (x, y) = (p % w, p / w);
            let mut best = UNASSIGNED;
            let consider = |n: usize, best: &mut u32| {
                if labels[n] != UNASSIGNED {
                    *best = (*best).min(labels[n]);
                }
            };
            if x > 0 {
                consider(p - 1, &mut best);
            }
            if x + 1 < w {
                consider(p + 1, &mut best);
            }
            if y > 0 {
                consider(p - w, &mut best);
            }
            if y + 1 < h {
                consider(p + w, &mut best);
            }
            if best != UNASSIGNED {
                pending.push((p, best));
            }
        }
        if pending.is_empty() {
            break;
        }
        for &(p, id) in &pending {
            labels[p] = id;
        }
        rounds += 1;
    }

    let left = labels.iter().filter(|&&v| v == UNASSIGNED).count() as u32;
    (rounds, left)
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

    /// 沒有 seed 的封閉區被列為孤兒，面積遞減排序。
    #[test]
    fn unseeded_areas_are_listed_largest_first() {
        // 7x1：[有主][線][孤兒 x1][線][孤兒 x3]
        let line = vec![false, true, false, true, false, false, false];
        let labels = vec![
            0,
            UNASSIGNED,
            UNASSIGNED,
            UNASSIGNED,
            UNASSIGNED,
            UNASSIGNED,
            UNASSIGNED,
        ];
        let orphans = find_orphans(&labels, &line, 7, 1);
        assert_eq!(orphans.len(), 2);
        assert_eq!(orphans[0].area, 3);
        assert_eq!(orphans[0].anchor, (4, 0));
        assert_eq!(orphans[1].area, 1);
        assert_eq!(orphans[1].anchor, (2, 0));
    }

    /// 線像素本身不是孤兒——它們由 `close` 指派。
    #[test]
    fn line_pixels_are_not_orphans() {
        let line = vec![false, true, false];
        let labels = vec![0, UNASSIGNED, 1];
        assert!(find_orphans(&labels, &line, 3, 1).is_empty());
    }

    /// `close` 把線像素瓜分給兩側，平手取較小 id——確定性的來源。
    #[test]
    fn close_splits_the_line_and_breaks_ties_toward_the_smaller_id() {
        // 4x1：[0][線][線][1]，兩個線像素各由一側吃掉
        let mut labels = vec![0, UNASSIGNED, UNASSIGNED, 1];
        let (rounds, left) = close(&mut labels, 4, 1);
        assert_eq!(labels, vec![0, 0, 1, 1]);
        assert_eq!((rounds, left), (1, 0));
    }

    /// 寬度為奇數的線：正中央的像素兩側等距，必須歸較小 id。
    #[test]
    fn the_middle_of_an_odd_width_line_goes_to_the_smaller_id() {
        let mut labels = vec![0, UNASSIGNED, UNASSIGNED, UNASSIGNED, 1];
        let (rounds, left) = close(&mut labels, 5, 1);
        assert_eq!(labels, vec![0, 0, 0, 1, 1], "第 2 輪時中央同時鄰接 0 與 1，取 0");
        assert_eq!((rounds, left), (2, 0));
    }

    /// 一個 seed 都沒有 → 沒有東西可以擴散，回報剩餘量而不是無限迴圈。
    #[test]
    fn close_terminates_when_there_is_nothing_to_grow_from() {
        let mut labels = vec![UNASSIGNED; 4];
        assert_eq!(close(&mut labels, 4, 1), (0, 4));
    }

    /// §3.1 ④ 的分野：小碎片靜默併入，大塊留下來報 `orphan-area`。
    #[test]
    fn small_fragments_merge_and_large_ones_are_reported() {
        // 9x1：[區 0 x2][線][碎片 x1][線][大塊 x3]，門檻 3
        let line = vec![false, false, true, false, true, false, false, false, false];
        let mut labels = vec![0, 0, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED];
        let kept = merge_small_orphans(&mut labels, &line, 9, 1, &[2], 3);

        assert_eq!(kept.len(), 1, "只有 ≥3px 的那塊該留下來");
        assert_eq!((kept[0].area, kept[0].anchor), (4, (5, 0)));
        assert_eq!(labels[3], 0, "1px 碎片穿過線併進區 0");
        assert_eq!(labels[5], UNASSIGNED, "大塊不動，交給 orphan-area 回報");
    }

    /// 碎片兩側都有區域時取**面積最大**的那一邊，不是先掃到的那一邊。
    #[test]
    fn a_fragment_joins_the_largest_neighbour_not_the_first() {
        // 5x1：[區 0][線][碎片][線][區 1]，區 1 比區 0 大
        let line = vec![false, true, false, true, false];
        let mut labels = vec![0, UNASSIGNED, UNASSIGNED, UNASSIGNED, 1];
        let kept = merge_small_orphans(&mut labels, &line, 5, 1, &[10, 99], 500);
        assert!(kept.is_empty());
        assert_eq!(labels[2], 1, "區 1 面積 99 > 區 0 的 10");
    }

    /// 面積平手取較小 id——與 `close` 的等距規則同一個理由：確定性。
    #[test]
    fn a_tie_in_neighbour_area_goes_to_the_smaller_id() {
        let line = vec![false, true, false, true, false];
        let mut labels = vec![0, UNASSIGNED, UNASSIGNED, UNASSIGNED, 1];
        merge_small_orphans(&mut labels, &line, 5, 1, &[50, 50], 500);
        assert_eq!(labels[2], 0);
    }

    /// 一個 seed 都沒有 → 沒有鄰居可併，碎片原地不動而不是 panic。
    #[test]
    fn a_fragment_with_no_reachable_region_is_left_alone() {
        let line = vec![false, true, false];
        let mut labels = vec![UNASSIGNED; 3];
        assert!(merge_small_orphans(&mut labels, &line, 3, 1, &[], 500).is_empty());
        assert_eq!(labels, vec![UNASSIGNED; 3]);
    }

    /// `from_labels` 的面積與首像素：`check::output` 的 `region-count-drift`
    /// 座標全靠 `first_pixel`。
    #[test]
    fn region_map_from_labels_counts_area_and_first_pixel() {
        let map = RegionMap::from_labels(vec![1, 1, 0, 0, 1, 1], 3, 2, 2);
        assert_eq!(map.areas, vec![2, 4]);
        assert_eq!(map.first_pixel, vec![2, 0]);
        assert_eq!(map.coord(map.first_pixel[0]), (2, 0));
    }

    #[test]
    fn split_detection_counts_pieces() {
        // id 0 被 id 1 從中間切開 → 兩塊
        let ids = vec![0, 1, 0, 0, 1, 0];
        assert_eq!(count_components_per_id(&ids, 3, 2, 2), vec![2, 1]);
    }
}
