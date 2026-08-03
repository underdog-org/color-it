//! 合成素材產生器（`specs/baker-core-design.md §5`）。
//!
//! `torture-01` 與 negative fixture 是同一件事的正反面，共用同一組 zone 原語，所以
//! 它住在 baker 而不是 xtask。`xtask gen-torture` 與拒收測試都直接呼叫這裡。
//!
//! **產物必須逐位元決定性**：`torture-01` 進 LFS，重跑一次就多一份是不能接受的。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::image::{PngOptions, display_p3_profile, encode_rgba};

/// index 0 是分隔框，1..=15 給區塊內容用。高飽和、彼此差異大（`assets-spec §4.2 ②`）。
///
/// **不含 `#FF00FF`**：它是 `assets-spec §6.1` 縫隙檢查的保留色，baker 會以
/// `reserved-color` 拒收。原 `PALETTE[5]` 是洋紅，改為淡青 `[128,255,255]`——
/// 與其餘 15 色的最小歐氏距離 128，且與洋紅在視覺上完全分得開。
pub const PALETTE: [[u8; 3]; 16] = [
    [64, 64, 64],
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [128, 255, 255],
    [0, 255, 255],
    [255, 128, 0],
    [128, 0, 255],
    [0, 255, 128],
    [255, 0, 128],
    [128, 255, 0],
    [0, 128, 255],
    [200, 0, 0],
    [0, 160, 0],
    [160, 160, 0],
];

/// `reference[p] = PALETTE[PERM[flats_idx[p]]]`。`i*7+3 mod 16` 是 0..15 的雙射且無不動點：
/// 雙射保證每區仍是單一純色（§2.4 唯一的那條檢查），無不動點保證檔案位元 ≠ `flats.png`
/// ——能抓到「baker 偷懶直接比檔案而非比區域」的錯誤實作。
pub const PERM: [u8; 16] = {
    let mut p = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        p[i] = ((i * 7 + 3) % 16) as u8;
        i += 1;
    }
    p
};

const FRAME: u8 = 0;

/// 一張 palette index map。所有 zone 原語都作用在它上面。
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub idx: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            idx: vec![FRAME; (width * height) as usize],
        }
    }

    fn at(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.idx[self.at(x, y)]
    }

    pub fn set(&mut self, x: u32, y: u32, color: u8) {
        if x < self.width && y < self.height {
            let i = self.at(x, y);
            self.idx[i] = color;
        }
    }

    pub fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u8) {
        for yy in y..(y + h).min(self.height) {
            for xx in x..(x + w).min(self.width) {
                let i = self.at(xx, yy);
                self.idx[i] = color;
            }
        }
    }

    pub fn flats_rgba(&self) -> Vec<u8> {
        self.idx
            .iter()
            .flat_map(|&i| {
                let c = PALETTE[i as usize];
                [c[0], c[1], c[2], 255]
            })
            .collect()
    }

    pub fn reference_rgba(&self) -> Vec<u8> {
        self.idx
            .iter()
            .flat_map(|&i| {
                let c = PALETTE[PERM[i as usize] as usize];
                [c[0], c[1], c[2], 255]
            })
            .collect()
    }

    /// 線稿 = 區域邊界，1px 不透明黑、其餘真透明。
    pub fn lineart_rgba(&self) -> Vec<u8> {
        let (w, h) = (self.width, self.height);
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let here = self.get(x, y);
                let edge = (x + 1 < w && self.get(x + 1, y) != here)
                    || (y + 1 < h && self.get(x, y + 1) != here)
                    || (x > 0 && self.get(x - 1, y) != here)
                    || (y > 0 && self.get(x, y - 1) != here);
                if edge {
                    out[(self.at(x, y)) * 4 + 3] = 255;
                }
            }
        }
        out
    }
}

/// 一份可以直接寫成來源目錄的合成素材。
pub struct Asset {
    pub id: String,
    pub title: String,
    pub category: String,
    pub notes: String,
    pub width: u32,
    pub height: u32,
    pub flats: Vec<u8>,
    pub lineart: Vec<u8>,
    pub reference: Vec<u8>,
    pub shade: Option<Vec<u8>>,
    /// 只有 `display-p3` fixture 會用到。
    pub flats_icc: Option<Vec<u8>>,
    /// fixture 走 Fast：測試只在乎內容，不在乎檔案大小。
    pub compression: png::Compression,
}

impl Asset {
    pub fn write(&self, parent: &Path) -> Result<PathBuf> {
        let dir = parent.join(&self.id);
        std::fs::create_dir_all(&dir).with_context(|| format!("建立 {} 失敗", dir.display()))?;

        let opts = |icc: Option<Vec<u8>>| PngOptions {
            srgb: icc.is_none(),
            icc,
            compression: self.compression,
        };
        let write = |name: &str, rgba: &[u8], icc: Option<Vec<u8>>| -> Result<()> {
            let bytes = encode_rgba(rgba, self.width, self.height, opts(icc))?;
            std::fs::write(dir.join(name), bytes).with_context(|| format!("寫入 {name} 失敗"))
        };
        write(crate::source::FLATS, &self.flats, self.flats_icc.clone())?;
        write(crate::source::LINEART, &self.lineart, None)?;
        write(crate::source::REFERENCE, &self.reference, None)?;
        if let Some(shade) = &self.shade {
            write(crate::source::SHADE, shade, None)?;
        }

        let meta = serde_json::json!({
            "id": self.id,
            "title": self.title,
            "category": self.category,
            "notes": self.notes,
        });
        std::fs::write(
            dir.join(crate::source::META),
            format!("{}\n", serde_json::to_string_pretty(&meta)?),
        )
        .context("寫入 meta.json 失敗")?;
        Ok(dir)
    }
}

// ── torture-01 ────────────────────────────────────────────────────────

const T_W: u32 = 3072;
const T_H: u32 = 4096;
const ZONE: u32 = 1024;
const COLS: u32 = T_W / ZONE;
const ROWS: u32 = T_H / ZONE;

/// 分隔框寬度。有它才不必煩惱跨區塊的「相鄰同色」。
///
/// **8px 不是隨便選的**：`build_lineart` 在每條邊界兩側各畫 1px 線，降採樣後
/// `line_mask` 會從邊界往內吃掉 2 個輸出像素，而 `dilate` 再把它們判給鄰區。
/// 4px 的框在輸出只有 2px，兩側各被吃 2px 之後整條消失，直接撞上
/// `region-count-drift`。8px（輸出 4px，中間 2px 不在 `line_mask` 內）才活得下來。
const FRAME_W: u32 = 8;

/// 區塊內用色：永遠落在 1..=15，不會撞到分隔框。
fn c(base: u32, k: u32) -> u8 {
    (1 + (base + k) % 15) as u8
}

/// 每個 zone 的可用矩形——已經扣掉分隔框，zone 原語不必知道框的存在。
fn zone_rect(zx: u32, zy: u32) -> (u32, u32, u32, u32) {
    let half = FRAME_W / 2;
    let x0 = if zx == 0 { 0 } else { zx * ZONE + half };
    let x1 = if zx == COLS - 1 {
        T_W
    } else {
        (zx + 1) * ZONE - half
    };
    let y0 = if zy == 0 { 0 } else { zy * ZONE + half };
    let y1 = if zy == ROWS - 1 {
        T_H
    } else {
        (zy + 1) * ZONE - half
    };
    (x0, y0, x1 - x0, y1 - y0)
}

/// 均勻網格。餘數併進最後一格，所以每一格都 ≥ `cell`——切短的格子會掉到
/// `tiny-region` 門檻以下，製造警告牆。
fn zone_grid(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32, cell: u32) {
    let (x0, y0, w, h) = r;
    let (nx, ny) = (w / cell, h / cell);
    for cy in 0..ny {
        for cx in 0..nx {
            let cw = if cx + 1 == nx { w - cx * cell } else { cell };
            let ch = if cy + 1 == ny { h - cy * cell } else { cell };
            // 3 與 5 都不是 15 的因數 → 任兩個 4-鄰的格子必不同色
            let color = (1 + (3 * cx + 5 * cy + base) % 15) as u8;
            g.rect(x0 + cx * cell, y0 + cy * cell, cw, ch, color);
        }
    }
}

/// 等寬長條。相鄰必不同色，同一個顏色在不相鄰處反覆出現——正是
/// `assets-spec §4.2 ④` 允許的重用。
fn zone_stripes(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32, thick: u32, vertical: bool) {
    let (x0, y0, w, h) = r;
    let span = if vertical { w } else { h };
    let n = span / thick;
    for i in 0..n {
        let (off, len) = (i * thick, if i + 1 == n { span - i * thick } else { thick });
        if vertical {
            g.rect(x0 + off, y0, len, h, c(base, i));
        } else {
            g.rect(x0, y0 + off, w, len, c(base, i));
        }
    }
}

/// 同心方環，環厚在 8/12/16 之間循環——細長區域的極端，但每一環都活得過
/// 降採樣＋膨脹（見 `FRAME_W` 的說明）。
fn zone_rings(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32) {
    let (x0, y0, w, h) = r;
    let mut inset = 0u32;
    let mut i = 0u32;
    loop {
        let (side_w, side_h) = (w - 2 * inset, h - 2 * inset);
        if side_w <= 96 || side_h <= 96 {
            break;
        }
        g.rect(x0 + inset, y0 + inset, side_w, side_h, c(base, i));
        inset += [8, 12, 16][(i % 3) as usize];
        i += 1;
    }
}

/// 放射狀楔形：中心一個區域就有 24 個鄰居。內圈半徑 96 讓每個楔形在最窄處
/// 仍有 2π·96/24 ≈ 25px。
fn zone_pie(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32) {
    const WEDGES: f64 = 24.0;
    const INNER: f64 = 96.0;
    let (x0, y0, w, h) = r;
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let color = if dx.hypot(dy) < INNER {
                c(base, 3)
            } else {
                let t = dy.atan2(dx) + std::f64::consts::PI;
                let wedge = ((t / std::f64::consts::TAU) * WEDGES) as u32;
                c(base, wedge.min(WEDGES as u32 - 1) % 3)
            };
            g.set(x0 + x, y0 + y, color);
        }
    }
}

/// 阿基米德螺旋：通道是一條極長的細區域，對 tile 化的渲染與 flood fill 都是最壞情況。
/// 圈距 32、壁厚 8；中心 64px 另給一色，避開螺旋在原點的退化。
fn zone_spiral(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32) {
    let (x0, y0, w, h) = r;
    let (wall, chan, core) = (c(base, 0), c(base, 1), c(base, 2));
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let a = 32.0 / std::f64::consts::TAU;
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let radius = dx.hypot(dy);
            if radius < 64.0 {
                g.set(x0 + x, y0 + y, core);
                continue;
            }
            let theta = dy.atan2(dx) + std::f64::consts::PI;
            let k = (radius / a - theta) / std::f64::consts::TAU;
            let dist = (k - k.round()).abs() * a * std::f64::consts::TAU;
            g.set(x0 + x, y0 + y, if dist < 4.0 { wall } else { chan });
        }
    }
}

/// 大格棋盤：只用兩色，同色僅在對角相接——4-連通下是兩個獨立區域，
/// 8-連通下會被誤併。連通性語意由這一塊定住（§2.1）。
fn zone_reuse(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32, cell: u32) {
    let (x0, y0, w, h) = r;
    let (nx, ny) = (w / cell, h / cell);
    for cy in 0..ny {
        for cx in 0..nx {
            let cw = if cx + 1 == nx { w - cx * cell } else { cell };
            let ch = if cy + 1 == ny { h - cy * cell } else { cell };
            g.rect(
                x0 + cx * cell,
                y0 + cy * cell,
                cw,
                ch,
                c(base, (cx + cy) % 2),
            );
        }
    }
}

/// 極端長寬比的孤立碎片。最短邊一律 ≥8px，輸出面積最小 256px——
/// 是壓力素材，不是警告產生器。
fn zone_fragments(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32) {
    const SHAPES: [(u32, u32); 16] = [
        (200, 8),
        (8, 200),
        (32, 32),
        (64, 16),
        (16, 64),
        (128, 8),
        (8, 128),
        (40, 32),
        (48, 24),
        (24, 48),
        (96, 12),
        (12, 96),
        (64, 32),
        (32, 64),
        (160, 8),
        (8, 160),
    ];
    let (x0, y0, w, h) = r;
    g.rect(x0, y0, w, h, c(base, 0));
    for (i, (sw, sh)) in SHAPES.iter().enumerate() {
        let i = i as u32;
        g.rect(
            x0 + 40 + (i % 4) * 240,
            y0 + 40 + (i / 4) * 240,
            *sw,
            *sh,
            c(base, 1 + i % 3),
        );
    }
}

/// 貼齊畫布邊與角的區域——邊界處理的 off-by-one 都會在這裡現形。
/// 這一塊排在畫布左下角，特徵壓在**真正的畫布邊**上。
fn zone_edges(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32) {
    let (x0, y0, w, h) = r;
    g.rect(x0, y0, w, h, c(base, 0));
    let bottom = y0 + h;
    g.rect(x0, bottom - 32, 32, 32, c(base, 1)); // 畫布左下角
    g.rect(x0, y0 + 128, 8, 512, c(base, 2)); // 貼左緣
    g.rect(x0 + 200, bottom - 8, 512, 8, c(base, 3)); // 貼下緣
    g.rect(x0 + 800, bottom - 512, 8, 512, c(base, 4)); // 由內觸底
}

fn torture_canvas() -> Canvas {
    let mut g = Canvas::new(T_W, T_H);
    for zy in 0..ROWS {
        for zx in 0..COLS {
            let r = zone_rect(zx, zy);
            let base = zx * 5 + zy * 3;
            match (zx, zy) {
                (0, 0) | (1, 0) | (2, 0) | (0, 1) => zone_grid(&mut g, r, base, 32),
                (1, 1) => zone_stripes(&mut g, r, base, 8, true),
                (2, 1) => zone_stripes(&mut g, r, base, 8, false),
                (0, 2) => zone_rings(&mut g, r, base),
                (1, 2) => zone_pie(&mut g, r, base),
                (2, 2) => zone_spiral(&mut g, r, base),
                (0, 3) => zone_edges(&mut g, r, base),
                (1, 3) => zone_reuse(&mut g, r, base, 128),
                _ => zone_fragments(&mut g, r, base),
            }
        }
    }
    // 分隔框畫在區塊之間，不畫在畫布外緣——zone_edges 要真的碰到畫布邊。
    let half = FRAME_W / 2;
    for zx in 1..COLS {
        g.rect(zx * ZONE - half, 0, FRAME_W, T_H, FRAME);
    }
    for zy in 1..ROWS {
        g.rect(0, zy * ZONE - half, T_W, FRAME_W, FRAME);
    }
    g
}

pub const TORTURE_NOTES: &str = "由 `cargo xtask gen-torture` 決定性產生，不是繪師交付。\
12 個壓力區塊（密集格線、細長條、同心環、放射楔形、螺旋、對角同色重用、極端長寬比碎片、貼畫布邊特徵），\
所有特徵最短邊 ≥8px 且對齊偶數邊界，降採樣＋膨脹後仍存活。3:4 且無 shade，跑 has_shade = false 那條路徑。\
category 取 mandala 只因幾何圖形最接近，不代表難度分級。";

pub fn torture_01() -> Asset {
    let canvas = torture_canvas();
    Asset {
        id: "torture-01".to_owned(),
        title: "Torture Test 01".to_owned(),
        category: "mandala".to_owned(),
        notes: TORTURE_NOTES.to_owned(),
        width: T_W,
        height: T_H,
        flats: canvas.flats_rgba(),
        lineart: canvas.lineart_rgba(),
        reference: canvas.reference_rgba(),
        shade: None,
        flats_icc: None,
        // 進 LFS，壓縮率比產生速度重要。
        compression: png::Compression::High,
    }
}

/// `cargo xtask gen-torture` 的實作。
pub fn write_torture(repo_root: &Path) -> Result<PathBuf> {
    torture_01().write(&repo_root.join("assets/source"))
}

// ── 合格的小型素材（端到端測試用）───────────────────────────────────

/// 一張合規的合成素材：粗網格 ＋ 邊界線稿 ＋ PERM 配色。
///
/// CI 的端到端跑的是它而不是 LFS 素材（§6）。
pub fn valid(id: &str, width: u32, height: u32, cell: u32, has_shade: bool) -> Asset {
    let mut canvas = Canvas::new(width, height);
    zone_grid(&mut canvas, (0, 0, width, height), 0, cell);
    let shade = has_shade.then(|| {
        // 由上而下的柔和漸層，luma 一律 ≥ 60（`assets-spec §4.4`）。
        let mut buf = vec![255u8; (width * height * 4) as usize];
        for y in 0..height {
            let v = 255 - (y * 100 / height) as u8;
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                buf[i..i + 3].copy_from_slice(&[v, v, v]);
            }
        }
        buf
    });
    Asset {
        id: id.to_owned(),
        title: format!("Synthetic {id}"),
        category: "mandala".to_owned(),
        notes: "baker::synth 產生的合成素材，不是繪師交付。".to_owned(),
        width,
        height,
        flats: canvas.flats_rgba(),
        lineart: canvas.lineart_rgba(),
        reference: canvas.reference_rgba(),
        shade,
        flats_icc: None,
        compression: png::Compression::Fast,
    }
}

// ── negative fixture（§5.2：放的是生成器程式碼，不是 PNG）────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Negative {
    /// 未指派像素（`flats` alpha < 255）。
    Gap,
    /// 在某一塊裡塗第二個顏色——不是相鄰區同色，那是合法的。
    RefMismatch,
    /// 帶 Display P3 描述檔。
    DisplayP3,
    /// 開了抗鋸齒的 `flats`：邊界上生出一堆只佔零星像素的混色。
    Antialiased,
    /// 1px 特徵，降採樣後整批消失。
    Vanishing1px,
}

pub struct Fixture {
    pub asset: Asset,
    /// 預期的 `code`。
    pub expect: &'static str,
    /// 刻意植入的母帶座標。拒收測試斷言回報的座標**落在這裡面**——
    /// 只斷言「失敗」會讓任何理由的失敗都變綠燈（§6）。
    pub planted: Vec<(u32, u32)>,
}

/// fixture 必須是完整 4096 級尺寸，否則會先撞到「長邊 4096」檢查，測不到想測的那條。
const F_W: u32 = 3072;
const F_H: u32 = 4096;

pub fn negative(kind: Negative) -> Fixture {
    let id = match kind {
        Negative::Gap => "fixture-gap",
        Negative::RefMismatch => "fixture-ref-mismatch",
        Negative::DisplayP3 => "fixture-display-p3",
        Negative::Antialiased => "fixture-antialiased",
        Negative::Vanishing1px => "fixture-vanishing-1px",
    };
    let mut canvas = Canvas::new(F_W, F_H);
    zone_grid(&mut canvas, (0, 0, F_W, F_H), 0, 256);
    let mut asset = Asset {
        id: id.to_owned(),
        title: format!("Fixture {id}"),
        category: "mandala".to_owned(),
        notes: "baker::synth::negative 產生的預期拒收素材。".to_owned(),
        width: F_W,
        height: F_H,
        flats: canvas.flats_rgba(),
        lineart: canvas.lineart_rgba(),
        reference: canvas.reference_rgba(),
        shade: None,
        flats_icc: None,
        compression: png::Compression::Fast,
    };
    let px = |x: u32, y: u32| ((y * F_W + x) * 4) as usize;

    let (expect, planted) = match kind {
        Negative::Gap => {
            let spots = [(1234, 2345), (2000, 100), (7, 4090)];
            for (x, y) in spots {
                asset.flats[px(x, y) + 3] = 0;
            }
            (crate::report::code::UNASSIGNED_PIXEL, spots.to_vec())
        }
        Negative::RefMismatch => {
            // 在單一區域內部塗第二個顏色。raster order 的第一個相異像素就是左上角。
            let (bx, by) = (600, 900);
            for y in by..by + 16 {
                for x in bx..bx + 16 {
                    asset.reference[px(x, y)..px(x, y) + 3].copy_from_slice(&[1, 2, 3]);
                }
            }
            (crate::report::code::REF_MISMATCH, vec![(bx, by)])
        }
        Negative::DisplayP3 => {
            asset.flats_icc = Some(display_p3_profile());
            (crate::report::code::COLOR_SPACE, Vec::new())
        }
        Negative::Antialiased => {
            // 300 個各佔 1px 的混色，模擬沒關抗鋸齒的邊界。唯一色數仍遠低於 1024，
            // 所以撞到的是**實判**而不是快篩——這正是 §2.6 要證明的事。
            let mut planted = Vec::new();
            for i in 0..300u32 {
                let (x, y) = (1000 + i, 2000);
                asset.flats[px(x, y)..px(x, y) + 3].copy_from_slice(&[200, i as u8, 7]);
                planted.push((x, y));
            }
            (crate::report::code::TINY_COLOR_AREA, planted)
        }
        Negative::Vanishing1px => {
            // 300 個孤立單像素，全部同一個顏色（總面積 300 ≥ 100，過得了母帶實判），
            // 每個都落在 2×2 區塊裡的單一位置 → majority 是 1:3，必敗。
            let mut planted = Vec::new();
            for i in 0..300u32 {
                let (x, y) = (513 + 8 * i, 1001);
                canvas.set(x, y, 5);
                asset.flats[px(x, y)..px(x, y) + 3].copy_from_slice(&PALETTE[5]);
                let ref_color = PALETTE[PERM[5] as usize];
                asset.reference[px(x, y)..px(x, y) + 3].copy_from_slice(&ref_color);
                planted.push((x, y));
            }
            asset.lineart = canvas.lineart_rgba();
            (crate::report::code::REGION_COUNT_DRIFT, planted)
        }
    };

    Fixture {
        asset,
        expect,
        planted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_does_not_contain_the_reserved_color() {
        assert!(!PALETTE.contains(&crate::segment::RESERVED_COLOR));
    }

    #[test]
    fn perm_is_a_bijection_without_fixed_points() {
        let mut seen = [false; 16];
        for (i, &p) in PERM.iter().enumerate() {
            assert_ne!(
                p as usize, i,
                "PERM 有不動點，reference 會與 flats 位元相同"
            );
            assert!(!seen[p as usize]);
            seen[p as usize] = true;
        }
    }

    /// 分隔框畫在區塊之間，四個畫布邊必須留給 zone_edges。
    #[test]
    fn frame_does_not_touch_the_canvas_border() {
        let g = torture_canvas();
        assert_ne!(g.get(0, T_H - 1), FRAME);
        assert_ne!(g.get(0, 0), FRAME);
    }

    #[test]
    fn zone_grid_merges_the_remainder_into_the_last_cell() {
        let mut g = Canvas::new(100, 40);
        zone_grid(&mut g, (0, 0, 100, 40), 0, 32);
        // 100 / 32 = 3 → 最後一格寬 36，不是 4
        assert_eq!(g.get(99, 0), g.get(64, 0));
        assert_ne!(g.get(63, 0), g.get(64, 0));
    }

    #[test]
    fn torture_is_bit_identical_across_runs() {
        let a = torture_canvas();
        let b = torture_canvas();
        assert_eq!(a.idx, b.idx);
    }
}
