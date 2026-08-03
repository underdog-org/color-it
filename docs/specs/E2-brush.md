# E2 · 筆刷值

> 狀態：定稿（2026-08-03）｜里程碑：[E2](../roadmap/E2.md)｜計畫：[E2-spec-plan](./E2-spec-plan.md)
>
> 四份的第一份。`opacity` 的乘法時機、`edge_boost` 的演算法、`build_up` 的 blend 切換
> 都**不在**本文，見 [E2-commit](./E2-commit.md)。

## 涵蓋 `E2.md` 的哪幾條

| `E2.md` 實作清單 | 本文 |
|---|---|
| 四張 tip 貼圖：軟圓、硬圓、顆粒紋理、大軟圓 | §2（**實際是三張**，見 §7 回寫） |
| 五個 `BrushPreset` 常數（含 `edge_boost`，非水彩皆為 0） | §3 |
| 麥克筆（硬圓 ＋ Multiply）與蠟筆（顆粒紋理 ＋ Normal） | §3.2 §3.3 |
| 噴槍（`build_up = true`，同筆內 over 累積） | §3.4 |
| 筆刷透明度可由使用者調整 | §5（欄位側；乘法時機在 `E2-commit`） |
| `stroke` golden test：固定 `seed`、jitter 可重現 | §4 §6 |
| golden test 進 CI | §6 |

不涵蓋：commit 三路徑（`E2-commit`）、縮放平移與 mask 收束（`E2-viewport`）、
D5 盲測劇本與水彩去留的判定（`E2-tuning`）。

---

## 1. 範圍

**E2 只填值、補兩張 tip、啟用三組已存在但恆 0 的欄位。** 型別一個都不新增：
`BrushPreset` 十四欄、`Curve`、`TipId`、`Dab`、`generate_dabs` 的契約都在 E1 定完，
本文不得重新定義。

`stroke` 仍然純 CPU、零 GPU 依賴。tip 的生成住 `render`，因為它產的是貼圖。

---

## 2. 三張 tip

`TipId` 三個變體＝ `texture_2d_array` 的三層，E1 已把 `TIP_LAYERS` 開到 3 但只填 layer 0。
E2 填滿另外兩層，**bind group layout 不動**。

三張都**程序生成**，都不進 `assets/`、不進 LFS、不進 baker、不進 app bundle。
它們是程式碼常數，不是文件資產。

| layer | tip | 生成方式 |
|---|---|---|
| 0 | 軟圓 | 徑向衰減（E1 已有） |
| 1 | 硬圓 | 同一條徑向路徑，邊緣換成窄過渡的 `smoothstep` |
| 2 | 顆粒 | value noise × 徑向遮罩 |

### 2.1 顆粒為什麼也是程序生成

**這一條推翻 `E2-spec-plan`「已拍板」表的 tip 來源列**（原定內嵌 PNG）。

原理由是「顆粒的價值恰好在不規則，而不規則正是參數調不動的」。這對 noise 不成立：
noise 的不規則性由 seed、頻率、對比三個常數完全控制，改一個常數就重跑——**正是同一張表
給軟圓與硬圓的理由**。而顆粒是五支裡最需要反覆調的一張 tip（蠟筆的辨識度幾乎全押在它
身上），PNG 讓每一次調校都是一份二進位進 git，再加一道「生成器與產物是否同步」的 CI 題。

徑向遮罩不是裝飾：少了它，方形的 noise 貼圖會讓每個 dab 都是方塊。

### 2.2 `is_implemented()` 移除

三個變體全實作後 `TipId::is_implemented()` 恆為真，`render/dab.rs` 的 fallback 分支
永遠走不到。**兩者一併移除。**

留著的壞處不只是死碼：日後新增第四張 tip 卻忘了填 atlas 時，它看起來像有保護、實際上
是靜默 fallback 成軟圓——那是最難查的那種畫面錯誤。移掉之後缺層就是明顯的空白筆跡。

---

## 3. 五支的差異軸

**本節不列十四欄的數值表。** 數值的唯一真相是 `core/stroke/src/preset.rs`；D5 盲測
必然會調動其中一半，一份寫死七十個數字的 spec 在里程碑結束前就已過期
（`E2-spec-plan`「撰寫約束」★）。

spec 擁有的是**方向**：每支靠哪幾欄與另外四支區分、為什麼是這幾欄。D5 調的是幅度；
若方向被推翻，那才是要回寫本節的事。

下表四欄是定性的，D5 調不到：

| Preset | tip | blend | build_up | 主要差異軸 |
|---|---|---|---|---|
| 軟圓筆 | 軟圓 | Normal | false | —（它是另外四支的對照） |
| 麥克筆 | 硬圓 | **Multiply** | false | 硬邊 ＋ 疊色變深 |
| 蠟筆 | **顆粒** | Normal | false | 顆粒 ＋ jitter 打散 ＋ 大 spacing 留白 |
| 噴槍 | 軟圓 | Normal | **true** | 同筆內累積 |
| 水彩 | 軟圓 | **Multiply** | **true** | **`edge_boost > 0`** |

### 3.1 軟圓筆

基準。壓感同時驅動 size 與 opacity，無 jitter，`velocity_to_size = 0`。
它要乾淨——四支的可辨性都是相對它量的。

### 3.2 麥克筆

兩軸：**硬圓 tip** ＋ **Multiply**。

`pressure_to_opacity` 的區間要**窄**、接近恆定：真實麥克筆的墨水濃度均勻，壓感該走
size 而不是濃度。`opacity` 高、接近不透明。`spacing` 小，否則硬邊在轉彎處會露出扇貝狀鋸齒。
`velocity_to_size = 0`——快畫的麥克筆不會變細。

**已知風險**（`architecture.md §4.6`）：Multiply 白色 ＝ 原色，所以在未上色的白底上麥克筆與
軟圓筆看起來完全一樣。D5 必須在鋪底畫布上進行，判定歸 `E2-tuning`。

### 3.3 蠟筆

一軸主導：**顆粒 tip**。但單靠貼圖不夠——同一張 noise 沿路徑重複貼會看出規律條紋，
那看起來像貼圖 bug 而不是蠟筆。

所以 **`jitter_angle` 與 `jitter_pos` 是必要的，不是裝飾**：它們把每個 dab 的顆粒轉開、
錯開，讓重複消失。`spacing` 大，讓 dab 之間留白——「蠟筆沒塗滿」的觀感來自這裡，
不是來自貼圖本身。`flow` 低於 1，單 dab 不飽和。

`jitter_size` 可為 0：size 的抖動在留白已經夠明顯時只會讓邊界變髒。

### 3.4 噴槍

一軸，而且是**行為軸**而非外觀軸：**`build_up = true`**。

其餘四支走 Max blend，同一處來回塗濃度不變（E1 已驗證）；噴槍走 over，停留越久、
來回越多次越濃。盲測時受試者只要畫個圈就會發現——這是五支裡最容易辨識的一支，
也因此它不需要獨特的 tip。

配合：`spacing` 極小、`flow` 很低（單 dab 極淡，濃度靠累積）。tip 是軟圓，
**「大軟圓」＝同一張 tip 由 `Tool::Brush.size` 放大，不是第四個 `TipId`**。

Pass 1 的 blend 切換 E1 已定，**本文不重新定義**。E2 只認一條驗證：
**同一筆之內 over 真的累積**（§8 驗收）。

### 3.5 水彩

兩軸與人共享（Multiply 給了麥克筆、build_up 給了噴槍），**獨佔軸只有一個：
`edge_boost > 0`**——commit 時的 unsharp 讓筆跡邊緣加深。

這正是它風險最高的原因：拿掉 `edge_boost`，水彩就是「Multiply 的噴槍」，盲測必然
混淆。`E2.md` 的 3 天時間盒與 `architecture.md §14 R7` 的退路都是為這一條而設。

**本文只擁有 `edge_boost` 的定值**（五支裡僅它非 0）。unsharp 演算法、blur 半徑隨
size 縮放、R7 退路 ①② 的實作歸 `E2-commit`；砍成四支的判定歸 `E2-tuning`。

---

## 4. `velocity_to_size` 啟用，`tilt_to_size` 不做

### 4.1 tilt 不做

`InputSample.tilt` 只有 Apple Pencil 有值，手指恆為 0，而 roadmap 明列「Pencil 進階」
是 v1 不做。五支的 `tilt_to_size` 全為 0。**E2 不實作這條路徑**——寫一條沒有輸入來源的
耦合，只會多一組永遠測不到的分支。欄位保留，理由記在 §7 已否決。

### 4.2 velocity 的三個決定

只有**噴槍與水彩**非 0（快掃留下較細的筆觸，這兩支的真實對應物都有這個性質）。
軟圓筆要乾淨、麥克筆墨水均勻、蠟筆的稀疏該由 `spacing` 與 jitter 表達——都是 0。

**語意方向**：速度越快，`size` 越小。欄位值是耦合強度。

**per-dab 速度從哪來**：`Dab` 是沿弧長走出來的，沒有自己的時間戳。速度在控制點之間
內插，**與 `pressure` 走同一條路**——來源是 One-Euro **濾過後**的位置與樣本時間。
用濾波前的原始位置會讓手指的高頻抖動直接變成粗細抖動。

**怎麼正規化到 `[0, 1]`**：用**固定參考速度常數**，不用 per-stroke running baseline。
`majorRadius` 需要 baseline 是因為手指粗細因人而異；速度沒有這個問題，而 baseline 會讓
一筆從頭到尾都慢的筆畫也跑滿整個粗細範圍——慢筆就該全程粗。參考速度是實機調校項，
登記進 `E2-tuning`。

---

## 5. `opacity` 覆寫的欄位側

`Tool::Brush.opacity` → `engine` → commit。E1 已接通（`engine` 在起筆時取覆寫值，
`None` 時退回 `preset.opacity`）。

本節只把一條既有的程式碼約定升為 spec 級：**`stroke` 不碰 `opacity`。**
它是整筆的上限，在 Pass 2 commit 時才乘，不進 `Dab`。乘的時機與 uniform 佈局歸
`E2-commit`。

`preset.opacity` 是**預設值**，不是上限的上限——使用者調高時它就該被蓋過去。

---

## 6. RNG 契約與測試

### 6.1 jitter 與 golden 的決定性如何共存

`E2-spec-plan` 必答 B 擔心「jitter 啟用後 RNG 的推進次數隨參數變化，任何 spacing 改動
都會讓 fixture 全紅」。**E1 的實作已經避開了這件事**：`emit()` 無條件抽四個值，
jitter 參數為 0 也照抽。所以從 0 啟用不改變串流節奏——啟用前後，第 k 個 dab 拿到的是
同一組亂數，只是乘上非 0 幅度。

剩下的風險是排序問題，不是設計問題：改參數本來就會改 dab 的位置與數量，golden 本來
就要重產。**E1 把 golden 標 `#[ignore]` 的理由是「參數還在調」，不是「jitter 不決定性」。**

### 6.2 契約（spec 級，不得更動）

- 一筆一個 `seed`
- 每個 dab **固定抽四個值**，順序 `pos.x` → `pos.y` → `size` → `angle`
- **參數為 0 也照抽**。抽或不抽若取決於參數，調一次 jitter 就會連帶改變其後每個 dab 的隨機序列
- **jitter 是後製擾動，不回饋進取樣**：三個 jitter 欄位不影響 dab 的數量、弧長間距、
  以及擾動前的位置。它們只改寫已經決定好的 `Dab` 欄位

### 6.3 現在就進 CI 的 property test

這組**與參數值無關**，所以不必等 D5，也不會因調校而變紅：

- 同 `seed` 兩次執行逐位元相同；不同 `seed` 對有 jitter 的 preset 必不同
- **jitter 三欄全 0 時，輸出與啟用 jitter 前逐位元相同**（守住 6.2 第三條）
- **改變 jitter 三欄的值，不改變 dab 數量與擾動前位置**（守住 6.2 第四條）
- `velocity_to_size = 0` 的三支，啟用 velocity 前後輸出逐位元相同

### 6.4 golden 擴充與解 ignore

**3 條軌跡 × 5 支 preset ＝ 15 個 fixture**（現為 3 條 × 軟圓 1 支）。fixture 記下
preset 名，失敗訊息才指得出是哪一支。重產是一條 `UPDATE_GOLDEN=1` 命令，JSON 體積小。

**解 `#[ignore]` 排在所有參數定案之後**，即 `E2-spec-plan`「建議的實作順序」的 ④。
提前解除只會再標回去——那正是 E1 留下這筆待辦的原因。

---

## 7. 已否決

| 做法 | 理由 |
|---|---|
| 顆粒 tip 用內嵌 PNG | 推翻 `E2-spec-plan` 拍板：noise 的不規則性由三個常數控制，可調可重現；PNG 讓每次調校都是二進位進 git（§2.1） |
| 顆粒 tip 手繪 | 質感上限最高但不可調，D5 不過就只能重畫。單人開發，顆粒又是風險第二高的一支 |
| 「大軟圓」當第四個 `TipId` | 它是同一張軟圓 tip 由 `size` 放大（§3.4） |
| 保留 `is_implemented()` 的 fallback | 缺層應該明顯失敗，不該靜默退回軟圓（§2.2） |
| E2 實作 `tilt_to_size` | v1 不做 Pencil 進階，手指 tilt 恆 0，等於一條沒有輸入的路徑（§4.1） |
| jitter 改用位置雜湊 | 同一點疊畫兩筆會拿到相同擾動，顆粒紋理出現重複；量化格點還會帶來空間相關性（§6.1） |
| velocity 用 per-stroke baseline 正規化 | 全程慢的筆畫會跑滿粗細範圍，但慢筆就該全程粗（§4.2） |
| golden 對 jitter 筆只比統計量 | 會失去 `E2.md` 驗收第 3 條「刻意改參數會失敗」的強度（§6.4） |
| spec 寫死五支十四欄的數值表 | D5 必然調動其中一半，寫完即過期（§3） |

---

## 8. 驗收

- [ ] tip atlas 三層都有內容；移除 fallback 後刻意缺一層會產生明顯的空白筆跡
- [ ] 五支的差異軸**都已實作**（可辨性的盲測判定歸 `E2-tuning` D5）
- [ ] **噴槍同一筆內來回塗會變深，其餘四支不會**（§3.4）
- [ ] 蠟筆的長直筆畫看不出貼圖重複的規律條紋（§3.3 的 jitter 生效）
- [ ] §6.3 的 property test 在 CI 綠燈——**不需等參數定案**
- [ ] 15 條 golden 在參數定案後解 `#[ignore]` 並在 CI 綠燈
- [ ] 刻意改動任一支的任一欄，至少一條 golden 變紅
- [ ] `stroke` 全測在無 GPU 環境通過（Boundary 2 未被 tip 生成污染）

---

## 9. 要回寫的既有文件

| 文件 | 改什麼 |
|---|---|
| `E2-spec-plan.md`「已拍板」表 | tip 來源列：顆粒改為**程序生成**，理由換成 §2.1 |
| `E2-spec-plan.md` 必答 A | 標記已答，且**推翻原拍板**（§2.1） |
| `E2-spec-plan.md` 必答 B | 標記已答：`emit()` 恆抽四值已解掉漂移，剩下的是排序約束（§6.1） |
| `E2-spec-plan.md` 必答 C | 標記已答：移除（§2.2） |
| `E2.md` 實作清單 | 「四張 tip 貼圖」→ **三張**（軟圓、硬圓、顆粒）。大軟圓是軟圓放大 |
| `architecture.md §4.6` | 同上，`tip` 欄的「軟圓 / 硬圓 / 顆粒 / 蠟筆紋」四種與 `TipId` 的三個變體不符 |
| `architecture.md §4.6` | D5 之後補四支的實際值——**登記在此，執行歸 `E2-tuning`** |
| `docs/README.md` | 加本文一筆（里程碑期間有效，收尾後刪除） |

`velocity_to_size` 的參考速度常數、`edge_boost` 定值、jitter 幅度都是實機調校項，
一併登記進 `E2-tuning` 的調校清單。
