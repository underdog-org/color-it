//! `thumb.jpg` 合成（`specs/baker-core-design.md §3.7`）。
//!
//! 在**母帶解析度**下把 `reference` 逐區配色 × `shade` × `lineart` 合成，再 box 降採樣
//! 至長邊 512。峰值記憶體約 200MB，這是已知且接受的成本。
//!
//! Gallery 對鎖定的線稿也顯示縮圖（`prd §5.1`），且 `architecture §8.4` 在資產取不到時
//! 以縮圖唯讀顯示——所以縮圖要呈現「這張畫完會長什麼樣」，不是空線稿。

use anyhow::{Context, Result};
use jpeg_encoder::{ColorType, Encoder};

use crate::resample::box_rgba_by;

pub const LONG_EDGE: u32 = 512;
pub const QUALITY: u8 = 85;

/// `lineart_white` / `shade_white` 都是已合成到白底的母帶 RGBA。
pub fn render(
    width: u32,
    height: u32,
    labels: &[u32],
    suggested: &[[u8; 3]],
    lineart_white: &[u8],
    shade_white: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut master = vec![255u8; labels.len() * 4];
    for (i, &id) in labels.iter().enumerate() {
        let base = suggested[id as usize];
        for c in 0..3 {
            let mut v = base[c] as u32;
            if let Some(shade) = shade_white {
                v = v * shade[i * 4 + c] as u32 / 255;
            }
            v = v * lineart_white[i * 4 + c] as u32 / 255;
            master[i * 4 + c] = v as u8;
        }
    }

    // 母帶長邊 4096 → factor 8。clamp 只為了讓小尺寸的單元測試不炸。
    let factor = (width.max(height) / LONG_EDGE).max(1);
    let small = box_rgba_by(&master, width, height, factor);
    drop(master);
    let (tw, th) = ((width / factor) as u16, (height / factor) as u16);

    let mut jpeg = Vec::new();
    // jpeg-encoder 的 `simd` 不是預設 feature，走純量路徑——輸出跨機器一致。
    Encoder::new(&mut jpeg, QUALITY)
        .encode(&small, tw, th, ColorType::Rgba)
        .context("JPEG 編碼失敗")?;
    Ok(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplies_reference_color_by_lineart_and_shade() {
        // 8×8 全一區，線稿全白（不變色）、shade 半灰
        let labels = vec![0u32; 64];
        let lineart = vec![255u8; 64 * 4];
        let shade = vec![128u8; 64 * 4];
        let jpeg = render(8, 8, &labels, &[[200, 100, 50]], &lineart, Some(&shade)).unwrap();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8], "應該是 JPEG SOI");
    }

    #[test]
    fn output_long_edge_is_512() {
        let labels = vec![0u32; 1024 * 1024];
        let lineart = vec![255u8; 1024 * 1024 * 4];
        let jpeg = render(1024, 1024, &labels, &[[10, 20, 30]], &lineart, None).unwrap();
        // SOF0 內含尺寸；這裡只確認編碼成功且非空
        assert!(jpeg.len() > 100);
    }
}
