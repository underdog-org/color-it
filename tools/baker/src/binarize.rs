//! 線稿二值化（`specs/baker-seeds.md §3`）。
//!
//! `lineart.png` 是透明底交付（`assets-spec §4.1`），所以 alpha 就是線的覆蓋度，
//! 不需要看 RGB。線稿的顏色不限純黑，看 luma 會把深褐色線稿判得比黑線稿淡。

/// `lineart.alpha ≥` 此值視為線。
pub const DEFAULT_LINE_THRESHOLD: u8 = 128;

/// 線像素佔比超過此值 → `line-coverage` 警告（§4.1）。
pub const MAX_LINE_RATIO: f32 = 0.35;

/// 每個像素一個 bool，長度 = `rgba.len() / 4`。
pub fn line_mask(rgba: &[u8], threshold: u8) -> Vec<bool> {
    rgba.chunks_exact(4).map(|px| px[3] >= threshold).collect()
}

pub fn line_ratio(mask: &[bool]) -> f32 {
    if mask.is_empty() {
        return 0.0;
    }
    mask.iter().filter(|&&b| b).count() as f32 / mask.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(alphas: &[u8]) -> Vec<u8> {
        alphas.iter().flat_map(|&a| [0, 0, 0, a]).collect()
    }

    /// 門檻是 `≥` 不是 `>`——差一個像素的線寬在 4096 母帶上會變成整條線的斷續。
    #[test]
    fn alpha_at_the_threshold_counts_as_line() {
        assert_eq!(line_mask(&rgba(&[127, 128]), 128), vec![false, true]);
    }

    #[test]
    fn a_fully_transparent_lineart_has_no_line_pixels() {
        assert_eq!(line_ratio(&line_mask(&rgba(&[0, 0, 0, 0]), 128)), 0.0);
    }

    /// 白底交付的線稿 alpha 全滿，整張都會被判成線。這是 `line-coverage`
    /// 警告存在的唯一理由——沒有它，繪師交錯格式時的症狀是「一個區域都認不出來」。
    #[test]
    fn an_opaque_lineart_trips_the_coverage_ceiling() {
        let ratio = line_ratio(&line_mask(&rgba(&[255, 255, 255, 255]), 128));
        assert!(ratio > MAX_LINE_RATIO, "佔比 {ratio}");
    }

    #[test]
    fn ratio_of_an_empty_mask_is_zero_not_nan() {
        assert_eq!(line_ratio(&[]), 0.0);
    }
}
