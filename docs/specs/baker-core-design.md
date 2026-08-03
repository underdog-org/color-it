# Baker 核心設計

> M1 資產管線的實作規格。範圍：`tools/baker` ＋ `core/colorpack`。
> 狀態：v1.2（2026-08-03）——v1.2 是**實作落地後的回寫**：§0 的文件修正清單已全數完成，
> §1／§3.2／§4.1／§4.2／§5 依實作對齊（deps 補齊、zip 權限措辭改為「釘死」、清冊補 `source-incomplete`、
> `unassigned-pixel` 降回純 Master、`Diagnostic` 補 `coord_total`、fixture 生成器路徑與 torture-01 實測值）。
> v1.1 修正膨脹的迭代語意、把抗鋸齒判準改回 `assets-spec` 的面積承諾（唯一色數改為快篩）、
> 補上 §4 的檢查清冊與 §1 的完整依賴，刪去 §2.4 那條會誤退合格素材的鄰接檢查、定死 4-連通。
> 相關：[roadmap/M1](../roadmap/M1.md)｜[architecture §9](../architecture.md)｜[assets-spec](../assets-spec.md)

---

## 0. 本文件先解掉的既有矛盾

實作前必須先改文件，否則管線核心無法成立。

**直式比例正名 3:4。** `assets-spec §3` 的母帶「4:5 → 3072×4096」實際是 3:4；`prd §6` 與 `architecture §4.1.1` 的 runtime「4:5 → 1536×1920」則是 4:5。3072→1536 是 ÷2，4096→1920 是 ÷2.133，兩軸倍率不同會壓扁畫面，且 `architecture §9.1` 明訂「4096→2048 是整數倍降採樣，這對 flats 的正確性很重要」在直式路徑上不成立。

> **決議**：母帶不動（既有兩張直式素材已是 3072×4096），runtime 改為 **1536×2048**，比例名稱改為 **3:4**。兩種比例都是乾淨的 ÷2。像素數 3.15M，仍低於 1:1 的 4.19M，`§4.1.1`「以 1:1 為記憶體上界」的論證不變。

**文件修正清單**（M1 驗收「決策已寫回文件」）——**全數完成於 v1.2**：

| 檔案 | 修正 |
|---|---|
| `prd §6`、`architecture §4.1.1 / §9.1`、`assets-spec §3` | 直式正名 3:4、runtime 1536×2048 |
| `assets-spec §4.2 / §7` | 不動（SSOT 已正確）。記錄釐清：本設計 §2.6 曾以「唯一色數 > 1024」取代「任一顏色總面積 < 100px」，那不是等價替換，v1.1 已改回實判、快篩並存 |
| `architecture §9.2` | 補：降採樣對象是 ID map、majority 平手規則、膨脹的精確語意 |
| `architecture §9.3` | 補：未指派像素定義、AA 門檻常數、新增「輸出解析度下區域斷裂」警告，以及 `assets-spec §7` 有而它漏列的三條（保留色、`shade` luma、區域數過多過少） |
| `architecture §9.4` | 釐清 `palette[]` 與 `suggested_color` 的分工 |
| `specs/build-infra.md` | `bake` 狀態、`deps-policy` 兩條新依賴、`baker` 同時是 bin 與 lib（§1） |

---

## 1. 模組邊界

`core/colorpack` 是 baker 與 runtime 的唯一共用面（`architecture §6` Boundary 4）。它只懂容器格式，**不依賴 `png`**——PNG bytes 對它是不透明 blob。

```
core/colorpack/            deps: serde, serde_json, zip, sha2, flate2
                           dev-deps: jsonschema（§3.8）、proptest（§6）
                           flate2 不被直接使用，只為 zip 選定 deflate backend
  lib.rs        ColorPack::write_to() / ColorPack::open()
  manifest.rs   Manifest serde ＋ schema_version major 檢查
  region.rs     RegionEntry
  rle.rs        R16 RLE 編解碼
  container.rs  zip 讀寫：副檔名→壓縮方式、決定性 metadata
  hash.rs       content_hash 的正規化定義

tools/baker/               deps: colorlull-colorpack, png, jpeg-encoder（§3.7）,
                                 clap, anyhow, serde, serde_json
                           dev-dep: tempfile（§5.2）
  main.rs       clap 薄殼
  lib.rs        bake(dir, opts) -> Report        管線編排，一條直線
  source.rs     來源目錄解析、meta.json 讀取與驗證
  image.rs      PNG 解碼 ＋ 色彩空間判定
  segment.rs    唯一色掃描 → connected components → RegionMap
  resample.rs   majority（ID map）／ box（RGBA）
  compose.rs    lineart / shade 合成到白底
  dilate.rs     區域向線稿下方膨脹
  reference.rs  一致性驗證 ＋ 逐區取建議色
  thumb.rs      縮圖合成
  check/master.rs, check/output.rs
  report.rs     Diagnostic → 文字 / JSON
  synth.rs      合成素材產生器（zone 原語）＋ torture-01 ＋ negative fixture
```

**`synth.rs` 放在 baker 而非 xtask**：torture-01 與 negative fixture 是同一件事的正反面，共用同一組 zone 原語。`xtask` 改為依賴 `colorlull-baker` lib，`gen-torture` 與 `bake` 都直接呼叫，不 shell out（錯誤訊息才帶得回來）。

`xtask/deps-policy.toml` 補兩行：

```toml
[crates.baker]
internal = ["colorpack"]

[crates.xtask]
internal = ["baker"]
```

**`tools/baker` 必須同時是 bin 與 lib。** 現行 `Cargo.toml` 只有 `[[bin]]`，但 `lib.rs` 的 `bake(dir, opts) -> Report` 要被 xtask 呼叫，需補 `[lib] name = "baker"`——package 名 `colorlull-baker`、lib 名用短名，照 `build-infra.md §1` 的慣例。

**版本集中在 root `Cargo.toml` 的 `[workspace.dependencies]`**（既有慣例，同 `build-infra.md §1`）。需新增 `zip`、`jsonschema`、`proptest`、`tempfile`，以及 `colorlull-baker = { path = "tools/baker" }`（xtask 依賴它）。`sha2`、`png`、`clap`、`anyhow`、`serde`、`serde_json` 已在。

鐵律檢查：無 wgpu import ✓｜`core/stroke` 未觸碰 ✓｜`core/colorpack` 無平台 SDK ✓。

---

## 2. 管線

照 `architecture §9.2` 的順序寫成一條直線，中間型別明確：

```
Source ──▶ Master ──▶ RegionMap(母帶) ──▶ Output ──▶ ColorPack
```

CLI：

```
baker <src-dir> [--out <dir>] [--report <path>.json]
```

`--out` 預設 `assets/packs/`（gitignore，走 R2）。輸出檔名為 `<id>.colorpack`。
`cargo xtask bake <dir>` 直接呼叫 `baker::bake()`，不 shell out。

`difficulty` 依**輸出解析度**的 `region_count` 判定（門檻 SSOT 在 `assets-spec §8`）。

以下是架構文件未定義、但實作必須定死的語意。

### 2.1 連通性一律是 4-連通；降採樣的對象是 region ID，不是 RGB

**connected components 用 4-連通**，全文的「相鄰」都指這個。8-連通會把只在對角接觸的兩塊同色區域併成一塊，而 `assets-spec §4.2 ④` 給繪師的心智模型是「相連才會合併」——對角相觸算不算相連，在繪師端是模稜兩可的。取保守的 4-連通，繪師照 `assets-spec §6.1` 的洋紅檢查做就不會意外多併。

一個 2×2 區塊可能同時含 A(紅)、B(綠)、C(紅)，其中 A 與 C 不相鄰所以合法同色。對 RGB 取眾數會得到「紅」但無從得知是哪個區域。**majority 必須作用在 ID map 上。**

### 2.2 majority 平手規則

平手時取**母帶面積最小的區域**；面積再平手取 ID 最小。

理由：這條規則直接服務於「降採樣後區域數與母帶一致」這條硬性錯誤。細區域是唯一會被吃掉的一方，讓它贏；大區域損失的是邊緣 ≤1px，而那一帶本來就在線稿底下。成本是查一次面積表（CC 完就有）。完全決定性。

### 2.3 「未指派像素」的唯一定義

**`flats` 的 alpha < 255。** 任何 RGB 值都視為某個區域的識別色。這條寫死之後該檢查就是一次線性掃描。

### 2.4 `reference` 一致性只需一條線性掃描

**對每個 flats region，`reference` 在其內部像素同色 —— 否則錯誤 ＋ 首個相異像素座標。** 只有這一條。

推論：若每個 region 內部的 `reference` 都是單一顏色，`reference` 的顏色變化就只可能發生在 `flats` 的邊界上——「`reference` 不引入 `flats` 沒有的邊界」是它的直接結果，不是另一條要檢查的事。

**相鄰區 `reference` 同色是合法的**（`assets-spec §4.3`：同色相接處的邊界消失，`reference` 的邊界通常比 `flats` 少）。任何「相鄰區顏色必須相異」的檢查都會退掉合格素材。

附帶效果：baker **完全不需要建區域鄰接圖**。

### 2.5 膨脹的語意

ID map 是滿的、沒有洞，所以「膨脹」不可能是填洞，只能是**重新分配線稿覆蓋帶內的所有權**：

```
line_mask = 降採樣後 lineart alpha ≥ LINE_ALPHA_THRESHOLD（= 32）的像素
resolved  = 所有非 line_mask 像素          ← 每輪擴張
跑 2 輪，每輪讀上一輪的快照、寫進新緩衝：
  對 line_mask 內、尚未 resolved 的像素 p：
    候選 = p 的 4-鄰像素中已 resolved 者的 ID
    候選非空 → p 取候選中母帶面積最小者（平手取 ID 最小），並加入 resolved
2 輪後仍未 resolved 的像素：保留降採樣 majority 給的原 ID，不做任何事
非 line_mask 像素永不被覆寫
```

**逐輪擴張的 `resolved` 是關鍵**：若候選來源固定為「非 `line_mask`」，第 2 輪的來源集合與第 1 輪完全相同，等於空跑一輪，線稿帶內側第 2px 的像素永遠拿不到鄰居——實際效果只有 1px。

**每輪必須讀快照、寫新緩衝**：in-place 會讓結果取決於掃描順序，違反 §3.2 的決定性。

鄰域是 §2.1 的 4-鄰：2 輪 4-鄰膨脹正好推進 2px，就是 `architecture §9.2` 陷阱 (2) 要的距離。

**仍未 resolved 的像素保留原 ID**：那是粗線稿的中心帶，本來就被線稿完全遮住，填什麼都看不到；保留 majority 的結果比引入第三種規則便宜也更可預期。

效果是每個區域的邊界被推進線稿中線以下，兩側區域在線底下重疊，majority 與 box 的半像素落差被吸收。這正是它必須在降採樣之後的原因。

### 2.6 `flats` 抗鋸齒偵測：快篩 ＋ 實判

兩條都跑，`code` 分開，都在母帶解析度：

| 檢查 | 定位 |
|---|---|
| 唯一色數 > `MAX_UNIQUE_COLORS`（1024）→ 錯誤 | **快篩**。命中代表圖徹底壞掉（例如色彩空間被轉換過），可以在算面積直方圖之前就停，錯誤訊息也更精準 |
| 任一顏色的總面積 < `MIN_COLOR_AREA`（100px）→ 錯誤 | **實判**。這才是 `assets-spec §4.2 / §7` 對繪師承諾的那條 |

**唯一色數不能拿來取代面積判準。** `assets/source/adventure-time-demo-1/flats.png` 有 171 個顏色、其中 166 個總面積 < 200px（它自己的 `meta.json` 已載明不合規、待重做），唯一色數 171 遠低於 1024——只跑快篩會把一張已知不合規的素材放行。

常數具名。報告中一律列出實際唯一色數，之後要調有依據；違反實判時列出違規的顏色與其面積。

### 2.7 新增後置警告：區域在輸出解析度下斷成多塊

母帶連通的區域，1px 頸部被降採樣吃掉後可能裂成兩塊。ID 還在（區域數一致檢查會過），但 runtime 的 Mode A 遮罩是 `id == active_region_id`——使用者點一塊，另一塊也會被填。架構文件漏了這條，補為**警告**（不拒收：被線稿切斷的髮束是合理交付）。判定用 §2.1 的 4-連通，與母帶 CC 同一套——否則「母帶連通、輸出斷裂」這句話沒有意義。

---

## 3. `.colorpack` 容器

### 3.1 Zip 佈局

entry 順序固定；`shade.png` 在 `has_shade = false` 時整個不存在。

| entry | 壓縮 |
|---|---|
| `manifest.json` | Deflate |
| `regions.json` | Deflate |
| `regions.bin` | Stored |
| `lineart.png` | Stored |
| `shade.png` | Stored（選配） |
| `thumb.jpg` | Stored |

**規則：副檔名決定壓縮方式，無例外。** 二進位走 Stored 讓 runtime 可以 mmap 整個 pack 並零拷貝取 slice；JSON 走 Deflate，因為高區域數的 `regions.json` 會到 MB 級。對已壓縮的 PNG/JPEG 再 deflate 收益近零，卻要付一份峰值記憶體。

### 3.2 決定性

mtime 全部固定為 zip epoch（1980-01-01 00:00:00）、不寫 extra field／comment、deflate level 固定為 6。同輸入重跑 → 位元相同。

**unix 權限與 host system 是「釘死」而不是「不寫」。** zip 格式的 central directory 一定有 `external_attributes`，而 zip crate 的 `normalize()` 會把 `None` 補成它自己的預設值——沒有「不寫」這個選項。所以顯式指定 **`0o644`** 與 **`System::Unix`**：前者不吃 crate 預設（換版本時預設值變了也不會改到輸出位元），後者消掉 zip crate 依 `cfg!(windows)` 在 Dos／Unix 之間分歧的行為，否則「同輸入重跑 → 位元相同」只在同一個 OS 家族內成立。有測試釘住 `external_attributes`。

### 3.3 `content_hash`

**SHA-256 over 未壓縮內容，不是 hash 整個 zip 檔。**

正規化串流，固定 entry 順序，排除 `manifest.json` 自己：

```
for entry in ENTRY_ORDER:
    name_len: u32 LE ‖ name ‖ data_len: u64 LE ‖ data
```

以 `"sha256:" + lowercase hex` 寫入 `manifest.content_hash`。

兩個理由：manifest 在 zip 內又要含 hash 會循環；更重要的是 `architecture §8.4` 規定「文件永遠指向它原本的 `asset_hash`，不自動升級」——**hash 必須不受 zip crate 版本與 deflate 實作影響**，否則升級一個依賴就會讓全世界的使用者作品失效。

因為同一個理由，**必須有凍結向量**：只驗「改內容會換 hash」這種相對性質的話，把長度前綴改成 BE、調換 `ENTRY_ORDER`、或改 `RegionEntry` 的欄位順序，測試都會全綠。凍結兩層——`hash::content_hash` 的最小輸入一組，完整 sample pack 一組。**改到那兩個字面值就是改了 major 契約。**

`ColorPack::open()` **重算 hash 並比對**。既然 hash 取的是未壓縮內容，讀端重算幾乎不用額外成本；而 runtime 拿 `asset_hash` 對應使用者作品，內容與 hash 對不上時繼續讀，等於把錯的東西當成對的存進去。同理 `open()` 也驗 `region_ids` 全部 `< region_count`——超界 ID 會變成 Mode A 永遠點不到的幽靈區。

### 3.4 `manifest.json`

```json
{
  "schema_version": "1.0",
  "id": "kirby-demo-1",
  "content_hash": "sha256:...",
  "canvas_size": [2048, 2048],
  "aspect": "1:1",
  "region_count": 187,
  "difficulty": "easy",
  "category": "cartoon",
  "has_shade": true,
  "palette": ["#RRGGBB", "..."]
}
```

| 欄位 | 值域 |
|---|---|
| `aspect` | `"1:1"` \| `"3:4"` |
| `difficulty` | `"easy"`（≤60）\| `"medium"`（61–200）\| `"focused"`（>200）。門檻 SSOT 在 `assets-spec §8` |
| `category` | `anime` \| `mandala` \| `animal` \| `botanical` \| `scenery` \| `cartoon` |

`schema_version` 為 `major.minor`，runtime 拒絕未知 major。M1 起即為 `"1.0"`。

### 3.5 `palette[]` 與 `suggested_color` 的分工

`architecture §9.4` 讓兩者並存，看起來是兩份真相。對照 `prd §4.4`（「每張線稿附帶一組**建議調色盤**」，是色票選擇器的預設值），正確解讀是兩個不同的東西：

| 欄位 | 語意 |
|---|---|
| `manifest.palette[]` | **去重後**的建議色票清單，給色盤 UI。依總面積遞減排序，平手取較小 region id。不設上限，UI 自行取前 N 個 |
| `regions.json[].suggested_color` | 逐區建議色。同筆記錄的 `bbox` 供 `§4.5` 擴散動畫、`area` 供 `§4.7` 進度計算 |

### 3.6 `regions.bin`

R16 ID map，RLE 無損。ID 範圍 `0..=region_count-1`，`region_count ≤ 65535`。

### 3.7 `thumb.jpg`

母帶解析度下把 `reference` 逐區配色 × `shade` × `lineart` 合成，再 box 降採樣至**長邊 512**，JPEG quality 85。

Gallery 對鎖定的線稿也顯示縮圖（`prd §5.1`），且 `architecture §8.4` 在資產取不到時以縮圖唯讀顯示——所以縮圖要呈現「這張畫完會長什麼樣」，不是空線稿。

### 3.8 契約

`contracts/colorpack.schema.json` 是 SSOT。Rust 端 `Manifest` 有一條測試斷言「序列化樣本通過 schema」（dev-dep `jsonschema`）。

---

## 4. 檢查清冊與報告

### 4.1 檢查清冊

`code` 是固定字彙表——測試斷言的是特定 `code`（§6），繪師收到的退件也照它分類。新增檢查必須同時進這張表。

| 檢查項 | `code` | 階段 | severity | 來源 |
|---|---|---|---|---|
| 來源目錄缺檔（三張必交 PNG 或 `meta.json`） | `source-incomplete` | Master | Error | 本設計 §2 |
| 四張圖尺寸與對齊一致 | `size-mismatch` | Master | Error | `assets-spec §7`、`arch §9.3` |
| 長邊 4096、比例 1:1 或 3:4 | `canvas-size` | Master | Error | `assets-spec §7`、`arch §9.3` |
| 色彩描述檔為 sRGB | `color-space` | Master | Error | `assets-spec §7`、`arch §9.3` |
| `flats` 唯一色數 > 1024 | `unique-color-overflow` | Master | Error | 本設計 §2.6（快篩） |
| `flats` 任一顏色總面積 < 100px（**母帶**解析度） | `tiny-color-area` | Master | Error | `assets-spec §7`「`flats` 無抗鋸齒」、`arch §9.3` |
| `flats` 使用保留色 `#FF00FF` | `reserved-color` | Master | Error | `assets-spec §7` |
| 未指派像素（`flats` alpha < 255，§2.3） | `unassigned-pixel` | Master | Error | `assets-spec §7`、`arch §9.3` |
| `reference` 每區顏色唯一（§2.4） | `ref-mismatch` | Master | Error | `assets-spec §7` 的兩列（「每區顏色唯一」＋「不含 `flats` 沒有的邊界」，後者是前者的推論）、`arch §9.3` |
| `shade` 有 luma < 60 的像素 | `shade-too-dark` | Master | Error | `assets-spec §7` |
| `meta.json` 的 `id` 與資料夾名不一致 | `meta-id-mismatch` | Master | Error | `assets-spec §7`、`arch §9.3` |
| `meta.json` 的 `category` 非六個允許值 | `meta-bad-category` | Master | Error | `assets-spec §7`、`arch §9.3` |
| 降採樣後區域數與母帶不一致 | `region-count-drift` | Output | Error | `assets-spec §7`、`arch §9.3` |
| 區域數 > 65535 | `region-count-overflow` | Output | Error | `assets-spec §7`、`arch §9.3` |
| 碎片區域（面積 < 200px，**輸出**解析度） | `tiny-region` | Output | Warning | `assets-spec §7`、`arch §9.3` |
| 區域數過多或過少 | `region-count-range` | Output | Warning | `assets-spec §7`、`arch §9.3` |
| 區域在輸出解析度下斷成多塊 | `region-split` | Output | Warning | 本設計 §2.7 |

面積門檻分屬不同解析度，是最容易混用的地方：`tiny-color-area` 的 100px 在**母帶**，`tiny-region` 的 200px 在**輸出**（繪師端對應的母帶數字是 800px，見 `assets-spec §7` 註）。

**`unassigned-pixel` 只在 Master。** `arch §9.3` 原本標「母帶 ＋ 輸出」，但輸出階段它**恆真**——majority 必定為每個輸出像素產出一個既有 ID，沒有第三種可能。與其留一條永遠不會觸發的檢查，不如降成 Master 專屬，實作端只保留一條 `debug_assert`。

### 4.2 報告

```rust
struct Diagnostic {
    severity: Severity,      // Error | Warning
    code: &'static str,      // "unassigned-pixel" / "ref-mismatch" / ...
    stage: Stage,            // Master | Output
    message: String,
    coords: Vec<(u32, u32)>, // 上限 16
    coord_total: usize,      // 超過上限時的實際總數
    region: Option<u32>,
}
```

- **階段內不 fail-fast**：跑完該階段全部檢查才決定。繪師一次拿到所有問題，來回從 N 天變 1 天。
  **唯二的例外是算不下去的兩條**——`size-mismatch`（逐像素會越界）與 `unique-color-overflow`（connected components 會切出幾百萬區）。除此之外一律往下跑：`canvas-size` 命中時像素檢查仍然成立，`reserved-color` / `tiny-color-area` 命中時 `label_regions` 仍然成立。把它們綁進同一個提早退出，就會讓繪師改完尺寸重交才第一次看到縫隙問題。
- **階段間 fail-fast**：母帶有 Error 就不進降採樣，後面的結果沒有意義。
- **`flats` 的唯一色數一律列出**，掛在 `Report` 而非 `Summary`——「之後要調 `MAX_UNIQUE_COLORS` 才有依據」講的正是被退件的那些素材，只在成功路徑輸出等於沒有。快篩命中時掃描提前中止，該值是**下界**，渲染成 `≥N` 而不是假裝是實際值。
- **座標一律換算回母帶座標系**（繪師在 CSP 裡看到的那個）。輸出階段發現的問題 ×2 換算並標註「於輸出解析度發現」。這是 `assets-spec §7`「退件附失敗座標」這個承諾能否兌現的關鍵。
- coords 上限 16，超過印「另有 N 處」。一張全錯的圖不該吐四百萬行。
- exit code：`0` 通過（可含警告）／ `1` 有 Error ／ `2` baker 自身故障。
- 文字輸出與 `--report x.json` 從同一個 `Vec<Diagnostic>` 渲染，不是兩份真相。

---

## 5. 測試素材

### 5.1 torture 拆成兩組

現行 `xtask gen-torture` 產出的 `torture-01` **必然被 baker 拒收**：它的 comb／checker／rings／edges 都是 1px 特徵，母帶 ÷2 的 2×2 majority 下每個區塊都是 2:2 平手，整片塌成單色，區域數從上萬掉到個位數，撞上「降採樣後區域數與母帶一致 → 錯誤」。它是 negative fixture，不是 happy path 素材。

| 產物 | 定位 |
|---|---|
| `assets/source/torture-01/` | **合格**壓力素材。所有特徵最短邊 **≥8px**，軸對齊的特徵落在偶數邊界（降採樣後仍 ≥4px）。實測 **4236 區**，`has_shade = false`，3:4 |
| `baker::synth::negative()`（生成器程式碼） | 5 組預期拒收：`gap`（未指派像素）、`ref-mismatch`（在某一塊裡塗第二個顏色——不是相鄰區同色，那是合法的）、`display-p3`、`antialiased`、`vanishing-1px`（原本的 1px 特徵搬來） |

**「對齊偶數邊界」只約束軸對齊的特徵。** `zone_pie` 是 `atan2` 放射楔形，邊界本來就不是水平／垂直線，21 個區域的 bbox 落在奇數座標——但那些區域最窄處 ≈25px，降採樣後遠 >2px。要求放射楔形量化到偶數格會扭曲它想測的東西（非軸對齊邊界在 majority 下的行為），得不償失。

**區域數是 4236，不是「上萬」。** 這是 `zone_grid` cell=32 在 4 個 zone 上的實際結果。4236 已經遠超 `REGION_COUNT_MAX`（2000）、落在 `focused` 難度，壓力測試的目的達到了；為了湊一個整數而把 cell 再切細，只是讓測試更慢。**`torture-01` 有一條 e2e 測試直接烘它**（用生成器現場產生，不讀 LFS 檔），斷言它**通過**且唯一的診斷是 `region-count-range` 警告——出現 `tiny-region` 或 `region-split` 就代表 ≥8px／偶數對齊的設計沒守住。

**生成器重寫時 `PALETTE` 必須移除 `#FF00FF`。** `xtask/src/torture.rs` 的 `PALETTE[5] = [255, 0, 255]`，而 `c()` 回傳 1..=15——洋紅確實出現在現行的 `flats.png`。那是 `assets-spec §6.1` 縫隙檢查的保留色（清冊裡的 `reserved-color`），所以即使把所有特徵重做成 ≥4px，`torture-01` 仍會被 baker 自己拒收。改用另一個高飽和、且與其餘 15 色差異夠大的顏色。

`torture-01` 補產 `reference.png`：`reference[p] = PERM[flats[p]]`，`PERM` 是 0..15 的固定雙射。雙射保證每區仍是單一純色（§2.4 唯一的那條檢查），但檔案位元 ≠ `flats.png`，能抓到「baker 偷懶直接比檔案而非比區域」的錯誤實作。

**生成器與 committed 產物之間有 drift 守門。** `gen-torture` 順帶寫出 `assets/source/torture-01/synth-lock.json`，內含三張圖**原始 RGBA** 的正規化 hash；一條測試重新生成並比對。刻意不 hash PNG bytes——那會綁到 `png` crate 的 deflate 實作，換一次依賴版本就誤報；要守的是「改了 `synth.rs` 卻忘了重跑 `gen-torture`」。lock 檔落在 `assets/source/**/*.json`，依 `.gitattributes` 不進 LFS，所以 CI 的 `lfs: false` checkout 也讀得到。

### 5.2 negative fixture 不進 git

fixture 必須是完整 4096 級尺寸（否則會先撞到「長邊 4096」檢查，測不到想測的那條）。5 組 × 4 張進 git 是幾百 MB。

**放進 repo 的是生成器程式碼，不是 PNG。** 每條測試自己生一組到 tempdir，跑完即丟。repo 零增重，fixture 隨規格演進不會膠死。

生成器住在 **`tools/baker/src/synth.rs`**（不是 `tests/fixtures/`）——negative fixture 與 `torture-01` 共用同一組 zone 原語，而 `torture-01` 要被 `xtask gen-torture` 當 library 呼叫，只有在 `src/` 才做得到。`tests/` 底下的東西 xtask 看不見。

### 5.3 現有素材的修正

- `kirby-demo-1` / `adventure-time-demo-1` 的 `meta.json` 的 `category: "cartoon"` 已隨 `assets-spec` 補列而合法
- `kirby-demo-1` 的 `flats.png` / `reference.png` 的 iCCP 名稱是泛用的 `ICC Profile`（與 `lineart` / `shade` 的 chunk 組成不同，是分批導出的）。**色彩空間判定不能只看 iCCP 名稱**，必須解析 profile 的 colorant 與 white point，或退回 `gAMA` + `cHRM` 判定
- `assets/` 尚未 commit，LFS 尚未生效。首次 commit 需確認 `.gitattributes` 的 filter 有作用

---

## 6. 測試層級

| 層 | 內容 |
|---|---|
| 單元 | rle round-trip（proptest）、majority 平手規則、dilate、色彩空間判定（含泛用名稱 iCCP）、`content_hash` 凍結向量（§3.3）、zip 的 mtime／權限／entry 順序 |
| 拒收 | 5 個 negative fixture 各一條測試。斷言 **特定 `code` 出現 ＋ 座標落在生成器刻意植入的位置**——只斷言「失敗」會讓任何理由的失敗都變綠燈 |
| 端到端 | 素材 → pack → 用 colorpack reader 開回來 → 斷言 `region_count` / `difficulty` / `has_shade`，並**連跑兩次比對位元相同**。另有一條專跑 `torture-01`（§5.1） |
| 階段 | 一張同時踩到四條互相獨立檢查的素材，斷言報告**四條全含**——這是 §4.2「階段內不 fail-fast」唯一守得住的方式 |

**`display-p3` 是唯一沒有座標斷言的拒收測試**：問題在 PNG chunk 不在像素，沒有座標可報。改斷言「訊息指名是哪一張圖」＋「只有 `flats` 命中，其餘三張不被連坐」。

**端到端跑的是 `synth` 生成的素材，不是 LFS 的三張手繪。** CI 以 `lfs: false` checkout（`build-infra.md §5`），真實素材在 CI 裡只拿得到 pointer；而且把 golden 值綁在手繪素材上，素材一改就要改測試。真實素材的烘焙留給本地的 `cargo xtask bake`。連帶結果：`assets/source/**` 觸發 CI 時，實際驗得到的只有 `meta.json` 與 `synth-lock.json`。

**`core/colorpack` 的 reader 在 M1 就做**，不留到 E1：round-trip 是驗證 writer 正確最便宜的手段，而 reader 本身只是 zip central directory 解析加 RLE 解碼。

CI：`tools/baker/**` 或 `assets/source/**` 變動 → 跑全部四層。

---

## 7. 不在本設計範圍

- runtime 端的 pack 載入與 GPU 上傳（E1）
- `.colorpack` 上傳 R2 與分發（`architecture §11.2`）
- 縮圖的視覺設計細節（本設計只定「reference 配色 × shade × lineart 合成後降採樣」與 JPEG 參數）
