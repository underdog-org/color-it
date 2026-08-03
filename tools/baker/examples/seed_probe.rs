//! Phase 0 可行性驗證（`specs/baker-seeds.md §7`）。**驗證完即刪。**

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baker::binarize::{DEFAULT_LINE_THRESHOLD, MAX_LINE_RATIO, line_mask, line_ratio};
use baker::image::{Image, PngOptions, encode_rgba};
use baker::seeds::Seed;
use baker::segment::{self, UNASSIGNED};

const MIN_ORPHAN_AREA: u32 = 500;
const QUANT_SHIFT: u8 = 3;

/// 第二眾數要佔封閉區的多少面積，才算「這裡本來想分兩塊」。
const BIMODAL_SHARE: f32 = 0.20;

/// 兩個眾數色的 RGB 歐氏距離門檻。低於此值是同一片顏色的漸層，不是兩個意圖。
const BIMODAL_DISTANCE: f32 = 60.0;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let src: PathBuf = args
        .next()
        .context("用法：seed_probe <素材目錄> <輸出目錄>")?
        .into();
    let out: PathBuf = args
        .next()
        .context("用法：seed_probe <素材目錄> <輸出目錄>")?
        .into();
    std::fs::create_dir_all(&out)?;

    let lineart = Image::load(&src.join("lineart.png"))?;
    let reference = Image::load(&src.join("reference.png"))?;
    let (w, h) = (lineart.width, lineart.height);
    let total_px = (w as u64) * (h as u64);
    anyhow::ensure!(
        (reference.width, reference.height) == (w, h),
        "lineart {w}×{h} 與 reference {}×{} 尺寸不一致",
        reference.width,
        reference.height
    );
    println!("素材 {w}×{h}（{:.1}M px）", total_px as f64 / 1e6);

    // ── 二值化 ──────────────────────────────────────────────────────
    let line = line_mask(&lineart.rgba, DEFAULT_LINE_THRESHOLD);
    let ratio = line_ratio(&line);
    println!(
        "線像素佔比：{:.2}%{}",
        ratio * 100.0,
        if ratio > MAX_LINE_RATIO {
            "  ← 超過 MAX_LINE_RATIO"
        } else {
            ""
        }
    );

    // ── A：線稿自身的封閉區普查 ─────────────────────────────────────
    let census = label_free_areas(&line, w, h);
    let big: Vec<u32> = (0..census.count)
        .filter(|&id| census.areas[id as usize] >= MIN_ORPHAN_AREA)
        .collect();
    let big_px: u64 = big.iter().map(|&id| census.areas[id as usize] as u64).sum();
    let line_px = line.iter().filter(|&&b| b).count() as u64;
    println!("\n── A 線稿封閉區普查 ─────────────────────");
    println!("封閉區總數：{}", census.count);
    println!(
        "≥{MIN_ORPHAN_AREA}px：{} 個，佔非線像素 {:.2}%",
        big.len(),
        big_px as f64 / (total_px - line_px) as f64 * 100.0
    );
    println!(
        "<{MIN_ORPHAN_AREA}px 的碎片：{} 個，合計 {} px（{:.3}% 畫布）",
        census.count as usize - big.len(),
        total_px - line_px - big_px,
        (total_px - line_px - big_px) as f64 / total_px as f64 * 100.0
    );
    let mut sorted = big.clone();
    sorted.sort_by_key(|&id| std::cmp::Reverse(census.areas[id as usize]));
    println!("最大的 10 個封閉區（面積 / 錨點）：");
    for &id in sorted.iter().take(10) {
        println!(
            "  {:>9} px @{:?}",
            census.areas[id as usize], census.anchors[id as usize]
        );
    }

    // ── B：對 reference 的欠分割交叉檢查 ────────────────────────────
    let modes = reference_modes(&census, &big, &reference.rgba);
    let under: Vec<&RegionModes> = modes.iter().filter(|m| m.is_bimodal()).collect();
    println!("\n── B 欠分割交叉檢查（collision 代理）──────");
    println!(
        "雙峰封閉區：{} / {}（{:.1}%）← 線稿在這些地方少一筆分界",
        under.len(),
        big.len(),
        under.len() as f32 / big.len().max(1) as f32 * 100.0
    );
    for m in under.iter().take(20) {
        println!(
            "  {:>8} px @{:?}  {:?} {:.0}% ／ {:?} {:.0}%  Δ{:.0}",
            census.areas[m.id as usize],
            census.anchors[m.id as usize],
            m.first,
            m.first_share * 100.0,
            m.second,
            m.second_share * 100.0,
            m.distance()
        );
    }
    if under.len() > 20 {
        println!("  …另有 {} 個未列出", under.len() - 20);
    }
    // 判準沒命中時，光憑「0 個」看不出是線稿真的封閉、還是量測根本沒在動。
    // 把最接近門檻的十個攤開來，讓 0 這個數字可稽核。
    let mut ranked: Vec<&RegionModes> = modes.iter().collect();
    ranked.sort_by(|a, b| {
        let score = |m: &RegionModes| m.second_share * m.distance();
        score(b).total_cmp(&score(a))
    });
    println!("最接近雙峰的 10 個封閉區（門檻 share≥{BIMODAL_SHARE} 且 Δ≥{BIMODAL_DISTANCE}）：");
    for m in ranked.iter().take(10) {
        println!(
            "  {:>8} px @{:?}  {:?} {:.0}% ／ {:?} {:.0}%  Δ{:.0}",
            census.areas[m.id as usize],
            census.anchors[m.id as usize],
            m.first,
            m.first_share * 100.0,
            m.second,
            m.second_share * 100.0,
            m.distance()
        );
    }

    // 線稿比 reference 細多少：同一個建議色被幾個封閉區共用。
    // 這個倍率就是 B 案下繪師「除了一色一點之外還要多點幾下」的成本。
    let mut per_color: HashMap<[u8; 3], u32> = HashMap::new();
    for m in &modes {
        *per_color.entry(m.first).or_default() += 1;
    }
    let mut shared: Vec<(&[u8; 3], &u32)> = per_color.iter().collect();
    shared.sort_by_key(|&(c, n)| (std::cmp::Reverse(*n), *c));
    println!(
        "\n建議色種類：{}，封閉區 {} 個 → 平均一色 {:.1} 區",
        per_color.len(),
        big.len(),
        big.len() as f32 / per_color.len().max(1) as f32
    );
    println!("被最多封閉區共用的建議色：");
    for (c, n) in shared.iter().take(5) {
        println!("  {c:?} × {n} 區");
    }

    // ── §3 管線實跑：每個 ≥門檻封閉區放一個色標 ────────────────────
    let seeds = build_seeds(&census, &modes);
    println!("\n── §3 管線實跑 ─────────────────────────");
    println!("色標數：{}", seeds.len());
    let mut g = segment::grow(&seeds, &line, w, h);
    println!(
        "collision：{} 組（每區一標，非 0 就是 grow 有 bug）",
        g.collisions.len()
    );
    println!("anchor 落在線上：{} 個", g.on_line.len());

    let orphans = segment::find_orphans(&g.labels, &line, w, h);
    let big_orphans: Vec<_> = orphans
        .iter()
        .filter(|o| o.area >= MIN_ORPHAN_AREA)
        .collect();
    println!(
        "orphan：{} 個（≥{MIN_ORPHAN_AREA}px：{} 個）",
        orphans.len(),
        big_orphans.len()
    );

    let (rounds, left) = segment::close(&mut g.labels, w, h);
    println!("close：{rounds} 輪，剩餘未指派 {left} px");

    // ── 目視產物 ────────────────────────────────────────────────────
    write_png(&out.join("preview.png"), &preview(&g.labels), w, h)?;
    write_png(
        &out.join("overlay.png"),
        &overlay(&lineart.rgba, &seeds, &census, &under, w, h),
        w,
        h,
    )?;
    println!("\n已寫出 {}/preview.png 與 overlay.png", out.display());
    Ok(())
}

// ── A：自由區標記 ───────────────────────────────────────────────────

struct Census {
    /// 每像素所屬的自由區 id，線像素為 `UNASSIGNED`。
    labels: Vec<u32>,
    count: u32,
    areas: Vec<u32>,
    /// 區內最接近重心的像素——當色標落點，凹形區也保證在區內。
    anchors: Vec<(u32, u32)>,
}

/// 非線像素的 4-連通標記。三趟線性掃描（標記 → 累加重心 → 取最近點），
/// 不為每個區配一個 `Vec`——母帶 12.6M 像素下那會爆記憶體。
fn label_free_areas(line: &[bool], width: u32, height: u32) -> Census {
    let (w, h) = (width as usize, height as usize);
    let mut labels = vec![UNASSIGNED; w * h];
    let mut areas: Vec<u32> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..w * h {
        if line[start] || labels[start] != UNASSIGNED {
            continue;
        }
        let id = areas.len() as u32;
        let mut area = 0u32;
        labels[start] = id;
        stack.push(start);
        while let Some(p) = stack.pop() {
            area += 1;
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<usize>| {
                if !line[n] && labels[n] == UNASSIGNED {
                    labels[n] = id;
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
        areas.push(area);
    }

    let n = areas.len();
    let (mut sx, mut sy) = (vec![0u64; n], vec![0u64; n]);
    for (i, &id) in labels.iter().enumerate() {
        if id != UNASSIGNED {
            sx[id as usize] += (i % w) as u64;
            sy[id as usize] += (i / w) as u64;
        }
    }
    let mut best = vec![(u64::MAX, 0usize); n];
    for (i, &id) in labels.iter().enumerate() {
        if id == UNASSIGNED {
            continue;
        }
        let id = id as usize;
        let a = areas[id].max(1) as u64;
        let (cx, cy) = ((sx[id] / a) as i64, (sy[id] / a) as i64);
        let (dx, dy) = ((i % w) as i64 - cx, (i / w) as i64 - cy);
        let d = (dx * dx + dy * dy) as u64;
        if d < best[id].0 {
            best[id] = (d, i);
        }
    }

    Census {
        count: n as u32,
        anchors: best
            .iter()
            .map(|&(_, i)| ((i % w) as u32, (i / w) as u32))
            .collect(),
        areas,
        labels,
    }
}

// ── B：封閉區內的 reference 色分佈 ──────────────────────────────────

struct RegionModes {
    id: u32,
    first: [u8; 3],
    first_share: f32,
    second: [u8; 3],
    second_share: f32,
}

impl RegionModes {
    fn distance(&self) -> f32 {
        let d = |a: u8, b: u8| (a as f32 - b as f32).powi(2);
        (d(self.first[0], self.second[0])
            + d(self.first[1], self.second[1])
            + d(self.first[2], self.second[2]))
        .sqrt()
    }

    /// 第二片顏色面積夠大**且**與第一片差異夠大 → 這個封閉區本來該是兩塊。
    fn is_bimodal(&self) -> bool {
        self.second_share >= BIMODAL_SHARE && self.distance() >= BIMODAL_DISTANCE
    }
}

/// 只量 `big` 裡的封閉區。量化到每通道 5 bits，取前兩名眾數。
fn reference_modes(census: &Census, big: &[u32], reference: &[u8]) -> Vec<RegionModes> {
    let wanted: HashMap<u32, usize> = big.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut hist: Vec<HashMap<u16, u32>> = vec![HashMap::new(); big.len()];
    for (i, &id) in census.labels.iter().enumerate() {
        let Some(&slot) = wanted.get(&id) else {
            continue;
        };
        let q = |c: u8| (c >> QUANT_SHIFT) as u16;
        let key =
            (q(reference[i * 4]) << 10) | (q(reference[i * 4 + 1]) << 5) | q(reference[i * 4 + 2]);
        *hist[slot].entry(key).or_default() += 1;
    }

    big.iter()
        .zip(hist)
        .map(|(&id, h)| {
            let mut top: Vec<(u16, u32)> = h.into_iter().collect();
            // 平手取量化鍵較小者——不釘死的話同一張圖每次跑結論可能不同。
            top.sort_by_key(|&(key, n)| (std::cmp::Reverse(n), key));
            let total = census.areas[id as usize] as f32;
            let at = |i: usize| top.get(i).copied().unwrap_or((0, 0));
            let (k0, n0) = at(0);
            let (k1, n1) = at(1);
            RegionModes {
                id,
                first: unquantize(k0),
                first_share: n0 as f32 / total,
                second: unquantize(k1),
                second_share: n1 as f32 / total,
            }
        })
        .collect()
}

/// 量化鍵還原成該格的中心色。
fn unquantize(key: u16) -> [u8; 3] {
    let half = 1u16 << (QUANT_SHIFT - 1);
    let c = |shift: u16| ((((key >> shift) & 0x1f) << QUANT_SHIFT) + half).min(255) as u8;
    [c(10), c(5), c(0)]
}

/// 每個 ≥門檻封閉區一個色標，落點取區內最接近重心者，顏色取 reference 的眾數。
/// 依 raster order 排序（§3.1 ③）。
fn build_seeds(census: &Census, modes: &[RegionModes]) -> Vec<Seed> {
    let mut out: Vec<Seed> = modes
        .iter()
        .map(|m| Seed {
            anchor: census.anchors[m.id as usize],
            color: m.first,
            solid_area: census.areas[m.id as usize],
        })
        .collect();
    out.sort_by_key(|s| (s.anchor.1, s.anchor.0));
    out
}

// ── 目視產物 ────────────────────────────────────────────────────────

/// 隨機高對比配色。**這是 §6.1 洋紅檢查的替代品**：兩塊有沒有融成一塊，一眼看見。
/// 用黃金角推色相，相鄰 id 在色環上盡量分開；確定性，同一張圖每次配色相同。
fn preview(labels: &[u32]) -> Vec<u8> {
    labels
        .iter()
        .flat_map(|&id| {
            if id == UNASSIGNED {
                return [0, 0, 0, 255];
            }
            let [r, g, b] = hsv(id as f32 * 137.507_76 % 360.0, 0.72, 0.96);
            [r, g, b, 255]
        })
        .collect()
}

/// 線稿合成到白底 ＋ 色標位置（實心方塊，顏色是建議色）＋ 診斷標記
/// （欠分割的封閉區畫紅框）。
fn overlay(
    lineart: &[u8],
    seeds: &[Seed],
    census: &Census,
    under: &[&RegionModes],
    w: u32,
    h: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(lineart.len());
    for px in lineart.chunks_exact(4) {
        let a = px[3] as u32;
        let over = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        out.extend_from_slice(&[over(px[0]), over(px[1]), over(px[2]), 255]);
    }
    for s in seeds {
        stamp(&mut out, w, h, s.anchor, 12, s.color, false);
    }
    for m in under {
        stamp(
            &mut out,
            w,
            h,
            census.anchors[m.id as usize],
            40,
            [255, 0, 0],
            true,
        );
    }
    out
}

/// 在 `(cx, cy)` 蓋一個邊長 `2*r` 的方塊。`hollow` 為真時只畫 3px 寬的框。
fn stamp(buf: &mut [u8], w: u32, h: u32, (cx, cy): (u32, u32), r: i64, rgb: [u8; 3], hollow: bool) {
    for dy in -r..=r {
        for dx in -r..=r {
            if hollow && dx.abs() < r - 3 && dy.abs() < r - 3 {
                continue;
            }
            let (x, y) = (cx as i64 + dx, cy as i64 + dy);
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                continue;
            }
            let i = (y as usize * w as usize + x as usize) * 4;
            buf[i..i + 3].copy_from_slice(&rgb);
        }
    }
}

fn hsv(hue: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}

fn write_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<()> {
    let bytes = encode_rgba(rgba, w, h, PngOptions::default())?;
    std::fs::write(path, bytes).with_context(|| format!("寫入 {} 失敗", path.display()))?;
    Ok(())
}
