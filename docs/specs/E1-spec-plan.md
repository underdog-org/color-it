# E1 Spec 拆分計畫

> 狀態：定稿（2026-08-03）｜里程碑：[E1](../roadmap/E1.md)
>
> **這份不是 spec，是六份 spec 的拆分依據與撰寫契約。** 六份寫完後它仍留著，
> 用途是回答「為什麼是這樣切」與「哪份 spec 擁有哪個型別」。

## 為什麼要拆

`E1.md` 有 26 條實作清單、8 條驗收，橫跨 `render` / `stroke` / `document` 三個空 crate
與 iOS 端，且是全專案風險最高的里程碑（時間盒 8 週，過不了後面全部沒有意義）。
單一 spec 會長到沒人讀，也無法平行撰寫。

## 六份

放 `docs/specs/`，前綴 `E1-` 讓六份成組。

**長度**：原訂每份 120–180 行，前三份寫完後修正為 **250–320 行**。
低估的原因是每份都要帶四張固定的表（涵蓋對照、已否決、驗收、回寫清單），
那四張加起來就 60–80 行，扣掉之後本文只剩不到 100 行的預算——不夠寫決定。

實際（六份寫完）：`E1-wgpu` 269、`E1-composite` 272、`E1-stroke` 338、
`E1-bucket` 279、`E1-input` 252、`E1-perf` 219。

`E1-perf` 原估 100 行——低估的原因與前三份相同：那個估計沒算四張固定表，
而流程文件的表反而更多（裝置、劇本、調校項、版型）。

| 檔 | 涵蓋 `E1.md` 的哪幾組 | crate |
|---|---|---|
| `E1-wgpu.md` | wgpu 起手四條 ＋ `T_region` 解碼上傳 | `render`（新）、`colorpack`（讀） |
| `E1-composite.md` | composite pass 三條 ＋ 擴散動畫 shader 側 | `render` |
| `E1-stroke.md` | stroke 管線四條（CPU ＋ Pass 1/2） | `stroke`（新）、`render` |
| `E1-bucket.md` | 油漆桶三條 ＋ `document.apply(Op)` 最小版 | `document`（新）、`render` |
| `E1-input.md` | 輸入四條 ＋ present 路徑定案 | `apps/ios`、`engine` |
| `E1-perf.md` | 量測六條 ＋ Mask Mode ＋ D2／D3／D4 | 無（流程文件） |

## 撰寫順序

`composite` / `stroke` / `bucket` 全部讀寫同一組 GPU 資源，因此
**`E1-wgpu.md` 必須先定稿**，其餘四份以它為共同輸入；`E1-perf.md` 最後，
因為它要引用前五份的量測掛鉤。

```
E1-wgpu ──▶ composite ／ stroke ／ bucket ／ input ──▶ E1-perf
```

六份全部在主 thread 依序撰寫，不派 subagent——spec 之間的型別一致性靠的是
同一個 context 記得前一份寫了什麼，這正是分派出去會失去的東西。

## 型別歸屬

### `E1-wgpu.md` 獨佔定義，其餘四份只引用

1. **`DocumentResources`** — 七個資源的 Rust 型別、wgpu 格式、usage flags、生命週期、擁有者
2. **Pass ↔ 資源 read/write 矩陣** — 避免三份 spec 各自宣稱擁有 `T_wet`
3. **`RenderContext`** — instance／adapter／device／queue／surface 的持有結構，
   與 `attach_surface` / `resize_surface` / `detach_surface` 的狀態機
4. **Mask Mode 的傳遞方式** — uniform 佈局；composite 與 stroke 都要用

### 已存在，不得重新定義

| 型別 | 來源 | 定於 |
|---|---|---|
| `InputSample` `Rgba` `Tool` `Transform` `SurfaceHandle` `UiState` | `core/engine/src/ffi.rs` | S0（v0） |
| `Manifest` `RegionEntry` `Aspect` `rle::decode` `ColorPack::open` | `core/colorpack` | M1 |

**一個真實的轉換邊界**：FFI 的 `InputSample` 是扁平的（`x, y, tilt_x, tilt_y`，uniffi 限制），
而 `architecture.md §5.2` 的 `stroke::InputSample` 用 `Vec2`。轉換歸屬由 `E1-stroke.md` 定死，
否則 `engine` 與 `stroke` 會各寫一份。

## 每份 spec 的大綱

### `E1-wgpu.md`

1. 範圍與非範圍
2. `RenderContext`：backend 限 Metal、required limits
3. Surface：`CAMetalLayer` ptr → `SurfaceTargetUnsafe`；attach／resize／detach 狀態機；present mode
4. `DocumentResources` 七個資源表；`T_shade` 缺席時綁 1×1 白 dummy（不做 shader variant）
5. `T_region` 載入路徑：`ColorPack::open` → `regions.bin` → `rle::decode` → R16Uint upload ＋ NEAREST sampler
6. Pass ↔ 資源 read/write 矩陣
7. Mask Mode 的 uniform 佈局
8. `attach_surface` 的 `Result` 語意變更（S0 永遠 `Ok`，E1 開始真的會失敗）
9. 記憶體帳：實際 usage flags 對 `architecture.md §4.1.1` 64MB 小計的偏差
10. headless 測試策略
11. 驗收

**必答**：`T_region` 是否同時在 CPU 側保留一份 `Vec<u16>`（+8 MB）。油漆桶取 ID 若走 GPU
readback 會很痛，但 CPU 副本要付記憶體，且與 `§4.1.1` 的預算表對帳。

### `E1-composite.md`

1. 範圍
2. WGSL 六層，逐行對照 `architecture.md §4.2 Pass 3`
3. 色彩空間
4. `PAPER_WHITE` 常數
5. `set_viewport`：`Transform` → matrix、full-screen triangle、畫布外的背景
6. 擴散動畫 shader 側：`fill_progress` / `fill_origin` / `max_radius` 的 buffer 佈局
7. `T_wet` 的 in-progress 疊加（`tint × mask`）
8. 每 frame 成本估算
9. 驗收

**必答**：色彩空間。surface format 是 `Bgra8UnormSrgb` 還是 `Unorm`？`over` 與 `multiply`
在 linear 還是 sRGB 空間做？`architecture.md §4.2` 沒寫，但錯了會在 D3 盲測被誤判成手感問題。

### `E1-stroke.md`

1. 範圍：型別完整、只實作 `§4.2` 路徑 (a) ＋ `build_up` 切 blend
2. FFI `InputSample`（扁平）→ `stroke::InputSample`（`Vec2`）的轉換歸屬
3. One-Euro filter：參數、初值、per-stroke 狀態
4. Catmull-Rom 插值 → 依 `spacing` 取樣
5. `BrushPreset` 十四欄位完整定義 ＋ 軟圓筆初值表（其餘四支列表不實作）
6. `generate_dabs(samples, preset, seed) -> Vec<Dab>` 純函式契約
7. Pass 1：instanced quad × dab_count → `T_wet`、blend by `build_up`、scissor 至 stroke bbox
8. Pass 2 (a)：`tint × opacity × mask` → `T_paint`；MRT 預留給 (c)；收尾清 `T_wet`
9. `predicted: true` 樣本的覆蓋語意（`contracts.md` C4）
10. golden test 骨架（E2 才強制，E1 先立）
11. 驗收

### `E1-bucket.md`

1. 範圍
2. `document::apply(Op)` 最小版：`Op` enum（`Fill` / `BrushStroke`）、單一寫入口、
   與 `render` 的呼叫關係；明說 `TileSnapshotProvider` 是 E3 的事
3. `tap(x, y)` → viewport 逆變換 → 取 region ID（依 `E1-wgpu.md` 的決定）
4. `Buf_palette` 佈局與更新
5. `Fill` 清 `T_erase`：scissor 至 region bbox，以 `T_region` 為 mask
6. 擴散動畫 CPU 側：per-tap 狀態、180 ms ease-out 初值、多筆同時進行時的處理
7. `REGION_LINEART` 的 ID 約定 — **對 `tools/baker` 查證，不得自行約定**
8. 驗收

### `E1-input.md`

1. 範圍 ＋ present 路徑定案
2. FrameDriver：`CADisplayLink`、`preferredFrameRateRange`（ProMotion 120 Hz）、
   與 attach／detach 的生命週期耦合
3. `InputAdapter`：`coalescedTouches` ＋ `predictedTouches` → `[InputSample]`；
   一 frame 一次 `appendSamples`（`contracts.md` C3）
4. `majorRadius` → `pressure` 的 per-stroke running baseline 演算法
5. stylus 分支：`force / maximumPossibleForce`
6. 座標系：UIKit point → 畫布像素（含 viewport transform 的反向）
7. `cancelStroke`：palm rejection 與 `touchesCancelled`
8. `EngineCanvasView` 的增修（S0 已落地的部分）
9. 驗收

### `E1-perf.md`

1. motion-to-photon：240 fps 拍什麼、逐格怎麼算、**流程要可重複**
2. 測試裝置清單（iOS 高階 ＋ 中階各一台）
3. frame time p99：Instruments Metal System Trace 的取數方式
4. 記憶體峰值劇本
5. Mask Mode A／B 即時切換的開關做法 ＋ 比較劇本 → **D4**
6. 外部三人盲測劇本（**手指、不得自評**）→ **D3**
7. D2 Android spike 步驟
8. `docs/perf-baseline.md` 的版型
9. 調校項清單：One-Euro 參數、`majorRadius` 正規化、擴散動畫時長、
   細長區域追趕感（→ 是否值得改測地距離擴散）

**必答**：第 4 條。`architecture.md §13.1` 的記憶體劇本假設 undo pool 存在，E1 沒有——
145 MB 預算在 E1 只能驗到約 81 MB（貼圖 64 ＋ 解碼暫存 16）。D4 的
「記憶體超標則此時調畫布解析度」是否還成立，要在這份講清楚。

## 三個已拍板的決定

| 決定 | 內容 | 理由 |
|---|---|---|
| **E2 預留程度** | 型別寫完整（`BrushPreset` 十四欄位、`T_erase` 進資源表與 composite、四條 commit 路徑寫進 spec 的「未來長什麼樣」一節），**但 E1 只實作路徑 (a)** | 型別不預留會讓 E1 的 golden test 基準在 E2 全部作廢 |
| **present 路徑** | `CAMetalLayer` ＋ 自建 `CADisplayLink` FrameDriver。`MTKView` 列為退路 | 與 `architecture.md §10.3`「渲染不由輸入驅動」一致；wgpu 本來就吃 `CAMetalLayer`；S0 的 `EngineCanvasView` 已是這條路 |
| **單一寫入口** | E1 建 `document::apply(Op)` 最小版，只支援 `Fill` 與 `BrushStroke`，不接 oplog／history | 鐵律 #3；且 E3 只是在 `apply` 裡多接兩條線，不需要把邏輯從 `engine` 搬出來 |

## 每份 spec 的撰寫約束

- **引用不複製**：指到 `architecture.md §4.2`，不搬原文
- **每份 spec 開頭一張表**：涵蓋 `E1.md` 的哪幾條 checklist
- **一節「已否決」**：記下不採用的做法，避免 E2 重提
- **標為「必答」的項目要給答案並附理由**，不得寫 TBD

## 合併驗收

- [x] 六份 spec 寫完並 commit
- [x] checklist 對照表的**聯集**蓋滿 `E1.md` 實作清單，零遺漏、零重複宣稱
      > 兩處是刻意的分工而非重複：擴散動畫由 `E1-composite`（shader 側）與 `E1-bucket`
      > （CPU 推進）各認一半，`E1.md` 第 50 行本來就這樣寫；Mask Mode 由 `E1-composite`
      > 認實作、`E1-perf` 認 D4 的真機比較
- [x] `E1.md` 八條驗收標準各自能指到某份 spec 的某一節（六條在 `E1-perf`，
      Mask 決策在 `E1-composite §6` ＋ `E1-perf §5`，決策寫回在各份的最後一張表）
- [x] 三個「必答」有答案：色彩空間（`E1-composite §2`）、region ID 取法
      （`E1-wgpu §5.1` ＋ `E1-bucket §4 §8`）、E1 版記憶體劇本（`E1-perf §4`）
- [ ] 文件回寫：
  - `E1.md`：「iOS `MTKView` ＋ `CADisplayLink`」→ `CAMetalLayer`；補一條 `core/document` 最小 apply
  - `architecture.md §10.3`：`MTKView` vs `CAMetalLayer` 待決項結案
  - [x] `docs/README.md` 文件地圖 ＋ `CLAUDE.md` 文件索引各加六筆
  - 其餘散在六份 spec 各自的最後一張表，**尚未執行**

**不在本輪**：任何 Rust／Swift 實作。實作計畫另由 writing-plans 產出。

## 建議的實作順序

給實作計畫的輸入，不是本文的產出。垂直切片的價值在「多快能看到畫面」：

```
① wgpu 起手 ＋ T_region 上傳 ＋ composite 六層
     → 開 .colorpack 就看得到線稿（palette 全空 = 白紙）
② 油漆桶 tap ＋ Buf_palette ＋ 擴散動畫
     → 能點著色，且完全不碰 stroke 管線
③ CADisplayLink ＋ InputAdapter ＋ stroke 管線 ＋ Pass 1／2
     → 能塗抹。手感風險全部集中在這一段
④ 量測與調校
```

油漆桶排在筆刷前面，是因為它 O(1)、不碰 stroke 管線，卻能提早驗證
`T_region` / `Buf_palette` / composite 三者接對了。**筆刷卡住時，油漆桶已經在跑**——
這是 `roadmap/checkpoints.md` RS1「卡關無人可換手」在 E1 內部的版本。
