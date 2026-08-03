//! torture test 素材產生器（`cargo xtask gen-torture`）。
//!
//! 產出的不是合格交付，而是刻意扭曲的 baker / 區域抽取壓力測試：
//! 細碎區域、單像素縫隙、面積 < 200px 碎片。畫布取 4:5（`kirby-demo-1` 是 1:1，
//! 兩張合起來覆蓋兩種比例），且**無 `shade`**——`has_shade = false` 那條路徑靠它跑。
//!
//! 產物必須逐位元決定性：重跑一次要得到同樣的檔案，否則 LFS 每次都多一份。

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

use anyhow::{Context, Result};

const W: usize = 3072;
const H: usize = 4096;
const ZONE: usize = 1024;
const COLS: usize = W / ZONE;
const ROWS: usize = H / ZONE;

/// 區塊之間的分隔框。有它才不必煩惱跨區塊的「相鄰同色」，
/// 每個區塊內部的用色可以各自獨立設計。
const FRAME: u8 = 0;

/// index 0 是分隔框，1..=15 給區塊內容用。高飽和、彼此差異大（assets-spec §4.2 ②）。
const PALETTE: [[u8; 3]; 16] = [
    [64, 64, 64],
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [255, 0, 255],
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

/// 區塊內用色：永遠落在 1..=15，不會撞到分隔框。
fn c(base: usize, k: usize) -> u8 {
    (1 + (base + k) % 15) as u8
}

struct Grid {
    idx: Vec<u8>,
}

impl Grid {
    fn new() -> Self {
        Self {
            idx: vec![FRAME; W * H],
        }
    }

    fn set(&mut self, x: usize, y: usize, color: u8) {
        if x < W && y < H {
            self.idx[y * W + x] = color;
        }
    }

    fn get(&self, x: usize, y: usize) -> u8 {
        self.idx[y * W + x]
    }

    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u8) {
        for yy in y..(y + h).min(H) {
            for xx in x..(x + w).min(W) {
                self.idx[yy * W + xx] = color;
            }
        }
    }
}

// ── 各區塊 ────────────────────────────────────────────────────────────
// 每個函式負責填滿 [x, x+ZONE) × [y, y+ZONE)，之後分隔框會蓋掉邊緣兩列。

/// 均勻網格。`cell` 不整除 ZONE 時最後一排會被截短——那也是想測的。
fn zone_grid(g: &mut Grid, x0: usize, y0: usize, base: usize, cell: usize) {
    let n = ZONE.div_ceil(cell);
    for cy in 0..n {
        for cx in 0..n {
            let k = (cx & 1) + 2 * (cy & 1);
            g.rect(x0 + cx * cell, y0 + cy * cell, cell, cell, c(base, k));
        }
    }
}

/// 面積橫跨 200px 門檻兩側的孤立碎片（assets-spec §7 的警告項）。
fn zone_fragments(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    g.rect(x0, y0, ZONE, ZONE, c(base, 0));
    // (寬, 高)：1..199 在門檻下，200..900 在門檻上，剛好各跨一邊
    const SHAPES: [(usize, usize); 16] = [
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 5),
        (7, 7),
        (10, 10),
        (12, 12),
        (13, 13),
        (14, 14),
        (199, 1),
        (1, 199),
        (20, 10),
        (15, 15),
        (20, 20),
        (30, 30),
    ];
    for (i, (w, h)) in SHAPES.iter().enumerate() {
        let cx = i % 4;
        let cy = i / 4;
        // 每個碎片獨佔一格，彼此不相鄰
        g.rect(
            x0 + 40 + cx * 240,
            y0 + 40 + cy * 240,
            *w,
            *h,
            c(base, 1 + i % 3),
        );
    }
}

/// 單像素縫隙：封閉方框上開一道寬 `gap` 的缺口，讓內外同色區域經由缺口相連。
/// 這是使用者按下油漆桶時「漏色滿整張畫布」的最小重現。
fn zone_gaps(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    let outer = c(base, 0);
    let wall = c(base, 1);
    g.rect(x0, y0, ZONE, ZONE, outer);

    const BOX: usize = 300;
    const THICK: usize = 8;
    for i in 0..6 {
        let bx = x0 + 40 + (i % 2) * 480;
        let by = y0 + 40 + (i / 2) * 320;
        // 牆是實心方框，內部挖回 outer 色 → 內外只差這一圈牆
        g.rect(bx, by, BOX, BOX - 40, wall);
        g.rect(
            bx + THICK,
            by + THICK,
            BOX - 2 * THICK,
            BOX - 40 - 2 * THICK,
            outer,
        );
        // 缺口必須貫穿整個牆厚才會漏——只削掉牆的一層是不會漏的，
        let gap = [0usize, 1, 1, 1, 2, 3][i];
        match i {
            0 => {}                                                              // 對照組，不開缺口
            1 => g.rect(bx + BOX / 2, by, gap, THICK, outer),                    // 上邊中央
            2 => g.rect(bx, by + BOX / 3, THICK, gap, outer),                    // 左邊中段
            3 => g.rect(bx, by + THICK, THICK, gap, outer),                      // 緊鄰左上角
            _ => g.rect(bx + BOX / 2, by + BOX - 40 - THICK, gap, THICK, outer), // 下邊
        }
    }
}

/// 1px 寬的長條。相鄰必不同色，但同一個顏色在不相鄰處反覆出現
/// ——正是 assets-spec §4.2 ③ 允許的重用。
fn zone_comb(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    const HALF: usize = ZONE / 2;
    for i in 0..ZONE {
        g.rect(x0 + i, y0, 1, HALF, c(base, i % 2)); // 上半：1px 直條
    }
    for i in 0..HALF {
        g.rect(x0, y0 + HALF + i, ZONE, 1, c(base, 2 + i % 2)); // 下半：1px 橫條
    }
}

/// 1px 棋盤：4-連通下每個像素自成一區。區域抽取的最壞情況。
fn zone_checker(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    g.rect(x0, y0, ZONE, ZONE, c(base, 0));
    const PATCH: usize = 64;
    let (a, b) = (c(base, 1), c(base, 2));
    for y in 0..PATCH {
        for x in 0..PATCH {
            let color = if (x + y) % 2 == 0 { a } else { b };
            g.set(
                x0 + ZONE / 2 - PATCH / 2 + x,
                y0 + ZONE / 2 - PATCH / 2 + y,
                color,
            );
        }
    }
}

/// 同心方環，環厚在 1..=5 之間循環——1px 厚的環是細長區域的極端。
fn zone_rings(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    let mut r = 0usize;
    let mut i = 0usize;
    while r < ZONE / 2 {
        let t = 1 + i % 5;
        let side = ZONE - 2 * r;
        g.rect(x0 + r, y0 + r, side, side, c(base, i % 3));
        r += t;
        i += 1;
    }
}

/// 放射狀楔形：中心區一個人就有 180 個鄰居，壓 adjacency 結構。
fn zone_pie(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    const WEDGES: usize = 180;
    let cx = ZONE as f64 / 2.0;
    let cy = ZONE as f64 / 2.0;
    for y in 0..ZONE {
        for x in 0..ZONE {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let r = dx.hypot(dy);
            let color = if r < 60.0 {
                c(base, 3)
            } else {
                let t = dy.atan2(dx) + std::f64::consts::PI;
                let w = ((t / std::f64::consts::TAU) * WEDGES as f64) as usize;
                c(base, w.min(WEDGES - 1) % 3)
            };
            g.set(x0 + x, y0 + y, color);
        }
    }
}

/// 阿基米德螺旋：通道是一條極長的細區域，對 tile 化的渲染與 flood fill 都是最壞情況。
///
/// 壁厚取 2px 而非 1px——1px 的斜向牆在 4-連通下會自己裂成幾萬個碎片，
/// 那是光柵化雜訊，會把真正想測的東西淹掉。1px 特徵由 comb / checker / edges 負責。
fn zone_spiral(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    let (wall, chan) = (c(base, 0), c(base, 1));
    let cx = ZONE as f64 / 2.0;
    let cy = ZONE as f64 / 2.0;
    let a = 8.0 / std::f64::consts::TAU; // 圈距 8px
    for y in 0..ZONE {
        for x in 0..ZONE {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let r = dx.hypot(dy);
            let theta = dy.atan2(dx) + std::f64::consts::PI;
            let k = (r / a - theta) / std::f64::consts::TAU;
            let dist = (k - k.round()).abs() * a * std::f64::consts::TAU;
            g.set(x0 + x, y0 + y, if dist < 1.0 { wall } else { chan });
        }
    }
}

/// 貼齊畫布邊與角的區域——邊界處理的 off-by-one 都會在這裡現形。
///
/// 這一塊排在畫布左下角，特徵壓在**真正的畫布邊**上：分隔框只畫在區塊之間，
/// 壓在區塊邊上的特徵會被它蓋掉。
fn zone_edges(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    g.rect(x0, y0, ZONE, ZONE, c(base, 0));
    let bottom = H - 1;
    g.set(x0, bottom, c(base, 1)); // 畫布左下角的孤立單一像素
    g.rect(x0, y0 + 100, 1, 200, c(base, 2)); // 貼左緣的 1px 直條
    g.rect(x0 + 200, bottom, 200, 1, c(base, 2)); // 貼下緣的 1px 橫條
    g.rect(x0 + 500, bottom - 199, 1, 200, c(base, 3)); // 由內觸底的 1px 直條
}

/// 大格棋盤：只用兩色，同色僅在對角相接——4-連通下是兩個獨立區域，
/// 8-連通下會被誤併。區域抽取的連通性語意由這一塊定住。
fn zone_reuse(g: &mut Grid, x0: usize, y0: usize, base: usize) {
    const CELL: usize = 128;
    let n = ZONE / CELL;
    for cy in 0..n {
        for cx in 0..n {
            let k = (cx + cy) % 2;
            g.rect(x0 + cx * CELL, y0 + cy * CELL, CELL, CELL, c(base, k));
        }
    }
}

// ── 組裝 ──────────────────────────────────────────────────────────────

fn build_flats() -> Grid {
    let mut g = Grid::new();
    for zy in 0..ROWS {
        for zx in 0..COLS {
            let (x0, y0) = (zx * ZONE, zy * ZONE);
            let base = zx * 5 + zy * 3;
            match (zx, zy) {
                (0, 0) => zone_grid(&mut g, x0, y0, base, 32),
                (1, 0) => zone_grid(&mut g, x0, y0, base, 13),
                (2, 0) => zone_fragments(&mut g, x0, y0, base),
                (0, 1) => zone_gaps(&mut g, x0, y0, base),
                (1, 1) => zone_comb(&mut g, x0, y0, base),
                (2, 1) => zone_checker(&mut g, x0, y0, base),
                (0, 2) => zone_rings(&mut g, x0, y0, base),
                (1, 2) => zone_pie(&mut g, x0, y0, base),
                (2, 2) => zone_spiral(&mut g, x0, y0, base),
                (0, 3) => zone_edges(&mut g, x0, y0, base),
                (1, 3) => zone_reuse(&mut g, x0, y0, base),
                _ => g.rect(x0, y0, ZONE, ZONE, c(base, 0)),
            }
        }
    }

    // 分隔框只畫在區塊之間，不畫在畫布外緣——zone_edges 需要真的碰到畫布邊。
    for zx in 1..COLS {
        g.rect(zx * ZONE - 1, 0, 2, H, FRAME);
    }
    for zy in 1..ROWS {
        g.rect(0, zy * ZONE - 1, W, 2, FRAME);
    }
    g
}

/// 線稿 = flats 的區域邊界，硬邊、無抗鋸齒。
/// 刻意留下的縫隙在 flats 裡本來就沒有邊界，因此線稿在那裡自然也是斷的。
fn build_lineart(g: &Grid) -> Vec<u8> {
    let mut out = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let here = g.get(x, y);
            let edge = (x + 1 < W && g.get(x + 1, y) != here)
                || (y + 1 < H && g.get(x, y + 1) != here)
                || (x > 0 && g.get(x - 1, y) != here)
                || (y > 0 && g.get(x, y - 1) != here);
            if edge {
                out[(y * W + x) * 4 + 3] = 255; // 不透明黑
            }
        }
    }
    out
}

fn write_png(path: &Path, rgba: &[u8]) -> Result<()> {
    let file = File::create(path).with_context(|| format!("建立 {} 失敗", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), W as u32, H as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // sRGB chunk 就是 PNG 規格宣告色彩空間的方式（assets-spec §3）
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder
        .write_header()?
        .write_image_data(rgba)
        .with_context(|| format!("寫入 {} 失敗", path.display()))?;
    Ok(())
}

const META: &str = r#"{
  "id": "torture-01",
  "title": "Torture Test 01",
  "category": "mandala",
  "notes": "由 `cargo xtask gen-torture` 決定性產生，不是繪師交付。刻意含細碎區域、單像素縫隙與面積 < 200px 碎片，用來壓 baker 的區域抽取。4:5 且無 shade，跑 has_shade = false 那條路徑。category 取 mandala 只因幾何圖形最接近，不代表難度分級。"
}
"#;

pub fn run(root: &Path) -> Result<()> {
    let dir = root.join("assets/source/torture-01");
    fs::create_dir_all(&dir).with_context(|| format!("建立 {} 失敗", dir.display()))?;

    let flats = build_flats();
    let lineart = build_lineart(&flats);

    let mut rgba = vec![255u8; W * H * 4];
    for (i, &idx) in flats.idx.iter().enumerate() {
        rgba[i * 4..i * 4 + 3].copy_from_slice(&PALETTE[idx as usize]);
    }

    write_png(&dir.join("flats.png"), &rgba)?;
    write_png(&dir.join("lineart.png"), &lineart)?;
    fs::write(dir.join("meta.json"), META)?;

    println!("gen-torture：{W}×{H} → {}", dir.display());
    Ok(())
}
