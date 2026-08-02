# Color It — 架構設計文件

> 狀態：草案 v0.2
> 相關文件：[prd.md](./prd.md)｜[roadmap.md](./roadmap.md)

> **v1 平台範圍：iOS only。**
> 本文件中所有 Android 相關的規格（Compose、`SurfaceView`、`Choreographer`、Auto Backup、Vulkan / GLES）**全部保留為未來規格**，v1 不實作。
> 保留而非刪除的理由：架構的跨平台性正是 Rust 核心的存在理由，刪掉等於把未來的成本藏起來。實際排程見 `roadmap.md §6`。

---

## 目錄

1. [技術選型](#1-技術選型)
2. [系統分層](#2-系統分層)
3. [Repo 結構](#3-repo-結構)
4. [渲染模型](#4-渲染模型)
5. [Core crate 設計](#5-core-crate-設計)
6. [邊界拆分](#6-邊界拆分)
7. [契約層](#7-契約層)
8. [狀態與持久化](#8-狀態與持久化)
9. [資產管線](#9-資產管線)
10. [平台整合](#10-平台整合)
11. [雲端](#11-雲端)
12. [建置與 CI](#12-建置與-ci)
13. [效能觀測](#13-效能觀測)
14. [風險與退路](#14-風險與退路)

---

## 1. 技術選型

| 項目 | 選擇 | 理由 |
|---|---|---|
| 核心語言 | **Rust** | `uniffi` 自動產生 Swift + Kotlin binding，這是 FFI 邊界最大的省力點。記憶體安全對長時間執行的畫布狀態有實質價值 |
| 圖形 API | **wgpu** | 薄 RHI，非 2D 引擎。一套 shader 打 **Metal + Vulkan** + GLES fallback（Android 舊機保命） |
| Shader | **WGSL** | 單一來源，由 naga 編譯至各後端 |
| iOS UI | **SwiftUI** ＋ `MTKView` | 原生手感、原生手勢、原生 IAP |
| Android UI | **Compose** ＋ `SurfaceView` | 同上 |
| 資產管線 | **Rust CLI**（`tools/baker`） | 與 runtime 共用 `colorpack` crate，格式定義不會漂移 |
| 建置協調 | **`cargo xtask`** | Rust → iOS/Android 的產物生成流程複雜，需要單一入口 |

### 為什麼不選（決策留檔）

這一節存在的目的是避免半年後有人重問同樣的問題。

#### ❌ libmypaint

- 它是為**無邊界畫布 ＋ 壓感自由繪畫 ＋ 自然媒材模擬**設計的，與本產品的「受區域約束的塗抹」是不同問題。
- 它提供 dab 參數運算，**但渲染後端（`MyPaintSurface`）本來就要我們自己實作**——也就是最難的部分它沒有解決。
- 它帶來一整套用不到的東西：64×64 CPU tile 架構、`.myb` 格式、約 50 個 brush setting、OpenRaster。
- 我們只有 5 支筆刷，參數直接寫死成 `BrushPreset` 常數即可，不需要參數空間。
- **結論：port 它比自己寫更貴。** 保留作為 dab dynamics 數學的參考教材（`mypaint-brush.c` 的壓感映射曲線設計良好），但不引入為相依。
- 附註：授權需分開確認——libmypaint  本體為 ISC，但 MyPaint 應用本體為 GPLv2+，筆刷資產包另有授權。

#### ❌ Skia

- 曾為合理候選：提供 GPU surface、blend mode、path、圖片解碼，能省下 RHI 的工作。
- 否決理由：dab stamping 走 `drawImage` 迴圈的 draw call 開銷在高更新率下不如自寫 instanced quad；紋理筆刷的細緻控制有天花板；iOS 端額外背 5–8MB 二進位。
- 我們的渲染需求其實很窄（幾個 pass、一個 composite shader），用不到 Skia 90% 的能力。

#### ❌ Flutter / Impeller（Dart）

- 高頻觸控的延遲路徑不如原生可控，而延遲是本產品的核心體驗。
- Apple Pencil 的 `predictedTouches` 等平台專屬能力難以完整取得。

#### ❌ Kotlin Multiplatform / Compose Multiplatform

- 畫布是原生 surface。CMP on iOS 自行渲染進 Skia-backed `UIView`，在其中嵌 `CAMetalLayer` 並讓高頻觸控穿過 Compose 事件系統，每一層都在增加延遲。
- `predictedTouches`、`coalescedTouches`、`altitudeAngle` 等 UIKit 專屬資料會在抽象層被抹平或延遲。
- frame pacing 機制兩邊本就不同（`CADisplayLink` vs `Choreographer`），共用抽象沒有實益。
- 本產品的 UI chrome 很小（四條路由），共用省下的時間不是成本大頭。
- **已經有 Rust 核心的情況下，再加 KMP 等於 iOS 上背兩套 runtime。**

---

## 2. 系統分層

```
┌──────────────────────────────────────────────────────────────┐
│  App Shell                                                    │
│  SwiftUI (iOS)                    │  Compose (Android)        │
│  Gallery / Canvas / Share / Subscription                      │
├──────────────────────────────────────────────────────────────┤
│  Platform Bridge（各平台一份，不共用）                          │
│  ├─ EngineProtocol   介面定義（Shell 只依賴這個）               │
│  ├─ RustEngine       真實實作                                  │
│  ├─ MockEngine       假實作 ← 讓 Shell 可獨立開發               │
│  ├─ CanvasView       MTKView          │  SurfaceView           │
│  ├─ InputAdapter     UITouch/Pencil   │  MotionEvent           │
│  └─ FrameDriver      CADisplayLink    │  Choreographer         │
├══════════════ FFI（uniffi 由 Rust 生成）══════════════════════┤
│  Core（Rust）                                                  │
│                                                               │
│   engine     ── 對外 facade，唯一 FFI 出口                      │
│   app-state  ── 工具狀態機、色盤、進度                          │
│   document   ── 文件模型、palette、單一 apply 入口               │
│   history    ── Undo：tile COW ＋ palette diff                 │
│   oplog      ── 意圖記錄、序列化、縮時                          │
│   stroke     ── 平滑 → 插值 → Dab 生成（純 CPU、可測試）         │
│   render     ── Render Graph（唯一 import wgpu）                │
│   colorpack  ── 資產包讀取（與 baker 共用）                     │
└──────────────────────────────────────────────────────────────┘

        ┌─────────────────────────────────────┐
        │  tools/baker（離線 CLI）              │
        │  lineart + flats + shade → .colorpack│
        └─────────────────────────────────────┘
```

---

## 3. Repo 結構

> **v1 不建立 `apps/android/`。** 此處列出是為了固定未來的位置，避免 Android 版開工時重新爭論目錄結構。

```
color-it/
├─ core/
│  ├─ colorpack/          資產包格式定義與讀寫（runtime + baker 共用）
│  ├─ stroke/             純 CPU，無 GPU 依賴，golden test
│  ├─ render/             ★ 唯一 import wgpu 的 crate
│  ├─ document/           文件模型 ＋ 單一 apply 入口
│  ├─ history/            Undo/Redo
│  ├─ oplog/              操作紀錄
│  ├─ app-state/          工具與 UI 狀態機
│  └─ engine/             facade ＋ uniffi UDL
│
├─ contracts/             ★ 跨語言／跨版本／跨工具的獨立規格
│  ├─ colorpack.schema.json
│  ├─ oplog.schema.json
│  └─ tokens.json
│
├─ tools/
│  └─ baker/              資產烘焙 CLI
│
├─ apps/
│  ├─ ios/
│  │  ├─ ColorApp/                 SwiftUI：Gallery / Canvas / Share / Subscription
│  │  ├─ EngineBridge/    ★ Platform Bridge（獨立 framework target）
│  │  │   ├─ EngineProtocol.swift
│  │  │   ├─ RustEngine.swift
│  │  │   ├─ MockEngine.swift
│  │  │   ├─ CanvasView.swift
│  │  │   ├─ InputAdapter.swift
│  │  │   └─ FrameDriver.swift
│  │  └─ Generated/                uniffi 產物（gitignore ＋ CI freshness）
│  │
│  ├─ android/
│  │  ├─ app/                      Compose
│  │  ├─ engine-bridge/   ★ Platform Bridge（獨立 Gradle module）
│  │  │   ├─ Engine.kt
│  │  │   ├─ RustEngine.kt
│  │  │   ├─ MockEngine.kt
│  │  │   ├─ CanvasSurfaceView.kt
│  │  │   ├─ InputAdapter.kt
│  │  │   └─ FrameDriver.kt
│  │  └─ generated/                uniffi 產物（gitignore ＋ CI freshness）
│  │
│  └─ web/                         行銷頁（靜態，Astro, GSAP, Cloudflare Pages）
│
├─ assets/
│  ├─ source/                      繪師交付 PNG（Git LFS）
│  └─ packs/                       baker 產物（不進 git，上 R2）
│
├─ docs/
│  ├─ prd.md
│  ├─ architecture.md
│  ├─ roadmap.md
│  └─ contracts.md                 （待補：等 UDL 與 schema 成形後撰寫）
│
└─ xtask/                          建置協調
```

### 目錄歸屬規則

| 目錄 | 放什麼 | 明確不放什麼 |
|---|---|---|
| `core/` | 跨平台共用的函式庫 | 任何平台 SDK 相依 |
| `contracts/` | **只放 uniffi 生不出來的契約**（見 §7） | FFI 型別定義（那是 Rust 的 SSOT） |
| `apps/` | 可建置的產物 | 可重用的邏輯（該往 `core/` 放） |
| `tools/` | 開發期工具 | 執行期會用到的東西 |
| `assets/packs/` | baker 產物 | **不進 git**（走 R2） |

**Platform Bridge 為什麼在 `apps/` 底下而不是獨立頂層目錄**：它是兩份*不共用*的程式碼，且必須綁定各自的 build system（Xcode target / Gradle module）。但它必須是**獨立的 target/module**，不能散落在 App code 裡——理由見 §10.1。

---

## 4. 渲染模型

### 4.1 每份文件持有的 GPU 資源

| 資源 | 格式 | 生命週期 | 說明 |
|---|---|---|---|
| `T_line` | RGBA8 sRGB | 文件 | 線稿。抗鋸齒開啟，Multiply 蓋頂 |
| `T_shade` | RGBA8 sRGB | 文件（選配） | 陰影/質感，Multiply |
| `T_region` | **R16Uint, NEAREST** | 文件 | 區域 ID map |
| `T_paint` | RGBA8 | 文件 | 使用者筆刷結果，持久 |
| `T_erase` | **R8** | 文件 | **底色被擦除的程度**（見 §4.1.2） |
| `T_wet` | **R8** | 單一筆畫 | 當前這一筆的暫存層 |
| `Buf_palette` | `StorageBuffer<vec4>[N]` | 文件 | 每個區域的油漆桶填色 |

`T_region` 有三條不可違反的規則：

1. **只能無損壓縮**（PNG-8 / RLE）。ASTC/ETC2 等有損壓縮會混淆 ID，導致油漆桶填錯區域。
2. **只能 NEAREST 取樣**。任何 filtering 都會產生不存在的 ID。
3. **解析度不得低於線稿**。

### 4.1.1 畫布規格與記憶體預算

只有兩種比例（產品理由見 `prd.md §6`）：

| 比例 | runtime 尺寸 | 像素數 |
|---|---|---|
| 1:1 | 2048 × 2048 | 4.19 M |
| 4:5 | 1536 × 1920 | 2.95 M |

**4:5 的長邊刻意選 1920 而非「與 1:1 等像素」的 2290**——讓直式畫布的記憶體低於 1:1，用 1:1 當作記憶體上界，只需要驗證一個最壞情況。

**1:1（最壞情況）的 GPU 記憶體**：

| 資源 | 大小 |
|---|---|
| `T_line` RGBA8 | 16 MB |
| `T_shade` RGBA8（選配） | 16 MB |
| `T_region` R16 | 8 MB |
| `T_paint` RGBA8 | 16 MB |
| `T_erase` R8 | 4 MB |
| `T_wet` R8 | 4 MB |
| **貼圖小計** | **64 MB** |
| Undo pool（見下） | **上限 64 MB** |
| colorpack 解碼暫存 | 約 16 MB |
| **峰值預算** | **約 145 MB** |

**Undo pool 必須有記憶體上限。** 一筆畫可能碰到 4–16 個 256×256 tile（RGBA8 每 tile 256KB），即 1–4MB／步。若不設限，20 步就可能吃掉 80MB，而使用者可以連續畫數百筆。

> **規則**：undo pool 設 64MB 上限，超過時丟棄最舊的 entry——**undo 深度是動態的**，複雜筆畫的可撤銷步數比簡單筆畫少。這比固定步數更符合記憶體現實。

這份預算是 **E1 的驗收目標**（`roadmap.md` D4）。若真機量測超標，此時調整畫布解析度的代價最低——繪師尚未量產，且母帶是 4096 長邊，重新降採樣即可，不需要繪師返工。

### 4.1.2 `T_erase` — 為什麼需要它

**產品需求**：橡皮擦要能擦掉油漆桶填的底色，而且只擦掉筆刷覆蓋到的面積（`prd.md §4.1`）。

**為什麼不是免費的**：油漆桶的顏色不在 raster 裡，而是在 `Buf_palette[id]`——**一個區域只有一個值**。因此「只擦掉這一小塊底色」在 palette 模型下是物理上不可能的：只能整個區域清掉，或整個區域不清。

`T_erase` 是逐像素的擦除遮罩，讓 composite 能對底色做局部衰減。代價：

| 項目 | 代價 |
|---|---|
| 記憶體 | +4 MB |
| Undo | `PaintTiles` 要同時快照兩張圖的 dirty tile。R8 是 RGBA8 的 1/4，undo 體積 **+25%** |
| Draw call | **零**——commit 用 MRT 一次寫兩個 target |
| OpLog | **零**——`EraseStroke` 本來就存在 |

**不採用的替代方案**：橡皮擦 tap 時清整個區域、drag 時只擦 `T_paint`。這是兩個工具偽裝成一個，使用者無法預期哪次點擊會造成破壞性的整區清除，違反 P3。

### 4.2 三個 Pass

#### Pass 1 — Stroke（有輸入時，scissor 至 stroke bbox）

```
instanced quad × dab_count → T_wet

blend 依 preset.build_up：
  false → coverage = max(dst, dab)      同筆內不疊暗
  true  → coverage = over 累積          噴槍 / 水彩
```

#### Pass 2 — Commit（抬筆時，一次）

**三種 commit 路徑，共用同一個 pass，差別在 blend 與 target。**

**(a) 一般筆刷**（軟圓筆、蠟筆、噴槍——`blend = Normal`）

```
T_paint = over(T_paint, tint(T_wet, preset.color) × preset.opacity × mask(T_region))
```

**(b) Multiply 類筆刷**（麥克筆、水彩）— 需要 bbox ping-pong

Composite 的順序是 `over(palette, T_paint)`，所以 `T_paint` 內部的 Multiply **只會讓筆刷疊筆刷變深，疊在油漆桶底色上不會變深**——而使用者的預期正好相反。

解法是在 commit 時把底色烘進去：

```
1. 把 stroke bbox 範圍的「當前 composite 結果」複製到暫存 T_bg
     T_bg = over(palette[id] × (1 - T_erase), T_paint)
2. commit 時以 T_bg 為背景計算
     result  = multiply(T_bg, preset.color)
     T_paint = over(T_paint, result × T_wet × preset.opacity × mask)
```

**為什麼是 bbox 而不是全畫布 ping-pong**：全畫布需要第二張 `T_paint`（+16MB）。stroke bbox 通常只有畫布的幾個百分比，而 commit 只在抬筆時發生一次。

讀 `T_bg` 而非只讀 `palette`，同時解決了「麥克筆疊麥克筆」也要變深。

> **代價（已接受，見 `prd.md §10 T3`）**：底色被烘進 `T_paint` 之後，事後改變油漆桶顏色不會更新已畫的麥克筆。這與實體直覺一致，且替代方案（per-pixel 記錄 blend 類型、或把 palette 併進 raster）的複雜度高一個量級——後者還會摧毀 §8.1 的全部紅利。

**(c) 橡皮擦** — MRT 雙寫

```
target 0: T_paint  ← destination-out（擦掉筆刷）
target 1: T_erase  ← additive       （擦掉底色）
兩者皆受 Mode B mask
```

**所有路徑共同的收尾**：

```
dirty tiles（T_paint ＋ T_erase）→ Undo pool（GPU-side copy，非同步）
clear T_wet
```

#### Pass 3 — Composite（每 frame，一個 full-screen triangle）

```wgsl
let id     = textureLoad(T_region, coord).r;
let erased = textureLoad(T_erase,  coord).r;               // 0..1
var color  = mix(palette[id], PAPER_WHITE, erased);        // 油漆桶底色，可被局部擦除
color      = over(color, textureLoad(T_paint, coord));     // 已提交的筆刷
color      = over(color, tint(textureLoad(T_wet, coord),
                              brush_color) * mask(id));    // 進行中的筆畫
color      = color * textureSample(T_shade, coord);        // Multiply
color      = color * textureSample(T_line,  coord);        // Multiply，線稿蓋頂
```

`PAPER_WHITE` 是常數——未上色 = 白紙，不是透明（`prd.md §4.1`）。

**`T_shade` 是選配**：沒有 shade 的文件綁定一張 1×1 的白色 dummy texture，不做 shader variant。多一個 pipeline 變體換不到任何效能。

Composite 成本低，每 frame 全畫面重跑即可。真正的成本在 Stroke pass，而它只畫 stroke bbox。

### 4.3 `T_wet` — 單筆畫暫存層

**這是渲染模型中最容易被誤刪的設計，記錄它存在的理由。**

一筆畫由大量重疊的 dab 組成。`spacing = 0.05` 表示每前進筆尖直徑的 5% 就蓋一個 dab——任一像素會被約 20 個 dab 覆蓋。若每個 dab 直接混合進 `T_paint`：

```
設定 opacity = 30%
實際結果 = 1 - (1 - 0.3)^20 ≈ 99.9%   ← 全不透明
```

並伴隨三個症狀：

| 症狀 | 原因 |
|---|---|
| 畫慢比畫快更深 | 速度慢 → dab 更密 → 疊得更多 |
| 筆畫自交處出現「結」 | 畫圈回頭時交叉區被疊兩倍 |
| `opacity` 參數失去意義 | 調的是每個 dab 的濃度，不是這一筆的濃度 |

`T_wet` 買到的七件事：

| # | 效果 |
|---|---|
| 1 | **筆內不疊暗**——自交、慢速、來回塗抹，濃度一致 |
| 2 | **`opacity` 語意正確**——它是整筆的上限 |
| 3 | **遮罩只算一次**——commit 時算，不是每個 dab 都算 |
| 4 | **進行中的筆畫可無痛取消**——palm rejection 事後判定失敗或使用者取消，直接清空 `T_wet`，`T_paint` 從未被污染 |
| 5 | **Undo 粒度正確**——一筆 = 一個 undo entry，dirty tile 只快照一次 |
| 6 | **`build_up` 只是切 blend mode**——同一份程式碼，一個參數 |
| 7 | **橡皮擦共用路徑**——只差 commit 時的 blend |

**為什麼是 R8 而不是 RGBA8**：一整筆是單一顏色，`T_wet` 只需要存 coverage，顏色在 commit 時由 `preset.color` 帶入。2048² 下 RGBA8 為 16MB，R8 為 4MB。

> 例外：若日後加入色相抖動或紋理帶色的筆刷，才需要升為 RGBA8。目前 5 支 preset 皆不需要。

### 4.4 遮罩模式

```wgsl
// Mode A 嚴格（油漆桶）
mask = select(0.0, 1.0, id == active_region_id);

// Mode B 寬鬆（筆刷、橡皮擦）
mask = select(0.0, 1.0, id != REGION_LINEART);
```

Mask mode 是 `Tool` 的參數，不是全域設定。產品語意見 `prd.md §4.1`。

**橡皮擦固定 Mode B，不可切換。** 它沒有建立 `active_region_id` 的語意（沒有「先選中再擦」的動作），而且使用者拿橡皮擦時是在修正錯誤——最不希望被區域邊界卡住的時刻。`id != REGION_LINEART` 已經提供足夠的無害性：怎麼擦都不會擦掉線稿。

> `prd.md` 附錄 A6 記錄了一個未決提案（全域「封閉線稿」開關），它會把 mask mode 從工具參數變成使用者設定。在 D4 拍板之前，實作以本節為準。

### 4.5 油漆桶

```
tap(x, y) → id = T_region[x, y] → palette[id] = color
                                → clear T_erase within region bbox（見下）
```

**O(1)。不做泛洪填充。** 沒有容差參數、沒有漏色、沒有抗鋸齒邊緣的顏色滲漏。

**`Fill` 必須順帶清掉該區域的 `T_erase`。** 否則「填色 → 擦掉一塊 → 再填色」會在新底色上留下一個洞，使用者會認為是 bug。實作是 scissor 到 region bbox，以 `T_region` 為 mask 做一次 clear——成本可忽略。

擴散動畫在 composite shader 內完成：

```wgsl
let t = fill_progress[id];                       // 0..1，由 CPU 每 frame 更新
let d = distance(uv, fill_origin[id]);
let a = smoothstep(d - EDGE, d, t * max_radius[id]);
```

**`max_radius` 是 per-tap 的，不是 per-region 的。** `fill_origin` 是點擊處，所以「離 origin 最遠的距離」隨每次點擊而變。實作上以 region bbox 的對角線作為近似即可（保守地略大，寧可動畫早一點結束）。

> **已知的視覺限制**：對細長區域（葉子、花瓣、緞帶），從點擊處做圓形擴散會有明顯的「追趕感」——最遠的角落最後才填到。真正的解法是沿區域內測地距離擴散，成本高一個量級。**先做圓形近似，在 E1 實機看過再決定是否值得。**

### 4.6 筆刷參數

五支筆刷共用同一條渲染路徑，差異只在常數：

```rust
pub struct BrushPreset {
    pub tip: TipId,                    // 軟圓 / 硬圓 / 顆粒 / 蠟筆紋
    pub spacing: f32,                  // dab 間距（筆尖直徑比）
    pub pressure_to_size: Curve,
    pub pressure_to_opacity: Curve,
    pub velocity_to_size: f32,
    pub tilt_to_size: f32,
    pub jitter_pos: f32,
    pub jitter_size: f32,
    pub jitter_angle: f32,
    pub blend: BlendMode,              // Normal / Multiply
    pub flow: f32,                     // 單 dab 濃度
    pub opacity: f32,                  // 整筆上限
    pub build_up: bool,                // 同筆內是否疊加
}
```

| Preset | tip | spacing | blend | build_up |
|---|---|---|---|---|
| 軟圓筆 | 軟圓 | 0.05 | Normal | false |
| 麥克筆 | 硬圓 | 0.04 | Multiply | false |
| 蠟筆 | 顆粒紋理 | 0.08 | Normal | false |
| 噴槍 | 大軟圓 | 0.02 | Normal | **true** |
| 水彩 | 軟圓＋邊緣強化 | 0.06 | Multiply | **true** |

**兩支筆刷的實作風險要先寫下來**：

| Preset | 風險 |
|---|---|
| **麥克筆** | Multiply 白色 = 原色，所以**在未上色的白底上，麥克筆與軟圓筆看起來完全一樣**。它的辨識度完全依賴使用者先鋪底。D5 盲測**必須在已鋪底的畫布上進行**，否則會誤判它沒有辨識度而砍掉 |
| **水彩** | 「邊緣暈染加深」是 **per-stroke** 的效果（整筆的外緣變深），但 `tip` 是 **per-dab** 的貼圖——兩者不是同一件事。要做出真正的水彩邊緣，需要在 commit 時對 `T_wet` 取鄰域算梯度，這條路徑目前尚未設計。**這是五支裡最可能做不出辨識度的一支**，見 §14 R7 |

### 4.7 進度計算

**產品需求**：Gallery 的進度環與完成建議，定義見 `prd.md §5.2`——已上色區域數 ÷ 總區域數，「已上色」＝ 油漆桶填過 **或** 該區域筆刷覆蓋率 > 50%。

油漆桶的部分是 CPU 側資料（`palette`），直接數即可，零成本。**筆刷的部分需要 per-region 覆蓋率統計**：

```wgsl
// compute shader，每個 workgroup 處理一塊 tile
let id = textureLoad(T_region, coord).r;
if (textureLoad(T_paint, coord).a > 0.5) {
    atomicAdd(&counter[id], 1u);
}
```

輸出 `StorageBuffer<u32>[region_count]`，readback 後除以 `regions.json` 裡**已經存在**的 `area` 欄位。

**三條讓它便宜的規則**：

1. **不是每 frame 跑。** 進度是低頻資訊，抬筆後節流至最多每 500ms 一次
2. **只跑 dirty tile。** 增量累加，穩態成本接近零
3. **readback 走非同步** —— 與 §14 R6 的 undo readback **共用同一套 ring buffer ＋ fence**

第 3 點是重點：這套非同步 readback 基礎設施本來就必須為 undo 而建，因此進度計算的邊際成本只是「多一個 compute shader」，不是「多一套機制」。兩者應在 E3 一起設計。

---

## 5. Core crate 設計

### 5.1 職責與依賴方向

```
                    engine  (facade, uniffi)
                      │
        ┌─────────────┼─────────────┬──────────┐
        ▼             ▼             ▼          ▼
    app-state     document      history     render
                      │             │          │
                      ├─────────────┘          │
                      ▼                        ▼
                    oplog                   stroke
                      │                        │
                      └────────┬───────────────┘
                               ▼
                          colorpack
```

**依賴只能向下。** `stroke` 不得依賴 `render`，`document` 不得依賴 `engine`。

**一個必須先解決的張力**：`UndoEntry::PaintTiles { blob: TileBlob }` 的資料只能由 GPU 側產生（`render`），但依賴圖上 `history` 與 `render` 平行、互不依賴。若讓 `engine` 來牽線，就違反了「`engine` 不負責任何業務邏輯」。

> **解法**：由 `document` 擔任協調者（它已經是單一 `apply` 入口，天然就是 orchestrator），並以 trait 反轉依賴——`document` 定義 `TileSnapshotProvider`，`render` 實作它。這樣依賴方向仍然向下。
>
> 這件事在 E3 實作時若沒有明確規則，一定會被隨手打破成「`engine` 從 `render` 拿 blob 再塞給 `history`」。

| Crate | 職責 | 不負責 |
|---|---|---|
| `engine` | FFI facade、生命週期、把 UI 事件翻成 `Op` | 任何業務邏輯 |
| `app-state` | 目前工具、顏色、筆刷大小、進度、`UiState` 投影 | 持久化 |
| `document` | 文件模型、`palette`、**單一 `apply` 入口** | 渲染 |
| `history` | Undo/Redo stack | 記錄意圖（那是 oplog 的事） |
| `oplog` | `Op` 序列化、縮時重播 | 實作 undo |
| `stroke` | 輸入平滑、樣條插值、Dab 生成 | 任何 GPU 概念 |
| `render` | Render graph、wgpu、WGSL | 任何業務語意 |
| `colorpack` | 資產包讀寫 | 資產生成（那是 baker 的事） |

### 5.2 `stroke` 的可測試性契約

```rust
pub fn generate_dabs(
    samples: &[InputSample],
    preset: &BrushPreset,
    seed: u32,
) -> Vec<Dab>;

pub struct InputSample {
    pub pos: Vec2,
    pub t: f32,
    pub pressure: f32,      // 觸控筆的真實壓感；手指模式下由 radius 正規化而來
    pub radius: f32,        // ★ 接觸半徑，手指模式的動態來源，見 §10.2
    pub tilt: Vec2,
    pub predicted: bool,    // 預測點不進 oplog
}

pub struct Dab {
    pub pos: Vec2,
    pub size: f32,
    pub angle: f32,
    pub alpha: f32,
    pub tip: TipId,
}
```

**純資料進、純資料出、零 GPU 依賴。**

這條界線讓筆刷邏輯可以在 CI 上跑 golden test（給定輸入軌跡 → 比對 dab 序列），不需要 GPU、不需要模擬器。**這是唯一能長期防止手感回歸的機制。**

`seed` 參數存在的理由：jitter 必須可重現，否則縮時重播會與原作不同。

### 5.3 輸入處理鏈

```
原始 sample → One-Euro filter 平滑 → Catmull-Rom 插值 → 依 spacing 取樣 → Dab
```

One-Euro filter 的參數需實機調校：太強會有「拖尾感」，太弱會有抖動。

---

## 6. 邊界拆分

### Boundary 1｜Native ↔ Core

**歸屬**：

| Core 擁有 | Native 擁有 |
|---|---|
| 所有可持久化狀態 | 畫面佈局與轉場 |
| 所有 GPU 資源 | IAP 與訂閱狀態 |
| 渲染、Undo、OpLog | 檔案系統路徑 |
| 工具狀態機 | 網路傳輸 |
| 進度計算 | 系統整合（分享、觸覺、備份） |

**兩條紅線**：

1. **GPU resource 絕不跨界。** Native 只交出 surface handle，之後一律不碰。
2. **每個 touch 一次 FFI call 是錯的。** iOS 一個 frame 可能有 10 個以上的 coalesced touch，Android 有 historical points——一律用 `append_samples` 批次送，一個 frame 一次。

**FFI 表面**（刻意做小、做粗粒度）：

```rust
Engine::new(surface: RawSurfaceHandle, pack: PathBuf, doc: Option<PathBuf>) -> Engine

// 工具
fn set_tool(&self, tool: Tool);           // Brush{preset,color,size} | Eraser | Bucket{color}

// 輸入（唯一高頻路徑）
fn begin_stroke(&self, s: InputSample);
fn append_samples(&self, s: Vec<InputSample>);   // ★ 批次
fn end_stroke(&self);
fn cancel_stroke(&self);                          // palm rejection
fn tap(&self, x: f32, y: f32);

// 編輯
fn undo(&self); fn redo(&self);

// 渲染（由 FrameDriver 驅動）
fn render(&self);
fn set_viewport(&self, transform: Transform);

// 狀態
fn state(&self) -> UiState;
fn subscribe(&self, listener: Box<dyn StateListener>);

// 持久化
fn save(&self) -> Result<()>;
fn export_png(&self) -> Vec<u8>;
fn export_timelapse(&self) -> Vec<u8>;
```

### Boundary 2｜`stroke` ↔ `render`

`stroke` 產出 `Vec<Dab>`，`render` 只吃 `&[Dab]`。見 §5.2。

### Boundary 3｜Core ↔ wgpu

**只有 `render` crate 可以 `use wgpu`。**

CI 以 lint 強制（禁止其他 crate 在 `Cargo.toml` 列入 wgpu）。

價值：如果 §14 的退路被觸發（改用手寫 Metal/Vulkan），影響範圍是一個 crate，不是整個核心。

### Boundary 4｜runtime ↔ baker

共用 `core/colorpack` crate。baker 是獨立 binary，不被 runtime 依賴。

`manifest.schema_version` 從第一天存在，runtime 拒絕載入未知的 major 版本。

### Boundary 5｜單一寫入口（架構鐵律）

```rust
impl Document {
    /// 唯一能改變文件狀態的函式
    pub fn apply(&mut self, op: Op) -> Result<()>;
}
```

**任何狀態變更都必須先成為一個 `Op`，且只能經由 `apply` 執行。**

這保證 ops-log 永遠完整——不會有「某個路徑忘了記錄」的漏洞。這條規則違反一次，縮時影片與備份還原就永久不可靠，而且是難以察覺的靜默失敗。

```rust
pub enum Op {
    BeginStroke { preset: BrushId, color: Rgba, size: f32, seed: u32 },
    StrokeSamples { samples: Vec<QuantizedSample> },   // 量化，見 §8.2
    EndStroke,
    Fill { region: u16, color: Rgba },                 // 順帶清該區的 T_erase
    EraseStroke { preset: BrushId, size: f32, seed: u32 },
}
```

**沒有 `Undo` / `Redo` op。** 這是刻意的，理由見 Boundary 6。

**移除了 `EraseRegion`。** 橡皮擦改為逐像素擦除（`T_erase`）之後，「清除整個區域」不再有對應的 UI 動作。要清空一個區域，語意上等同於用油漆桶填成白色——`Fill` 已經涵蓋。多留一個沒有 UI 的 op 只會讓向前相容的負擔變大。

### Boundary 6｜Undo ≠ OpsLog

**這兩者經常被誤認為同一件事，實際上職責與實作方式完全不同。**

| | Undo/Redo | Ops-Log |
|---|---|---|
| 實作 | tile COW 快照 ＋ palette diff | 意圖記錄 |
| 用途 | 使用者操作 | 備份、縮時、遙測 |
| 要求 | 即時、精確、可預期 | 體積小、可演進 |
| 是否為真相來源 | **本機是** | **不是** |
| 是否持久化 | **否**（純記憶體，session 內） | **是** |

**為什麼 undo 不能用 ops-log 重播實作**

理由是**複雜度，不是浮點誤差**。

> ⚠️ v0.1 寫的理由是「GPU 浮點捨入跨廠商不一致」。**那個理由是錯的**——undo 永遠發生在同一台裝置、同一個 driver、同一個 session 內，重播是確定性的。錯誤的理由很容易被後人正確地反駁掉，然後連正確的結論一起推翻。

真正的理由：undo 一步需要從最近的 keyframe 重播到第 n-1 步。一張投入 30 分鐘的作品可能有 300–600 筆畫，每按一次 undo 就重播數百筆——**這是 O(n) 的操作，而 undo 是使用者按了會期待「立刻」的操作。**

```rust
pub enum UndoEntry {
    PaintTiles { tile_ids: Vec<u32>, blob: TileBlob },  // T_paint ＋ T_erase 的髒 tile
    Fill { region: u16, old: Rgba, new: Rgba },         // 油漆桶，8 bytes
}
```

油漆桶的 undo 只需 8 bytes——這是把 `palette` 抽成獨立 buffer 而非烘進 raster 的紅利。

Undo pool 的記憶體上限與動態深度見 §4.1.1。

### ★ 兩者如何保持一致：磁碟上的 oplog 只含有效 op

**這是本節最重要的一條不變式。**

若 undo 不反映到 oplog，備份還原時重播 oplog 會把使用者**已經撤銷的筆畫全部畫回來**——產生一份看起來正常但內容錯誤的作品，正是 §7 所說「不得靜默略過」要防的那類失敗。

有兩種解法，我們選第二種：

| 方案 | 磁碟內容 | 問題 |
|---|---|---|
| oplog 記 `Undo`/`Redo` marker，重播時兩趟解析 | 含死 op | **反覆修改會讓 oplog 無界成長**（存了大量永不重播的死 op），與 §8.2 的體積上限直接衝突。備份與縮時都要過濾 |
| **★ 磁碟只含有效 op** | 線性、無 marker | 需要 truncate，崩潰安全性要處理 |

**採用的模型**：

```
記憶體：undo stack（tile COW）  ← session 內，不持久化
磁碟：  oplog                   ← 只含有效 op，undo 表現為「有效長度縮短」
```

- undo → 記憶體 stack 動作 ＋ 標記 oplog 有效長度縮短
- undo 後畫新的一筆 → 覆寫被截短的尾巴，死 op 直接消失
- 寫檔時只寫有效長度以內的部分

得到的好處是連鎖的：

| | 兩趟解析 | **只寫有效 op** |
|---|---|---|
| 備份還原 | 需先解析再重播 | **直接重播** |
| 縮時渲染 | 需過濾 | **直接播** |
| oplog schema | 要定義 `Undo`/`Redo` 及其向前相容語意 | **不需要** |
| 反覆修改的體積 | 無上限 | **有界** |

**兩條必須遵守的規則**：

1. **undo 之後要觸發一次寫檔。** 否則「undo 三步 → 崩潰 → 重啟」會讓那三步復活，狀態不一致。undo 是低頻操作，即時寫檔成本可接受。
2. **不得就地截斷檔案。** 寫新檔 ＋ atomic rename，見 §8.3。

### Undo 不跨 session 保留（產品決策）

App 重啟後 undo stack 是空的。三個理由：

1. **成本不對稱**——tile COW 快照是 GPU raster，一步可能數百 KB 到數 MB。跨 session 存活就得寫進磁碟，本機存檔會從「通常 < 1MB」變成數十 MB，只為了一個低頻需求
2. **心智模型不支持**——關掉 App 隔天再打開，然後撤銷昨天的某一筆，使用者已經不記得那一筆是什麼。Procreate、Photoshop 皆同
3. **與 P1 不衝突**——P1 保護的是「作品不消失」，不是「編輯歷史不消失」

崩潰時進行中未 commit 的筆畫應被丟棄——`T_wet` 從未 commit，這是自然行為，但要在 E3 明確測試。

---

## 7. 契約層

### 核心原則

> **一份契約只能存在一次，其餘全部由它生成。**
> 不維護「兩份」，而是維護「一份 ＋ 兩個薄皮」。

### SSOT 對照表

| 契約 | SSOT | 生成方式 | 為什麼 |
|---|---|---|---|
| FFI 型別與函式 | **Rust**（`core/engine`） | uniffi 生成 Swift ＋ Kotlin | 跨語言，uniffi 的正職 |
| `.colorpack` 格式 | **`contracts/colorpack.schema.json`** | 手寫規格 ＋ Rust 驗證 | 跨的是 baker ↔ runtime ↔ 外部工具鏈，不是跨語言 |
| Ops-Log schema | **`contracts/oplog.schema.json`** | 手寫規格 ＋ Rust 驗證 | 跨的是**版本**（v1 App 要能讀 v3 的檔）。uniffi 沒有向前相容概念 |
| Design tokens | **`contracts/tokens.json`** | 生成 Swift / Kotlin / CSS 常數 | 跨 SwiftUI / Compose / web 三端 |

`contracts/` **只放 uniffi 生不出來的東西**。FFI 型別不進 `contracts/`——那會製造第二份真相。

### FFI semver 規則

| 變更 | 版本 |
|---|---|
| 新增函式、新增結構欄位（有預設值） | **minor** |
| 改變既有欄位語意、刪除欄位、改變函式簽章 | **major** |
| 純內部實作變更 | patch |

major bump 必須同步更新兩端 Bridge，且需在 `docs/contracts.md` 記錄遷移方式。

### Ops-Log 的向前相容規則（呼應 PRD P1-b）

1. **Op 的識別碼永不重用、永不重編號。**（已移除的 `EraseRegion` 佔用的識別碼也不得回收）
2. 遇到未知的 op 型別時，**不得靜默略過**——靜默略過會產生一份看似正常但內容錯誤的作品。正確行為是：以唯讀模式開啟，保留原始檔案，並提示使用者升級 App。
3. 新增欄位必須有預設值，舊版讀取時走預設。
4. **schema 裡不存在 `Undo` / `Redo`。** 磁碟上的 oplog 永遠是線性的有效 op 序列——見 Boundary 6。這條規則讓向前相容少掉一整個維度的複雜度，不要因為「加個 marker 比較好實作」而破壞它。

### CI 守門

```bash
cargo xtask verify-generated
```

重新生成 binding 後，若 `apps/ios/Generated/` 或 `apps/android/generated/` 出現 diff 則 CI 失敗。

這是「一份契約」唯一能被真正強制的機制——沒有這道 gate，兩份實作的漂移只是時間問題。

---

## 8. 狀態與持久化

### 8.1 文件狀態的兩個部分

**這兩部分的性質完全不同，因此持久化策略也不同。**

| 部分 | 內容 | 大小 | 跨裝置重播確定性 |
|---|---|---|---|
| `palette[]` | 油漆桶填色（region_id → color） | **< 2KB** | **100%**——純資料，不經過渲染 |
| `T_paint` | 筆刷塗抹的 raster | 依塗抹量而定 | **不保證**——經過 GPU |

**關鍵觀察**：相當比例的使用者只用油漆桶。這類作品的完整狀態就是 2KB 的 palette，零 raster，完美還原。

### 8.2 兩套存檔格式

```
本機存檔（精確優先）
  document.bin
    ├─ header      { schema_version, doc_id, asset_id, asset_hash }
    ├─ palette     完整
    ├─ paint_tiles T_paint 的髒 tile，QOI / PNG 無損（通常 < 1MB）
    └─ oplog       完整操作紀錄（供縮時與備份用）

雲端備份（體積優先，接受近似）
  backup.bin
    ├─ header      { schema_version, doc_id, asset_id, asset_hash }
    ├─ palette     完整          ← 精確還原
    ├─ oplog       量化＋壓縮     ← 近似還原筆刷
    └─ thumbnail
  ★ 不含 paint_tiles raster
```

**為什麼備份不含 raster**：Android Auto Backup 有 **25MB 硬上限**（v1 雖不做 Android，但格式現在就要定，否則之後要改格式）。若備份 raster，使用者存約 20 張圖就會超額，之後的備份全部靜默失敗。

**還原時的行為**：
- `palette` 直接套用 → 油漆桶的部分 100% 精確
- `oplog` 重播 → 筆刷的部分為近似

這個取捨是刻意的：油漆桶是主要玩法且能精確還原，筆刷是次要且近似可接受（`prd.md §10 T5`）。

#### ★ OpLog 的量化——不得儲存 raw sample

**天真的作法會低估體積一個量級。**

`StrokeSamples` 若直接存原始 `InputSample`：120Hz 取樣、一張圖 30 分鐘、假設一半時間在下筆 ≈ 100k+ samples，每個約 24 bytes（`f32` × 6）→ 未壓縮 2.4MB，壓縮後仍有數百 KB。單一重度作品就可能接近 1MB，30 張就會撞上 25MB 上限——**而超額的失敗方式是靜默的**，使用者不會收到任何通知。這是 P1 最惡劣的違反方式。

因此磁碟格式必須量化：

```rust
pub struct QuantizedSample {
    pub x: i16, pub y: i16,   // 畫布空間座標，2048 以內無損
    pub dt: u8,               // 與前一點的時間差（ms），溢位時插入額外 op
    pub pressure: u8,         // 0..255
    pub radius: u8,
    pub tilt: (i8, i8),
}   // 9 bytes vs 24 bytes
```

配合 delta 編碼（座標存差值，多數情況下 ±127 以內）後可再降一個量級。**目標：重度作品壓縮後 < 200KB。**

> 這個目標必須在 E3 以真實資料驗證，不能只在紙上算——見 `roadmap.md` E3 驗收。

#### 配額管理策略

當備份總量接近上限時，**不得靜默失敗**。降級順序：

1. 對最舊、且已分享過的作品，丟棄 `oplog`，只保留 `palette` ＋ thumbnail（仍可開啟，油漆桶部分完整）
2. 仍不足時，通知使用者並引導至手動匯出（`prd.md §8`）
3. **任何情況下都不刪除本機文件**——雲端是備份，不是真相來源

### 8.3 Keyframe 策略

本機存檔採用 keyframe ＋ 增量：

- 每 N 個 op 或每 30 秒打一次 keyframe（`palette` ＋ `paint_tiles` ＋ `erase_tiles` 快照）
- 存檔 = 最近的 keyframe ＋ 其後的 ops
- 崩潰復原 = 載入最近 keyframe ＋ 重播其後的 ops（範圍小，誤差可忽略）

**寫入必須是 atomic replace，不是就地截斷。**

因為 undo 會讓 oplog 的有效長度縮短（Boundary 6），存檔是「寫出前 N 個 op」而非單純 append。若就地 truncate 時斷電，會留下一個長度與內容不一致的檔案。

```
1. 寫入 document.bin.tmp
2. fsync
3. atomic rename → document.bin
```

undo 之後必須觸發一次這樣的寫入，否則「undo → 崩潰 → 重啟」會讓被撤銷的操作復活。

### 8.4 `asset_hash` 與版本失效

每份文件記錄它所依賴的 `asset_id` ＋ `asset_hash`。

若線稿被重新製作（修正縫隙、調整區域切分），region ID 可能位移，舊的 ops 會塗到錯誤的區域。

**處理方式（呼應 PRD P1-b）**：

1. `.colorpack` **不可變**。重新烘焙產生**新版本**，舊版本永久保留於 R2。
2. 文件永遠指向它原本的 `asset_hash`——不自動升級。
3. 若因任何原因無法取得對應版本的資產：以唯讀模式開啟並顯示縮圖，**絕不刪除文件、絕不套用錯誤的資產**。

### 8.5 已觸碰資產的永久釘選（呼應 PRD P1-a）

本機資產快取的淘汰規則：

| 狀態 | 淘汰策略 |
|---|---|
| 從未開啟過 | LRU 可淘汰 |
| **已開始編輯** | **永久釘選，不淘汰** |

且**開啟既有文件時不檢查 entitlement**——訂閱狀態只在「開啟新的付費線稿」時檢查。

---

## 9. 資產管線

### 9.1 繪師交付規格

每張線稿交付三個檔案，同尺寸、同對齊：

| 檔案 | 內容 | 硬性要求 |
|---|---|---|
| `lineart.png` | 線稿，透明背景 | **抗鋸齒開啟**，RGBA |
| `flats.png` | 塗り分け（分色圖），每個封閉區域一個唯一純色 | **抗鋸齒關閉**（硬邊）、無漸層、無縫隙 |
| `shade.png`（選配） | 陰影 / 質感，供 Multiply 疊加 | 抗鋸齒開啟 |

#### 解析度與比例（硬性規格）

| 項目 | 規格 |
|---|---|
| **交付母帶** | **長邊 4096** |
| 比例 | **1:1（4096×4096）或 4:5（3072×4096）**，二選一 |
| runtime 尺寸 | 由 baker 降採樣產生：1:1 → 2048²、4:5 → 1536×1920 |

**為什麼要求母帶高於 runtime**：runtime 解析度是一個尚未定案的工程參數（要等 E1 的記憶體量測，見 §4.1.1）。若繪師直接交付 runtime 尺寸，日後任何解析度調整都需要繪師**逐張返工**——而 `flats` 不能用一般縮放，返工成本極高。

母帶制讓解析度變成 baker 的參數，調整時只需重跑烘焙。**這是整條管線裡最便宜的一份保險。**

4096 → 2048 是整數倍降採樣，這對 `flats` 的正確性很重要，見 §9.2。

**`flats.png` 是整條管線的關鍵前提。**

它是動畫與漫畫產線的標準交付物，CSP 繪師普遍熟悉（「囲って塗る」工具，或「自動選択（隙間閉じ 開）＋ 塗りつぶし，アンチエイリアスなし」）。每張圖約增加 10–20 分鐘工時。

**沒有 flats 的代價**：必須從抗鋸齒線稿自動做 gap closing ＋ 區域切割，需要大量啟發式參數且每張圖需人工修正——這會是整個專案最大的時間黑洞。**與繪師建立這個交付習慣，價值高於任何工程優化。**

### 9.2 Baker 流程

```
母帶（長邊 4096）
   │
   ├─ 掃描 flats 唯一 RGB
   ├─ Connected components → region ID
   ├─ 前置驗證（見 9.3，在母帶解析度）
   │
   ├─ ★ 降採樣至 runtime 尺寸（三種濾波器，見下）
   │
   ├─ ★ 區域向線稿下方膨脹 2px      ← 必須在降採樣「之後」
   ├─ 後置驗證（見 9.3，在輸出解析度）
   ├─ 從 flats 顏色推導建議調色盤
   └─ 依 region_count 計算難度分級
        │
        ▼
   .colorpack
```

#### ★ 三個順序與濾波器的陷阱

**(1) 三張圖必須用不同的降採樣濾波器**

| 檔案 | 濾波器 | 為什麼 |
|---|---|---|
| `flats` | **2×2 majority（取眾數）** | 不得引入新顏色。純 NEAREST 也不引入新顏色，但它取的是每個 2×2 的左上角像素——**細於 2px 的區域會隨機消失**，取決於它落在哪個格子。majority 保留細區域的能力好得多，成本可忽略 |
| `lineart` | box filter / Lanczos | 抗鋸齒的連續影像，用 NEAREST 會直接產生鋸齒 |
| `shade` | box filter / Lanczos | 同上 |

**套用同一套濾波器是這條管線最容易犯的錯。**

**(2) 膨脹必須在降採樣之後**

母帶空間下的 2px，降採樣後只剩 1px——不足以遮住白邊光暈。

更重要的是：`lineart` 用 box filter、`flats` 用 majority，兩者在邊界的落點可能差半個像素。**膨脹正是用來吸收這個誤差的**，所以它必須作用在最終解析度上。

> 順序寫反的症狀是 runtime 出現細白邊，而且極難回溯到是降採樣造成的。

**(3) 驗證要跑兩次**

母帶通過不代表輸出通過——降採樣可能製造出新的碎片區域或縫隙。碎片區域的面積門檻必須在**輸出解析度**判定。

**關於區域邊界的抗鋸齒**：不需要特別處理。著色書的每條區域邊界上方必定有線稿，而線稿是抗鋸齒的且以 Multiply 蓋在最上層——它天然遮住了 ID map 的鋸齒邊。膨脹是為了確保沒有縫隙，不是為了抗鋸齒。

### 9.3 Baker 驗證（CI gate）

baker 必須**拒絕收檔**而非產出有問題的資產：

| 檢查 | 時機 | 失敗行為 |
|---|---|---|
| 三張圖尺寸與對齊一致 | 母帶 | 錯誤 |
| 長邊為 4096，比例為 1:1 或 4:5 | 母帶 | 錯誤 |
| flats 無抗鋸齒（顏色數在合理範圍） | 母帶 | 錯誤 |
| 無未指派像素（線稿覆蓋處除外） | 母帶 ＋ **輸出** | 錯誤 |
| 碎片區域（面積 < 200px） | **輸出** | 警告 ＋ 列出座標 |
| 降採樣後區域數與母帶一致 | **輸出** | 錯誤——代表有區域被降採樣吃掉 |
| 區域數在合理範圍 | 輸出 | 警告 |
| region_count ≤ 65535（`R16Uint` 上限） | 輸出 | 錯誤 |

**寧可在 baker 階段拒絕，也不要讓問題在 runtime 才以「漏色」的形式出現。** runtime 的漏色極難回溯到是哪張圖的哪個區域出問題。

### 9.4 `.colorpack` 格式

```
manifest.json    { id, schema_version, content_hash, canvas_size, aspect,
                   region_count, palette[], difficulty, category, has_shade }
lineart.png      RGBA8，抗鋸齒
shade.png        RGBA8，抗鋸齒（選配）
regions.bin      R16 ID map，RLE 無損
regions.json     [{ id, centroid, area, bbox, suggested_color }]
thumb.jpg
```

**`.colorpack` 一經發布即不可變**，由 `content_hash` 標識。修改內容 = 產生新的 pack，舊版本永久保留。

---

## 10. 平台整合

### 10.1 Platform Bridge 的三條規則

1. **不進 `core/`**——它是兩份不共用的程式碼，且必須綁定平台 build system。放進 core 會讓 Rust 端被平台 SDK 污染。
2. **不做成頂層目錄**——它是 app 的一部分，跟著 app 的生命週期走。
3. **★ 必須先定義介面，並提供 Real ＋ Mock 兩個實作。**

第 3 點在單人配置下的理由與多人不同——不是為了並行，而是為了：

1. **介面紀律**——介面先定，引擎只做被需要的東西，不會過度打磨
2. **卡關時可切軌**——E1 是最高風險里程碑，卡住時能轉去做 Shell 而不是空轉
3. **可獨立除錯**——UI 出問題時能用 Mock 排除引擎因素

```swift
protocol EngineProtocol {
    func setTool(_ tool: Tool)
    func beginStroke(_ s: InputSample)
    func appendSamples(_ s: [InputSample])
    func endStroke()
    func tap(x: Float, y: Float)
    func undo(); func redo()
    var state: AnyPublisher<UiState, Never> { get }
    func makeCanvasView() -> UIView
    func exportPNG() -> Data
}

final class MockEngine: EngineProtocol {
    // makeCanvasView() 回傳顯示靜態範例圖的 view
    // tap() 假裝改變 progress，state 照常發送
    // 其餘 no-op
}
```

App Shell 只依賴 `EngineProtocol`。引擎完成後把 `MockEngine` 換成 `RustEngine`，Shell 一行不改。

### 10.2 InputAdapter

| | iOS | Android |
|---|---|---|
| 高頻取樣 | `coalescedTouches` | `MotionEvent` historical points |
| 預測 | **`predictedTouches`** | 無等價機制 |
| 壓感（觸控筆） | `force` / `maximumPossibleForce` | `getPressure()` |
| **接觸半徑（手指）** | **`majorRadius`** | `getTouchMajor()` |
| 傾斜 | `altitudeAngle` / `azimuthAngle` | `AXIS_TILT` / `AXIS_ORIENTATION` |

`predictedTouches` 是 iOS 上降低感知延遲最有效的單一手段，必須使用。預測點需標記為 `predicted: true`——它們**不進入 ops-log**，只影響當前 frame 的 `T_wet` 渲染，下一個真實 sample 到達時覆蓋。

#### ★ 接觸半徑是手指模式的壓感替代

**主要使用者用手指，而手指沒有壓感**（`prd.md §2`）。若不處理，`pressure_to_size` / `pressure_to_opacity` 兩條曲線在主要情境下是常數，五支筆刷的差異就只剩 tip 與 spacing——這會讓 D5 盲測大概率砍掉不只一支筆刷。

**`majorRadius` 就是手指版的壓感**：手指壓得越用力，接觸面積越大。兩個平台都有原生支援，這是最常被忽略的訊號。

處理方式：

```
無 stylus 時：pressure ← normalize(radius)
有 stylus 時：pressure ← force / maximumPossibleForce
```

`BrushPreset` 的曲線定義完全不需要改，差異只在 `pressure` 的來源。

**正規化必須自適應。** `majorRadius` 的絕對值因手指大小而異，不能用固定的 min/max。以 per-stroke 的 running baseline 或使用者層級的長期基線做正規化，具體參數需實機調校（與 One-Euro filter 的參數一起，列在 E1）。

輔助的動態來源（皆已在 `BrushPreset` 中）：**速度**（`velocity_to_size`，快 → 細）與**停留時間**（`build_up = true` 的天然行為，對噴槍特別有效）。

### 10.3 FrameDriver

| | iOS | Android |
|---|---|---|
| 驅動 | `CADisplayLink` | `Choreographer` |
| 高更新率 | ProMotion，需設定 `preferredFrameRateRange` | 需查詢並設定 display mode |

渲染由 FrameDriver 驅動，**不由輸入事件驅動**。輸入事件只累積 sample，`render()` 每 frame 呼叫一次。

---

## 11. 雲端

### 11.1 整體形狀

**無後端、無帳號、無資料庫。**

```
資產分發    Cloudflare R2 ＋ CDN
解鎖驗證    Cloudflare Worker（約 100 行，無狀態）
使用者資料  裝置本機 ＋ 平台原生備份
```

### 11.2 資產分發

```
免費圖包   → bundle 進 App，或公開 R2
付費圖包   → 經 Worker 取得 presigned URL
圖庫目錄   → R2 上的版本化 JSON，client 以 ETag 快取
```

**為什麼需要 Worker**：若 client 自行判斷 entitlement，R2 bucket 實質是公開的——任何人抓一次目錄就能下載全部素材。而素材是本產品成本最高的資產（繪師工時）。

```
POST /unlock  { receipt }
  → 向 Apple / Google 驗證收據
  → 回傳 R2 presigned URL（15 分鐘有效）
```

無資料庫、無帳號、無 session。仍然完全 serverless。

### 11.3 使用者資料備份

| | iOS（v1） | Android（未來） |
|---|---|---|
| 機制 | 見下方待決策 | Auto Backup for Apps |
| 額外登入提示 | 無（沿用系統 iCloud） | 無 |
| 容量 | 使用者 iCloud 配額 | **25MB 硬上限** |
| 還原時機 | 隨時 | 僅裝置初始設定時 |
| 開發成本 | 見下 | 極低（一份 XML 設定） |

**為什麼 Android 不用 Google Drive App Data**：`drive.appdata` scope 需要 Google OAuth 登入，會出現帳號選擇器與授權畫面——這違反 PRD P4，且與 iOS 端的無感體驗不對等。

> **⚠️ 待決策｜iOS 備份機制：CloudKit vs iCloud Drive**
>
> v0.1 選了 CloudKit 私有資料庫。以下是替代方案，**建議在 S3 開始前拍板**。
>
> | | CloudKit 私有 DB | **iCloud Drive ubiquity container** |
> |---|---|---|
> | 開發成本 | 中——需設計 record type、處理衝突與 sync 狀態 | **低**——檔案放進 container 就自動同步 |
> | 手動匯出 | 需另外實作 | **免費附贈**（檔案直接出現在「檔案」App） |
> | 額外登入 | 無 | 無 |
> | 使用者誤刪風險 | 無（不可見） | **有**（可見於檔案 App） |
> | 精細控制 | 高（可查詢、可部分同步） | 低（整包檔案） |
>
> **傾向 iCloud Drive**：單人專案下它同時解決備份與手動匯出兩件事，省下的時間以週計；而本產品的備份需求很單純（一堆獨立文件，沒有跨文件關聯查詢），CloudKit 的能力用不上。
>
> 誤刪風險可用「放在子資料夾並在 App 內提示」緩解，且能自己刪除也意味著使用者真正擁有這些檔案——與 P4 的精神一致。
>
> **未登入 iCloud 的降級路徑**（無論選哪個都要做）：不彈窗、不阻斷，在設定頁的備份區塊顯示「未啟用」狀態並引導至手動匯出。

**已知限制**：Android 版推出後，兩平台之間無法自動同步。這是刻意的取捨，處理方式見 `prd.md §8`。

---

## 12. 建置與 CI

### 12.1 xtask 指令

```bash
cargo xtask ios              # 產生 .xcframework ＋ Swift binding → apps/ios/Generated/
cargo xtask android          # （v1 不實作）.so 4 ABI ＋ Kotlin binding
cargo xtask bake <dir>       # 執行 baker
cargo xtask verify-generated # ★ CI gate：binding 是否為最新
```

**uniffi 從 S0 就導入，不等到 Android。**

v0.1 把 uniffi 排在最後，理由是「等真正知道邊界該畫在哪」。這個推理是反的——uniffi 不會阻止你改邊界，它讓改邊界**更便宜**（改 UDL 重新生成 vs 手改兩端 binding）。而手寫 C ABI binding 再全部丟掉，對單人專案是不可接受的浪費。

v1 只生成 Swift；Kotlin 是加一行設定的事。

**不要用 Xcode Run Script 直接呼叫 cargo**——增量建置行為難以預測，且會讓 CI 與本機行為不一致。

### 12.2 產物策略

| 產物 | 進 git？ |
|---|---|
| `apps/*/Generated/` | ❌ gitignore，CI 每次重建 |
| `assets/source/` | ✅ Git LFS |
| `assets/packs/` | ❌ 上 R2 |

### 12.3 CI 觸發矩陣

| 變更路徑 | 觸發（v1） |
|---|---|
| `core/**` | 全部（Rust test、golden test、iOS build、verify-generated） |
| `contracts/**` | 全部 ＋ schema 驗證 |
| `apps/ios/**` | iOS build |
| `apps/android/**` | （v1 無此目錄）Android build |
| `tools/baker/**` | baker test ＋ 對範例資產跑一次烘焙驗證 |
| `assets/source/**` | baker 驗證（新資產必須通過 §9.3 才能合併，**含降採樣後的後置驗證**） |

---

## 13. 效能觀測

**本節不設硬性數字。** 具體門檻需要在 E1 取得真機基準線後才有意義，屆時回填。

現階段定義的是**要量什麼**與**怎麼量**。

### 13.1 觀測項

| 指標 | 量測方式 | 為什麼重要 |
|---|---|---|
| **motion-to-photon 延遲** | 高速攝影（240fps 以上）拍攝手指與螢幕，逐格計算 | 決定「跟手」的主觀感受，本產品最重要的單一指標 |
| frame time 分佈 | 平台 profiler（Instruments / Perfetto），看 p99 而非平均 | 掉 frame 比平均慢更影響體感 |
| **記憶體峰值** | 開啟 1:1 最大畫布 ＋ 連續塗抹 30 秒 ＋ undo pool 塞滿 | **對照 §4.1.1 的 145MB 預算**。超標則在 E1 就調整畫布解析度——此時繪師尚未量產，代價最低 |
| 首次可互動時間 | 從點擊線稿到可下筆 | 影響 Gallery → Canvas 的流暢感 |
| `.colorpack` 大小 | baker 輸出統計 | 影響下載體驗與 R2 成本 |
| Undo 提交延遲 | commit 到可再次下筆的間隔 | GPU readback 若同步會造成頓挫 |
| **oplog 體積** | 連續塗抹 30 分鐘後的壓縮體積 | 決定備份配額策略是否成立，見 §8.2 |

### 13.2 基準線建立

E1 完成時建立第一份基準線，記錄於 `docs/perf-baseline.md`（屆時新增），內容包含：

- 測試裝置清單（v1 至少涵蓋 iOS 高階與中階各一台；Android 三檔留待 Android 版）
- 各指標的實測值
- 依此回填本節與 `roadmap.md` 的驗收標準

**手感相關的量測必須用手指進行**，用觸控筆會得到不具代表性的結果（`prd.md §2`）。

### 13.3 回歸防護

基準線建立後，CI 對 `stroke` crate 跑 golden test（見 §5.2）。GPU 端的效能回歸無法在 CI 偵測，改以每個里程碑結束前的人工量測把關。

---

## 14. 風險與退路

| # | 風險 | 何時會顯現 | 退路 |
|---|---|---|---|
| R1 | **wgpu 的 present 路徑延遲不可接受** | E1 | 改為手寫 Metal / Vulkan。因 Boundary 3，影響範圍限於 `render` crate |
| R2 | **Android 低階機的 Vulkan driver 或 R16Uint 支援問題** | Android 版（若不預先驗證） | ID map 降級為 RG8 打包；或拉高最低支援規格。**即使 v1 不做 Android，也必須在 E1 期間以一天的 spike 驗證**——它保護的是資產格式 |
| R3 | **繪師 flats 品質不穩**（縫隙、抗鋸齒未關、對齊偏移） | M0 開始，隨圖庫規模放大 | baker 驗證嚴格化並納入 CI；與繪師建立回饋循環。寧可拒收，不可讓問題流到 runtime |
| R4 | **每週 3–5 張的更新節奏無法維持** | 上線後 2–3 個月 | 這是商業風險而非技術風險，但技術上要確保 baker 全自動、資產上架零手動步驟 |
| R5 | **FFI 介面在開發期需要修正** | E2 / E3 | 已預期。S0 的介面標記為 v0。因 uniffi 從 S0 導入，修正成本是「改 UDL 重新生成」而非手改 binding |
| R6 | **GPU readback 造成 undo 提交頓挫** | E3 | ring buffer ＋ fence 非同步化；必要時改為 GPU-side texture array 複製，避免 readback 到 CPU。**進度計算共用同一套設施**（§4.7） |
| R7 | **水彩做不出辨識度** | E2 / D5 | 「邊緣暈染」是 per-stroke 效果，但 `tip` 是 per-dab 貼圖——實作路徑尚未設計。退路：commit 時對 `T_wet` 取鄰域算梯度；若仍不可辨，**砍成四支筆刷**（PRD P2 已授權此決策） |
| R8 | **oplog 體積超出備份配額** | E3 | 量化 ＋ delta 編碼（§8.2）。若實測仍超標，降級策略是丟棄最舊已分享作品的 oplog，只保留 palette |
| R9 | **單人專案的 bus factor** | 全程 | 決策脈絡全在一個人腦中。這三份文件就是唯一解方——**決策改變時必須即時寫回文件**。見 `roadmap.md §5` |

### R2 的提前驗證（即使 v1 不做 Android 也要做）

在 E1 期間插入一天的獨立 spike：

- [ ] 取得一台低階 Android 裝置
- [ ] 跑 wgpu hello-triangle
- [ ] 驗證 R16Uint 貼圖的建立與 NEAREST 取樣
- [ ] 驗證 GLES fallback 路徑

**為什麼 v1 不做 Android 卻仍要驗證**：它保護的不是 Android 版的程式碼，而是**資產格式**。若 R16Uint 在 Android 不可用而我們到那時才發現，代價是改格式並重新烘焙全部圖庫——那時圖庫已經是 50 張以上。

成本一天，換掉一個會在一年後引爆的問題。
