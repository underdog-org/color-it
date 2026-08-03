# baker 色標交付 · Phase 0 實施計畫

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 回答一個問題——`adventure-time-demo-1` 的線稿封閉區，推得出跟現行 `flats.png` 一樣的區域嗎？答案決定 `docs/specs/baker-seeds.md` 走或不走。

**Architecture:** 三個模組（`binarize` / `seeds` / `segment` 的 grow-orphan-close）**就是 spec §3 要的正式模組**，用 TDD 建在 `tools/baker/src/` 底下，`cargo test -p colorlull-baker` 直接跑得到。另加一支 `examples/seed_probe.rs`：從現有的 `flats.png` + `reference.png` 反推色標，餵進這三個模組，印出統計數字並產出兩張目視圖。**只有那支 example 是拋棄式的**——Phase 0 通過的話，Phase 1 已經做掉一半；否決的話刪掉四個檔即可。

**Tech Stack:** Rust 2024 edition、既有的 `baker::image`（PNG 讀寫）、`png` crate。無新增依賴，無新增 workspace member，`xtask/deps-policy.toml` 不需改動。

## Global Constraints

- Workspace lint 是 `clippy::all = deny`（`Cargo.toml` `[workspace.lints.clippy]`）。任何 clippy 警告都會讓建置失敗。
- `tools/baker` 同時是 lib（`baker`）與 bin（`baker`）。新模組要在 `src/lib.rs` 的 `pub mod` 清單裡宣告才對 example 可見。
- 母帶尺寸是 3072×4096（3:4），12.6M 像素。所有逐像素演算法用 `Vec` 索引，不要用 `HashMap<(u32,u32), _>`。
- 4-連通，不是 8-連通（`baker-core-design §2.1`，既有 `segment::label_regions` 的測試 `diagonal_touch_is_not_connected` 釘死了這條）。
- 未指派的 label 值一律用 `u32::MAX`，並以 `segment::UNASSIGNED` 常數表示。
- 所有涉及 `HashMap` 迭代的取值（例如取眾數）必須有**確定性的平手規則**，否則測試會隨機紅。
- commit message 格式 `type(scope): subject`，scope 用 `m0`。不加 AI footer。
- 素材在 `assets/source/adventure-time-demo-1/`（走 Git LFS，已確認本機有實體檔，非 pointer）。

---

## File Structure

| 檔案 | 責任 |
|---|---|
| `tools/baker/src/binarize.rs`（新） | 線稿 RGBA → line mask（`Vec<bool>`）＋ 線像素佔比。無其他職責。 |
| `tools/baker/src/seeds.rs`（新） | 色標圖 RGBA → `Vec<Seed>`。連通分量、眾數色、anchor 落點。 |
| `tools/baker/src/segment.rs`（改） | 既有內容全保留；新增 `UNASSIGNED` / `grow` / `find_orphans` / `close`。 |
| `tools/baker/src/lib.rs`（改） | 只加兩行 `pub mod`。 |
| `tools/baker/examples/seed_probe.rs`（新，拋棄式） | 反推色標 → 跑管線 → 印統計 → 寫兩張 PNG。 |
| `docs/specs/baker-seeds.md`（改） | Task 6 補「Phase 0 實測結果」與 GO/NO-GO。 |

`segment.rs` 目前 223 行，加完約 380 行。Phase 1 刪掉 `label_regions` / `color_histogram` 之後會回到 250 行左右，不需要拆檔。

---

## Task 1: `binarize` — 線稿二值化

**Files:**
- Create: `tools/baker/src/binarize.rs`
- Modify: `tools/baker/src/lib.rs`（`pub mod` 清單）

**Interfaces:**
- Consumes: 無
- Produces: `baker::binarize::line_mask(rgba: &[u8], threshold: u8) -> Vec<bool>`、`baker::binarize::line_ratio(mask: &[bool]) -> f32`、常數 `DEFAULT_LINE_THRESHOLD: u8 = 128`、`MAX_LINE_RATIO: f32 = 0.35`

- [ ] **Step 1: 建檔並寫失敗測試**

建立 `tools/baker/src/binarize.rs`：

```rust
//! 線稿二值化（`specs/baker-seeds.md §3`）。
//!
//! `lineart.png` 是透明底交付（`assets-spec §4.1`），所以 alpha 就是線的覆蓋度，
//! 不需要看 RGB。線稿的顏色不限純黑，看 luma 會把深褐色線稿判得比黑線稿淡。

/// `lineart.alpha ≥` 此值視為線。
pub const DEFAULT_LINE_THRESHOLD: u8 = 128;

/// 線像素佔比超過此值 → `line-coverage` 警告（§4.1）。
pub const MAX_LINE_RATIO: f32 = 0.35;

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
```

在 `tools/baker/src/lib.rs` 的 `pub mod` 清單加一行（清單是字母序，`binarize` 排在 `check` 前）：

```rust
pub mod binarize;
pub mod check;
```

- [ ] **Step 2: 跑測試確認失敗**

```bash
cargo test -p colorlull-baker binarize
```

Expected: 編譯失敗，`cannot find function 'line_mask' in this scope`（四條測試都是）。

- [ ] **Step 3: 寫最小實作**

在 `binarize.rs` 的 `#[cfg(test)]` 之前加入：

```rust
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
```

- [ ] **Step 4: 跑測試確認通過**

```bash
cargo test -p colorlull-baker binarize
```

Expected: `test result: ok. 4 passed`

- [ ] **Step 5: 確認 clippy 乾淨**

```bash
cargo clippy -p colorlull-baker --all-targets
```

Expected: 無 warning（workspace 設定是 `deny`，有 warning 就是 error）。

- [ ] **Step 6: Commit**

```bash
git add tools/baker/src/binarize.rs tools/baker/src/lib.rs
git commit -m "feat(m0): baker 新增線稿二值化模組"
```

---

## Task 2: `seeds` — 色標圖解析

**Files:**
- Create: `tools/baker/src/seeds.rs`
- Modify: `tools/baker/src/lib.rs`

**Interfaces:**
- Consumes: 無
- Produces: `baker::seeds::Seed { anchor: (u32, u32), color: [u8; 3], solid_area: u32 }`（`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`）、`baker::seeds::read(rgba: &[u8], width: u32, height: u32) -> Vec<Seed>`、常數 `MIN_SEED_AREA: u32 = 64`

- [ ] **Step 1: 建檔並寫失敗測試**

建立 `tools/baker/src/seeds.rs`：

```rust
//! 色標圖解析（`specs/baker-seeds.md §2.2`）。

use std::cmp::Reverse;
use std::collections::HashMap;

/// 色標的 `alpha == 255` 面積下限（母帶）。低於此值取不出可靠的眾數色。
pub const MIN_SEED_AREA: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed {
    /// flood fill 的起點。色標塊的重心；重心若落在塊外（凹形色標）則取塊內最接近重心者。
    pub anchor: (u32, u32),
    /// 塊內 `alpha == 255` 像素的眾數色。
    pub color: [u8; 3],
    /// 塊內 `alpha == 255` 的像素數。
    pub solid_area: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(r, g, b, a)` 的扁平陣列建圖。
    fn img(px: &[[u8; 4]]) -> Vec<u8> {
        px.iter().flatten().copied().collect()
    }

    const T: [u8; 4] = [0, 0, 0, 0];
    const R: [u8; 4] = [255, 0, 0, 255];
    const G: [u8; 4] = [0, 255, 0, 255];

    /// 兩個分離的色標 → 兩個 seed，依 anchor 的 raster order 排序（§3.1 ③）。
    #[test]
    fn separate_dots_become_separate_seeds_in_raster_order() {
        // 3x3：左上一點綠、右下一點紅
        let rgba = img(&[G, T, T, T, T, T, T, T, R]);
        let seeds = read(&rgba, 3, 3);
        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[0].anchor, (0, 0));
        assert_eq!(seeds[0].color, [0, 255, 0]);
        assert_eq!(seeds[1].anchor, (2, 2));
        assert_eq!(seeds[1].color, [255, 0, 0]);
    }

    /// **抗鋸齒痛點消失的機制**：半透明的過渡色不計入眾數，也不計入 solid_area。
    #[test]
    fn antialiased_fringe_does_not_affect_the_mode_colour() {
        // 1x4：三個實心紅 ＋ 一個半透明的髒色
        let dirty = [200, 40, 40, 128];
        let rgba = img(&[R, R, R, dirty]);
        let seeds = read(&rgba, 4, 1);
        assert_eq!(seeds.len(), 1, "半透明像素與實心像素相連，是同一個色標");
        assert_eq!(seeds[0].color, [255, 0, 0]);
        assert_eq!(seeds[0].solid_area, 3);
    }

    /// 凹形色標（C 形）的重心落在塊外。anchor 直接用重心的話 flood fill
    /// 會從色標外面起跑，整個區域判定就錯了。
    #[test]
    fn anchor_of_a_concave_dot_stays_inside_the_dot() {
        // 3x3 的 C 形：中間與右中是空的
        let rgba = img(&[R, R, R, R, T, T, R, R, R]);
        let seeds = read(&rgba, 3, 3);
        assert_eq!(seeds.len(), 1);
        let (x, y) = seeds[0].anchor;
        let i = (y * 3 + x) as usize;
        assert_eq!(rgba[i * 4 + 3], 255, "anchor ({x},{y}) 落在色標外");
    }

    /// 平手規則必須釘死：`HashMap` 的迭代順序不確定，不釘死的話同一張圖
    /// 每次跑可能得到不同的建議色，golden test 會隨機紅。
    #[test]
    fn a_tie_in_the_histogram_goes_to_the_lexicographically_smaller_colour() {
        let rgba = img(&[R, G]);
        let seeds = read(&rgba, 2, 1);
        assert_eq!(seeds[0].color, [0, 255, 0], "各 1 px 平手，取字典序小者");
    }

    #[test]
    fn a_blank_image_yields_no_seeds() {
        assert!(read(&img(&[T, T, T, T]), 2, 2).is_empty());
    }
}
```

在 `lib.rs` 的 `pub mod` 清單加 `pub mod seeds;`（字母序排在 `segment` 前）。

- [ ] **Step 2: 跑測試確認失敗**

```bash
cargo test -p colorlull-baker seeds
```

Expected: 編譯失敗，`cannot find function 'read' in this scope`。

- [ ] **Step 3: 寫實作**

在 `seeds.rs` 的 `#[cfg(test)]` 之前加入：

```rust
/// `alpha > 0` 的 4-連通塊即一個色標。
///
/// 回傳依 `anchor` 的 raster order 排序——region id 因此與繪師點色標的先後無關。
pub fn read(rgba: &[u8], width: u32, height: u32) -> Vec<Seed> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut blob: Vec<usize> = Vec::new();

    for start in 0..w * h {
        if seen[start] || rgba[start * 4 + 3] == 0 {
            continue;
        }
        blob.clear();
        seen[start] = true;
        stack.push(start);
        while let Some(p) = stack.pop() {
            blob.push(p);
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<usize>| {
                if !seen[n] && rgba[n * 4 + 3] > 0 {
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
        out.push(describe(&blob, rgba, w));
    }

    out.sort_by_key(|s| (s.anchor.1, s.anchor.0));
    out
}

fn describe(blob: &[usize], rgba: &[u8], w: usize) -> Seed {
    let mut hist: HashMap<[u8; 3], u32> = HashMap::new();
    let (mut sx, mut sy) = (0u64, 0u64);
    for &p in blob {
        sx += (p % w) as u64;
        sy += (p / w) as u64;
        if rgba[p * 4 + 3] == 255 {
            let rgb = [rgba[p * 4], rgba[p * 4 + 1], rgba[p * 4 + 2]];
            *hist.entry(rgb).or_default() += 1;
        }
    }
    let solid_area: u32 = hist.values().sum();
    // 平手取字典序較小的顏色——見 `a_tie_in_the_histogram_...` 測試。
    let color = hist
        .iter()
        .max_by_key(|(rgb, n)| (**n, Reverse(**rgb)))
        .map(|(rgb, _)| *rgb)
        .unwrap_or([0, 0, 0]);

    let n = blob.len() as u64;
    let (cx, cy) = ((sx / n) as i64, (sy / n) as i64);
    let anchor = blob
        .iter()
        .map(|&p| ((p % w) as u32, (p / w) as u32))
        .min_by_key(|&(x, y)| {
            let (dx, dy) = (x as i64 - cx, y as i64 - cy);
            (dx * dx + dy * dy, y, x)
        })
        .expect("blob 至少有一個像素");

    Seed {
        anchor,
        color,
        solid_area,
    }
}
```

- [ ] **Step 4: 跑測試確認通過**

```bash
cargo test -p colorlull-baker seeds
```

Expected: `test result: ok. 5 passed`

- [ ] **Step 5: clippy**

```bash
cargo clippy -p colorlull-baker --all-targets
```

Expected: 無 warning。

- [ ] **Step 6: Commit**

```bash
git add tools/baker/src/seeds.rs tools/baker/src/lib.rs
git commit -m "feat(m0): baker 新增色標圖解析"
```

---

## Task 3: `segment::grow` — 逐 seed flood fill 與碰撞偵測

**Files:**
- Modify: `tools/baker/src/segment.rs`（在檔案結尾的 `#[cfg(test)]` 之前插入）

**Interfaces:**
- Consumes: `baker::seeds::Seed`
- Produces: `baker::segment::UNASSIGNED: u32`、`baker::segment::Grown { labels: Vec<u32>, collisions: Vec<(u32, u32)>, on_line: Vec<u32> }`、`baker::segment::grow(seeds: &[Seed], line: &[bool], width: u32, height: u32) -> Grown`

- [ ] **Step 1: 寫失敗測試**

在 `segment.rs` 既有的 `mod tests` **內部**加入（既有的 `use super::*;` 已在，另外需要 `use crate::seeds::Seed;`）：

```rust
    fn seed(x: u32, y: u32) -> Seed {
        Seed {
            anchor: (x, y),
            color: [0, 0, 0],
            solid_area: 100,
        }
    }

    /// 一條垂直線把 3x1 切成左右兩半，兩個 seed 各自佔一半。
    #[test]
    fn two_seeds_separated_by_a_line_get_their_own_regions() {
        let line = vec![false, true, false];
        let g = grow(&[seed(0, 0), seed(2, 0)], &line, 3, 1);
        assert_eq!(g.labels, vec![0, UNASSIGNED, 1]);
        assert!(g.collisions.is_empty());
        assert!(g.on_line.is_empty());
    }

    /// **線稿缺口的症狀**：線沒把兩邊隔開，第二個 seed 撞進第一個的區域。
    /// 必須報 collision 而不是靜默切一條分界線——那是繪師唯一需要知道的事。
    #[test]
    fn a_gap_in_the_line_is_reported_as_a_collision() {
        let line = vec![false, false, false];
        let g = grow(&[seed(0, 0), seed(2, 0)], &line, 3, 1);
        assert_eq!(g.collisions, vec![(0, 1)], "先佔住的是 seed 0");
        assert_eq!(g.labels, vec![0, 0, 0], "整條仍歸 seed 0，preview 才畫得出來");
    }

    /// anchor 落在線上 → flood fill 起不來，回報而非 panic。
    #[test]
    fn a_seed_sitting_on_the_line_is_reported_and_skipped() {
        let line = vec![false, true, false];
        let g = grow(&[seed(1, 0)], &line, 3, 1);
        assert_eq!(g.on_line, vec![0]);
        assert!(g.labels.iter().all(|&v| v == UNASSIGNED));
    }

    /// 沒有 seed 的封閉區維持未指派——`find_orphans` 之後才判定它是不是漏點。
    #[test]
    fn a_closed_area_with_no_seed_stays_unassigned() {
        // 5x1：左半有 seed，右半被線隔開且沒有 seed
        let line = vec![false, false, true, false, false];
        let g = grow(&[seed(0, 0)], &line, 5, 1);
        assert_eq!(g.labels, vec![0, 0, UNASSIGNED, UNASSIGNED, UNASSIGNED]);
    }
```

- [ ] **Step 2: 跑測試確認失敗**

```bash
cargo test -p colorlull-baker segment::tests
```

Expected: 編譯失敗，`cannot find function 'grow'` 與 `cannot find value 'UNASSIGNED'`。

- [ ] **Step 3: 寫實作**

在 `segment.rs` 的 `#[cfg(test)]` 之前加入（檔頭補 `use crate::seeds::Seed;`）：

```rust
/// `labels` 裡代表「還沒有 region id」的值。
pub const UNASSIGNED: u32 = u32::MAX;

#[derive(Debug, Clone)]
pub struct Grown {
    /// 每像素的 region id，`UNASSIGNED` 表示未指派。索引即 seed 在輸入切片的位置。
    pub labels: Vec<u32>,
    /// `(先佔住該封閉區的 seed id, 撞進來的 seed id)`。線稿有缺口的證據。
    pub collisions: Vec<(u32, u32)>,
    /// anchor 落在線像素上的 seed id。
    pub on_line: Vec<u32>,
}

/// 逐 seed 4-連通 flood fill，只走非線像素。
///
/// 用逐 seed 而非多源同步 BFS：線稿封閉時兩者等價，不封閉時只有逐 seed
/// 能指出「哪兩個 seed 連在一起」。同步 BFS 會在中間切一條任意分界線然後
/// 靜默通過——那正是要避免的失敗模式（`baker-seeds §3.1 ①`）。
pub fn grow(seeds: &[Seed], line: &[bool], width: u32, height: u32) -> Grown {
    let (w, h) = (width as usize, height as usize);
    let mut labels = vec![UNASSIGNED; w * h];
    let mut collisions = Vec::new();
    let mut on_line = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for (id, s) in seeds.iter().enumerate() {
        let id = id as u32;
        let start = s.anchor.1 as usize * w + s.anchor.0 as usize;
        if line[start] {
            on_line.push(id);
            continue;
        }
        if labels[start] != UNASSIGNED {
            collisions.push((labels[start], id));
            continue;
        }

        labels[start] = id;
        stack.push(start);
        while let Some(p) = stack.pop() {
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
    }

    Grown {
        labels,
        collisions,
        on_line,
    }
}
```

- [ ] **Step 4: 跑測試確認通過**

```bash
cargo test -p colorlull-baker segment
```

Expected: `ok`，既有的 5 條 segment 測試 ＋ 新增 4 條全綠。

- [ ] **Step 5: clippy**

```bash
cargo clippy -p colorlull-baker --all-targets
```

- [ ] **Step 6: Commit**

```bash
git add tools/baker/src/segment.rs
git commit -m "feat(m0): baker 新增逐 seed flood fill 與碰撞偵測"
```

---

## Task 4: `segment::find_orphans` 與 `segment::close`

**Files:**
- Modify: `tools/baker/src/segment.rs`

**Interfaces:**
- Consumes: `segment::UNASSIGNED`、`segment::grow` 產出的 `labels`
- Produces: `baker::segment::Orphan { area: u32, anchor: (u32, u32) }`、`baker::segment::find_orphans(labels: &[u32], line: &[bool], width: u32, height: u32) -> Vec<Orphan>`、`baker::segment::close(labels: &mut [u32], width: u32, height: u32) -> (u32, u32)`

**呼叫順序有硬性要求**：`grow` → `find_orphans` → `close`。`close` 會把 id 擴散到所有未指派像素，包含孤兒區——先 close 再找孤兒，孤兒就消失了。

- [ ] **Step 1: 寫失敗測試**

在 `segment.rs` 的 `mod tests` 內加入：

```rust
    /// 沒有 seed 的封閉區被列為孤兒，面積遞減排序。
    #[test]
    fn unseeded_areas_are_listed_largest_first() {
        // 7x1：[有主][線][孤兒 x1][線][孤兒 x2]
        let line = vec![false, true, false, true, false, false, false];
        let labels = vec![0, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED, UNASSIGNED];
        let orphans = find_orphans(&labels, &line, 7, 1);
        assert_eq!(orphans.len(), 2);
        assert_eq!(orphans[0].area, 3);
        assert_eq!(orphans[0].anchor, (4, 0));
        assert_eq!(orphans[1].area, 1);
        assert_eq!(orphans[1].anchor, (2, 0));
    }

    /// 線像素本身不是孤兒——它們由 `close` 指派。
    #[test]
    fn line_pixels_are_not_orphans() {
        let line = vec![false, true, false];
        let labels = vec![0, UNASSIGNED, 1];
        assert!(find_orphans(&labels, &line, 3, 1).is_empty());
    }

    /// `close` 把線像素瓜分給兩側，平手取較小 id——確定性的來源。
    #[test]
    fn close_splits_the_line_and_breaks_ties_toward_the_smaller_id() {
        // 4x1：[0][線][線][1]，兩個線像素各由一側吃掉
        let mut labels = vec![0, UNASSIGNED, UNASSIGNED, 1];
        let (rounds, left) = close(&mut labels, 4, 1);
        assert_eq!(labels, vec![0, 0, 1, 1]);
        assert_eq!((rounds, left), (1, 0));
    }

    /// 寬度為奇數的線：正中央的像素兩側等距，必須歸較小 id。
    #[test]
    fn the_middle_of_an_odd_width_line_goes_to_the_smaller_id() {
        let mut labels = vec![0, UNASSIGNED, UNASSIGNED, UNASSIGNED, 1];
        let (rounds, left) = close(&mut labels, 5, 1);
        assert_eq!(labels, vec![0, 0, 0, 1, 1], "第 2 輪時中央同時鄰接 0 與 1，取 0");
        assert_eq!((rounds, left), (2, 0));
    }

    /// 一個 seed 都沒有 → 沒有東西可以擴散，回報剩餘量而不是無限迴圈。
    #[test]
    fn close_terminates_when_there_is_nothing_to_grow_from() {
        let mut labels = vec![UNASSIGNED; 4];
        assert_eq!(close(&mut labels, 4, 1), (0, 4));
    }
```

- [ ] **Step 2: 跑測試確認失敗**

```bash
cargo test -p colorlull-baker segment
```

Expected: 編譯失敗，`cannot find function 'find_orphans'` / `'close'`。

- [ ] **Step 3: 寫實作**

在 `segment.rs` 的 `#[cfg(test)]` 之前加入：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orphan {
    pub area: u32,
    /// raster order 的第一個像素。診斷座標用，不需要是重心。
    pub anchor: (u32, u32),
}

/// 沒有任何 seed 認領的自由區（非線像素且未指派）的 4-連通塊。
///
/// **必須在 `close` 之前呼叫**——`close` 會把 id 擴散進這些區域，之後就找不到了。
/// 回傳依面積遞減排序，平手取 anchor 的 raster order（§5「可疑度排序」）。
pub fn find_orphans(labels: &[u32], line: &[bool], width: u32, height: u32) -> Vec<Orphan> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    let free = |i: usize| !line[i] && labels[i] == UNASSIGNED;

    for start in 0..w * h {
        if seen[start] || !free(start) {
            continue;
        }
        let mut area = 0u32;
        seen[start] = true;
        stack.push(start);
        while let Some(p) = stack.pop() {
            area += 1;
            let (x, y) = (p % w, p / w);
            let mut visit = |n: usize, stack: &mut Vec<usize>| {
                if !seen[n] && free(n) {
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
        out.push(Orphan {
            area,
            anchor: ((start % w) as u32, (start / w) as u32),
        });
    }

    out.sort_by_key(|o| (Reverse(o.area), o.anchor.1, o.anchor.0));
    out
}

/// 同步 BFS 波前：每輪把所有「與已標記像素相鄰的未指派像素」一次指派完，
/// 取鄰居中**最小的 id**。
///
/// 同步是為了確定性——逐像素推進的話結果依賴掃描順序，golden test 守不住。
/// 取最小 id 讓等距處的分界線有唯一解，視覺上落在線的中軸。
///
/// 回傳 `(輪數, 收斂後仍未指派的像素數)`。剩餘量非 0 代表整張圖有連一個 seed
/// 都碰不到的孤島。
pub fn close(labels: &mut [u32], width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width as usize, height as usize);
    let mut rounds = 0u32;
    let mut pending: Vec<(usize, u32)> = Vec::new();

    loop {
        pending.clear();
        for p in 0..w * h {
            if labels[p] != UNASSIGNED {
                continue;
            }
            let (x, y) = (p % w, p / w);
            let mut best = UNASSIGNED;
            let mut consider = |n: usize, best: &mut u32| {
                if labels[n] != UNASSIGNED {
                    *best = (*best).min(labels[n]);
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
            if best != UNASSIGNED {
                pending.push((p, best));
            }
        }
        if pending.is_empty() {
            break;
        }
        for &(p, id) in &pending {
            labels[p] = id;
        }
        rounds += 1;
    }

    let left = labels.iter().filter(|&&v| v == UNASSIGNED).count() as u32;
    (rounds, left)
}
```

檔頭需要 `use std::cmp::Reverse;`（`segment.rs` 目前只 import 了 `std::collections::HashMap`）。

- [ ] **Step 4: 跑測試確認通過**

```bash
cargo test -p colorlull-baker segment
```

Expected: `ok`，14 條全綠。

- [ ] **Step 5: clippy**

```bash
cargo clippy -p colorlull-baker --all-targets
```

- [ ] **Step 6: Commit**

```bash
git add tools/baker/src/segment.rs
git commit -m "feat(m0): baker 新增孤兒區偵測與測地擴張封閉"
```

---

## Task 5: `seed_probe` example — 反推色標並跑完管線

**Files:**
- Create: `tools/baker/examples/seed_probe.rs`

**Interfaces:**
- Consumes: `baker::image::{Image, PngOptions, encode_rgba}`、`baker::binarize::{line_mask, line_ratio, DEFAULT_LINE_THRESHOLD, MAX_LINE_RATIO}`、`baker::seeds::Seed`、`baker::segment::{self, UNASSIGNED, label_regions}`
- Produces: 無（拋棄式二進位）

**這支 example 沒有單元測試**——它的驗證方式是 Task 6 的實測與目視。所有有邏輯的部分都已在 Task 1–4 用單元測試蓋掉了；這裡只有 IO 與統計加總。

- [ ] **Step 1: 寫 example**

建立 `tools/baker/examples/seed_probe.rs`：

```rust
//! Phase 0 可行性驗證（`specs/baker-seeds.md §7`）。**驗證完即刪。**
//!
//! 從現有的 `flats.png` + `reference.png` 反推色標，餵進 seeds 管線，回答唯一的問題：
//! **線稿的封閉區推得出跟 flats 一樣的區域嗎？**
//!
//! ```text
//! cargo run --release -p colorlull-baker --example seed_probe -- \
//!     assets/source/adventure-time-demo-1 /tmp/probe
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};
use baker::binarize::{DEFAULT_LINE_THRESHOLD, MAX_LINE_RATIO, line_mask, line_ratio};
use baker::image::{Image, PngOptions, encode_rgba};
use baker::seeds::Seed;
use baker::segment::{self, UNASSIGNED};

/// 與 `baker-seeds.md §3.3` 同值。低於此面積的自由區在 Phase 1 會併入鄰居。
const MIN_ORPHAN_AREA: u32 = 500;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let src: PathBuf = args.next().context("用法：seed_probe <素材目錄> <輸出目錄>")?.into();
    let out: PathBuf = args.next().context("用法：seed_probe <素材目錄> <輸出目錄>")?.into();
    std::fs::create_dir_all(&out)?;

    let lineart = Image::load(&src.join("lineart.png"))?;
    let flats = Image::load(&src.join("flats.png"))?;
    let reference = Image::load(&src.join("reference.png"))?;
    let (w, h) = (lineart.width, lineart.height);
    println!("素材 {}×{}", w, h);

    // ── 反推色標 ────────────────────────────────────────────────────
    let seeds = seeds_from_flats(&flats, &reference);
    println!("flats 區域數 / 反推色標數：{}", seeds.len());

    // ── 二值化 ──────────────────────────────────────────────────────
    let line = line_mask(&lineart.rgba, DEFAULT_LINE_THRESHOLD);
    let ratio = line_ratio(&line);
    println!(
        "線像素佔比：{:.2}%{}",
        ratio * 100.0,
        if ratio > MAX_LINE_RATIO { "  ← 超過 MAX_LINE_RATIO" } else { "" }
    );

    // ── grow → find_orphans → close（順序不可換）────────────────────
    let mut g = segment::grow(&seeds, &line, w, h);
    println!("collision：{} 組（線稿缺口）", g.collisions.len());
    for &(first, second) in g.collisions.iter().take(20) {
        println!(
            "  seed {} @{:?}  ←  seed {} @{:?}",
            first, seeds[first as usize].anchor, second, seeds[second as usize].anchor
        );
    }
    if g.collisions.len() > 20 {
        println!("  …另有 {} 組未列出", g.collisions.len() - 20);
    }
    println!("anchor 落在線上：{} 個", g.on_line.len());

    let orphans = segment::find_orphans(&g.labels, &line, w, h);
    let big: Vec<_> = orphans.iter().filter(|o| o.area >= MIN_ORPHAN_AREA).collect();
    let total_px = (w as u64) * (h as u64);
    let big_area: u64 = big.iter().map(|o| o.area as u64).sum();
    println!(
        "orphan(≥{MIN_ORPHAN_AREA}px)：{} 塊，合計 {} px（{:.3}%）",
        big.len(),
        big_area,
        big_area as f64 / total_px as f64 * 100.0
    );
    for o in big.iter().take(20) {
        println!("  {} px @{:?}", o.area, o.anchor);
    }

    let (rounds, left) = segment::close(&mut g.labels, w, h);
    println!("close：{rounds} 輪，剩餘未指派 {left} px");

    // ── 目視產物 ────────────────────────────────────────────────────
    write_png(&out.join("preview.png"), &preview(&g.labels), w, h)?;
    write_png(
        &out.join("overlay.png"),
        &overlay(&lineart.rgba, &seeds, &g.collisions, &big, w, h),
        w,
        h,
    )?;
    println!("已寫出 {}/preview.png 與 overlay.png", out.display());
    Ok(())
}

/// 每個 flats 區域取一個「最接近重心且仍在區域內」的像素當 anchor，
/// 顏色取 `reference` 在該點的值——正是繪師在 B 案下會做的事。
fn seeds_from_flats(flats: &Image, reference: &Image) -> Vec<Seed> {
    let map = segment::label_regions(&flats.rgba, flats.width, flats.height);
    let w = flats.width as usize;
    let n = map.count as usize;
    let (mut sx, mut sy) = (vec![0u64; n], vec![0u64; n]);
    for (i, &id) in map.labels.iter().enumerate() {
        sx[id as usize] += (i % w) as u64;
        sy[id as usize] += (i / w) as u64;
    }
    let mut best = vec![(u64::MAX, 0usize); n];
    for (i, &id) in map.labels.iter().enumerate() {
        let id = id as usize;
        let a = map.areas[id].max(1) as u64;
        let (cx, cy) = ((sx[id] / a) as i64, (sy[id] / a) as i64);
        let (dx, dy) = ((i % w) as i64 - cx, (i / w) as i64 - cy);
        let d = (dx * dx + dy * dy) as u64;
        if d < best[id].0 {
            best[id] = (d, i);
        }
    }
    let mut out: Vec<Seed> = best
        .iter()
        .enumerate()
        .map(|(id, &(_, i))| Seed {
            anchor: ((i % w) as u32, (i / w) as u32),
            color: [
                reference.rgba[i * 4],
                reference.rgba[i * 4 + 1],
                reference.rgba[i * 4 + 2],
            ],
            solid_area: map.areas[id],
        })
        .collect();
    out.sort_by_key(|s| (s.anchor.1, s.anchor.0));
    out
}

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

/// 線稿合成到白底 ＋ 色標位置（實心方塊）＋ 診斷標記
/// （collision 的兩個 seed 畫紅框、orphan 畫黃框）。
fn overlay(
    lineart: &[u8],
    seeds: &[Seed],
    collisions: &[(u32, u32)],
    orphans: &[&segment::Orphan],
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
    for &(first, second) in collisions {
        for id in [first, second] {
            stamp(&mut out, w, h, seeds[id as usize].anchor, 40, [255, 0, 0], true);
        }
    }
    for o in orphans {
        stamp(&mut out, w, h, o.anchor, 40, [255, 220, 0], true);
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

fn write_png(path: &std::path::Path, rgba: &[u8], w: u32, h: u32) -> Result<()> {
    let bytes = encode_rgba(rgba, w, h, PngOptions::default())?;
    std::fs::write(path, bytes).with_context(|| format!("寫入 {} 失敗", path.display()))?;
    Ok(())
}
```

- [ ] **Step 2: 確認編譯與 clippy**

```bash
cargo clippy -p colorlull-baker --all-targets
```

Expected: 無 warning。若 `PngOptions::default()` 的欄位或 `encode_rgba` 簽章與此處不符，以 `tools/baker/src/image.rs` 的實際定義為準修正呼叫端，不要改 `image.rs`。

- [ ] **Step 3: 用合成小圖確認跑得動（不碰 LFS 素材）**

先確認 example 在一組人工小圖上不會 panic。建立暫存素材：

```bash
mkdir -p /tmp/probe-smoke/src
cargo run -q -p colorlull-baker --example seed_probe -- /tmp/probe-smoke/src /tmp/probe-smoke/out
```

Expected: 因為目錄裡沒有 PNG，以「讀不到檔」的錯誤訊息退出（**不是 panic、不是 index out of bounds**）。這一步只驗參數處理與錯誤路徑。

- [ ] **Step 4: Commit**

```bash
git add tools/baker/examples/seed_probe.rs
git commit -m "feat(m0): 新增 seed_probe 可行性驗證 example"
```

---

## Task 6: 實測、記錄結論、GO / NO-GO

**Files:**
- Modify: `docs/specs/baker-seeds.md`（§7 之下新增「Phase 0 實測結果」一節）

**Interfaces:**
- Consumes: Task 5 的 example
- Produces: 一個明確的 GO / NO-GO 決定

- [ ] **Step 1: 對 demo 素材跑一次**

```bash
cargo run --release -p colorlull-baker --example seed_probe -- \
    assets/source/adventure-time-demo-1 /tmp/probe 2>&1 | tee /tmp/probe/report.txt
```

用 `--release`：母帶 12.6M 像素，debug build 的 `close` 會慢到以分鐘計。

**注意**：`assets/source/adventure-time-demo-1/flats.png` 目前有未提交的修改（`M` 狀態），而 `CLAUDE.md` 記載這張圖「待重做」。跑之前先確認它是不是你想驗的那一版；如果它本身就是壞的，Phase 0 的結論不可信。必要時 `git stash` 掉該檔的修改，用已提交的版本跑一次做對照。

- [ ] **Step 2: 目視兩張輸出**

開啟 `/tmp/probe/preview.png` 與 `/tmp/probe/overlay.png`。

`preview.png` 要看的是：**有沒有兩塊本該分開的區域變成同一個顏色。** 這是漏色的直接症狀。
`overlay.png` 要看的是：**每個紅框（collision）附近，線稿是不是真的缺一筆。**

- [ ] **Step 3: 依判準做決定**

**GO**（走 Phase 1）：
- collision 組數 ≤ 色標數的 5%，**且**逐一目視確認每組都對應「線稿確實該補一筆」的缺口
- `close` 剩餘未指派 = 0
- `preview.png` 上沒有大面積的融塊

**NO-GO**（退回 spec §8 的量化 snap 退路）：
- collision 組數 > 色標數的 20%
- 或 collision 集中在**刻意的開放線條**（裝飾線、斷筆風格）——那代表這個畫風下線稿不封閉是常態，不是繪師疏忽，逼繪師補線等於改變畫風

**灰色地帶（5%–20%）**：不要自己決定，把數字與 `overlay.png` 交給使用者判斷。

`orphan` 的數量**不是**判準，而是一個要記錄的觀察：probe 的色標是從 flats 每區自動生成的，每區都有種子，所以 orphan 只會來自「線稿把 flats 的一個區域切成更多塊」。orphan 多 = **線稿比 flats 更細**，B 案下區域數會比現在多。這是有用的發現，不是失敗。

- [ ] **Step 4: 把結論寫進 spec**

在 `docs/specs/baker-seeds.md` 的 §7 Phase 0 段落之後，插入一節（把 `<...>` 換成實際數字，不要留佔位符）：

```markdown
### Phase 0 實測結果（2026-08-XX，`adventure-time-demo-1`）

| 指標 | 數值 |
|---|---|
| 母帶尺寸 | 3072×4096 |
| flats 區域數 / 反推色標數 | <N> |
| 線像素佔比 | <R>% |
| collision | <K> 組（占色標數 <P>%） |
| orphan ≥500px | <M> 塊，合計 <A> px（<Q>%） |
| close 輪數 / 剩餘未指派 | <T> / <L> |

**判定：GO / NO-GO。** <一到三句話說明理由；NO-GO 的話寫明是哪一條判準命中。>

<若 GO：collision 逐組的目視結論，例如「3 組全部落在髮尾與背景交界，線稿確實斷了 2–4px」。>
<若有 orphan：說明它們是不是「線稿比 flats 更細」造成的，以及 B 案下區域數的預期變化。>
```

- [ ] **Step 5: Commit**

```bash
git add docs/specs/baker-seeds.md
git commit -m "docs(m0): 記錄 baker 色標交付 Phase 0 實測結果"
```

- [ ] **Step 6: 回報並停下**

把表格數字與判定交給使用者。**不要自動往 Phase 1 走**——GO/NO-GO 是產品決定，且 NO-GO 的話後續四個 Phase 要整份重寫。

---

## Phase 1–4（粗顆粒，等 Phase 0 結論後再展開成計畫）

這四個 Phase **刻意不寫細**：Phase 0 的結論會改變它們的內容（例如 collision 偏多但可接受的話，Phase 1 就要加上 trapped-ball 式的缺口封補，而 spec §9 現在把它列為不做）。

| Phase | 內容 | 相依 |
|---|---|---|
| 1 | `source.rs` 改讀 `seeds.png`；`check::master::seeds` 五條新診斷碼；刪 `flats` / `reference` 路徑；`lib.rs::bake` 換管線 | Phase 0 GO |
| 2 | `synth.rs` 改寫（可控線稿缺口、可控漏點）＋ golden test ＋ 不 fail-fast 迴歸 ＋ 差分測試 | Phase 1 |
| 3 | `--debug-out` 四件產物、座標聚類、可疑度排序 | Phase 1 |
| 4 | `assets-spec.md` 重寫 §4.2 §4.3 §5 §6，產出繪師 JD 可直接附的版本 | Phase 2 |

Task 5 的 `seed_probe.rs` 在 Phase 1 開始時刪除；Task 1–4 的三個模組**原地留用**，不需重寫。

---

## Self-Review

**Spec 覆蓋**（對照 `docs/specs/baker-seeds.md`）：
- §2.2 色標讀法（眾數色、`MIN_SEED_AREA`）→ Task 2 ✅
- §3 管線的 binarize / grow / close → Task 1、3、4 ✅
- §3.1 ① 逐 seed 而非同步 BFS → Task 3 測試 `a_gap_in_the_line_is_reported_as_a_collision` ✅
- §3.1 ② close 取最小 id → Task 4 測試 `the_middle_of_an_odd_width_line_...` ✅
- §3.1 ③ 依 anchor raster order 編號 → Task 2 測試 `separate_dots_..._in_raster_order` ✅
- §3.1 ④ 小碎片併入鄰居 → **Phase 0 不做**，`find_orphans` 只回報不合併。Phase 1 的工作，已記在上表。
- §3.3 四個參數 → Phase 0 只用到 `DEFAULT_LINE_THRESHOLD` / `MAX_LINE_RATIO` / `MIN_ORPHAN_AREA`，`MIN_SEED_AREA` 定義了但 probe 不驗（反推的色標沒有真實面積）。`--set` 覆寫是 Phase 1。
- §4 五條新診斷碼 → **Phase 0 不做**，probe 直接印統計。Phase 1。
- §5 `--debug-out` → Phase 0 的 `preview` / `overlay` 是它的原型，正式版是 Phase 3。
- §7 Phase 0 → 本計畫全部 ✅
- §8 退路 → Task 6 Step 3 的 NO-GO 分支 ✅

**型別一致性**：`Seed`（Task 2 定義）在 Task 3 的 `grow` 與 Task 5 的 `seeds_from_flats` 使用，欄位名 `anchor` / `color` / `solid_area` 三處一致。`UNASSIGNED`（Task 3 定義）在 Task 4 的 `find_orphans` / `close` 與 Task 5 的 `preview` 使用。`Orphan`（Task 4 定義）在 Task 5 的 `overlay` 以 `&segment::Orphan` 使用。`Grown` 的三個欄位在 Task 5 全部讀到。

**無佔位符**：Task 6 Step 4 的 `<N>` / `<R>` 等是**要填入實測數字的欄位**，不是未定的設計——這是本計畫唯一的模板，且 Step 4 明寫「不要留佔位符」。
