//! 降採樣：`flats` 用 majority（作用在 **ID map** 上）、`lineart` / `shade` 用 box。
//!
//! 套用同一套濾波器是這條管線最容易犯的錯（`architecture §9.2` 陷阱 1）。

/// 2×2 box filter，逐 channel 平均。`w`、`h` 必須是偶數（母帶尺寸保證如此）。
pub fn box_rgba(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    box_rgba_by(src, w, h, 2)
}

/// 整數倍 box filter。縮圖走 factor = 8（母帶長邊 4096 → 512）。
pub fn box_rgba_by(src: &[u8], w: u32, h: u32, factor: u32) -> Vec<u8> {
    let (w, h, f) = (w as usize, h as usize, factor as usize);
    let (ow, oh) = (w / f, h / f);
    let n = (f * f) as u32;
    let mut out = vec![0u8; ow * oh * 4];
    for oy in 0..oh {
        for ox in 0..ow {
            let mut acc = [0u32; 4];
            for dy in 0..f {
                let row = (oy * f + dy) * w + ox * f;
                for dx in 0..f {
                    let p = (row + dx) * 4;
                    for c in 0..4 {
                        acc[c] += src[p + c] as u32;
                    }
                }
            }
            let dst = (oy * ow + ox) * 4;
            for c in 0..4 {
                out[dst + c] = ((acc[c] + n / 2) / n) as u8;
            }
        }
    }
    out
}

/// 只降採樣 alpha channel。`dilate` 的 `line_mask` 要的是**降採樣後**的線稿覆蓋度，
/// 而合成到白底之後 alpha 全是 255，所以必須從原始 straight-alpha 取（§2.5）。
pub fn box_alpha(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let (ow, oh) = (w / 2, h / 2);
    let mut out = vec![0u8; ow * oh];
    for oy in 0..oh {
        for ox in 0..ow {
            let a = [
                src[((oy * 2) * w + ox * 2) * 4 + 3] as u32,
                src[((oy * 2) * w + ox * 2 + 1) * 4 + 3] as u32,
                src[((oy * 2 + 1) * w + ox * 2) * 4 + 3] as u32,
                src[((oy * 2 + 1) * w + ox * 2 + 1) * 4 + 3] as u32,
            ];
            out[oy * ow + ox] = ((a.iter().sum::<u32>() + 2) / 4) as u8;
        }
    }
    out
}

/// 2×2 majority，作用在 **region ID** 上。
///
/// 一個 2×2 區塊可能同時含 A(紅)、B(綠)、C(紅)，其中 A 與 C 不相鄰所以合法同色。
/// 對 RGB 取眾數會得到「紅」但無從得知是哪個區域——所以 majority 必須作用在 ID map 上（§2.1）。
///
/// 平手時取**母帶面積最小的區域**，面積再平手取 ID 最小（§2.2）：細區域是唯一會被吃掉
/// 的一方，讓它贏；大區域損失的是邊緣 ≤1px，而那一帶本來就在線稿底下。
pub fn majority_ids(labels: &[u32], w: u32, h: u32, areas: &[u32]) -> Vec<u32> {
    let (w, h) = (w as usize, h as usize);
    let (ow, oh) = (w / 2, h / 2);
    let mut out = vec![0u32; ow * oh];
    for oy in 0..oh {
        for ox in 0..ow {
            let q = [
                labels[(oy * 2) * w + ox * 2],
                labels[(oy * 2) * w + ox * 2 + 1],
                labels[(oy * 2 + 1) * w + ox * 2],
                labels[(oy * 2 + 1) * w + ox * 2 + 1],
            ];
            out[oy * ow + ox] = winner(&q, areas);
        }
    }
    out
}

fn winner(quad: &[u32; 4], areas: &[u32]) -> u32 {
    let mut best = quad[0];
    let mut best_votes = 0;
    for &candidate in quad {
        let votes = quad.iter().filter(|&&v| v == candidate).count();
        let better = match votes.cmp(&best_votes) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            // 平手：母帶面積小者贏，面積再平手取 ID 小者
            std::cmp::Ordering::Equal => {
                (areas[candidate as usize], candidate) < (areas[best as usize], best)
            }
        };
        if better {
            best = candidate;
            best_votes = votes;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_majority_wins() {
        // 3:1 → 大面積的 0 仍然贏，因為它票多
        let areas = [100, 1];
        assert_eq!(majority_ids(&[0, 0, 0, 1], 2, 2, &areas), vec![0]);
    }

    /// §2.2：2:2 平手時母帶面積小的贏——細區域是唯一會被吃掉的一方。
    #[test]
    fn tie_goes_to_the_smaller_master_region() {
        let areas = [10_000, 40];
        assert_eq!(majority_ids(&[0, 0, 1, 1], 2, 2, &areas), vec![1]);
        assert_eq!(majority_ids(&[1, 1, 0, 0], 2, 2, &areas), vec![1]);
    }

    #[test]
    fn tie_on_area_goes_to_the_smaller_id() {
        let areas = [40, 40];
        assert_eq!(majority_ids(&[1, 1, 0, 0], 2, 2, &areas), vec![0]);
    }

    /// 四個都不同（1:1:1:1）也是平手，同一條規則處理。
    #[test]
    fn four_way_tie_uses_the_same_rule() {
        let areas = [9, 8, 7, 6];
        assert_eq!(majority_ids(&[0, 1, 2, 3], 2, 2, &areas), vec![3]);
    }

    #[test]
    fn box_filter_averages_and_rounds_half_up() {
        let src = [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3];
        assert_eq!(box_rgba(&src, 2, 2), vec![2, 2, 2, 2]);
    }

    #[test]
    fn alpha_downsample_reads_alpha_not_rgb() {
        let src = [
            255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        assert_eq!(box_alpha(&src, 2, 2), vec![128]);
    }
}
