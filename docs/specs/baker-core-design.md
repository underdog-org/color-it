# Baker 核心設計

> `tools/baker` ＋ `core/colorpack` 的 SSOT。範圍：色標交付管線、`.colorpack` 容器格式、檢查清冊與報告、測試。
> 狀態：v2.0（2026-08-03）——**色標交付已實作**（Phase 0–4 全部完成）。本版把「色標交付」
> 設計合併進來，取代 v1.2 的 flats 管線：`flats.png`／`reference.png` 已刪，區域由線稿封閉區 ＋ 色標推導。
> 繪師端規格（給外包看的交付要求）見 [assets-spec](./assets-spec.md)。
> 相關：[roadmap/M1](../roadmap/M1.md)｜[architecture §9](../architecture.md)

---

## 0. 本文件先解掉的既有矛盾

**直式比例正名 3:4。** `assets-spec §3` 的母帶「4:5 → 3072×4096」實際是 3:4；`prd §6` 與
`architecture §4.1.1` 的 runtime「4:5 → 1536×1920」則是 4:5。3072→1536 是 ÷2，4096→1920 是 ÷2.133，
兩軸倍率不同會壓扁畫面。

> **決議**：母帶不動（既有兩張直式素材已是 3072×4096），runtime 改為 **1536×2048**，比例名稱改為
> **3:4**。兩種比例都是乾淨的 ÷2。像素數 3.15M，仍低於 1:1 的 4.19M，`§4.1.1`「以 1:1 為記憶體上界」
> 的論證不變。

---

## 1. 為什麼是色標交付

現行契約（v0.2）要求繪師手工產出一張**像素級精確的量化圖**（`flats.png`）：抗鋸齒必須關閉、整張不透明、
每個顏色總面積 ≥100px、相鄰區不得同色，交付前還要整層鋪洋紅、放大到 100% 掃過 4096×4096 找縫隙。

實測下來這條路不可靠，而且失敗模式**繪師無法從診斷訊息推回動作**：

| 繪師的體感 | baker 實際命中的檢查 |
|---|---|
| 「我明明填滿了」 | 邊緣抗鋸齒帶 alpha<255 → `unassigned-pixel`；過渡色各自成區 → `tiny-color-area` 噴上千條；過渡色破 1024 → `unique-color-overflow` |
| 「小地方沒塗到」 | 線稿交叉處的縫，100% 檢視下肉眼不可見 → `unassigned-pixel` |
| 「顏色飽和度不對」 | 非 sRGB → `color-space`；P3→sRGB 轉換讓同一純色裂成差 1 的鄰居 → 多出一區，面積不足再噴 `tiny-color-area` |

根因是分工位置錯了：PS / CSP 的填充工具**本來就靠線稿封閉區**在算填色範圍，現行契約要求繪師把工具
算出來的結果再手工修成完美。這一步應該在 baker 裡做。

### 1.1 新的交付契約

```
<id>/
  lineart.png     線稿（規格完全不變，抗鋸齒照開）——**區域邊界的唯一來源**
  seeds.png       ★ 新
  shade.png       選配（不變）
  meta.json       不變
```

**刪除 `flats.png` 與 `reference.png`。**

| 項目 | 要求 |
|---|---|
| `seeds.png` 格式 | RGBA，尺寸同 `lineart`，背景透明 |
| 內容 | 每個要獨立上色的區域裡點**一個**色點 |
| 色點 | 形狀不限、**抗鋸齒開著沒關係**、直徑約 ≥16px |
| 顏色 | **就是建議色**——希望 App 建議使用者塗的那個顏色 |
| 相鄰同色 | **允許**。區域由線稿決定，不由顏色決定 |
| 色彩描述檔 | sRGB。只影響建議色是否精準，不影響能否通過 |

繪師端的規則濃縮成一句：**每一塊你希望能單獨上色的地方，點一個點，顏色就是你的建議色。**

> **一個封閉區一個點——不是一個顏色一個點。** 線稿把一片顏色切成幾塊，就要點幾個點。

### 1.2 色點的讀法

`alpha > 0` 的像素做 4-連通 → 每個連通塊是一個 seed；取該塊內 `alpha == 255` 像素的**眾數色**當建議色。

抗鋸齒邊緣的過渡色永遠是少數，取不到眾數——**這就是抗鋸齒痛點消失的機制**。`alpha == 255` 面積不足
`MIN_SEED_AREA` → `seed-too-small`。

色點的顏色同時擔任「識別」與「建議配色」。`reference.png` 因此整張消失。代價是繪師無法在自己的工具裡
看見整體配色效果——`--debug-out` 的 `reference-preview.png`（§4.3）把這個能力還給他。

### 1.3 三條舊限制的下場

- 「不得有任何顏色總面積 <100px」→ 刪
- 「整張不透明、每個像素都要有顏色」→ 刪
- 「相鄰區不可同色」→ 刪。臉和脖子相連又同色會被合併的坑不再存在
- `#FF00FF` 保留色 → 刪。洋紅檢查沒了，這個顏色可自由使用

---

## 2. 管線

```
source::load          lineart + seeds + [shade] + meta.json
      ↓
母帶檢查              geometry（尺寸一致 / canvas-size）＋ color_space
      ↓
binarize              lineart.alpha ≥ line_threshold → line mask
      ↓
seeds::read           alpha>0 連通分量 → (重心, 眾數色)[]
      ↓
segment::grow         逐 seed 在 !line 上 4-連通 flood fill
      ↓
segment::close        測地擴張把 id 填進線像素，直到全覆蓋
      ↓
母帶檢查              seed-collision / orphan-area / seed-too-small / seed-on-line
      ↓
thumb → resample → dilate → check::output → 打包
```

改動全部集中在母帶階段；`.colorpack` 格式（§3）與 `resample`／`dilate`／`thumb` 一律不動。

### 2.1 連通性一律是 4-連通；majority 的對象是 region ID，不是 RGB

**connected components 用 4-連通**，全文的「相鄰」都指這個。8-連通會把只在對角接觸的兩塊同色區域併成
一塊，而繪師端的心智模型是「相連才會合併」——對角相觸算不算相連，在繪師端是模稜兩可的。

一個 2×2 區塊可能同時含 A(紅)、B(綠)、C(紅)，其中 A 與 C 不相鄰所以合法同色。對 RGB 取眾數會得到「紅」
但無從得知是哪個區域。**majority 必須作用在 ID map 上。**

### 2.2 majority 平手規則

平手時取**母帶面積最小的區域**；面積再平手取 ID 最小。

理由：這條規則直接服務於「降採樣後區域數與母帶一致」這條硬性錯誤。細區域是唯一會被吃掉的一方，讓它贏；
大區域損失的是邊緣 ≤1px，而那一帶本來就在線稿底下。完全決定性。

### 2.3 region 產生：`grow` ＋ `close`

**`grow` 用逐 seed flood fill，不用多源同步 BFS。** 線稿封閉時兩者等價。不封閉時逐 seed 能明確報
「seed A 撞進 seed B 已佔的區域」，同步 BFS 只會在中間切一條任意分界線然後靜默通過。要診斷，不要吞掉。

**`close` 用距離排序的測地擴張。** `region_ids` 必須全覆蓋（App 端不容許沒有 id 的像素）。BFS 波前逐輪
擴張，未指派像素從已標記鄰居取 id，等距時取**較小 id** → 確定性，且分界線自然落在線的中軸。

這一步在**母帶**做完，所以 `dilate` 的職責不變：它仍然只負責修降採樣造成的縫（§2.5）。

**region id 依 seed 重心的 raster order 編號。** 確定性，與繪師點色標的先後無關。

**小碎片併入鄰居，不報錯。** `grow` 之後未被認領的自由區塊，面積 < `MIN_ORPHAN_AREA` 的併進面積最大的
相鄰區；≥ 門檻才報 `orphan-area`。這是「繪師漏點一塊」與「線稿有個 3px 封閉小洞」的分野。

### 2.4 參數

| 參數 | 預設 | 作用 |
|---|---|---|
| `line_threshold` | 128 | `lineart.alpha ≥` 此值視為線 |
| `MIN_SEED_AREA` | 64 | 色點的 `alpha==255` 面積下限（母帶） |
| `MIN_ORPHAN_AREA` | 500 | 未認領自由區的報錯門檻（母帶） |
| `MAX_LINE_RATIO` | 0.35 | 線像素佔比超過此值發 `line-coverage` 警告 |

四個都可 `--set` 覆寫，且**納入 `content_hash`**。預設值視為契約的一部分——真要調參數就等於改契約，
全量重烘是應該的。

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

**逐輪擴張的 `resolved` 是關鍵**：若候選來源固定為「非 `line_mask`」，第 2 輪的來源集合與第 1 輪完全相同，
等於空跑一輪，線稿帶內側第 2px 的像素永遠拿不到鄰居——實際效果只有 1px。

**每輪必須讀快照、寫新緩衝**：in-place 會讓結果取決於掃描順序，違反決定性。

鄰域是 §2.1 的 4-鄰：2 輪 4-鄰膨脹正好推進 2px，就是 `architecture §9.2` 陷阱 (2) 要的距離。

**仍未 resolved 的像素保留原 ID**：那是粗線稿的中心帶，本來就被線稿完全遮住，填什麼都看不到。

效果是每個區域的邊界被推進線稿中線以下，兩側區域在線底下重疊，majority 與 box 的半像素落差被吸收。

### 2.6 新增後置警告：區域在輸出解析度下斷成多塊

母帶連通的區域，1px 頸部被降採樣吃掉後可能裂成兩塊。ID 還在（區域數一致檢查會過），但 runtime 的
Mode A 遮罩是 `id == active_region_id`——使用者點一塊、另一塊也會被填。補為**警告**（不拒收：被線稿
切斷的髮束是合理交付）。判定用 §2.1 的 4-連通，與母帶 CC 同一套。

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

**規則：副檔名決定壓縮方式，無例外。** 二進位走 Stored 讓 runtime 可以 mmap 整個 pack 並零拷貝取 slice；
JSON 走 Deflate，因為高區域數的 `regions.json` 會到 MB 級。對已壓縮的 PNG/JPEG 再 deflate 收益近零，
卻要付一份峰值記憶體。

### 3.2 決定性

mtime 全部固定為 zip epoch（1980-01-01 00:00:00）、不寫 extra field／comment、deflate level 固定為 6。
同輸入重跑 → 位元相同。

**unix 權限與 host system 是「釘死」而不是「不寫」。** zip 格式的 central directory 一定有
`external_attributes`，而 zip crate 的 `normalize()` 會把 `None` 補成它自己的預設值——沒有「不寫」這個
選項。所以顯式指定 **`0o644`** 與 **`System::Unix`**：前者不吃 crate 預設（換版本時預設值變了也不會改到
輸出位元），後者消掉 zip crate 依 `cfg!(windows)` 在 Dos／Unix 之間分歧的行為，否則「同輸入重跑 → 位元
相同」只在同一個 OS 家族內成立。有測試釘住 `external_attributes`。

### 3.3 `content_hash`

**SHA-256 over 未壓縮內容，不是 hash 整個 zip 檔。**

正規化串流，固定 entry 順序，排除 `manifest.json` 自己：

```
for entry in ENTRY_ORDER:
    name_len: u32 LE ‖ name ‖ data_len: u64 LE ‖ data
```

以 `"sha256:" + lowercase hex` 寫入 `manifest.content_hash`。

兩個理由：manifest 在 zip 內又要含 hash 會循環；更重要的是 `architecture §8.4` 規定「文件永遠指向它原本的
`asset_hash`，不自動升級」——**hash 必須不受 zip crate 版本與 deflate 實作影響**，否則升級一個依賴就會讓
全世界的使用者作品失效。

因為同一個理由，**必須有凍結向量**：只驗「改內容會換 hash」這種相對性質的話，把長度前綴改成 BE、調換
`ENTRY_ORDER`、或改 `RegionEntry` 的欄位順序，測試都會全綠。凍結兩層——`hash::content_hash` 的最小輸入
一組，完整 sample pack 一組。**改到那兩個字面值就是改了 major 契約。**

`ColorPack::open()` **重算 hash 並比對**。既然 hash 取的是未壓縮內容，讀端重算幾乎不用額外成本；而 runtime
拿 `asset_hash` 對應使用者作品，內容與 hash 對不上時繼續讀，等於把錯的東西當成對的存進去。同理 `open()`
也驗 `region_ids` 全部 `< region_count`——超界 ID 會變成 Mode A 永遠點不到的幽靈區。

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
| `difficulty` | `"easy"`（≤60）\| `"medium"`（61–200）\| `"focused"`（>200）。門檻 SSOT 在 `assets-spec` |
| `category` | `anime` \| `mandala` \| `animal` \| `botanical` \| `scenery` \| `cartoon` |

`schema_version` 為 `major.minor`，runtime 拒絕未知 major。M1 起即為 `"1.0"`。

### 3.5 `palette[]` 與 `suggested_color` 的分工

| 欄位 | 語意 |
|---|---|
| `manifest.palette[]` | **去重後**的建議色票清單，給色盤 UI。依總面積遞減排序，平手取較小 region id。不設上限，UI 自行取前 N 個 |
| `regions.json[].suggested_color` | 逐區建議色。同筆記錄的 `bbox` 供 `architecture §4.5` 擴散動畫、`area` 供 `§4.7` 進度計算 |

### 3.6 `regions.bin`

R16 ID map，RLE 無損。ID 範圍 `0..=region_count-1`，`region_count ≤ 65535`。

### 3.7 `thumb.jpg`

母帶解析度下把建議色逐區 × `shade` × `lineart` 合成，再 box 降採樣至**長邊 512**，JPEG quality 85。

Gallery 對鎖定的線稿也顯示縮圖（`prd §5.1`），且 `architecture §8.4` 在資產取不到時以縮圖唯讀顯示——所以
縮圖要呈現「這張畫完會長什麼樣」，不是空線稿。

### 3.8 契約

`contracts/colorpack.schema.json` 是 SSOT。Rust 端 `Manifest` 有一條測試斷言「序列化樣本通過 schema」
（dev-dep `jsonschema`）。

---

## 4. 檢查清冊與報告

### 4.1 檢查清冊

`code` 是固定字彙表——測試斷言的是特定 `code`，繪師收到的退件也照它分類。**新增檢查必須同時進這張表。**

| 檢查項 | `code` | 階段 | severity |
|---|---|---|---|
| 來源目錄缺檔（三張必交 PNG 或 `meta.json`） | `source-incomplete` | Master | Error |
| 四張圖尺寸與對齊一致 | `size-mismatch` | Master | Error |
| 長邊 4096、比例 1:1 或 3:4 | `canvas-size` | Master | Error |
| 色彩描述檔為 sRGB | `color-space` | Master | Error |
| 兩個以上色標落進同一封閉區 → **線稿有缺口** | `seed-collision` | Master | Error |
| ≥`MIN_ORPHAN_AREA` 的自由區沒有色標 → **漏點了** | `orphan-area` | Master | Error |
| 色標的 `alpha==255` 面積不足 | `seed-too-small` | Master | Error |
| 色標重心落在線像素上，flood fill 起不來 | `seed-on-line` | Master | Error |
| 線像素佔比 >`MAX_LINE_RATIO` → 二值化門檻不對或白底交付 | `line-coverage` | Master | Warning |
| `shade` 有 luma < 60 的像素 | `shade-too-dark` | Master | Error |
| `meta.json` 的 `id` 與資料夾名不一致 | `meta-id-mismatch` | Master | Error |
| `meta.json` 的 `category` 非六個允許值 | `meta-bad-category` | Master | Error |
| 降採樣後區域數與母帶不一致 | `region-count-drift` | Output | Error |
| 區域數 > 65535 | `region-count-overflow` | Output | Error |
| 碎片區域（面積 < 200px，**輸出**解析度） | `tiny-region` | Output | Warning |
| 區域數過多或過少 | `region-count-range` | Output | Warning |
| 區域在輸出解析度下斷成多塊 | `region-split` | Output | Warning |

面積門檻分屬不同解析度，是最容易混用的地方：`seed-too-small` 的 64px 與 `orphan-area` 的 500px 在
**母帶**，`tiny-region` 的 200px 在**輸出**（繪師端對應的母帶數字是 800px）。

**撞上 `seed-collision` 時仍然要產出 labels**：先到的 seed 佔住整個封閉區，後到的不指派區域但保留在診斷裡。
這樣 §4.3 的 `preview.png` 與 `seeds-overlay.png` 畫得出來——退件附件比「因為有錯所以什麼都不給你」有用得多。

### 4.2 報告

```rust
struct Diagnostic {
    severity: Severity,      // Error | Warning
    code: &'static str,      // "seed-collision" / "orphan-area" / ...
    stage: Stage,            // Master | Output
    message: String,
    coords: Vec<(u32, u32)>, // 上限 16（聚類後為叢集數上限）
    coord_total: usize,      // 超過上限時的實際總數
    region: Option<u32>,
}
```

- **階段內不 fail-fast**：跑完該階段全部檢查才決定。繪師一次拿到所有問題，來回從 N 天變 1 天。
  **唯二的例外是算不下去的兩條**——`size-mismatch`（逐像素會越界）與 `unique-color-overflow` 的後繼者
  `seed-collision` 判定必須在 grow 後才成立，其餘一律往下跑。
- **階段間 fail-fast**：母帶有 Error 就不進降採樣，後面的結果沒有意義。
- **座標一律換算回母帶座標系**（繪師在 CSP 裡看到的那個）。輸出階段發現的問題 ×2 換算並標註「於輸出解析度
  發現」。
- **座標聚類**：相鄰座標聚成叢集（`CLUSTER_GRID = 128px`），報「3 處，最大一處約 500px，在 (1204,880)」而不是
  16 個散落座標。**色標的四條診斷不聚類**——`seed-collision` 的兩個 anchor 若被聚成一叢，「在這兩點之間補線」
  就沒有意義了。只有逐像素症狀（`shade-too-dark`）與輸出階段的診斷才聚。
- **可疑度排序**：診斷之間按「該先看哪個」排（`SUSPICION`），面積大的 `orphan-area` 優先。
- exit code：`0` 通過（可含警告）／`1` 有 Error ／`2` baker 自身故障。
- 文字輸出與 `--report x.json` 從同一個 `Vec<Diagnostic>` 渲染，不是兩份真相。

### 4.3 `--debug-out <dir>`（退件附件）

四件產物：

| 檔 | 用途 |
|---|---|
| `preview.png` | 依鄰接關係挑高對比配色的區域圖 ＋ 線稿。**洋紅檢查的替代品**：兩塊有沒有融成一塊，一眼就看見 |
| `seeds-overlay.png` | 線稿 ＋ 色標位置 ＋ 診斷標記（collision 紅線連兩點、orphan 黃框） |
| `reference-preview.png` | 用建議色 ＋ 線稿 ＋（有的話）shade 渲染整張。`thumb::render` 已在做，只是現在只進 pack 不落地 |
| `regions.json` | 逐區面積 / bbox / 重心 / 建議色，給人看的 |

**繪師手上不會有 baker**——他們是外包，跑 CLI 的是專案方。所以前三張圖不是 debug 工具，是**退件附件**：
把圖丟回給繪師，比任何文字訊息都有效。

---

## 5. 測試素材

### 5.1 torture 與 synth

**`synth.rs` 改寫為產生 lineart ＋ seeds**：lineart 可指定在座標 P 開一個 N px 缺口、seeds 可指定漏點哪一區、
色標畫多小。**每個診斷碼都有一張剛好踩到它的合成素材。**

| 產物 | 定位 |
|---|---|
| `assets/source/torture-01/` | **合格**壓力素材。所有特徵最短邊 **≥8px**，軸對齊的特徵落在偶數邊界（降採樣後仍 ≥4px）。實測 **4236 區**，`has_shade = false`，3:4 |
| `baker::synth::negative()`（生成器程式碼） | 6 組預期拒收：`seed-collision`（缺口）、`orphan-area`（漏點）、`seed-too-small`（色標太小）、`seed-on-line`（色標壓線）、`line-coverage`（白底交付）、`display-p3` |

**「對齊偶數邊界」只約束軸對齊的特徵。** `zone_pie` 是 `atan2` 放射楔形，邊界本來就不是水平／垂直線，
21 個區域的 bbox 落在奇數座標——但那些區域最窄處 ≈25px，降採樣後遠 >2px。要求放射楔形量化到偶數格會扭曲
它想測的東西（非軸對齊邊界在 majority 下的行為），得不償失。

**區域數是 4236，不是「上萬」。** 這是 `zone_grid` cell=32 在 4 個 zone 上的實際結果。4236 已經遠超
`REGION_COUNT_MAX`（2000）、落在 `focused` 難度，壓力測試的目的達到了。**`torture-01` 有一條 e2e 測試直接
烘它**（用生成器現場產生，不讀 LFS 檔），斷言它**通過**且唯一的診斷是 `region-count-range` 警告——出現
`tiny-region` 或 `region-split` 就代表 ≥8px／偶數對齊的設計沒守住。

**生成器與 committed 產物之間有 drift 守門。** `gen-torture` 順帶寫出 `assets/source/torture-01/synth-lock.json`，
內含圖檔**原始 RGBA** 的正規化 hash；一條測試重新生成並比對。刻意不 hash PNG bytes——那會綁到 `png` crate 的
deflate 實作，換一次依賴版本就誤報；要守的是「改了 `synth.rs` 卻忘了重跑 `gen-torture`」。lock 檔落在
`assets/source/**/*.json`，依 `.gitattributes` 不進 LFS，所以 CI 的 `lfs: false` checkout 也讀得到。

### 5.2 negative fixture 不進 git

fixture 必須是完整 4096 級尺寸（否則會先撞到「長邊 4096」檢查，測不到想測的那條）。6 組 × 4 張進 git 是
幾百 MB。

**放進 repo 的是生成器程式碼，不是 PNG。** 每條測試自己生一組到 tempdir，跑完即丟。repo 零增重，fixture
隨規格演進不會膠死。

生成器住在 **`tools/baker/src/synth.rs`**——negative fixture 與 `torture-01` 共用同一組 zone 原語，而
`torture-01` 要被 `xtask gen-torture` 當 library 呼叫，只有在 `src/` 才做得到。`tests/` 底下的東西 xtask 看不見。

### 5.3 現有素材的修正

- `kirby-demo-1` / `adventure-time-demo-1` 的 `meta.json` 的 `category: "cartoon"` 已隨 `assets-spec` 補列而合法
- 兩支 demo 的 `seeds.png` 是從已廢止的 `reference.png` 反推的，繪師依 v2.0 重交後即為正式素材
- 色彩空間判定不能只看 iCCP 名稱，必須解析 profile 的 colorant 與 white point，或退回 `gAMA` + `cHRM` 判定

---

## 6. 測試層級

| 層 | 內容 |
|---|---|
| 單元 | rle round-trip（proptest）、majority 平手規則、dilate、色彩空間判定（含泛用名稱 iCCP）、`content_hash` 凍結向量（§3.3）、zip 的 mtime／權限／entry 順序 |
| 拒收 | 6 個 negative fixture 各一條測試。斷言 **特定 `code` 出現 ＋ 座標落在生成器刻意植入的位置**——只斷言「失敗」會讓任何理由的失敗都變綠燈 |
| 端到端 | 素材 → pack → 用 colorpack reader 開回來 → 斷言 `region_count` / `difficulty` / `has_shade`，並**連跑兩次比對位元相同**。另有一條專跑 `torture-01`（§5.1） |
| **golden** | 固定素材 → 固定 `region_ids` 位元組 ＋ manifest。`grow`＋`close` 的確定性靠這條守住 |
| 階段 | 一張同時踩到四條互相獨立檢查的素材，斷言報告**四條全含**——這是 §4.2「階段內不 fail-fast」唯一守得住的方式 |

**`display-p3` 是唯一沒有座標斷言的拒收測試**：問題在 PNG chunk 不在像素，沒有座標可報。改斷言「訊息指名
是哪一張圖」＋「只有 `flats` 命中，其餘三張不被連坐」。

**端到端跑的是 `synth` 生成的素材，不是 LFS 的手繪。** CI 以 `lfs: false` checkout，真實素材在 CI 裡只拿得到
pointer；而且把 golden 值綁在手繪素材上，素材一改就要改測試。真實素材的烘焙留給本地的 `cargo xtask bake`。

**`core/colorpack` 的 reader 在 M1 就做**，不留到 E1：round-trip 是驗證 writer 正確最便宜的手段，而 reader
本身只是 zip central directory 解析加 RLE 解碼。

CI：`tools/baker/**` 或 `assets/source/**` 變動 → 跑全部四層。

---

## 7. 退路與不做

**Phase 0 判定：GO。** 以 `adventure-time-demo-1`（3:4，3072×4096）實測：線稿封閉區 83 個，其中 ≥
`MIN_ORPHAN_AREA` 的 62 個佔非線像素 99.94%；62 個封閉區**每一個在 `reference` 裡都是單一平塗色**，
欠分割（`seed-collision` 代理）0/62；`close` 在 66 輪內收斂、剩餘未指派 **0px**——全覆蓋不變式成立。
碎片只佔畫布 0.061%，「併入鄰居」處理得掉。**線稿封閉性完全撐得住。**

兩個發現：

- **線稿比配色細 8.9 倍**（7 個建議色對 62 個封閉區）——代價落在繪師身上：要點 62 個點，不是 7 個點。
  所以 §1.1 那句「一個封閉區一個點」必須是第一句話。好處是這 62 區在 App 端本來就是 62 個可獨立上色的區，
  對著色體驗是加分。
- **裝飾性開放線條不造成問題**（背景草叢短撇、反光線）——不產生額外封閉區，也不觸發 collision。

### 7.1 退路

若色標交付走不通（保留 `flats.png` 交付形式）：改在 region 產生之前加一道量化——面積 ≥`MIN_COLOR_AREA` 的
顏色當錨色，其餘像素 snap 到最近錨色，alpha<255 先合成到不透明。治抗鋸齒與色偏，不治漏填。§4.3 的
`--debug-out` 與座標聚類在兩案下都成立，可獨立先做。

### 7.2 明確不做

- seeds 層的輔助分割線（線稿無分界但想切兩塊）——等真撞到再說
- trapped-ball 多階段半徑自動封補線稿缺口——先讓 `seed-collision` 逼繪師補線；自動封補會掩蓋線稿品質問題
- 骨架抽取 / 筆畫候選路徑
- exit code 語意改動
- runtime 端的 pack 載入與 GPU 上傳、`.colorpack` 上傳 R2 與分發、縮圖的視覺設計細節
