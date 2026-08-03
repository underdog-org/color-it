# baker · 色標交付（seeds）

> 狀態：草案（2026-08-03）｜取代 [assets-spec](./assets-spec.md) §4.2 §4.3 §5 §6、[baker-core-design](./baker-core-design.md) §2.1 §2.3 §2.4
>
> `.colorpack` 格式、App 端、`check/output.rs`、`resample` / `dilate` / `thumb` 一律不動。

## 1. 為什麼改

現行契約要求繪師手工產出一張**像素級精確的量化圖**（`flats.png`）：抗鋸齒必須關閉、整張不透明、每個顏色總面積 ≥100px、相鄰區不得同色，交付前還要整層鋪洋紅、放大到 100% 掃過 4096×4096 找縫隙。

實測下來這條路不可靠，而且失敗模式**繪師無法從診斷訊息推回動作**：

| 繪師的體感 | baker 實際命中的檢查 |
|---|---|
| 「我明明填滿了」 | 邊緣抗鋸齒帶 alpha<255 → `unassigned-pixel`；過渡色各自成區 → `tiny-color-area` 噴上千條；過渡色破 1024 → `unique-color-overflow` |
| 「小地方沒塗到」 | 線稿交叉處的縫，100% 檢視下肉眼不可見 → `unassigned-pixel` |
| 「顏色飽和度不對」 | 非 sRGB → `color-space`；P3→sRGB 轉換讓同一純色裂成差 1 的鄰居 → 多出一區，面積不足再噴 `tiny-color-area` |

根因是分工位置錯了：PS / CSP 的填充工具**本來就靠線稿封閉區**在算填色範圍，現行契約要求繪師把工具算出來的結果再手工修成完美。這一步應該在 baker 裡做。

## 2. 新的交付契約

```
<id>/
  lineart.png     線稿（規格完全不變，抗鋸齒照開）
  seeds.png       ★ 新
  shade.png       選配（不變）
  meta.json       不變
```

**刪除 `flats.png` 與 `reference.png`。** `assets-spec §6.1` 洋紅縫隙檢查整條刪除。

### 2.1 `seeds.png`

| 項目 | 要求 |
|---|---|
| 格式 | RGBA，尺寸同 `lineart` |
| 背景 | 透明 |
| 內容 | 每個要獨立上色的區域裡點**一個**色點 |
| 色點 | 形狀不限、**抗鋸齒開著沒關係**、直徑約 ≥16px |
| 顏色 | **就是建議色**——希望 App 建議使用者塗的那個顏色 |
| 相鄰同色 | **允許**。區域由線稿決定，不由顏色決定 |
| 色彩描述檔 | sRGB。只影響建議色是否精準，不影響能否通過 |

繪師端的規則濃縮成一句：**每一塊你希望能單獨上色的地方，點一個點，顏色就是你的建議色。**

CSP 流程從七步變四步：新建畫布 → 畫線稿導出 → 新圖層點色標導出 →（選配 shade）。沒有工具屬性面板設定、沒有不透明度陷阱、沒有縫隙自檢。

### 2.2 色點的讀法

`alpha > 0` 的像素做 4-連通 → 每個連通塊是一個 seed；取該塊內 `alpha == 255` 像素的**眾數色**當建議色。

抗鋸齒邊緣的過渡色永遠是少數，取不到眾數——**這就是抗鋸齒痛點消失的機制**。`alpha == 255` 面積不足 `MIN_SEED_AREA` → `seed-too-small`。

### 2.3 色點顏色一色兩用

色點的顏色同時擔任「識別」與「建議配色」。`reference.png` 因此整張消失。

代價是繪師無法在自己的工具裡看見整體配色效果——`--debug-out` 的 `reference-preview.png`（§5）把這個能力還給他。

### 2.4 三條舊限制的下場

- 「不得有任何顏色總面積 <100px」→ 刪
- 「整張不透明、每個像素都要有顏色」→ 刪
- 「相鄰區不可同色」→ 刪。`assets-spec §4.2 ④`「臉和脖子相連又同色會被合併」的坑不再存在
- `#FF00FF` 保留色 → 刪。洋紅檢查沒了，這個顏色可自由使用

## 3. 管線

```
source::load          lineart + seeds + [shade] + meta.json
      ↓
母帶檢查              geometry（尺寸一致 / canvas-size）＋ color_space
      ↓
binarize              lineart.alpha ≥ line_threshold → line mask     ← 新模組
      ↓
seeds::read           alpha>0 連通分量 → (重心, 眾數色)[]            ← 取代 reference.rs
      ↓
segment::grow         逐 seed 在 !line 上 4-連通 flood fill          ← 取代 label_regions
      ↓
segment::close        測地擴張把 id 填進線像素，直到全覆蓋           ← 新
      ↓
母帶檢查              seed-collision / orphan-area
      ↓
thumb → resample → dilate → check::output → 打包                     ← 全部不動
```

改動全部集中在母帶階段。

### 3.1 四個設計決定

**① `grow` 用逐 seed flood fill，不用多源同步 BFS。**
線稿封閉時兩者等價。不封閉時逐 seed 能明確報「seed A 撞進 seed B 已佔的區域」，同步 BFS 只會在中間切一條任意分界線然後靜默通過。要診斷，不要吞掉。

**② `close` 用距離排序的測地擴張。**
`region_ids` 必須全覆蓋（App 端不容許沒有 id 的像素）。BFS 波前逐輪擴張，未指派像素從已標記鄰居取 id，等距時取**較小 id** → 確定性，且分界線自然落在線的中軸。

這一步在**母帶**做完，所以 `dilate_under_lineart` 的職責不變：它仍然只負責修降採樣造成的縫。

**③ region id 依 seed 重心的 raster order 編號。**
確定性，與繪師點色標的先後無關。

**④ 小碎片併入鄰居，不報錯。**
`grow` 之後未被認領的自由區塊，面積 < `MIN_ORPHAN_AREA` 的併進面積最大的相鄰區；≥ 門檻才報 `orphan-area`。這是「繪師漏點一塊」與「線稿有個 3px 封閉小洞」的分野。

### 3.2 模組帳

| 動作 | 對象 |
|---|---|
| 新增 | `binarize.rs`、`seeds.rs`、`segment::grow`、`segment::close` |
| 刪除 | `reference.rs::read`、`check::master::flats`、`segment::label_regions`、`color_histogram`、`MAX_UNIQUE_COLORS`、`MIN_COLOR_AREA`、`RESERVED_COLOR` |
| 搬移 | `reference::palette` → `seeds.rs`（實作不動） |
| 不動 | `segment::count_components_per_id`、`check/output.rs` 全部、`resample`、`dilate`、`thumb`、`compose`、`image`、`report` |

`synth.rs` 要改寫成產生 lineart + seeds，**是本次最大的單一工作量**（§6）。

### 3.3 參數

| 參數 | 預設 | 作用 |
|---|---|---|
| `line_threshold` | 128 | `lineart.alpha ≥` 此值視為線 |
| `MIN_SEED_AREA` | 64 | 色點的 `alpha==255` 面積下限（母帶） |
| `MIN_ORPHAN_AREA` | 500 | 未認領自由區的報錯門檻（母帶） |
| `MAX_LINE_RATIO` | 0.35 | 線像素佔比超過此值發 `line-coverage` 警告 |

四個都可 `--set` 覆寫，且**納入 `content_hash`**。預設值視為契約的一部分——真要調參數就等於改契約，全量重烘是應該的。

## 4. 診斷

### 4.1 新增

| 碼 | 嚴重度 | 意義 |
|---|---|---|
| `seed-collision` | Error | 兩個以上色標落進同一封閉區 → **線稿有缺口**。附兩點座標，訊息是「請在它們之間補線」 |
| `orphan-area` | Error | ≥`MIN_ORPHAN_AREA` 的自由區沒有色標 → **漏點了**。附座標與面積，按面積遞減排序 |
| `seed-too-small` | Error | 色標的 `alpha==255` 面積不足，取不出可靠眾數色 |
| `seed-on-line` | Error | 色標重心落在線像素上，flood fill 起不來 |
| `line-coverage` | Warning | 線像素佔比 >`MAX_LINE_RATIO` → 二值化門檻不對，或線稿是白底交付的 |

**撞上 `seed-collision` 時仍然要產出 labels**：先到的 seed 佔住整個封閉區，後到的不指派區域但保留在診斷裡。這樣 §5 的 `preview.png` 與 `seeds-overlay.png` 畫得出來——退件附件比「因為有錯所以什麼都不給你」有用得多。

五條新碼**每一條都對應一個繪師能直接做的動作**：補一筆線、補一個點、把點畫大一點、把點移開線。

### 4.2 刪除

`unique-color-overflow`、`tiny-color-area`、`reserved-color`、`ref-mismatch`、`unassigned-pixel`。

### 4.3 保留不動

`source-incomplete`（檔案清單隨 §2 更新）、`size-mismatch`、`canvas-size`、`color-space`、`shade-too-dark`、`meta-id-mismatch`、`meta-bad-category`，以及 `check/output.rs` 的五條全部。

### 4.4 階段內不 fail-fast

`baker-core-design §4.2` 照舊，而且比現在更重要：`seed-collision` 與 `orphan-area` 必須一次全報，否則繪師補一條線交一次、補一個點又交一次。exit code 0/1/2 語意不變。

## 5. `--debug-out <dir>`

四件產物：

| 檔 | 用途 |
|---|---|
| `preview.png` | 隨機高對比配色的區域圖 ＋ 線稿。**`§6.1` 洋紅檢查的替代品**：兩塊有沒有融成一塊，一眼就看見 |
| `seeds-overlay.png` | 線稿 ＋ 色標位置 ＋ 診斷標記（collision 紅線連兩點、orphan 黃框） |
| `reference-preview.png` | 用建議色 ＋ 線稿 ＋（有的話）shade 渲染整張。`thumb::render` 已在做，只是現在只進 pack 不落地 |
| `regions.json` | 逐區面積 / bbox / 重心 / 建議色，給人看的 |

**繪師手上不會有 baker**——他們是外包，跑 CLI 的是專案方。所以前三張圖不是 debug 工具，是**退件附件**：把圖丟回給繪師，比任何文字訊息都有效。它們的畫法以「給繪師看」為準，不是以「給我 debug」為準。

診斷報告本身另改兩件事：

- **座標聚類**：相鄰座標聚成叢集，報「3 處，最大一處約 500px，在 (1204,880)」而不是 16 個散落座標
- **可疑度排序**：診斷之間按「該先看哪個」排，面積大的 `orphan-area` 優先

## 6. 測試

**`synth.rs` 改寫**：新合成器要能產生 lineart（可指定在座標 P 開一個 N px 缺口）＋ seeds（可指定漏點哪一區、色標畫多小）。每個新診斷碼都有一張剛好踩到它的合成素材。

三張新的網：

1. **Golden test（最重要）**。現行 `label_regions` 是精確色比對，確定性是白送的；`grow` + `close` 的確定性要靠測試守住。固定素材 → 固定 `region_ids` 位元組 ＋ manifest。
2. **階段內不 fail-fast 迴歸**。照現有 `an_early_failure_does_not_hide_the_independent_checks` 的形式改寫：一張同時有 `seed-collision` + `orphan-area` + `seed-too-small` + `canvas-size` 的素材，四條必須全報。
3. **差分測試**。從 `adventure-time-demo-1/flats.png` 每區取重心自動生成 seeds，跑本案，比對區域數與現行結果。

## 7. 實施順序

**Phase 0 — 可行性驗證（先做）**
用現有 demo 素材：從 `flats.png` 自動生成 seeds → prototype 的 binarize + flood fill → 數區域數、數 collision。

**這一步的結果決定本案能不能走。** 若 `adventure-time-demo-1` 的線稿封閉性撐不住（大量 collision，且缺口是風格而非疏忽），退回退路方案（§8）。這是整個計畫唯一的不可逆風險點。

**Phase 1** `binarize` + `seeds` + `grow` + `close` + 新診斷碼
**Phase 2** `synth.rs` 改寫 + 三張測試網
**Phase 3** `--debug-out` 四件產物 + 座標聚類 + 可疑度排序
**Phase 4** `assets-spec.md` 重寫（§4.2 §4.3 §5 §6 大改），產出可直接附進繪師 JD 的版本

### Phase 0 實測結果（2026-08-03，`adventure-time-demo-1`）

**改用 `reference.png` 而非 `flats.png` 當真值。** `meta.json` 記載這張 `flats.png` 不合規：171 色、其中 166 色總面積 <200px，且 7 個區域跨線稿合併、合計佔畫布 57.66%。從它反推的色標，collision 反映的會是 flats 壞掉而不是線稿不封閉——而 Phase 0 要決定的偏偏是後者。同一份 `meta.json` 記載 `reference.png` 的幾何 99.95% 對齊線稿，所以改量兩件事：

- **A 線稿封閉區普查**：只看線稿，數非線像素的 4-連通塊。無假陽性。
- **B 欠分割交叉檢查**：每個封閉區內 `reference` 的顏色是不是雙峰（第二眾數佔 ≥20% 且 RGB 距離 ≥60）。雙峰 = 繪師本想分兩塊但線稿沒隔開 = **真實工作流下的 `seed-collision`**。

| 指標 | 數值 |
|---|---|
| 母帶尺寸 | 3072×4096（12.6M px） |
| 線像素佔比 | 4.98%（遠低於 `MAX_LINE_RATIO` 0.35） |
| 封閉區總數 | 83 |
| ≥`MIN_ORPHAN_AREA` 的封閉區 | 62 個，佔非線像素 99.94% |
| <`MIN_ORPHAN_AREA` 的碎片 | 21 個，合計 7733 px（0.061% 畫布） |
| **欠分割（collision 代理）** | **0 / 62（0.0%）** |
| 建議色種類 | 7 |
| `close` 輪數 / 剩餘未指派 | 66 / **0** |

**判定：GO。**

線稿封閉性完全撐得住：62 個封閉區**每一個在 `reference` 裡都是單一平塗色**（第一眾數皆 ≈100%），沒有任何一個封閉區橫跨兩片意圖不同的顏色。`close` 在 66 輪內收斂、剩餘 0 px，全覆蓋不變式成立。碎片只佔畫布 0.061%，§3.1 ④ 的「併入鄰居」處理得掉。

### Phase 0 的兩個發現

**① 線稿比配色細 8.9 倍——這是本案真正的成本，不是風險。**

7 個建議色對上 62 個封閉區。白色一色就橫跨 27 個封閉區（Finn 的帽子與身體、雲、Jake 的眼睛與口鼻、以及**被交叉線切成約 8 條的劍身**）。

代價落在繪師身上：**要點 62 個點，不是 7 個點。** 繪師若照直覺「一個顏色點一個」，會有 55 個封閉區變成 `orphan-area`，一次退件噴 55 條。

這件事必須寫死在 §2.1 與 Phase 4 的 JD 裡，而且要是第一句話：

> **一個封閉區一個點——不是一個顏色一個點。** 線稿把一片顏色切成幾塊，就要點幾個點。

好處是這 62 區在 App 端本來就是 62 個可獨立上色的區，比 A 案的 flats 更細，對著色體驗是加分。`--debug-out` 的 `preview.png`（§5）是繪師確認「我有沒有漏點」的唯一實際手段，優先度因此比原本估的高。

**② 裝飾性開放線條不造成問題。** 背景的草叢短撇、劍身的反光線都是不封閉的開放筆畫，不產生額外封閉區，也沒觸發任何 collision。§9「不做 trapped-ball 自動封補」的判斷在這個畫風下成立。

## 8. 退路

若 Phase 0 否決本案：保留 `flats.png` 交付形式，改在 `label_regions` 之前加一道量化——面積 ≥`MIN_COLOR_AREA` 的顏色當錨色，其餘像素 snap 到最近錨色，alpha<255 先合成到不透明。治抗鋸齒與色偏，不治漏填。§5 的 `--debug-out` 與座標聚類在兩案下都成立，可獨立先做。

## 9. 明確不做

- seeds 層的輔助分割線（線稿無分界但想切兩塊）——等真撞到再說
- trapped-ball 多階段半徑自動封補線稿缺口——先讓 `seed-collision` 逼繪師補線；自動封補會掩蓋線稿品質問題
- 骨架抽取 / 筆畫候選路徑
- exit code 語意改動
