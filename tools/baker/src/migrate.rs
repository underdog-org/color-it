//! 從舊契約的 `reference.png` 反推 `seeds.png`（**過渡用**）。
//!
//! `baker-seeds.md §2` 把交付從 flats+reference 換成 seeds，舊契約交出的手繪素材
//! 因此缺件。重交是繪師的事（Phase 4 的 JD），但在那之前專案方需要能把既有素材
//! 烘出來——這個模組就是那條橋。
//!
//! 方法是 Phase 0 驗證過的那個：**只信線稿**決定區域（非線像素的 4-連通塊），
//! `reference` 只用來取每塊的眾數色。§7 的實測顯示 `adventure-time-demo-1` 的
//! 62 個封閉區每一個在 `reference` 裡都是單一平塗色，所以眾數是可靠的。
//!
//! 繪師重交 `seeds.png` 之後這個模組就該刪掉。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::binarize;

/// 色點目標面積。`MIN_SEED_AREA` 是 64，取兩倍留餘裕——反推的色標不必省。
const DOT_AREA: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub seeds: Vec<u8>,
    /// 點出色標的封閉區數。
    pub seeded: usize,
    /// 面積不足 `min_orphan_area`、刻意不點的碎片數（會被 `merge_small_orphans` 併掉）。
    pub skipped: usize,
}

pub fn seeds_from_reference(
    lineart: &[u8],
    reference: &[u8],
    width: u32,
    height: u32,
    line_threshold: u8,
    min_orphan_area: u32,
) -> Derived {
    let (w, h) = (width as usize, height as usize);
    let line = binarize::line_mask(lineart, line_threshold);
    let depth = depth_from_line(&line, w, h);

    let mut out = vec![0u8; w * h * 4];
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let mut blob: Vec<usize> = Vec::new();
    let (mut seeded, mut skipped) = (0, 0);

    for start in 0..w * h {
        if seen[start] || line[start] {
            continue;
        }
        blob.clear();
        seen[start] = true;
        stack.push(start);
        while let Some(p) = stack.pop() {
            blob.push(p);
            neighbors(p, w, h, |n| {
                if !seen[n] && !line[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            });
        }

        if (blob.len() as u32) < min_orphan_area {
            skipped += 1;
            continue;
        }
        seeded += 1;

        // 眾數色，平手取字典序小者——與 `seeds::describe` 同一條規則。
        let mut hist: HashMap<[u8; 3], u32> = HashMap::new();
        for &p in &blob {
            *hist
                .entry([reference[p * 4], reference[p * 4 + 1], reference[p * 4 + 2]])
                .or_default() += 1;
        }
        let color = hist
            .iter()
            .max_by_key(|(rgb, n)| (**n, std::cmp::Reverse(**rgb)))
            .map(|(rgb, _)| *rgb)
            .expect("blob 非空");

        // anchor 取**離線最遠**的像素，色點才不會貼著邊——凹形區域的重心會落在區外。
        let anchor = *blob
            .iter()
            .max_by_key(|&&p| (depth[p], std::cmp::Reverse(p)))
            .expect("blob 非空");
        paint_dot(&mut out, anchor, &blob, color, w, h);
    }

    Derived {
        seeds: out,
        seeded,
        skipped,
    }
}

/// 每個非線像素到最近線像素（或畫布外）的 4-連通距離。線像素本身是 0。
fn depth_from_line(line: &[bool], w: usize, h: usize) -> Vec<u32> {
    let mut depth = vec![u32::MAX; w * h];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for p in 0..w * h {
        let (x, y) = (p % w, p / w);
        // 畫布邊界也當成邊：背景區的 anchor 才不會貼在畫布角落。
        if line[p] || x == 0 || y == 0 || x + 1 == w || y + 1 == h {
            depth[p] = 0;
            queue.push_back(p);
        }
    }
    while let Some(p) = queue.pop_front() {
        let d = depth[p] + 1;
        neighbors(p, w, h, |n| {
            if depth[n] == u32::MAX {
                depth[n] = d;
                queue.push_back(n);
            }
        });
    }
    depth
}

/// 由 anchor 起在 blob 內 BFS 取 `DOT_AREA` 個像素。BFS 保證色點連通——
/// 斷開的話 `seeds::read` 會把它讀成兩個 seed，然後報 collision。
fn paint_dot(out: &mut [u8], anchor: usize, blob: &[usize], color: [u8; 3], w: usize, h: usize) {
    let inside: HashSet<usize> = blob.iter().copied().collect();
    let mut taken = HashSet::from([anchor]);
    let mut queue = VecDeque::from([anchor]);
    while let Some(p) = queue.pop_front() {
        out[p * 4..p * 4 + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        if taken.len() >= DOT_AREA {
            continue;
        }
        neighbors(p, w, h, |n| {
            if taken.len() < DOT_AREA && inside.contains(&n) && taken.insert(n) {
                queue.push_back(n);
            }
        });
    }
}

fn neighbors(p: usize, w: usize, h: usize, mut f: impl FnMut(usize)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeds;

    /// 線稿：`#` 是線。用字串排版建圖，讓測試看得出形狀。
    fn build(rows: &[&str]) -> (Vec<u8>, u32, u32) {
        let h = rows.len() as u32;
        let w = rows[0].len() as u32;
        let mut lineart = Vec::new();
        for row in rows {
            for c in row.chars() {
                lineart.extend_from_slice(&[0, 0, 0, if c == '#' { 255 } else { 0 }]);
            }
        }
        (lineart, w, h)
    }

    fn flat(w: u32, h: u32, left: [u8; 3], right: [u8; 3]) -> Vec<u8> {
        let mut out = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let c = if x < w / 2 { left } else { right };
                out.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        out
    }

    /// 一條線切成兩個封閉區 → 兩個色標，各自取自己那半的 reference 色。
    #[test]
    fn each_closed_area_gets_one_seed_coloured_by_its_reference_mode() {
        let (lineart, w, h) = build(&[
            "....#....",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
        ]);
        let reference = flat(w, h, [200, 10, 10], [10, 20, 200]);
        let d = seeds_from_reference(&lineart, &reference, w, h, 128, 4);

        assert_eq!((d.seeded, d.skipped), (2, 0));
        let read = seeds::read(&d.seeds, w, h);
        assert_eq!(read.len(), 2, "色點斷開就會多出 seed");
        assert_eq!(read[0].color, [200, 10, 10]);
        assert_eq!(read[1].color, [10, 20, 200]);
    }

    /// 面積不足門檻的碎片不點——留給 `merge_small_orphans` 併掉。
    #[test]
    fn fragments_below_the_threshold_are_left_unseeded() {
        let (lineart, w, h) = build(&["..#.#....", "..#.#....", "..#.#...."]);
        let reference = flat(w, h, [1, 2, 3], [4, 5, 6]);
        // 中間那條寬 1、高 3 = 3px，門檻 4 → 跳過
        let d = seeds_from_reference(&lineart, &reference, w, h, 128, 4);
        assert_eq!(d.skipped, 1, "3px 的細條該被跳過");
    }

    /// anchor 取離線最遠處：色點不會貼著線，也不會落在凹形區域的外面。
    #[test]
    fn the_anchor_sits_deep_inside_not_against_the_line() {
        // 中央一個 7×7 的空腔，四周是線
        let (lineart, w, h) = build(&[
            "#########",
            "#.......#",
            "#.......#",
            "#.......#",
            "#.......#",
            "#.......#",
            "#.......#",
            "#.......#",
            "#########",
        ]);
        let reference = flat(w, h, [9, 9, 9], [9, 9, 9]);
        let d = seeds_from_reference(&lineart, &reference, w, h, 128, 4);
        let read = seeds::read(&d.seeds, w, h);
        assert_eq!(read.len(), 1);
        let (x, y) = read[0].anchor;
        assert!(
            (2..=6).contains(&x) && (2..=6).contains(&y),
            "anchor {:?} 貼著線了",
            read[0].anchor
        );
    }
}
