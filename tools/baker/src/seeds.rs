//! 色標圖解析（`specs/baker-seeds.md §2.2`）。

use std::cmp::Reverse;
use std::collections::HashMap;

/// 色標的 `alpha == 255` 面積下限（母帶）。低於此值取不出可靠的眾數色。
pub const MIN_SEED_AREA: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed {
    /// flood fill 的起點。色標塊的重心；重心若落在塊外（凹形色標）則取塊內最接近重心者。
    pub anchor: (u32, u32),
    /// 塊內 `alpha == 255` 像素的眾數色。
    pub color: [u8; 3],
    /// 塊內 `alpha == 255` 的像素數。
    pub solid_area: u32,
}

/// `alpha > 0` 的 4-連通塊即一個色標。
///
/// 回傳依 `anchor` 的 raster order 排序——region id 因此與繪師點色標的先後無關。
pub fn read(rgba: &[u8], width: u32, height: u32) -> Vec<Seed> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut blob: Vec<usize> = Vec::new();

    for start in 0..w * h {
        if seen[start] || rgba[start * 4 + 3] == 0 {
            continue;
        }
        blob.clear();
        seen[start] = true;
        stack.push(start);
        while let Some(p) = stack.pop() {
            blob.push(p);
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<usize>| {
                if !seen[n] && rgba[n * 4 + 3] > 0 {
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
        out.push(describe(&blob, rgba, w));
    }

    out.sort_by_key(|s| (s.anchor.1, s.anchor.0));
    out
}

fn describe(blob: &[usize], rgba: &[u8], w: usize) -> Seed {
    let mut hist: HashMap<[u8; 3], u32> = HashMap::new();
    let (mut sx, mut sy) = (0u64, 0u64);
    for &p in blob {
        sx += (p % w) as u64;
        sy += (p / w) as u64;
        if rgba[p * 4 + 3] == 255 {
            let rgb = [rgba[p * 4], rgba[p * 4 + 1], rgba[p * 4 + 2]];
            *hist.entry(rgb).or_default() += 1;
        }
    }
    let solid_area: u32 = hist.values().sum();
    // 平手取字典序較小的顏色——見 `a_tie_in_the_histogram_...` 測試。
    let color = hist
        .iter()
        .max_by_key(|(rgb, n)| (**n, Reverse(**rgb)))
        .map(|(rgb, _)| *rgb)
        .unwrap_or([0, 0, 0]);

    let n = blob.len() as u64;
    let (cx, cy) = ((sx / n) as i64, (sy / n) as i64);
    let anchor = blob
        .iter()
        .map(|&p| ((p % w) as u32, (p / w) as u32))
        .min_by_key(|&(x, y)| {
            let (dx, dy) = (x as i64 - cx, y as i64 - cy);
            (dx * dx + dy * dy, y, x)
        })
        .expect("blob 至少有一個像素");

    Seed {
        anchor,
        color,
        solid_area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(r, g, b, a)` 的扁平陣列建圖。
    fn img(px: &[[u8; 4]]) -> Vec<u8> {
        px.iter().flatten().copied().collect()
    }

    const T: [u8; 4] = [0, 0, 0, 0];
    const R: [u8; 4] = [255, 0, 0, 255];
    const G: [u8; 4] = [0, 255, 0, 255];

    /// 兩個分離的色標 → 兩個 seed，依 anchor 的 raster order 排序（§3.1 ③）。
    #[test]
    fn separate_dots_become_separate_seeds_in_raster_order() {
        // 3x3：左上一點綠、右下一點紅
        let rgba = img(&[G, T, T, T, T, T, T, T, R]);
        let seeds = read(&rgba, 3, 3);
        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[0].anchor, (0, 0));
        assert_eq!(seeds[0].color, [0, 255, 0]);
        assert_eq!(seeds[1].anchor, (2, 2));
        assert_eq!(seeds[1].color, [255, 0, 0]);
    }

    /// **抗鋸齒痛點消失的機制**：半透明的過渡色不計入眾數，也不計入 solid_area。
    #[test]
    fn antialiased_fringe_does_not_affect_the_mode_colour() {
        // 1x4：三個實心紅 ＋ 一個半透明的髒色
        let dirty = [200, 40, 40, 128];
        let rgba = img(&[R, R, R, dirty]);
        let seeds = read(&rgba, 4, 1);
        assert_eq!(seeds.len(), 1, "半透明像素與實心像素相連，是同一個色標");
        assert_eq!(seeds[0].color, [255, 0, 0]);
        assert_eq!(seeds[0].solid_area, 3);
    }

    /// 凹形色標（C 形）的重心落在塊外。anchor 直接用重心的話 flood fill
    /// 會從色標外面起跑，整個區域判定就錯了。
    #[test]
    fn anchor_of_a_concave_dot_stays_inside_the_dot() {
        // 5x3 的 C 形：右上與右下缺角，重心 (2,1) 正好落在洞裡
        let rgba = img(&[
            R, R, R, R, R, //
            R, T, T, T, T, //
            R, R, R, R, R,
        ]);
        let seeds = read(&rgba, 5, 3);
        assert_eq!(seeds.len(), 1);
        let (x, y) = seeds[0].anchor;
        let i = (y * 5 + x) as usize;
        assert_eq!(rgba[i * 4 + 3], 255, "anchor ({x},{y}) 落在色標外");
    }

    #[test]
    fn a_tie_in_the_histogram_goes_to_the_lexicographically_smaller_colour() {
        let rgba = img(&[R, G]);
        let seeds = read(&rgba, 2, 1);
        assert_eq!(seeds[0].color, [0, 255, 0], "各 1 px 平手，取字典序小者");
    }

    #[test]
    fn a_blank_image_yields_no_seeds() {
        assert!(read(&img(&[T, T, T, T]), 2, 2).is_empty());
    }
}
