//! PNG 解碼 ＋ 色彩空間判定（`specs/baker-core-design.md §5.3`）。
//!
//! 判定順序：`iCCP` → `sRGB` chunk → `gAMA` + `cHRM` → 三者皆無。
//! **不能只看 iCCP 的名稱**——`kirby-demo-1` 的 `flats` / `reference` 帶的是泛用名
//! `ICC Profile`，照名稱判會誤退一張合格素材。所以解 profile 的 tag table，
//! 拿 `wtpt` 與 `rXYZ`/`gXYZ`/`bXYZ` 與 sRGB 比對。

use std::path::Path;

use anyhow::{Context, Result, bail};

/// 一張已解碼成 RGBA8 的母帶。
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGBA8，長度 = `width * height * 4`。
    pub rgba: Vec<u8>,
    pub color_space: ColorSpace,
}

impl Image {
    pub fn load(path: &Path) -> Result<Self> {
        let file =
            std::fs::File::open(path).with_context(|| format!("開啟 {} 失敗", path.display()))?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        // 8-bit 以外的位元深度與調色盤都先正規化掉，之後統一當 8-bit 處理。
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder
            .read_info()
            .with_context(|| format!("{} 不是合法的 PNG", path.display()))?;

        let color_space = ColorSpace::classify(reader.info());
        let info = reader.info();
        let (width, height) = (info.width, info.height);
        let color_type = info.color_type;
        if info.bit_depth != png::BitDepth::Eight {
            bail!(
                "{} 的位元深度是 {:?}，assets-spec §3 要求 8 bit／channel",
                path.display(),
                info.bit_depth
            );
        }

        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let frame = reader
            .next_frame(&mut buf)
            .with_context(|| format!("解碼 {} 失敗", path.display()))?;
        buf.truncate(frame.buffer_size());

        let rgba = to_rgba8(&buf, color_type).with_context(|| {
            format!(
                "{} 的色彩型別 {color_type:?} 無法轉成 RGBA8",
                path.display()
            )
        })?;

        Ok(Self {
            width,
            height,
            rgba,
            color_space,
        })
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y as usize * self.width as usize) + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
}

fn to_rgba8(buf: &[u8], color_type: png::ColorType) -> Result<Vec<u8>> {
    let expand =
        |chunk: usize, f: fn(&[u8]) -> [u8; 4]| buf.chunks_exact(chunk).flat_map(f).collect();
    Ok(match color_type {
        png::ColorType::Rgba => buf.to_vec(),
        png::ColorType::Rgb => expand(3, |p| [p[0], p[1], p[2], 255]),
        png::ColorType::Grayscale => expand(1, |p| [p[0], p[0], p[0], 255]),
        png::ColorType::GrayscaleAlpha => expand(2, |p| [p[0], p[0], p[0], p[1]]),
        png::ColorType::Indexed => bail!("調色盤 PNG 應已被 normalize_to_color8 展開"),
    })
}

/// 寫出 RGBA8 PNG。
///
/// 決定性：png crate 的 filter／deflate 參數不隨呼叫改變，同輸入同輸出。
pub fn encode_rgba(rgba: &[u8], width: u32, height: u32, opts: PngOptions) -> Result<Vec<u8>> {
    let mut info = png::Info::with_size(width, height);
    info.color_type = png::ColorType::Rgba;
    info.bit_depth = png::BitDepth::Eight;
    if let Some(icc) = opts.icc {
        info.icc_profile = Some(icc.into());
    }
    if opts.srgb {
        info.srgb = Some(png::SrgbRenderingIntent::Perceptual);
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::with_info(std::io::Cursor::new(&mut out), info)
        .context("建立 PNG encoder 失敗")?;
    encoder.set_compression(opts.compression);
    encoder
        .write_header()
        .context("寫 PNG header 失敗")?
        .write_image_data(rgba)
        .context("寫 PNG 影像資料失敗")?;
    Ok(out)
}

pub struct PngOptions {
    pub srgb: bool,
    pub icc: Option<Vec<u8>>,
    pub compression: png::Compression,
}

impl Default for PngOptions {
    fn default() -> Self {
        Self {
            srgb: true,
            icc: None,
            compression: png::Compression::Balanced,
        }
    }
}

// ── 色彩空間 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSpace {
    pub is_srgb: bool,
    /// 判定依據，會原樣進報告——被退件的繪師要看得出 baker 讀的是哪個 chunk。
    pub basis: String,
}

impl ColorSpace {
    fn ok(basis: impl Into<String>) -> Self {
        Self {
            is_srgb: true,
            basis: basis.into(),
        }
    }
    fn bad(basis: impl Into<String>) -> Self {
        Self {
            is_srgb: false,
            basis: basis.into(),
        }
    }

    pub fn classify(info: &png::Info<'_>) -> Self {
        // 1. iCCP —— 有 profile 就以 profile 的**內容**為準。
        if let Some(profile) = &info.icc_profile
            && let Some(colorants) = icc::colorants(profile)
        {
            let deltas = icc::max_delta(&colorants);
            return if deltas <= icc::TOLERANCE {
                Self::ok(format!("iCCP colorant 與 sRGB 相符（最大差 {deltas:.4}）"))
            } else {
                Self::bad(format!(
                    "iCCP 的 primaries／white point 不是 sRGB（最大差 {deltas:.4}，容差 {}）。\
                     導出時色彩描述檔請選 sRGB",
                    icc::TOLERANCE
                ))
            };
        }
        // iCCP 存在但解不出 colorant（非 matrix/TRC profile）時不直接拒收，
        // 往下走其餘訊號——寧可漏放也不要誤退一張合格素材。

        // 2. sRGB chunk —— PNG 規格宣告 sRGB 的正式方式。
        if info.srgb.is_some() {
            return Self::ok("sRGB chunk");
        }

        // 3. gAMA + cHRM。用 raw chunk 而非 source_*：後者會被 sRGB chunk 填上推導值。
        match (info.gama_chunk, info.chrm_chunk) {
            (Some(gama), Some(chrm)) => {
                let gamma = gama.into_value();
                let xy = [
                    (chrm.white.0.into_value(), chrm.white.1.into_value()),
                    (chrm.red.0.into_value(), chrm.red.1.into_value()),
                    (chrm.green.0.into_value(), chrm.green.1.into_value()),
                    (chrm.blue.0.into_value(), chrm.blue.1.into_value()),
                ];
                let xy_delta = SRGB_XY
                    .iter()
                    .zip(xy.iter())
                    .map(|(a, b)| (a.0 - b.0).abs().max((a.1 - b.1).abs()))
                    .fold(0.0f32, f32::max);
                let gamma_ok = (gamma - SRGB_GAMA).abs() <= 0.02;
                if gamma_ok && xy_delta <= 0.01 {
                    Self::ok(format!("gAMA {gamma:.5} + cHRM（最大差 {xy_delta:.4}）"))
                } else {
                    Self::bad(format!(
                        "gAMA {gamma:.5} + cHRM 不是 sRGB（primaries 最大差 {xy_delta:.4}）"
                    ))
                }
            }
            // 4. assets-spec §3 明寫「沒有嵌任何色彩描述檔也算通過」。
            _ => Self::ok("未嵌色彩描述檔（assets-spec §3 明列為通過）"),
        }
    }
}

/// sRGB 的 gAMA 值（1/2.2 的 PNG 慣用寫法 45455/100000）。
const SRGB_GAMA: f32 = 0.45455;
/// sRGB 的 white point 與 primaries，xy 座標，順序 white / red / green / blue。
const SRGB_XY: [(f32, f32); 4] = [
    (0.3127, 0.3290),
    (0.6400, 0.3300),
    (0.3000, 0.6000),
    (0.1500, 0.0600),
];

/// ICC profile 的最小解析——只取需要的四個 XYZType tag，不引第三方 crate。
mod icc {
    /// 容差。sRGB 與 Display P3 的 rXYZ.X 差 0.08，取 0.02 既擋得住 P3
    /// 也吃得下不同廠牌 sRGB profile 的末位差異。
    pub const TOLERANCE: f64 = 0.02;

    /// colorant 一律是 PCS（D50）相對值，所以這裡是 sRGB 經 Bradford 調適後的值。
    /// **真正的判準是這三個**——Display P3 的 rXYZ.X 是 0.5151，差 0.08。
    pub const SRGB_R: [f64; 3] = [0.436066, 0.222488, 0.013916];
    pub const SRGB_G: [f64; 3] = [0.385147, 0.716873, 0.097076];
    pub const SRGB_B: [f64; 3] = [0.143066, 0.060608, 0.714096];

    /// `wtpt` **不能拿來分辨 sRGB 與 Display P3**（兩者的實際白點都是 D65），
    /// 而且兩種寫法在真實檔案裡都會遇到：ICC v4 存 PCS 調適後的 D50，v2 慣例存實際白點 D65。
    /// `kirby-demo-1` 的四張圖存的都是 D65。所以這裡只用它擋掉「白點根本不是這兩個」的
    /// 怪 profile，不參與 sRGB／P3 的判別。
    pub const D50: [f64; 3] = [0.964203, 1.000000, 0.824905];
    pub const D65: [f64; 3] = [0.950455, 1.000000, 1.089050];

    pub struct Colorants {
        pub wtpt: Option<[f64; 3]>,
        pub r: [f64; 3],
        pub g: [f64; 3],
        pub b: [f64; 3],
    }

    /// `None` 代表這不是一個帶 RGB colorant 的 matrix/TRC profile，判定要往下一個訊號走。
    pub fn colorants(profile: &[u8]) -> Option<Colorants> {
        Some(Colorants {
            wtpt: xyz_tag(profile, b"wtpt"),
            r: xyz_tag(profile, b"rXYZ")?,
            g: xyz_tag(profile, b"gXYZ")?,
            b: xyz_tag(profile, b"bXYZ")?,
        })
    }

    pub fn max_delta(c: &Colorants) -> f64 {
        let cmp =
            |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0f64, f64::max);
        let mut worst = cmp(c.r, SRGB_R).max(cmp(c.g, SRGB_G)).max(cmp(c.b, SRGB_B));
        if let Some(wtpt) = c.wtpt {
            worst = worst.max(cmp(wtpt, D50).min(cmp(wtpt, D65)));
        }
        worst
    }

    /// header 128 bytes，接著 u32 tag count，再來是 `[sig][offset][size]` 的 tag table。
    fn xyz_tag(profile: &[u8], sig: &[u8; 4]) -> Option<[f64; 3]> {
        const HEADER: usize = 128;
        let count = be_u32(profile, HEADER)? as usize;
        for i in 0..count.min(256) {
            let entry = HEADER + 4 + i * 12;
            if profile.get(entry..entry + 4)? != sig {
                continue;
            }
            let offset = be_u32(profile, entry + 4)? as usize;
            let size = be_u32(profile, entry + 8)? as usize;
            // XYZType：'XYZ ' ‖ 4 bytes reserved ‖ 3 × s15Fixed16
            if size < 20 || profile.get(offset..offset + 4)? != b"XYZ " {
                return None;
            }
            return Some([
                s15fixed16(profile, offset + 8)?,
                s15fixed16(profile, offset + 12)?,
                s15fixed16(profile, offset + 16)?,
            ]);
        }
        None
    }

    fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    }

    fn s15fixed16(bytes: &[u8], at: usize) -> Option<f64> {
        Some(be_u32(bytes, at)? as i32 as f64 / 65536.0)
    }

    /// 產生一份最小的 matrix/TRC profile。測試與 negative fixture 共用
    /// （`baker-core-design §5.2`：fixture 是程式碼不是檔案）。
    pub fn synth_profile(wtpt: [f64; 3], r: [f64; 3], g: [f64; 3], b: [f64; 3]) -> Vec<u8> {
        let tags: [(&[u8; 4], [f64; 3]); 4] =
            [(b"wtpt", wtpt), (b"rXYZ", r), (b"gXYZ", g), (b"bXYZ", b)];
        let table_len = 4 + tags.len() * 12;
        let data_start = 128 + table_len;

        let mut out = vec![0u8; 128];
        out[12..16].copy_from_slice(b"mntr");
        out[16..20].copy_from_slice(b"RGB ");
        out[20..24].copy_from_slice(b"XYZ ");
        out[36..40].copy_from_slice(b"acsp");
        out.extend_from_slice(&(tags.len() as u32).to_be_bytes());
        for (i, (sig, _)) in tags.iter().enumerate() {
            out.extend_from_slice(*sig);
            out.extend_from_slice(&((data_start + i * 20) as u32).to_be_bytes());
            out.extend_from_slice(&20u32.to_be_bytes());
        }
        for (_, xyz) in tags {
            out.extend_from_slice(b"XYZ ");
            out.extend_from_slice(&[0u8; 4]);
            for v in xyz {
                out.extend_from_slice(&(((v * 65536.0).round()) as i32).to_be_bytes());
            }
        }
        let len = out.len() as u32;
        out[0..4].copy_from_slice(&len.to_be_bytes());
        out
    }

    /// 照 `kirby-demo-1` 的實況：v2 慣例，白點存 D65。
    pub fn srgb_profile() -> Vec<u8> {
        synth_profile(D65, SRGB_R, SRGB_G, SRGB_B)
    }

    /// Display P3，D50 調適後的 colorant。這是 §5.3 那條「一律拒收」要擋的東西。
    pub fn display_p3_profile() -> Vec<u8> {
        synth_profile(
            D65,
            [0.515121, 0.241196, -0.001053],
            [0.291977, 0.692245, 0.041885],
            [0.157104, 0.066574, 0.784073],
        )
    }
}

pub use icc::{display_p3_profile, srgb_profile};

#[cfg(test)]
mod tests {
    use super::*;

    fn info_with(f: impl FnOnce(&mut png::Info<'static>)) -> png::Info<'static> {
        let mut info = png::Info::with_size(4, 4);
        f(&mut info);
        info
    }

    #[test]
    fn generic_named_iccp_is_judged_by_content_not_name() {
        // kirby-demo-1 的 flats / reference：iCCP 名稱是泛用的 "ICC Profile"。
        let info = info_with(|i| i.icc_profile = Some(srgb_profile().into()));
        let verdict = ColorSpace::classify(&info);
        assert!(verdict.is_srgb, "{}", verdict.basis);
    }

    /// D50 與 D65 兩種 `wtpt` 寫法都要放行——差別是 ICC v4 與 v2 的慣例，
    /// 不是色域的差別。實測 `kirby-demo-1` 的四張圖存的都是 D65。
    #[test]
    fn both_d50_and_d65_white_points_pass() {
        for wtpt in [icc::D50, icc::D65] {
            let profile = icc::synth_profile(wtpt, icc::SRGB_R, icc::SRGB_G, icc::SRGB_B);
            let info = info_with(|i| i.icc_profile = Some(profile.into()));
            assert!(ColorSpace::classify(&info).is_srgb, "{wtpt:?} 應該通過");
        }
    }

    /// 但白點真的離譜時仍要擋下。
    #[test]
    fn an_exotic_white_point_is_still_rejected() {
        let profile = icc::synth_profile([0.5, 1.0, 0.5], icc::SRGB_R, icc::SRGB_G, icc::SRGB_B);
        let info = info_with(|i| i.icc_profile = Some(profile.into()));
        assert!(!ColorSpace::classify(&info).is_srgb);
    }

    #[test]
    fn display_p3_iccp_is_rejected() {
        let info = info_with(|i| i.icc_profile = Some(display_p3_profile().into()));
        let verdict = ColorSpace::classify(&info);
        assert!(!verdict.is_srgb);
        assert!(verdict.basis.contains("sRGB"));
    }

    #[test]
    fn srgb_chunk_alone_passes() {
        // torture-01 就只有這個 chunk。
        let info = info_with(|i| i.srgb = Some(png::SrgbRenderingIntent::Perceptual));
        assert!(ColorSpace::classify(&info).is_srgb);
    }

    #[test]
    fn gama_plus_chrm_alone_passes() {
        let info = info_with(|i| {
            i.gama_chunk = Some(png::ScaledFloat::new(SRGB_GAMA));
            i.chrm_chunk = Some(png::SourceChromaticities::new(
                SRGB_XY[0], SRGB_XY[1], SRGB_XY[2], SRGB_XY[3],
            ));
        });
        assert!(ColorSpace::classify(&info).is_srgb);
    }

    #[test]
    fn wide_gamut_chrm_is_rejected() {
        let info = info_with(|i| {
            i.gama_chunk = Some(png::ScaledFloat::new(SRGB_GAMA));
            i.chrm_chunk = Some(png::SourceChromaticities::new(
                (0.3127, 0.3290),
                (0.680, 0.320), // Display P3 的 red
                (0.265, 0.690),
                (0.150, 0.060),
            ));
        });
        assert!(!ColorSpace::classify(&info).is_srgb);
    }

    #[test]
    fn no_color_chunk_at_all_passes() {
        assert!(ColorSpace::classify(&info_with(|_| {})).is_srgb);
    }

    /// iCCP 在但解不出 colorant 時要往下一個訊號走，不能直接拒收。
    #[test]
    fn unparsable_iccp_falls_through_to_srgb_chunk() {
        let info = info_with(|i| {
            i.icc_profile = Some(vec![0u8; 132].into());
            i.srgb = Some(png::SrgbRenderingIntent::Perceptual);
        });
        assert!(ColorSpace::classify(&info).is_srgb);
    }
}
