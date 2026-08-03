//! 區域向線稿下方膨脹（`specs/baker-core-design.md §2.5`）。
//!
//! ID map 是滿的、沒有洞，所以「膨脹」不可能是填洞，只能是**重新分配線稿覆蓋帶內的
//! 所有權**。它必須在降採樣之後：`lineart` 用 box、`flats` 用 majority，兩者在邊界的
//! 落點可能差半個像素，膨脹正是用來吸收這個誤差的（`architecture §9.2` 陷阱 2）。

/// 降採樣後的 lineart alpha ≥ 這個值就算被線稿覆蓋。
pub const LINE_ALPHA_THRESHOLD: u8 = 32;
/// 2 輪 4-鄰膨脹正好推進 2px。
pub const ROUNDS: usize = 2;

/// `ids` 就地更新。`line_alpha` 是**降採樣後**的線稿 alpha（每像素一個 byte）。
///
/// 每輪讀上一輪的快照、寫進新緩衝——in-place 會讓結果取決於掃描順序，違反決定性（§3.2）。
pub fn dilate_under_lineart(ids: &mut [u32], w: u32, h: u32, line_alpha: &[u8], areas: &[u32]) {
    let (w, h) = (w as usize, h as usize);
    let line_mask: Vec<bool> = line_alpha
        .iter()
        .map(|&a| a >= LINE_ALPHA_THRESHOLD)
        .collect();
    // resolved 每輪擴張。若候選來源固定為「非 line_mask」，第 2 輪的來源集合會與第 1 輪
    // 完全相同，等於空跑一輪，線稿帶內側第 2px 的像素永遠拿不到鄰居（§2.5）。
    let mut resolved: Vec<bool> = line_mask.iter().map(|&m| !m).collect();

    for _ in 0..ROUNDS {
        let snapshot_ids = ids.to_vec();
        let snapshot_resolved = resolved.clone();
        for p in 0..w * h {
            if !line_mask[p] || snapshot_resolved[p] {
                continue;
            }
            let (x, y) = (p % w, p / w);
            let mut best: Option<u32> = None;
            let consider = |n: usize, best: &mut Option<u32>| {
                if !snapshot_resolved[n] {
                    return;
                }
                let id = snapshot_ids[n];
                let better = match *best {
                    None => true,
                    Some(cur) => (areas[id as usize], id) < (areas[cur as usize], cur),
                };
                if better {
                    *best = Some(id);
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
            // 候選為空 → 保留降採樣 majority 給的原 ID，不做任何事。
            if let Some(id) = best {
                ids[p] = id;
                resolved[p] = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 5 格橫線，中間 3 格被線稿蓋住。兩側各推進 1px，中心保留原 ID。
    #[test]
    fn two_rounds_advance_two_pixels_from_each_side() {
        let mut ids = vec![0, 9, 9, 9, 1];
        let line = vec![0, 255, 255, 255, 0];
        // 面積：讓 0 比 1 小，中心格平手時 0 勝
        dilate_under_lineart(&mut ids, 5, 1, &line, &[10, 20, 0, 0, 0, 0, 0, 0, 0, 999]);
        assert_eq!(ids, vec![0, 0, 0, 1, 1]);
    }

    /// 這條是 §2.5 那個「每輪擴張的 resolved 是關鍵」的迴歸：
    /// 線稿帶寬 4px 時，若來源集合固定為非 line_mask，內側兩格永遠拿不到鄰居。
    #[test]
    fn resolved_set_grows_each_round() {
        let mut ids = vec![0, 9, 9, 9, 9, 1];
        let line = vec![0, 255, 255, 255, 255, 0];
        dilate_under_lineart(&mut ids, 6, 1, &line, &[10, 20, 0, 0, 0, 0, 0, 0, 0, 999]);
        assert_eq!(ids, vec![0, 0, 0, 1, 1, 1]);
    }

    /// 5 輪也吃不完的粗線：中心帶保留 majority 的原 ID，不引入第三種規則。
    #[test]
    fn pixels_beyond_reach_keep_their_original_id() {
        let mut ids = vec![0, 9, 9, 9, 9, 9, 9, 1];
        let line = vec![0, 255, 255, 255, 255, 255, 255, 0];
        dilate_under_lineart(&mut ids, 8, 1, &line, &[10, 20, 0, 0, 0, 0, 0, 0, 0, 999]);
        assert_eq!(ids, vec![0, 0, 0, 9, 9, 1, 1, 1]);
    }

    #[test]
    fn non_line_pixels_are_never_overwritten() {
        let mut ids = vec![5, 5, 5, 5];
        let line = vec![0, 0, 0, 0];
        dilate_under_lineart(&mut ids, 4, 1, &line, &[0, 0, 0, 0, 0, 1]);
        assert_eq!(ids, vec![5, 5, 5, 5]);
    }

    #[test]
    fn alpha_below_threshold_is_not_line() {
        let mut ids = vec![0, 9, 1];
        let line = vec![0, LINE_ALPHA_THRESHOLD - 1, 0];
        dilate_under_lineart(&mut ids, 3, 1, &line, &[1, 1, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(ids, vec![0, 9, 1]);
    }
}
