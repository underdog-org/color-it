# Baker 核心設計

> M1 資產管線的實作規格。範圍：`tools/baker` ＋ `core/colorpack`。
> 狀態：v1.0（2026-08-03）
> 相關：[roadmap/M1](../roadmap/M1.md)｜[architecture §9](../architecture.md)｜[assets-spec](../assets-spec.md)

---

## 0. 本文件先解掉的既有矛盾

實作前必須先改文件，否則管線核心無法成立。

**(a) 直式比例正名 3:4。** `assets-spec §3` 的母帶「4:5 → 3072×4096」實際是 3:4；`prd §6` 與 `architecture §4.1.1` 的 runtime「4:5 → 1536×1920」則是 4:5。3072→1536 是 ÷2，4096→1920 是 ÷2.133，兩軸倍率不同會壓扁畫面，且 `architecture §9.1` 明訂「4096→2048 是整數倍降採樣，這對 flats 的正確性很重要」在直式路徑上不成立。

> **決議**：母帶不動（既有兩張直式素材已是 3072×4096），runtime 改為 **1536×2048**，比例名稱改為 **3:4**。兩種比例都是乾淨的 ÷2。像素數 3.15M，仍低於 1:1 的 4.19M，`§4.1.1`「以 1:1 為記憶體上界」的論證不變。

**(b) `category` 的 SSOT 沒跟上。** `architecture.md:1010` 已加入 `cartoon`（六值），`assets-spec §4.5` 仍是五值。SSOT 是 `assets-spec`，需補。

**文件修正清單**（M1 驗收「決策已寫回文件」）：

| 檔案 | 修正 |
|---|---|
| `prd §6`、`architecture §4.1.1 / §9.1`、`assets-spec §3` | 直式正名 3:4、runtime 1536×2048 |
| `assets-spec §4.5` | `category` 補 `cartoon` |
| `architecture §9.2` | 補：降採樣對象是 ID map、majority 平手規則、膨脹的精確語意 |
| `architecture §9.3` | 補：未指派像素定義、AA 門檻常數、新增「輸出解析度下區域斷裂」警告 |
| `architecture §9.4` | 釐清 `palette[]` 與 `suggested_color` 的分工 |
| `specs/build-infra.md` | `bake` 狀態、`deps-policy` 兩條新依賴 |

---

## 1. 模組邊界

`core/colorpack` 是 baker 與 runtime 的唯一共用面（`architecture §6` Boundary 4）。它只懂容器格式，**不依賴 `png`**——PNG bytes 對它是不透明 blob。

```
core/colorpack/            deps: serde, serde_json, zip, sha2
  lib.rs        ColorPack::write_to() / ColorPack::open()
  manifest.rs   Manifest serde ＋ schema_version major 檢查
  region.rs     RegionEntry
  rle.rs        R16 RLE 編解碼
  container.rs  zip 讀寫：副檔名→壓縮方式、決定性 metadata
  hash.rs       content_hash 的正規化定義

tools/baker/               deps: colorlull-colorpack, png, clap, anyhow, serde_json
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
  synth.rs      合成素材產生器（zone 原語）
```

**`synth.rs` 放在 baker 而非 xtask**：torture-01 與 negative fixture 是同一件事的正反面，共用同一組 zone 原語。`xtask` 改為依賴 `colorlull-baker` lib，`gen-torture` 與 `bake` 都直接呼叫，不 shell out（錯誤訊息才帶得回來）。

`xtask/deps-policy.toml` 補兩行：

```toml
[crates.baker]
internal = ["colorpack"]

[crates.xtask]
internal = ["baker"]
```

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

### 2.1 降採樣的對象是 region ID，不是 RGB

一個 2×2 區塊可能同時含 A(紅)、B(綠)、C(紅)，其中 A 與 C 不相鄰所以合法同色。對 RGB 取眾數會得到「紅」但無從得知是哪個區域。**majority 必須作用在 ID map 上。**

### 2.2 majority 平手規則

平手時取**母帶面積最小的區域**；面積再平手取 ID 最小。

理由：這條規則直接服務於「降採樣後區域數與母帶一致」這條硬性錯誤。細區域是唯一會被吃掉的一方，讓它贏；大區域損失的是邊緣 ≤1px，而那一帶本來就在線稿底下。成本是查一次面積表（CC 完就有）。完全決定性。

### 2.3 「未指派像素」的唯一定義

**`flats` 的 alpha < 255。** 任何 RGB 值都視為某個區域的識別色。這條寫死之後該檢查就是一次線性掃描。

### 2.4 `reference` 一致性不跑第二次 CC

兩條等價且更便宜的檢查：

1. 對每個 flats region，`reference` 在其內部像素同色 —— 否則錯誤 ＋ 首個相異像素座標
2. 對每對相鄰 region，`reference` 顏色相異 —— 否則錯誤 ＋ 邊界座標

兩條合起來 ⟺「對 `reference` 獨立跑 CC 會得到相同分割」，而且錯誤訊息直接指得出是哪一塊。

### 2.5 膨脹的語意

ID map 是滿的、沒有洞，所以「膨脹」不可能是填洞，只能是**重新分配線稿覆蓋帶內的所有權**：

```
line_mask = 降採樣後 lineart alpha ≥ LINE_ALPHA_THRESHOLD（= 32）的像素
迭代 2 次：
  line_mask 內的像素，採用相鄰非 line_mask 像素的 ID
  （多個候選取母帶面積最小者，平手取 ID 最小）
非 line_mask 像素永不被覆寫
```

效果是每個區域的邊界被推進線稿中線以下，兩側區域在線底下重疊，majority 與 box 的半像素落差被吸收。這正是它必須在降採樣之後的原因。

### 2.6 `flats` 抗鋸齒偵測

**唯一色數 > `MAX_UNIQUE_COLORS`（1024）→ 錯誤。** 常數具名，報告中一律列出實際唯一色數，之後要調有依據。

間距夠大不會誤判：200 區域的圖開了 AA 會產生數萬個混色；torture 只用 16 色；最複雜的曼陀羅手工分色也遠不到 1024。

### 2.7 新增後置警告：區域在輸出解析度下斷成多塊

母帶連通的區域，1px 頸部被降採樣吃掉後可能裂成兩塊。ID 還在（區域數一致檢查會過），但 runtime 的 Mode A 遮罩是 `id == active_region_id`——使用者點一塊，另一塊也會被填。架構文件漏了這條，補為**警告**（不拒收：被線稿切斷的髮束是合理交付）。

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

mtime 全部固定為 zip epoch（1980-01-01 00:00:00）、不寫 unix 權限／extra field／comment、deflate level 固定。同輸入重跑 → 位元相同。

### 3.3 `content_hash`

**SHA-256 over 未壓縮內容，不是 hash 整個 zip 檔。**

正規化串流，固定 entry 順序，排除 `manifest.json` 自己：

```
for entry in ENTRY_ORDER:
    name_len: u32 LE ‖ name ‖ data_len: u64 LE ‖ data
```

以 `"sha256:" + lowercase hex` 寫入 `manifest.content_hash`。

兩個理由：manifest 在 zip 內又要含 hash 會循環；更重要的是 `architecture §8.4` 規定「文件永遠指向它原本的 `asset_hash`，不自動升級」——**hash 必須不受 zip crate 版本與 deflate 實作影響**，否則升級一個依賴就會讓全世界的使用者作品失效。

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

## 4. 錯誤處理與報告

```rust
struct Diagnostic {
    severity: Severity,      // Error | Warning
    code: &'static str,      // "unassigned-pixel" / "ref-mismatch" / ...
    stage: Stage,            // Master | Output
    message: String,
    coords: Vec<(u32, u32)>, // 上限 16，超過只報總數
    region: Option<u32>,
}
```

- **階段內不 fail-fast**：跑完該階段全部檢查才決定。繪師一次拿到所有問題，來回從 N 天變 1 天。
- **階段間 fail-fast**：母帶有 Error 就不進降採樣，後面的結果沒有意義。
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
| `assets/source/torture-01/` | **合格**壓力素材。所有特徵 ≥4px 且對齊偶數邊界（降採樣後仍 ≥2px）。區域數上萬，`has_shade = false`，3:4 |
| `tools/baker/tests/fixtures/`（生成器程式碼） | 5 組預期拒收：`gap`（未指派像素）、`ref-mismatch`、`display-p3`、`antialiased`、`vanishing-1px`（現行的 1px 特徵搬來） |

`torture-01` 補產 `reference.png`：`reference[p] = PERM[flats[p]]`，`PERM` 是 0..15 的固定雙射。雙射保序相鄰相異，所以邊界一致性與每區單一純色都成立；但檔案位元 ≠ `flats.png`，能抓到「baker 偷懶直接比檔案而非比區域」的錯誤實作。

### 5.2 negative fixture 不進 git

fixture 必須是完整 4096 級尺寸（否則會先撞到「長邊 4096」檢查，測不到想測的那條）。5 組 × 4 張進 git 是幾百 MB。

**`tools/baker/tests/fixtures/` 放的是生成器程式碼，不是 PNG。** 每條測試自己生一組到 tempdir，跑完即丟。repo 零增重，fixture 隨規格演進不會膠死。

### 5.3 現有素材的修正

- `kirby-demo-1` / `adventure-time-demo-1` 的 `meta.json` 的 `category: "cartoon"` 已隨 `assets-spec` 補列而合法
- `kirby-demo-1` 的 `flats.png` / `reference.png` 的 iCCP 名稱是泛用的 `ICC Profile`（與 `lineart` / `shade` 的 chunk 組成不同，是分批導出的）。**色彩空間判定不能只看 iCCP 名稱**，必須解析 profile 的 colorant 與 white point，或退回 `gAMA` + `cHRM` 判定
- `assets/` 尚未 commit，LFS 尚未生效。首次 commit 需確認 `.gitattributes` 的 filter 有作用

---

## 6. 測試層級

| 層 | 內容 |
|---|---|
| 單元 | rle round-trip（proptest）、majority 平手規則、dilate、色彩空間判定（含泛用名稱 iCCP） |
| 拒收 | 5 個 negative fixture 各一條測試。斷言 **特定 `code` 出現 ＋ 座標落在生成器刻意植入的位置**——只斷言「失敗」會讓任何理由的失敗都變綠燈 |
| 端到端 | 三張素材 → pack → 用 colorpack reader 開回來 → 斷言 `region_count` / `difficulty` / `has_shade`，並**連跑兩次比對位元相同** |

**`core/colorpack` 的 reader 在 M1 就做**，不留到 E1：round-trip 是驗證 writer 正確最便宜的手段，而 reader 本身只是 zip central directory 解析加 RLE 解碼。

CI：`tools/baker/**` 或 `assets/source/**` 變動 → 跑全部三層。

---

## 7. 不在本設計範圍

- runtime 端的 pack 載入與 GPU 上傳（E1）
- `.colorpack` 上傳 R2 與分發（`architecture §11.2`）
- 縮圖的視覺設計細節（本設計只定「reference 配色 × shade × lineart 合成後降採樣」與 JPEG 參數）
