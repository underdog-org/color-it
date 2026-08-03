# E2 Spec 拆分計畫

> 狀態：定稿（2026-08-03）｜里程碑：[E2](../roadmap/E2.md)
>
> **這份不是 spec，是四份 spec 的拆分依據與撰寫契約。** 四份寫完後它仍留著，
> 用途是回答「為什麼是這樣切」與「哪份 spec 擁有哪個欄位」。

## 前提

四項，本輪確認，不是推測。前提改變則本文作廢。

1. **E1 完成並通過 D3。D4 拍板為 A**——mask mode 綁在 `Tool` 上（油漆桶嚴格、
   筆刷與橡皮擦寬鬆，即 `architecture.md §4.4` 現行寫法），`prd.md` 附錄 A6
   的全域「封閉線稿」開關**結案否決**
2. **記憶體峰值未超標**，畫布解析度不動，`T_erase` 的 +4 MB 在 `§4.1.1` 預算內
3. 下列 `E2.md` 條目 **E1 已完成，不列入 E2**：`core/document` 骨架、
   `core/app-state`、`T_erase` 貼圖配置、`Fill` 清 `T_erase`、五支 `BrushPreset`
   的型別骨架、golden fixture 骨架
4. `E2.md` 實作清單依第 3 點修訂（第 18、35、37、56–57 行）

## 為什麼要拆

`E2.md` 表面上 15 條清單、7 條驗收，扣掉前提第 3 點之後範圍如下：

| 組 | 內容 | 觸及 |
|---|---|---|
| 筆刷值 | 硬圓 tip（程序）＋ 顆粒 tip（內嵌 PNG）＋ 四支 preset 的曲線與 jitter 實際數值 ＋ `velocity_to_size`／`tilt_to_size` 從恆 0 啟用 ＋ `opacity` 覆寫接到 commit | `stroke` `render` `engine` |
| commit 三路徑 | Pass 2 (b) bbox ping-pong `T_bg`、(c) 橡皮擦 MRT 雙寫、(d) `edge_boost` unsharp | `render` |
| 縮放平移 | 雙指手勢 → `set_viewport` ＋ size 單位換算 ＋ zoom 下的 dab 尺寸下限 | `apps/ios` `engine` |
| 收束 | mask mode 改讀 `Tool`、移除 `set_mask_mode`／`MaskMode`、A6 結案回寫 | `engine` `render` `apps/ios` |
| 測試 | golden 解 `#[ignore]` 進 CI ＋ 四支 preset 的 fixture | `stroke` CI |

單一 spec 不是長度問題（E2 比 E1 小），是**擁有權問題**：`opacity`、
`edge_boost`、`build_up` 三個欄位都定義在 `BrushPreset` 而生效在 commit shader，
不先裁決歸屬，兩份 spec 會各寫一次。

## 四份

放 `docs/specs/`，前綴 `E2-`。長度預算每份 **200–280 行**——比 E1 的六份短，
因為 E2 沒有新 crate、沒有新 FFI 型別（`Tool.opacity`、`Transform` 在 S0 已定完）。

| 檔 | 涵蓋哪幾組 | crate |
|---|---|---|
| `E2-brush.md` | 筆刷值全組 ＋ golden fixture | `stroke`、`render`（tip）、`engine` |
| `E2-commit.md` | commit 三路徑 ＋ `T_erase` 寫入端 | `render` |
| `E2-viewport.md` | 縮放平移 ＋ 收束 | `apps/ios`、`engine`、`render` |
| `E2-tuning.md` | D5 盲測、水彩時間盒、調校項、決策回寫 | 無（流程文件） |

E1 的四張固定表沿用：涵蓋對照、已否決、驗收、回寫清單。

## 撰寫順序

```
E2-brush ──▶ E2-commit ──▶ E2-viewport ──▶ E2-tuning
```

`commit` 消費 `brush` 定的四個欄位；`viewport` 的 mask 收束要動 `commit` 建好的
mask binding；`tuning` 引用前三份的調校掛鉤。四份在主 thread 依序撰寫，不派
subagent——理由同 E1：型別一致性靠的是同一個 context 記得前一份寫了什麼。

## 型別歸屬

### 三個會被兩份搶的欄位

| 欄位 | 裁決 |
|---|---|
| `opacity` | 語意與五支的預設值歸 `E2-brush`；**乘的時機與 uniform 佈局歸 `E2-commit`**。`preset.rs` 已寫死「`stroke` 不碰它」，此處升為 spec 級約定 |
| `edge_boost` | **定值歸 `E2-brush`**（五支的表，僅水彩非 0）；**unsharp 演算法、blur 半徑隨 size 縮放、R7 退路 ①② 的實作歸 `E2-commit`**；退路 ③（砍成四支）的**判定**歸 `E2-tuning` |
| `build_up` | 值歸 `E2-brush`。Pass 1 的 blend 切換 E1 已定，**E2 不重新定義**，`E2-brush` 只認「噴槍要驗證同筆內 over 真的累積」 |

### 各自獨佔，其餘三份只引用

- **`E2-brush`**：五支 preset 的最終常數表、兩張新 tip 的生成方式、
  `TipId::is_implemented()` 的去留、**jitter 的 RNG 契約**
- **`E2-commit`**：`T_bg` 暫存資源（格式／尺寸策略／生命週期／配置者）、
  Pass 2 的 MRT 附件佈局、四條路徑要開幾個 pipeline
- **`E2-viewport`**：`Transform` 的手勢語意（scale 上下限、平移邊界）、
  size 單位換算的程式碼歸屬、dab 尺寸下限常數

### 已存在，不得重新定義

`BrushPreset` 十四欄位、`Curve`、`TipId`、`Dab`、`generate_dabs` 契約（E1，
`stroke` crate）｜`Tool` `Transform` `UiState` `InputSample`（S0，`engine/ffi.rs`）｜
`Op` `Effect` `Document::apply`（E1，`document`）

## 每份的大綱與必答

「必答」＝ 撰寫時必須給答案並附理由，不得寫 TBD。

### `E2-brush.md`

1. 範圍與非範圍
2. 兩張新 tip：硬圓（程序，smoothstep 邊緣）、顆粒（內嵌 PNG）；`tip_atlas()` 從單 layer 擴到三 layer
3. **五支 preset 的最終常數表**——十四欄逐支填滿，取代目前四支 `..soft_round()` 的佔位
4. `velocity_to_size` / `tilt_to_size` 從恆 0 啟用：速度來源、平滑、與 One-Euro 的關係
5. jitter 三欄啟用 ＋ RNG 契約
6. `opacity` 覆寫的傳遞鏈（欄位側）：`Tool::Brush.opacity` → `engine` → commit
7. golden fixture 擴充：三條軌跡 × 五支 preset，解 `#[ignore]`
8. 已否決／驗收／回寫

**必答 A**｜顆粒 PNG 怎麼產。手繪，還是「程序生成一次後存檔當常數」？後者的好處是
可重現可調，壞處是那就等於程序生成——這題要有立場。

**必答 B**｜**jitter 與 golden 的決定性如何共存。** `generate_dabs(.., seed)` 已吃
seed，但 jitter 恆 0 時 seed 走哪條路沒被測過。啟用後若 RNG 的推進次數隨 dab 數變化，
任何 spacing 改動都會讓三支 jitter 筆的 fixture 全紅——**這正是 E1 把 golden 全標
`#[ignore]` 的原因**，不能在解除 ignore 的同一個里程碑重演。

**必答 C**｜`TipId::is_implemented()` 的 fallback 在 E2 之後留不留。三個變體全實作後
它恆為真，留著是死碼，拿掉則 `dab.rs` 的 fallback 分支一併移除。

### `E2-commit.md`

1. 範圍：四條路徑（(a) 已有，(b)(c)(d) 新增）
2. **`T_bg` 資源**：格式、尺寸策略、生命週期、配置者
3. 路徑 (b) bbox ping-pong：`T_bg = over(palette × (1 - T_erase), T_paint)` 的兩步 pass、與 composite 第 ①②③ 層的一致性
4. 路徑 (c) 橡皮擦 MRT 雙寫：target 0 `T_paint` destination-out、target 1 `T_erase` additive
5. 路徑 (d) `edge_boost` unsharp：`box3x3`、半徑隨 size 縮放、R7 退路 ①② 的切換點
6. pipeline 變體策略：四條路徑要幾個 pipeline、幾個 shader、幾個 bind group layout
7. E1 資源矩陣的增修（`T_bg` 那一列）
8. 記憶體帳增補 → `architecture.md §4.1.1`
9. 已否決／驗收／回寫

**必答 D**｜**`T_bg` 是 per-document 常駐還是 per-stroke 動態配置。**
`architecture.md §4.2` 只說「bbox 而非全畫布」省下 +16 MB，沒說它住在哪。常駐要按
最壞 bbox 開（可能就是全畫布，白省），動態則每筆抬手 alloc 一次（而 commit 正是最
不能卡的時刻）。第三條路是固定上限的 tile 池。這題直接改 `§4.1.1` 的預算表。

**必答 E**｜**MRT 兩個 attachment 格式不同**（`T_paint` RGBA8、`T_erase` R8）、
blend state 也不同（destination-out vs additive），wgpu／Metal 是否支援同 pass 雙寫。

> ✅ **已驗證（2026-08-03）**：`core/render/tests/mrt_probe.rs` 在 headless Metal 上
> 通過——同 pass 兩個 attachment、格式與 blend state 各自獨立，讀回值符合預期。
> `§4.1.2` 的「Draw call 代價：零」成立，`E2-commit` 第 4 節照原路徑寫。
> **保留**：跑的是 macOS 的 Metal backend，iOS 真機在 E2 實作時順帶再驗一次。

### `E2-viewport.md`

1. 範圍
2. 雙指手勢 → `set_viewport`：pinch ＋ pan，與畫筆觸控的共存
3. `Transform` 的邊界：scale 上下限、平移不得把畫布推出畫面
4. **size 單位＝螢幕點**：換算歸 `engine`，`stroke` 只收畫布像素
5. **zoom 下的 dab 尺寸下限**
6. mask mode 收束：改讀 `Tool`、移除 `set_mask_mode`／`MaskMode`／Debug toggle
7. 已否決／驗收／回寫（含 `prd.md` A6 結案）

**必答 F**｜**畫布像素直徑小於 1 時怎麼辦。** `spacing` 是筆尖直徑比，直徑趨近 0 則
dab 間距趨近 0，一筆的 dab 數會爆量到 `MAX_DABS_PER_DRAW` 之外。clamp 直徑、還是給
spacing 一個畫布像素下限——視覺後果不同（前者是「再放大筆也不會更細」，後者是
「筆更細但變成虛線」）。

**必答 G**｜手勢與下筆的仲裁。第二根手指落下時第一根已經在畫，那一筆要 `cancel_stroke`
（`T_paint` 從未被污染）還是保留。

### `E2-tuning.md`

1. **D5 盲測劇本**：外部受試者（不得自評）、**手指**、**畫布必須已用油漆桶鋪底**——
   麥克筆與水彩的 Multiply 在白底上與軟圓筆完全一樣，白底盲測會誤判它們沒有辨識度
2. **水彩的 3 天時間盒**：怎麼計時、R7 三段退路各自的判定標準與切換時機
3. 調校項清單：五支 preset 的曲線、`edge_boost` 定值、jitter 幅度、zoom 上下限
4. `perf-baseline.md` 的 E2 增補：`T_erase` 寫入端 ＋ `T_bg` 之後，記憶體與 frame time 不得較 E1 回歸
5. **水彩結論的記錄處**——保留（附 `edge_boost` 定值）或砍成四支，`E2.md` 驗收第 6 條
   明寫不得懸而未決帶入 S1
6. 決策回寫清單

**必答 H**｜4 倍 zoom 的雙線性驗收（`E2.md` 驗收第 4 條）怎麼量。「無可見 texel 方塊」
是主觀描述，而同一條驗收自己又承認 `T_paint` 的像素化屬預期——**兩者在畫面上長得很
像**。要給一個能區分的判定方法，否則這條驗收既無法通過也無法失敗。

## 已拍板的決定

| 決定 | 內容 | 理由 |
|---|---|---|
| **Mask Mode** | D4 = A。E2 只是把寫死的改成讀 `Tool`，並移除 `set_mask_mode`／`MaskMode`／Debug toggle | `ffi.rs` 的 `MaskMode` 註解已預告「D4 拍板後一起移除」 |
| **tip 來源** | 軟圓／硬圓程序生成；**顆粒是內嵌 PNG**（`include_bytes!` 進 `core/render`），不進 `assets/`／LFS／baker | tip 在 E2 是要反覆調到盲測過的參數，程序生成改一個常數就能重跑；PNG 每調一次都是二進位檔進 git。顆粒是例外，它的價值恰好在「不規則」，而不規則正是參數調不動的。它是程式碼常數，不是文件資產 |
| **size 單位** | **螢幕點**。zoom 下筆跡的螢幕粗細不變，換算歸 `engine` | 本產品的縮放唯一用途是塗細節。畫布空間固定會讓「放大 → 筆太粗 → 調小 → 縮回去 → 又太細」變成每次都要做的兩步 |
| **拆分粒度** | 四份，邊界依型別擁有權而非對稱 | 見「為什麼要拆」 |

## 撰寫約束

E1 的三條沿用，另加兩條治 spec 過期。

- **引用不複製**：指到 `architecture.md §4.2`，不搬原文
- **每份開頭一張表**：涵蓋 `E2.md` 的哪幾條 checklist
- **一節「已否決」**：記下不採用的做法，避免 S1／E3 重提
- ★ **不寫實作細節**：不寫函式簽名、bind group layout、struct 欄位排列。要寫
  「`T_bg` 是 per-stroke 動態配置，理由 X，代價 Y」，不寫它的 `TextureDescriptor`——
  **實作細節寫完的那一刻就開始漂移，而原始碼才是真相**
- ★ **原始碼註解不引用 `E2-*.md`**：只引用 `architecture.md §N`，或直接把理由寫進
  註解本身。E1 的六份在原始碼留下 182 處引用，使得「spec 寫完就該刪」變成一筆債；
  E2 不再製造這筆債

## 合併驗收

- [ ] 四份寫完並 commit
- [ ] 涵蓋對照表的**聯集**蓋滿修訂後的 `E2.md` 實作清單，零遺漏、零重複宣稱
- [ ] `E2.md` 七條驗收各自能指到某份的某一節
- [ ] **八個必答 A–H 全部有答案**，無 TBD
- [ ] `E2.md` 依前提第 3 點修訂（刪去 E1 已完成的條目）

## 建議的實作順序

給實作計畫的輸入，不是本文的產出。

```
① 收束 ＋ 兩張 tip ＋ 四支 preset 填值
     → 五支筆刷畫得出來，可以開始看差異
② commit (b) ping-pong ＋ (c) MRT 雙寫
     → 麥克筆對底色生效、橡皮擦能擦底色。D5 盲測的前提條件到齊
③ 縮放平移
     → 與筆刷正交，卡住不影響 D5
④ 水彩 (d) edge_boost（3 天時間盒）＋ golden 解 ignore 進 CI
     → 參數定案才解 ignore，順序不能顛倒
```

水彩排最後而非最前，因為它是唯一有時間盒與砍掉退路的一支：**其餘四支先做完，
D5 才有「四支已經好用」的底氣去執行 R7 退路 ③**。golden 解 `#[ignore]` 必須在所有
參數定案之後——這是 E1 把它們標 ignore 的原因，提前解除只會再標回去。

## 不在本輪

- 任何 Rust／Swift／WGSL 實作。實作計畫另由 writing-plans 產出
- E1 六份 spec 的回寫表與原始碼 182 處引用的清理（獨立一輪）
