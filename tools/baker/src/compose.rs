//! `lineart` / `shade` 合成到白底（`architecture §9.2` 陷阱 0）。
//!
//! Composite pass 對這兩張是**純 RGB 相乘**，straight-alpha 的貼圖在透明處 RGB 通常是 0，
//! 直接相乘會把整張畫布乘成黑色。合成必須在**降採樣之前**——先合成再 box 降採樣，
//! 邊緣的抗鋸齒才是正確的。

/// straight-alpha RGBA → 合成到白底的 RGBA（alpha 一律 255）。
pub fn over_white(rgba: &[u8]) -> Vec<u8> {
    let mut out = vec![255u8; rgba.len()];
    for (dst, src) in out.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
        let a = src[3] as u32;
        for c in 0..3 {
            // src * a + 255 * (255 - a)，四捨五入的整數除法
            let v = src[c] as u32 * a + 255 * (255 - a);
            dst[c] = ((v + 127) / 255) as u8;
        }
    }
    out
}

/// Rec.709 luma。`shade` 的 `luma < 60` 判準用它（`assets-spec §4.4`）。
pub fn luma(rgb: [u8; 3]) -> u8 {
    let y = 0.2126 * rgb[0] as f32 + 0.7152 * rgb[1] as f32 + 0.0722 * rgb[2] as f32;
    y.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_becomes_white_opaque_is_untouched() {
        let src = [0, 0, 0, 0, 10, 20, 30, 255];
        assert_eq!(over_white(&src), vec![255, 255, 255, 255, 10, 20, 30, 255]);
    }

    #[test]
    fn half_transparent_black_lands_midway() {
        let out = over_white(&[0, 0, 0, 128]);
        assert_eq!(&out[..3], &[127, 127, 127]);
        assert_eq!(out[3], 255);
    }

    #[test]
    fn luma_of_white_and_black() {
        assert_eq!(luma([255, 255, 255]), 255);
        assert_eq!(luma([0, 0, 0]), 0);
    }
}
