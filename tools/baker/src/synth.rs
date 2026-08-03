//! 合成素材產生器（`specs/baker-core-design.md §5`）。
//!
//! `torture-01` 與 negative fixture 是同一件事的正反面，共用同一組 zone 原語，所以
//! 它住在 baker 而不是 xtask。`xtask gen-torture` 與拒收測試都直接呼叫這裡。
//!
//! **產物必須逐位元決定性**：`torture-01` 進 LFS，重跑一次就多一份是不能接受的。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::image::{PngOptions, display_p3_profile, encode_rgba};

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

    /// 每個 idx 連通塊點一個色標（`baker-seeds.md §2.1`）。顏色取 `PERM` 映射後的
    /// palette——色標顏色**就是**建議色，PERM 讓它與 idx 本身不同，避免測試在
    /// 「剛好相等」的巧合下變綠。
    ///
    /// 色點由 anchor 起在「同塊 ∩ 非線」上 BFS 取 `DOT_AREA` 個像素：BFS 保證色點
    /// 連通（`seeds::read` 用 4-連通，斷開就會變成兩個 seed），非線保證 anchor 不壓線。
    /// 塊內非線像素不足時就畫多少算多少——那正是 `seed-too-small` 該報的情況。
    pub fn seeds_rgba(&self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let line = crate::binarize::line_mask(
            &self.lineart_rgba(),
            crate::binarize::DEFAULT_LINE_THRESHOLD,
        );
        let mut out = vec![0u8; w * h * 4];
        let mut seen = vec![false; w * h];
        let mut stack: Vec<usize> = Vec::new();
        let mut blob: Vec<usize> = Vec::new();

        for start in 0..w * h {
            if seen[start] {
                continue;
            }
            let target = self.idx[start];
            blob.clear();
            seen[start] = true;
            stack.push(start);
            while let Some(p) = stack.pop() {
                blob.push(p);
                let (x, y) = (p % w, p / w);
                let mut visit = |n: usize, stack: &mut Vec<usize>| {
                    if !seen[n] && self.idx[n] == target {
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
            paint_dot(&mut out, &blob, &line, self.idx[start], w, h);
        }
        out
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

/// 色點的目標面積。`MIN_SEED_AREA` 是 64，留一點餘裕，讓「合格素材」不會卡在門檻上。
const DOT_AREA: usize = 96;

fn paint_dot(out: &mut [u8], blob: &[usize], line: &[bool], idx: u8, w: usize, h: usize) {
    let free: std::collections::HashSet<usize> =
        blob.iter().copied().filter(|&p| !line[p]).collect();
    if free.is_empty() {
        return;
    }
    // anchor 取最接近塊重心的非線像素，平手取 raster order 前者。
    let n = blob.len() as i64;
    let cx = blob.iter().map(|&p| (p % w) as i64).sum::<i64>() / n;
    let cy = blob.iter().map(|&p| (p / w) as i64).sum::<i64>() / n;
    let anchor = *free
        .iter()
        .min_by_key(|&&p| {
            let (dx, dy) = ((p % w) as i64 - cx, (p / w) as i64 - cy);
            (dx * dx + dy * dy, p)
        })
        .expect("free 非空");

    let color = PALETTE[PERM[idx as usize] as usize];
    let mut queue = std::collections::VecDeque::from([anchor]);
    let mut taken = std::collections::HashSet::from([anchor]);
    while let Some(p) = queue.pop_front() {
        out[p * 4..p * 4 + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        if taken.len() >= DOT_AREA {
            continue;
        }
        let (x, y) = (p % w, p / w);
        let mut visit = |n: usize, queue: &mut std::collections::VecDeque<usize>| {
            if taken.len() < DOT_AREA && free.contains(&n) && taken.insert(n) {
                queue.push_back(n);
            }
        };
        if x > 0 {
            visit(p - 1, &mut queue);
        }
        if x + 1 < w {
            visit(p + 1, &mut queue);
        }
        if y > 0 {
            visit(p - w, &mut queue);
        }
        if y + 1 < h {
            visit(p + w, &mut queue);
        }
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
    pub lineart: Vec<u8>,
    pub seeds: Vec<u8>,
    pub shade: Option<Vec<u8>>,
    /// 只有 `display-p3` fixture 會用到。掛在 `seeds.png` 上。
    pub seeds_icc: Option<Vec<u8>>,
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
        write(crate::source::LINEART, &self.lineart, None)?;
        write(crate::source::SEEDS, &self.seeds, self.seeds_icc.clone())?;
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

/// 梳齒狀迴廊：只有兩個區域，但通道是一條長達數萬像素的細長蛇行路徑——
/// 對 tile 化的渲染、flood fill 與 RLE 都是最壞情況。
///
/// **本來是阿基米德螺旋，改成軸對齊的梳齒。** 螺旋的曲線邊界（以及為了收邊而加的
/// 內外圓）與 2×2 majority 的方格網互相走樣：每一圈與圓相切之處都會被切出幾十個
fn zone_serpentine(g: &mut Canvas, r: (u32, u32, u32, u32), base: u32) {
    const SPINE: u32 = 8;
    const TOOTH: u32 = 8;
    const PITCH: u32 = 32;
    const RETURN: u32 = 24;

    let (x0, y0, w, h) = r;
    let (wall, chan) = (c(base, 0), c(base, 1));
    g.rect(x0, y0, w, h, chan);
    g.rect(x0, y0, SPINE, h, wall);
    let teeth = (h - TOOTH) / PITCH;
    for i in 0..teeth {
        g.rect(
            x0 + SPINE,
            y0 + TOOTH + i * PITCH,
            w - SPINE - RETURN,
            TOOTH,
            wall,
        );
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
                (2, 2) => zone_serpentine(&mut g, r, base),
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
12 個壓力區塊（密集格線、細長條、同心環、放射楔形、梳齒迴廊、對角同色重用、極端長寬比碎片、貼畫布邊特徵），\
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
        lineart: canvas.lineart_rgba(),
        seeds: canvas.seeds_rgba(),
        shade: None,
        seeds_icc: None,
        // 進 LFS，壓縮率比產生速度重要。
        compression: png::Compression::High,
    }
}

/// 生成器與 committed 產物之間的守門檔。放在 `assets/source/**/*.json`，
/// 依 `.gitattributes` 不進 LFS，所以 CI 以 `lfs: false` checkout 也讀得到。
pub const LOCK_FILE: &str = "synth-lock.json";

/// 兩張圖的**原始 RGBA** 的正規化 hash。
///
/// 刻意不 hash PNG bytes：那會綁到 `png` crate 的 deflate 實作，換一次依賴版本
/// 就誤報。要守的是「改了生成器卻忘了重跑 `gen-torture`」，那是內容層的事。
pub fn torture_content_hash() -> String {
    let asset = torture_01();
    colorpack::hash::content_hash(&[
        (crate::source::LINEART, asset.lineart.as_slice()),
        (crate::source::SEEDS, asset.seeds.as_slice()),
    ])
}

/// `cargo xtask gen-torture` 的實作。
pub fn write_torture(repo_root: &Path) -> Result<PathBuf> {
    let asset = torture_01();
    let dir = asset.write(&repo_root.join("assets/source"))?;
    let lock = serde_json::json!({
        "note": format!(
            "由 `cargo xtask gen-torture` 產生。改了 baker::synth 卻忘了重跑，\
             tools/baker/tests/torture.rs 會失敗。hash 取自原始 RGBA，不是 PNG bytes。"
        ),
        "content_hash": torture_content_hash(),
    });
    std::fs::write(
        dir.join(LOCK_FILE),
        format!("{}\n", serde_json::to_string_pretty(&lock)?),
    )
    .context("寫入 synth-lock.json 失敗")?;
    Ok(dir)
}

// ── 合格的小型素材（端到端測試用）───────────────────────────────────

/// 一張合規的合成素材：粗網格 ＋ 邊界線稿 ＋ PERM 配色。
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
        lineart: canvas.lineart_rgba(),
        seeds: canvas.seeds_rgba(),
        shade,
        seeds_icc: None,
        compression: png::Compression::Fast,
    }
}

// ── negative fixture（§5.2：放的是生成器程式碼，不是 PNG）────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Negative {
    /// 線稿有缺口 → 兩個色標落進同一封閉區。
    SeedCollision,
    /// 整個封閉區沒有色標 → 繪師漏點。
    OrphanArea,
    /// 色標太小，取不出可靠的眾數色。
    SeedTooSmall,
    /// 色標壓在線上，flood fill 起不來。
    SeedOnLine,
    /// 白底交付的線稿：alpha 全滿，整張都判成線。
    LineCoverage,
    /// 帶 Display P3 描述檔。
    DisplayP3,
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

/// fixture 的格線邊長。12×16 = 192 個封閉區。
const F_CELL: u32 = 256;

fn cell_rect(cx: u32, cy: u32) -> (u32, u32, u32, u32) {
    (cx * F_CELL, cy * F_CELL, F_CELL, F_CELL)
}

pub fn negative(kind: Negative) -> Fixture {
    let id = match kind {
        Negative::SeedCollision => "fixture-seed-collision",
        Negative::OrphanArea => "fixture-orphan-area",
        Negative::SeedTooSmall => "fixture-seed-too-small",
        Negative::SeedOnLine => "fixture-seed-on-line",
        Negative::LineCoverage => "fixture-line-coverage",
        Negative::DisplayP3 => "fixture-display-p3",
    };
    let mut canvas = Canvas::new(F_W, F_H);
    zone_grid(&mut canvas, (0, 0, F_W, F_H), 0, F_CELL);
    let mut lineart = canvas.lineart_rgba();

    // 缺口要在畫色標**之前**開：`seeds_rgba` 用線稿決定色點畫在哪些非線像素上。
    if kind == Negative::SeedCollision {
        for y in 64..192u32 {
            for x in [F_CELL - 1, F_CELL] {
                lineart[((y * F_W + x) * 4 + 3) as usize] = 0;
            }
        }
    }
    if kind == Negative::LineCoverage {
        for px in lineart.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    let mut seeds = canvas.seeds_rgba();
    let mut asset = Asset {
        id: id.to_owned(),
        title: format!("Fixture {id}"),
        category: "mandala".to_owned(),
        notes: "baker::synth::negative 產生的預期拒收素材。".to_owned(),
        width: F_W,
        height: F_H,
        lineart,
        seeds: Vec::new(),
        shade: None,
        seeds_icc: None,
        compression: png::Compression::Fast,
    };

    /// 把一個 cell 內的色標整個擦掉。
    fn clear_cell(seeds: &mut [u8], (x0, y0, w, h): (u32, u32, u32, u32)) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                seeds[((y * F_W + x) * 4 + 3) as usize] = 0;
            }
        }
    }
    fn dot(seeds: &mut [u8], x0: u32, y0: u32, w: u32, h: u32) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let i = ((y * F_W + x) * 4) as usize;
                seeds[i..i + 4].copy_from_slice(&[7, 200, 90, 255]);
            }
        }
    }

    let (expect, planted) = match kind {
        Negative::SeedCollision => {
            // 缺口讓 cell(0,0) 與 cell(1,0) 連成一個封閉區，兩個色標撞在一起。
            let anchors = anchors_in(&seeds, &[cell_rect(0, 0), cell_rect(1, 0)]);
            (crate::report::code::SEED_COLLISION, anchors)
        }
        Negative::OrphanArea => {
            let rect = cell_rect(5, 5);
            clear_cell(&mut seeds, rect);
            // orphan 的 anchor 是該塊在 raster order 的第一個非線像素。
            let anchor = first_free(&asset.lineart, rect).expect("cell 內必有非線像素");
            (crate::report::code::ORPHAN_AREA, vec![anchor])
        }
        Negative::SeedTooSmall => {
            let (x0, y0, ..) = cell_rect(3, 3);
            clear_cell(&mut seeds, cell_rect(3, 3));
            dot(&mut seeds, x0 + 8, y0 + 8, 2, 2);
            // 2×2 的重心落在左上角那格。
            (crate::report::code::SEED_TOO_SMALL, vec![(x0 + 8, y0 + 8)])
        }
        Negative::SeedOnLine => {
            clear_cell(&mut seeds, cell_rect(7, 7));
            // cell(6,7) 與 cell(7,7) 的分界：兩欄都是線像素。
            let (x, y) = (7 * F_CELL - 1, 1900);
            dot(&mut seeds, x, y, 2, 1);
            (crate::report::code::SEED_ON_LINE, vec![(x, y)])
        }
        Negative::LineCoverage => (crate::report::code::LINE_COVERAGE, Vec::new()),
        Negative::DisplayP3 => {
            asset.seeds_icc = Some(display_p3_profile());
            (crate::report::code::COLOR_SPACE, Vec::new())
        }
    };

    asset.seeds = seeds;
    Fixture {
        asset,
        expect,
        planted,
    }
}

/// 落在任一 rect 內的色標 anchor，依 `seeds::read` 的順序。
fn anchors_in(seeds: &[u8], rects: &[(u32, u32, u32, u32)]) -> Vec<(u32, u32)> {
    crate::seeds::read(seeds, F_W, F_H)
        .into_iter()
        .map(|s| s.anchor)
        .filter(|&(x, y)| {
            rects
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h)
        })
        .collect()
}

/// rect 內 raster order 的第一個非線像素。
fn first_free(lineart: &[u8], (x0, y0, w, h): (u32, u32, u32, u32)) -> Option<(u32, u32)> {
    let threshold = crate::binarize::DEFAULT_LINE_THRESHOLD;
    (y0..y0 + h)
        .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
        .find(|&(x, y)| lineart[((y * F_W + x) * 4 + 3) as usize] < threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每個 idx 連通塊剛好一個色標，且色點不壓線、面積過得了 `MIN_SEED_AREA`。
    /// 這是「合格素材」的定義——`valid()` 產出的東西必須自己先成立。
    #[test]
    fn every_cell_gets_exactly_one_usable_seed() {
        let mut g = Canvas::new(128, 128);
        zone_grid(&mut g, (0, 0, 128, 128), 0, 32);
        let line = crate::binarize::line_mask(
            &g.lineart_rgba(),
            crate::binarize::DEFAULT_LINE_THRESHOLD,
        );
        let seeds = crate::seeds::read(&g.seeds_rgba(), 128, 128);

        assert_eq!(seeds.len(), 16, "4×4 格 → 16 個色標，多一個就是色點斷開了");
        for s in &seeds {
            assert!(
                s.solid_area >= crate::seeds::MIN_SEED_AREA,
                "色標 {:?} 只有 {}px",
                s.anchor,
                s.solid_area
            );
            let i = (s.anchor.1 * 128 + s.anchor.0) as usize;
            assert!(!line[i], "色標 {:?} 的 anchor 壓在線上", s.anchor);
        }
    }

    /// 色標顏色**就是**建議色，取 `PERM` 映射避免與 idx 本身相等。
    #[test]
    fn seed_colour_is_the_permuted_palette_entry() {
        let mut g = Canvas::new(64, 64);
        zone_grid(&mut g, (0, 0, 64, 64), 0, 32);
        let seeds = crate::seeds::read(&g.seeds_rgba(), 64, 64);
        let idx = g.get(seeds[0].anchor.0, seeds[0].anchor.1);
        assert_eq!(seeds[0].color, PALETTE[PERM[idx as usize] as usize]);
        assert_ne!(seeds[0].color, PALETTE[idx as usize]);
    }

    #[test]
    fn perm_is_a_bijection_without_fixed_points() {
        let mut seen = [false; 16];
        for (i, &p) in PERM.iter().enumerate() {
            assert_ne!(p as usize, i, "PERM 有不動點，建議色會與 idx 的顏色相同");
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
