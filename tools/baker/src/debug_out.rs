//! `--debug-out <dir>` 的四件產物（`specs/baker-seeds.md §5`）。
//!
//! **這不是 debug 工具，是退件附件。** 繪師手上不會有 baker——他們是外包，跑 CLI 的
//! 是專案方。所以三張圖的畫法以「給繪師看」為準，不是以「給我 debug」為準：
//! 線要看得出位置、色標要看得出是哪一點、問題要圈出來。
//!
//! 全部在**母帶解析度**產出，而且在**母帶檢查之後、fail-fast 之前**呼叫——
//! `seed-collision` 與 `orphan-area` 正是最需要附件的兩種退件（§4.1 最後一段特意
//! 保留 labels 就是為了這裡）。

use std::path::Path;

use anyhow::{Context, Result};

use crate::image::{PngOptions, encode_rgba};
use crate::seeds::Seed;
use crate::segment::{Orphan, UNASSIGNED};

pub const PREVIEW: &str = "preview.png";
pub const SEEDS_OVERLAY: &str = "seeds-overlay.png";
pub const REFERENCE_PREVIEW: &str = "reference-preview.png";
pub const REGIONS_JSON: &str = "regions.json";

/// 4096 級母帶上的筆畫尺寸。縮到全圖看還要看得見，所以比直覺大一個量級。
const DOT_RADIUS: i64 = 28;
const STROKE: i64 = 5;

pub struct DebugInput<'a> {
    pub id: &'a str,
    pub width: u32,
    pub height: u32,
    /// `grow` + `merge_small_orphans` 之後、`close` **之前**的 labels，
    /// 所以線像素與孤兒區都還是 `UNASSIGNED`。preview 靠這一點才畫得出線。
    pub labels: &'a [u32],
    pub seeds: &'a [Seed],
    pub line: &'a [bool],
    /// 已合成到白底的母帶 RGBA。
    pub lineart_white: &'a [u8],
    pub shade_white: Option<&'a [u8]>,
    /// `(先佔住的 seed, 撞進來的 seed)`。
    pub collisions: &'a [(u32, u32)],
    pub orphans: &'a [Orphan],
}

pub fn write(dir: &Path, input: &DebugInput) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("建立 {} 失敗", dir.display()))?;
    let put = |name: &str, rgba: &[u8]| -> Result<()> {
        // Fast：退件附件的檔案大小不重要，母帶 4096 級走 High 要多花數十秒。
        let bytes = encode_rgba(
            rgba,
            input.width,
            input.height,
            PngOptions {
                srgb: true,
                icc: None,
                compression: png::Compression::Fast,
            },
        )?;
        std::fs::write(dir.join(name), bytes).with_context(|| format!("寫入 {name} 失敗"))
    };

    put(PREVIEW, &preview(input))?;
    put(SEEDS_OVERLAY, &seeds_overlay(input))?;
    put(REFERENCE_PREVIEW, &reference_preview(input))?;
    std::fs::write(dir.join(REGIONS_JSON), regions_json(input))
        .with_context(|| format!("寫入 {REGIONS_JSON} 失敗"))?;
    Ok(())
}

/// 高對比配色的區域圖 ＋ 線稿。**`assets-spec §6.1` 洋紅縫隙檢查的替代品**：
/// 兩塊該分開卻融成一塊，在這張圖上是同一個顏色，一眼就看見。
fn preview(input: &DebugInput) -> Vec<u8> {
    let palette = preview_palette(input);
    let mut out = vec![255u8; (input.width * input.height * 4) as usize];
    for (i, &id) in input.labels.iter().enumerate() {
        let rgb = if input.line[i] {
            [40, 40, 40]
        } else if id == UNASSIGNED {
            // 沒有 seed 認領的自由區。留白，在滿版彩色裡是最顯眼的。
            [255, 255, 255]
        } else {
            palette[id as usize]
        };
        out[i * 4..i * 4 + 3].copy_from_slice(&rgb);
    }
    out
}

/// 逐區配色，**依鄰接關係**挑，不是依 id 挑。
///
/// 第一版是「id × 黃金角」，實測在 `adventure-time-demo-1` 上讓 Jake 的身體與後腿
/// 拿到幾乎一樣的綠——因為 region id 是 raster order，空間上相鄰的兩區 id 不一定
/// 相鄰。**這張圖唯一的用途就是看出兩塊有沒有融成一塊，配色撞了就等於這張圖失效。**
///
/// 所以先掃一次鄰接，再貪婪挑「離已上色鄰居最遠」的候選色。id 遞增、平手取較小
/// 候選索引 → 確定性。
fn preview_palette(input: &DebugInput) -> Vec<[u8; 3]> {
    let n = input.seeds.len();
    let (w, h) = (input.width as usize, input.height as usize);

    // 鄰接必須**穿過線**去連。`close` 之前線像素是 UNASSIGNED，而真實線稿的線
    // （含抗鋸齒帶）動輒十幾 px 寬——第一版只看直接相鄰與跨 2px，結果一條邊都沒連到，
    // 整張圖變成同一個顏色。這種錯只有把圖打開才看得到，測試不會抓。
    //
    // 逐列（再逐行）找「下一個已標記像素」，中間隔的若只是線且不超過 `MAX_LINE_GAP`
    // 就算相鄰。單趟 O(px)，不必真的跑一次 close。
    let mut adjacent: Vec<std::collections::BTreeSet<u32>> = vec![Default::default(); n];
    let mut link = |a: u32, b: u32| {
        if a != b {
            adjacent[a as usize].insert(b);
            adjacent[b as usize].insert(a);
        }
    };
    let mut scan = |len: usize, count: usize, index: &dyn Fn(usize, usize) -> usize| {
        for k in 0..count {
            let mut prev: Option<(u32, usize)> = None;
            for t in 0..len {
                let id = input.labels[index(k, t)];
                if id == UNASSIGNED {
                    continue;
                }
                if let Some((pid, pt)) = prev
                    && t - pt <= MAX_LINE_GAP
                {
                    link(pid, id);
                }
                prev = Some((id, t));
            }
        }
    };
    scan(w, h, &|y, x| y * w + x);
    scan(h, w, &|x, y| y * w + x);

    let candidates: Vec<[u8; 3]> = (0..PREVIEW_COLORS)
        .map(|k| {
            let hue = (k as f32 * 360.0 / PREVIEW_COLORS as f32 + (k % 2) as f32 * 11.0) % 360.0;
            let value = if k % 2 == 0 { 0.97 } else { 0.66 };
            let sat = if k % 3 == 0 { 0.55 } else { 0.90 };
            hsv_to_rgb(hue, sat, value)
        })
        .collect();

    let mut chosen: Vec<Option<usize>> = vec![None; n];
    for id in 0..n {
        let taken: Vec<[u8; 3]> = adjacent[id]
            .iter()
            .filter_map(|&nb| chosen[nb as usize])
            .map(|k| candidates[k])
            .collect();
        // `Reverse(k)` 讓平手取較小索引——max_by_key 在平手時回傳最後一個。
        let best = (0..PREVIEW_COLORS)
            .max_by_key(|&k| {
                let score = taken
                    .iter()
                    .map(|c| rgb_distance(candidates[k], *c))
                    .min()
                    .unwrap_or(i32::MAX);
                (score, std::cmp::Reverse(k))
            })
            .expect("候選色非空");
        chosen[id] = Some(best);
    }
    chosen
        .into_iter()
        .map(|k| candidates[k.expect("每個 id 都上了色")])
        .collect()
}

/// 候選色數。太少會在高鄰接度的圖上被迫重用，太多則彼此變近。
const PREVIEW_COLORS: usize = 24;

/// 兩個已標記像素之間隔著多少像素還算「相鄰」。母帶 4096 級的線含抗鋸齒帶
/// 十幾 px 是常態，64 給足餘裕；隔超過這個距離的兩區，視覺上本來就分得開。
const MAX_LINE_GAP: usize = 64;

fn rgb_distance(a: [u8; 3], b: [u8; 3]) -> i32 {
    (0..3).map(|c| (a[c] as i32 - b[c] as i32).pow(2)).sum()
}

/// 線稿 ＋ 色標位置 ＋ 診斷標記。繪師拿它對照「我以為我點了幾個點」。
fn seeds_overlay(input: &DebugInput) -> Vec<u8> {
    let (w, h) = (input.width, input.height);
    let mut out = vec![255u8; (w * h * 4) as usize];
    // 線稿壓淡：主角是色標與標記，線只是定位用的底圖。
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        if input.line[i] {
            px[..3].copy_from_slice(&[170, 170, 170]);
        }
    }

    let mut canvas = Canvas {
        out: &mut out,
        w,
        h,
    };
    // 標記先畫、色標後畫：紅線壓過色標的話，繪師就認不出被連起來的是哪兩個點。
    //
    // 紅線連兩點：繪師要補的線就在這條線上（§4.1）。
    for &(first, second) in input.collisions {
        let (a, b) = (
            input.seeds[first as usize].anchor,
            input.seeds[second as usize].anchor,
        );
        canvas.line(a, b, [230, 20, 20]);
    }
    // 黃框圈出整塊沒被認領的區域。
    for o in input.orphans {
        canvas.rect_outline(o.bbox, [235, 190, 0]);
    }
    for s in input.seeds {
        canvas.disc(s.anchor, DOT_RADIUS, s.color);
        canvas.ring(s.anchor, DOT_RADIUS, STROKE, [20, 20, 20]);
    }
    out
}

/// 用建議色 ＋ 線稿 ＋（有的話）shade 渲染整張——**繪師交出 `reference.png` 的
/// 能力被 §2.3 拿掉了，這張還給他**：他在自己的工具裡看不到整體配色效果。
fn reference_preview(input: &DebugInput) -> Vec<u8> {
    // 未認領像素（線、孤兒區）指向一個額外的白色 id，`composite` 才不必知道
    // `UNASSIGNED` 的存在。
    let white = input.seeds.len() as u32;
    let labels: Vec<u32> = input
        .labels
        .iter()
        .map(|&id| if id == UNASSIGNED { white } else { id })
        .collect();
    let mut suggested: Vec<[u8; 3]> = input.seeds.iter().map(|s| s.color).collect();
    suggested.push([255, 255, 255]);
    crate::thumb::composite(&labels, &suggested, input.lineart_white, input.shade_white)
}

/// 逐區面積 / bbox / 重心 / 建議色，**給人看的**。
///
/// 是**母帶**解析度、`close` 之前的統計，與 pack 裡的 `regions.json`（輸出解析度、
/// `close` 之後）不是同一個東西——那一份要通過驗證才產得出來，而這一份在退件時
/// 也要有。檔頭寫明這件事，免得兩份數字對不起來時有人以為是 bug。
fn regions_json(input: &DebugInput) -> String {
    let n = input.seeds.len();
    let mut area = vec![0u64; n];
    let mut sum = vec![(0u64, 0u64); n];
    let mut bounds = vec![None::<[u32; 4]>; n];
    for (i, &id) in input.labels.iter().enumerate() {
        if id == UNASSIGNED {
            continue;
        }
        let id = id as usize;
        let (x, y) = (i as u32 % input.width, i as u32 / input.width);
        area[id] += 1;
        sum[id].0 += x as u64;
        sum[id].1 += y as u64;
        bounds[id] = Some(match bounds[id] {
            None => [x, y, 1, 1],
            Some([bx, by, bw, bh]) => {
                let (x0, y0) = (bx.min(x), by.min(y));
                let (x1, y1) = ((bx + bw - 1).max(x), (by + bh - 1).max(y));
                [x0, y0, x1 - x0 + 1, y1 - y0 + 1]
            }
        });
    }

    let regions: Vec<serde_json::Value> = (0..n)
        .map(|id| {
            let a = area[id].max(1);
            serde_json::json!({
                "id": id,
                "area": area[id],
                "bbox": bounds[id].unwrap_or([0; 4]),
                "centroid": [sum[id].0 / a, sum[id].1 / a],
                "suggested_color": colorpack::region::hex_color(input.seeds[id].color),
                "seed_anchor": [input.seeds[id].anchor.0, input.seeds[id].anchor.1],
            })
        })
        .collect();

    let doc = serde_json::json!({
        "id": input.id,
        "note": "母帶解析度、close 之前的統計，退件時也會產出。\
                 pack 裡的 regions.json 是輸出解析度（母帶的一半）且 close 之後，數字不會相等。",
        "master_size": [input.width, input.height],
        "regions": regions,
        "orphans": input.orphans.iter().map(|o| serde_json::json!({
            "area": o.area,
            "bbox": o.bbox,
            "anchor": [o.anchor.0, o.anchor.1],
        })).collect::<Vec<_>>(),
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).expect("可序列化")
    )
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

/// 最小繪圖原語。母帶是 12.6M px，這些筆畫只碰得到其中極小一塊，
/// 所以逐點寫入、不做任何最佳化。
struct Canvas<'a> {
    out: &'a mut [u8],
    w: u32,
    h: u32,
}

impl Canvas<'_> {
    fn put(&mut self, x: i64, y: i64, rgb: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let i = (y as usize * self.w as usize + x as usize) * 4;
        self.out[i..i + 3].copy_from_slice(&rgb);
    }

    fn disc(&mut self, center: (u32, u32), r: i64, rgb: [u8; 3]) {
        let (cx, cy) = (center.0 as i64, center.1 as i64);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.put(cx + dx, cy + dy, rgb);
                }
            }
        }
    }

    fn ring(&mut self, center: (u32, u32), r: i64, thickness: i64, rgb: [u8; 3]) {
        let (cx, cy) = (center.0 as i64, center.1 as i64);
        let inner = (r - thickness).max(0);
        for dy in -r..=r {
            for dx in -r..=r {
                let d = dx * dx + dy * dy;
                if d <= r * r && d > inner * inner {
                    self.put(cx + dx, cy + dy, rgb);
                }
            }
        }
    }

    /// Bresenham，加粗成 `STROKE` 寬。
    fn line(&mut self, a: (u32, u32), b: (u32, u32), rgb: [u8; 3]) {
        let (mut x, mut y) = (a.0 as i64, a.1 as i64);
        let (x1, y1) = (b.0 as i64, b.1 as i64);
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            for oy in -STROKE..=STROKE {
                for ox in -STROKE..=STROKE {
                    self.put(x + ox, y + oy, rgb);
                }
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn rect_outline(&mut self, bbox: [u32; 4], rgb: [u8; 3]) {
        let [x, y, w, h] = bbox.map(|v| v as i64);
        for t in 0..STROKE {
            for dx in 0..w {
                self.put(x + dx, y + t, rgb);
                self.put(x + dx, y + h - 1 - t, rgb);
            }
            for dy in 0..h {
                self.put(x + t, y + dy, rgb);
                self.put(x + w - 1 - t, y + dy, rgb);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(x: u32, y: u32, color: [u8; 3]) -> Seed {
        Seed {
            anchor: (x, y),
            color,
            solid_area: 100,
        }
    }

    /// 8×2：左右各一區，中間沒有線。
    fn input<'a>(
        labels: &'a [u32],
        line: &'a [bool],
        seeds: &'a [Seed],
        lineart: &'a [u8],
        collisions: &'a [(u32, u32)],
        orphans: &'a [Orphan],
    ) -> DebugInput<'a> {
        DebugInput {
            id: "t",
            width: labels.len() as u32,
            height: 1,
            labels,
            seeds,
            line,
            lineart_white: lineart,
            shade_white: None,
            collisions,
            orphans,
        }
    }

    /// **空間上相鄰**的兩區顏色必須差得夠遠。第一版用「id × 黃金角」只保證了
    /// id 相鄰，而 id 是 raster order——實測在 adventure-time-demo-1 上讓 Jake 的
    /// 身體與後腿拿到幾乎一樣的綠，這張圖唯一的用途就沒了。
    #[test]
    fn spatially_adjacent_regions_get_far_apart_colours() {
        // 8×3 條紋：0|線|1|線|2 …，每一條都與左右鄰居相接。
        let w = 9usize;
        let mut labels = vec![UNASSIGNED; w * 3];
        let mut line = vec![false; w * 3];
        for y in 0..3 {
            for x in 0..w {
                if x % 2 == 1 {
                    line[y * w + x] = true;
                } else {
                    labels[y * w + x] = (x / 2) as u32;
                }
            }
        }
        let seeds: Vec<Seed> = (0..5).map(|i| seed(i * 2, 1, [0, 0, 0])).collect();
        let lineart = vec![255u8; w * 3 * 4];
        let palette = preview_palette(&DebugInput {
            id: "t",
            width: w as u32,
            height: 3,
            labels: &labels,
            seeds: &seeds,
            line: &line,
            lineart_white: &lineart,
            shade_white: None,
            collisions: &[],
            orphans: &[],
        });

        for id in 0..4 {
            let d = rgb_distance(palette[id], palette[id + 1]);
            assert!(
                d > 10_000,
                "區 {id} 與 {}：{:?} vs {:?}",
                id + 1,
                palette[id],
                palette[id + 1]
            );
        }
    }

    /// 未認領像素在 preview 上是白的——滿版彩色裡最顯眼的就是留白。
    #[test]
    fn unassigned_pixels_stay_white_and_lines_stay_dark() {
        let labels = [0, UNASSIGNED, UNASSIGNED];
        let line = [false, true, false];
        let seeds = [seed(0, 0, [1, 2, 3])];
        let lineart = vec![255u8; 12];
        let out = preview(&input(&labels, &line, &seeds, &lineart, &[], &[]));
        assert_eq!(&out[4..7], &[40, 40, 40], "線像素");
        assert_eq!(&out[8..11], &[255, 255, 255], "孤兒區");
        assert_ne!(&out[0..3], &[255, 255, 255], "有主的區域要上色");
    }

    /// `reference-preview` 把未認領像素當白色處理，不是 panic——
    /// 拒收路徑上的 labels 本來就有洞，而這張圖正是退件附件。
    #[test]
    fn reference_preview_survives_holes_in_the_labels() {
        let labels = [0, UNASSIGNED];
        let line = [false, true];
        let seeds = [seed(0, 0, [200, 100, 50])];
        let lineart = vec![255u8; 8];
        let out = reference_preview(&input(&labels, &line, &seeds, &lineart, &[], &[]));
        assert_eq!(&out[0..3], &[200, 100, 50]);
        assert_eq!(&out[4..7], &[255, 255, 255]);
    }

    /// `regions.json` 的統計跳過未認領像素——否則孤兒區會被算進某一區的面積。
    #[test]
    fn regions_json_counts_only_claimed_pixels() {
        let labels = [0, 0, UNASSIGNED, 1];
        let line = [false, false, true, false];
        let seeds = [seed(0, 0, [1, 2, 3]), seed(3, 0, [4, 5, 6])];
        let lineart = vec![255u8; 16];
        let orphans = [Orphan {
            area: 1,
            anchor: (2, 0),
            bbox: [2, 0, 1, 1],
        }];
        let json: serde_json::Value = serde_json::from_str(&regions_json(&input(
            &labels,
            &line,
            &seeds,
            &lineart,
            &[],
            &orphans,
        )))
        .unwrap();

        assert_eq!(json["regions"][0]["area"], 2);
        assert_eq!(json["regions"][1]["area"], 1);
        assert_eq!(json["regions"][0]["suggested_color"], "#010203");
        assert_eq!(json["orphans"][0]["bbox"][0], 2);
    }

    /// 診斷標記真的畫上去了。collision 的紅線與 orphan 的黃框是退件附件的全部價值——
    /// 沒畫出來的話這張圖只是一張色標分佈圖。
    #[test]
    fn collision_and_orphan_markers_are_actually_drawn() {
        let labels = [0u32; 400];
        let line = [false; 400];
        let seeds = [seed(5, 0, [10, 10, 10]), seed(300, 0, [10, 10, 10])];
        let lineart = vec![255u8; 1600];
        let orphans = [Orphan {
            area: 50,
            anchor: (200, 0),
            bbox: [200, 0, 50, 1],
        }];
        let out = seeds_overlay(&input(
            &labels,
            &line,
            &seeds,
            &lineart,
            &[(0, 1)],
            &orphans,
        ));
        let px = |x: usize| [out[x * 4], out[x * 4 + 1], out[x * 4 + 2]];

        assert_eq!(px(150), [230, 20, 20], "兩個色標之間要有紅線");
        assert_eq!(px(220), [235, 190, 0], "孤兒區要有黃框");
        // 色標畫在標記之上——紅線壓過色標的話，繪師認不出被連起來的是哪兩個點。
        assert_eq!(px(5), [10, 10, 10], "色標本身用建議色畫");
    }
}
