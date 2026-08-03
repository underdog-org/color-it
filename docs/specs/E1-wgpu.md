# E1 · wgpu 起手與文件級 GPU 資源

> 狀態：草案（2026-08-03）｜里程碑：[E1](../roadmap/E1.md)｜計畫：[E1-spec-plan](./E1-spec-plan.md)
>
> **本文獨佔定義 `RenderContext`、`DocumentResources`、pass ↔ 資源矩陣、mask uniform。**
> `E1-composite` / `E1-stroke` / `E1-bucket` / `E1-input` 只引用，不重新定義。

## 涵蓋 `E1.md` 的哪幾條

| `E1.md` 實作清單 | 本文 |
|---|---|
| `core/render`：wgpu instance／adapter／device／queue 初始化 | §2 |
| iOS `CAMetalLayer` → surface handle；GPU resource 絕不跨界 | §3 |
| `core/colorpack`：讀 `.colorpack`、RLE 解碼 → `T_region`（R16Uint、NEAREST、無損） | §5 |
| 文件級 GPU 資源配置（七項 ＋ shade dummy） | §4 |

不涵蓋：三個 pass 的內容（`E1-composite` / `E1-stroke`）、`tap` 的區域查詢（`E1-bucket`）、
`CADisplayLink` 與輸入（`E1-input`）。

---

## 1. 範圍

`render` crate 的骨架——GPU 生命週期、資源配置、資源與 pass 的所有權契約。
不含任何 WGSL（除 §7 的 uniform 佈局）與任何業務語意：`render` 不知道「油漆桶」
是什麼，只知道 pass 與資源（`architecture.md §5.1`）。鐵律 #1 由 `Cargo.toml` lint 守。

---

## 2. `RenderContext`

```
Engine::new()          → 建 wgpu::Instance（不碰 GPU，headless 可跑）
attach_surface(handle) → 建 Adapter → Device → Queue → Surface → DocumentResources
detach_surface()       → 只丟 Surface
```

| 狀態 | Instance | Device／Queue | DocumentResources | Surface |
|---|---|---|---|---|
| `new` 之後 | ✅ | — | — | — |
| `attach` 之後 | ✅ | ✅ | ✅ | ✅ |
| `detach` 之後 | ✅ | ✅ | ✅ | — |
| 二次 `attach` | ✅ | 沿用 | **沿用** | 重建 |

- **`attach_surface` 是 GPU 初始化的唯一時機**——`Engine::new` 要能在無 GPU 環境成功
  （headless 測試的前提，`ffi-contract.md §3`）。這也解釋了 fallible 界線為何落在這裡。
- **`detach` 不丟 device 也不丟資源**（C5）。丟了的話使用者切出 App 再回來畫作就消失。
- `resize_surface` 只重設 surface configuration——畫布尺寸來自 `manifest.canvas_size`，與螢幕無關。

### 2.1 Adapter 與 limits

backend 限 `Backends::METAL`，`power_preference: HighPerformance`，
required features **無**（R16Uint 的 `TEXTURE_BINDING` 是 core），
required limits 只有 `max_texture_dimension_2d ≥ 2048`。

**刻意零 optional feature**，讓 `§14 R2` 的 Android 降級評估只需要看格式，不必看 feature 矩陣。

### 2.2 `attach_surface` 的 `Result` 語意變更

S0 永遠回 `Ok`（不碰 GPU）。**E1 起它真的會失敗**——adapter 取不到、device 建不出來、
surface 格式不支援。`EngineCanvasView.swift:71` 的 `assertionFailure` 分支要換成真的錯誤處理，
那條註解已經預告了。

---

## 3. Surface：`CAMetalLayer` 跨界

Swift 端交出的是 `SurfaceHandle { layer_ptr: u64, width_px, height_px, scale }`
（S0 已定，`core/engine/src/ffi.rs`）。Rust 端：

```
SurfaceTargetUnsafe::CoreAnimationLayer(layer_ptr as *mut c_void)
```

**Boundary 1 紅線 1**：Native 只交出這一個指標，之後一律不碰。Swift 端不持有
任何 `MTLTexture` / `MTLBuffer`，`CAMetalLayer` 的 drawable 由 wgpu 全權管理。

### 3.1 Present 設定

| 項目 | 值 | 理由 |
|---|---|---|
| format | `Bgra8Unorm` | Metal 原生。**非 sRGB 變體**——composite 直接輸出編碼值（§6） |
| present mode | `Fifo` | 由 `CADisplayLink` 驅動，vsync 對齊。`E1-input` 擁有 FrameDriver |
| alpha mode | `Opaque` | `EngineCanvasView.isOpaque = true` 已設 |
| color space | `Auto` | wgpu 30 的 `SurfaceConfiguration` 新欄位。非 sRGB 格式 ＋ `Auto` = presentation engine 直接把寫進去的值當編碼值，硬體不 decode／encode（§6） |
| **`maximumDrawableCount`** | **2（待 D3 實測）** | 預設 3 會多排一格 latency。這是 motion-to-photon 最便宜的一根調節桿，列為 `E1-perf` 的量測項 |

`maximumDrawableCount` 要在 Swift 端的 `CAMetalLayer` 上設，不是 wgpu 的 API——
這是 §3 紅線的一個記名例外：**layer 的顯示屬性由 Native 設，layer 的內容由 Rust 畫。**

---

## 4. `DocumentResources`

一份文件一組，`attach_surface` 時配置，`detach` 時保留。尺寸一律 `manifest.canvas_size`。

| 資源 | wgpu format | usage | 初始內容 |
|---|---|---|---|
| `T_line` | `Rgba8Unorm` | `TEXTURE_BINDING \| COPY_DST \| COPY_SRC` | 解碼 `lineart.png` |
| `T_shade` | `Rgba8Unorm` | `TEXTURE_BINDING \| COPY_DST \| COPY_SRC` | 解碼 `shade.png`，或 1×1 白 dummy |
| `T_region` | **`R16Uint`** | `TEXTURE_BINDING \| COPY_DST \| COPY_SRC` | 解 RLE 的 `Vec<u16>`（§5） |
| `T_paint` | `Rgba8Unorm` | `TEXTURE_BINDING \| RENDER_ATTACHMENT \| COPY_SRC` | 全 0（透明） |
| `T_erase` | `R8Unorm` | `TEXTURE_BINDING \| RENDER_ATTACHMENT \| COPY_SRC` | 全 0（未擦除） |
| `T_wet` | `R8Unorm` | `TEXTURE_BINDING \| RENDER_ATTACHMENT` | 全 0 |
| `Buf_palette` | `Buffer<vec4<f32>>` | `STORAGE \| COPY_DST` | 全 0（alpha = 0 表未填色） |

**`COPY_SRC` 現在就加。** E3 的 undo 要對 `T_paint` / `T_erase` 做 dirty tile 快照，
usage flag 不影響記憶體，事後追加卻要改資源建立的所有路徑。`T_wet` 不加——
它是單筆暫存，永遠不進 undo（`§4.3` #4）。

**`T_shade` 缺席時綁 1×1 全白 `Rgba8Unorm`，不做 shader variant**
（`architecture.md §4.1`）。它在 composite 是 Multiply，白色即單位元。

### 4.1 `Buf_palette` 的「未填色」表示

長度 `manifest.region_count`，元素 `vec4<f32>` linear RGBA。
**`a == 0` 表示該區域尚未被油漆桶填過**，composite 據此顯示 `PAPER_WHITE`。
不另開一個 bitmap——多一個資源換不到任何東西，而 alpha 通道本來就在那裡閒著。

### 4.2 記憶體帳

`architecture.md §4.1.1` 的 1:1 貼圖小計 64 MB 在本文的格式選擇下**不變**
（sRGB 變體與 unorm 同尺寸）。但有兩筆該預算表沒有算到的：

| 項目 | 大小 | 說明 |
|---|---|---|
| **swapchain drawable** | **約 24–36 MB** | 螢幕解析度 × `Bgra8` × `maximumDrawableCount`。iPhone 15 Pro 1179×2556 = 12.05 MB／張 |
| **`region_ids` CPU 副本** | **8 MB** | 見 §5.1。是常駐，不是「解碼暫存」 |

**兩筆合計 32–44 MB，佔 145 MB 預算的 22–30%，而預算表完全沒列。**
這會直接影響 D4「記憶體超標則此時調畫布解析度」的判定，
列為 `E1-perf.md` 的必答項——**不要在這裡拍板改預算表**，等真機數字。

---

## 5. `T_region` 的載入路徑

`ColorPack::open` 已經把 RLE 解完並回傳 `region_ids: Vec<u16>`（長度 = `w × h`），
`render` 只負責 `write_texture`。**E1 不需要動 `colorpack`**——`E1.md` 那條
「RLE 解碼」在 M1 就完成了。

### 5.1 必答：CPU 側保留 `region_ids` 副本

**保留。**

1. `ColorPack::open` 已經 materialize 出來了，丟掉是主動決定，不是省下
2. 油漆桶的 `tap` 必須**同步**拿到 region ID——擴散動畫要在同一 frame 起跑。
   單像素 readback 要 command buffer ＋ fence，至少一 frame stall。
   `pick_color` 可以吃那個 stall（C6，低頻），**油漆桶不行，它是主互動**
3. 代價是 8 MB **系統 RAM**，不是 GPU 記憶體
4. E3 的進度計算與 S1 的 `pick_color` 都會再用到

`DocumentResources` 持有 `region_ids: Vec<u16>` 與 `regions: Vec<RegionEntry>`
（後者的 `bbox` 給 `E1-bucket` 清 `T_erase` 用）。**`E1-bucket` 只讀，不得另開一份。**

### 5.2 「NEAREST 取樣」在 WGSL 是空條件

`R16Uint` 在 WGSL 是 `texture_2d<u32>`，**不能綁 sampler，只能 `textureLoad`**——
`architecture.md §4.1` 那條「只能 NEAREST 取樣」自動成立，runtime 想錯用 filtering 也做不到。
該條規則的真正作用範圍是**資產格式**（不得有損壓縮、不得降解析度）。

`T_line` / `T_shade` 相反：`textureSample` ＋ linear filter（畫布縮放時需要）。

---

## 6. 色彩空間

**全部在 sRGB 編碼值上直接合成，硬體不做任何 decode／encode。**
因此所有顏色資源用非 sRGB 變體的 `Unorm`，surface 用 `Bgra8Unorm`。

決定性理由是 baker 的 `thumb.jpg` 就是在編碼值上用 u8 整數乘法合成的
（`tools/baker/src/thumb.rs`），而 Gallery 顯示那張縮圖——**畫布必須跟它長一樣**。
`Buf_palette` 存的也是編碼值（`Rgba.r as f32 / 255`，不 linearize）。

**完整論證、代價與退路見 `E1-composite.md §2`**，本文只保證格式與那份一致。

---

## 7. Pass ↔ 資源矩陣

R = 讀，W = 寫，C = clear。**任何 pass 不得存取表上沒有的資源。**

| | `T_line` | `T_shade` | `T_region` | `T_paint` | `T_erase` | `T_wet` | `Buf_palette` | surface |
|---|---|---|---|---|---|---|---|---|
| **Pass 1 Stroke** | | | | | | **W** | | |
| **Pass 2 Commit (a)** | | | R | R+W | | R → C | | |
| **Pass 3 Composite** | R | R | R | R | R | R | R | **W** |
| **Fill**（非 pass，`E1-bucket`） | | | R | | C | | W | |

三件事：

1. **Pass 1 不讀 `T_region`。** 遮罩在 commit 時算一次（`§4.3` #3），不是每個 dab 算。
2. **Pass 2 的 `T_paint` 是 read-modify-write**，走硬體 blend（`over`），不是 shader 讀寫同一張圖。
3. E2 會補 Pass 2 的 (b) ping-pong `T_bg`、(c) MRT 寫 `T_erase`、(d) `edge_boost`。
   **E1 只實作 (a)**，但矩陣先把它們的欄位留著。

### 7.1 Mask uniform

Pass 2 與 Pass 3 共用，`E1-composite` / `E1-stroke` 都綁它：

```wgsl
struct MaskUniform {
    mode: u32,               // 0 = A（嚴格），1 = B（寬鬆）
    active_region_id: u32,   // mode A 才有意義
}
```

放在每 frame 更新的 global uniform buffer，不是 per-pass bind group——
D4 要能**在真機上即時切換**比較（`E1.md` 第 61 行），切 mode 不該重建 pipeline。

> ⚠️ **Mode B 的條件式要在 `E1-composite` 重新定義。** `architecture.md §4.4` 寫
> `id != REGION_LINEART`，但 baker 產出的 ID map 是滿的、沒有保留 ID
> （`baker-core-design.md §2.5`），該條件恆為真。線稿的無害性來自 composite 層序
> （`T_line` 永遠 Multiply 蓋頂），不來自 mask。

---

## 8. 測試策略

wgpu 在 macOS 上不需要 surface 就能建 device，所以 `render` 可以有真的測試：

- `RenderContext::headless()`——建 device／queue 不建 surface
- `DocumentResources` 配置：格式、尺寸、`T_shade` dummy 分支
- **`T_region` 上傳 round-trip**：`Vec<u16>` → texture → readback → 逐值比對。
  這是唯一能證明「無損」的機制，值得為它給 `T_region` 加 `COPY_SRC`。
  同一個理由對 `T_line` / `T_shade` 也成立——驗收第 5 條要證明缺席時綁的 dummy
  **是白的**（Multiply 的單位元），不讀回來就只能驗尺寸。三張唯讀貼圖一律加
- pass 的 offscreen 比對歸 `E1-composite`

**已驗**：開發機（macOS）拿得到 Metal adapter，上述測試全部真的在 GPU 上跑。
**待驗證**：CI 的 macOS runner 有沒有可用的 Metal device。若沒有就標 `#[ignore]`
改本機 pre-push 跑——**不要因此不寫**。

---

## 9. 已否決

| 做法 | 為何不 |
|---|---|
| `Engine::new` 就建 device | 破壞 headless 測試，且 `new` 的 `Result` 會混入 GPU 失敗與檔案失敗兩種語意 |
| `detach_surface` 釋放 `DocumentResources` 省記憶體 | 違反 C5，切出 App 再回來畫作消失 |
| `T_region` 只留 GPU、`tap` 走 readback | 油漆桶是主互動，不能吃一 frame stall（§5.1） |
| `T_shade` 缺席時另做一份 shader variant | `architecture.md §4.1` 已否決：多一個 pipeline 變體換不到效能 |
| 在 linear 空間合成 | 與 baker 的 `thumb.jpg` 不一致，Gallery 與 Canvas 會是兩個顏色（`E1-composite.md §2`） |
| 為「未填色」另開一張 bitmap | `Buf_palette` 的 alpha 通道本來就閒著（§4.1） |
| 把 `maximumDrawableCount` 交給 wgpu | wgpu 不暴露它。這是 layer 的顯示屬性，歸 Native（§3.1） |

---

## 10. 驗收

- [ ] `cargo xtask lint` 通過：除 `render` 外無 crate 依賴 wgpu
- [ ] `Engine::new` 在無 GPU 環境成功（headless 測試）
- [ ] attach → detach → attach 之後，`T_paint` 的內容不變（測試以已知 pattern 驗）
- [ ] `T_region` 上傳 round-trip 逐值相等（無損）
- [ ] `T_shade` 缺席的文件正常配置 1×1 dummy，且與有 shade 的文件走同一個 pipeline
- [ ] 真機量到 `DocumentResources` ＋ swapchain 的實際 GPU 記憶體，交給 `E1-perf` 對帳
- [ ] Mask mode 可在真機即時切換，不重建 pipeline

## 11. 要回寫的既有文件

| 文件 | 改什麼 |
|---|---|
| `architecture.md §4.4` | Mode B 的 `id != REGION_LINEART` 不成立（§7.1） |
| `architecture.md §4.1` | 「只能 NEAREST 取樣」應標明是**資產格式**規則，runtime 用 `textureLoad`（§5.2） |
| `architecture.md §4.1.1` | 預算表補 swapchain drawable 與 `region_ids` CPU 副本兩列（§4.2，數字待 `E1-perf`） |
| `roadmap/E1.md` | 「`CAMetalLayer` → `RawSurfaceHandle` 傳入 `Engine::new`」→ 改成 `attach_surface`（S0 已拆兩段） |
| `roadmap/E1.md` 第 61 行 | Mode B 的描述同 §7.1 |
| `contracts.md` ② | `attach_surface` 的 v0 狀態「記下 handle，不碰 GPU；永遠 `Ok`」在 E1 失效 |
