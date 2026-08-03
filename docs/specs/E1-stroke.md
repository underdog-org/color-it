# E1 · Stroke 管線

> 狀態：草案（2026-08-03）｜里程碑：[E1](../roadmap/E1.md)｜計畫：[E1-spec-plan](./E1-spec-plan.md)
>
> 資源與 pass ↔ 資源矩陣見 [E1-wgpu](./E1-wgpu.md)；色彩空間見 [E1-composite](./E1-composite.md) §2。
>
> **實作進度**：§3–§6、§10 的 `core/stroke` 已落地；§7–§9 的 Pass 1／Pass 2 尚未動工。
> 動工前先讀 **§14 執行期決議**。

## 涵蓋 `E1.md` 的哪幾條

| `E1.md` 實作清單 | 本文 |
|---|---|
| `core/stroke`：`generate_dabs` 純函式，零 GPU 依賴 | §3 §6 |
| One-Euro filter → Catmull-Rom → 依 `spacing` 取樣 → `Dab` | §4 |
| Pass 1 Stroke：instanced quad → `T_wet`，scissor 至 bbox；`build_up` 切 blend | §7 |
| Pass 2 Commit（僅路徑 (a)） | §8 |

不涵蓋：iOS 端的取樣與 `majorRadius` 讀取（`E1-input`）、`T_erase`／橡皮擦（E2）。

---

## 1. 範圍

**型別寫完整，只實作 `architecture.md §4.2` 的路徑 (a) ＋ `build_up` 切 blend。**
`BrushPreset` 十四個欄位全部定義，五支 preset 的表全部列出，但 E1 只有軟圓筆走得通。

`stroke` crate **純 CPU、零 GPU 依賴**（Boundary 2）。它不知道 `T_wet` 存在。

---

## 2. 三個型別的歸屬

```
FFI InputSample (扁平)  ──engine──▶  StrokeBuilder  ──▶  Vec<Dab>  ──▶  render Pass 1
   x, y, tilt_x, tilt_y                (stroke crate)
                                            │
                              end_stroke ────┴──▶ Op::BrushStroke ──▶ document.apply
```

| 誰 | 做什麼 |
|---|---|
| `engine` | 扁平 → `Vec2` 的機械轉換。**只有這件事**，不做平滑、不做正規化 |
| `stroke::StrokeBuilder` | 有狀態：One-Euro 濾波器、radius baseline、spacing 累積量、已收樣本 |
| `stroke::generate_dabs` | 無狀態純函式（`architecture.md §5.2`）。golden test 與 E3 的 oplog 重播用 |
| `document` | 只在 `end_stroke` 收到一個 `Op::BrushStroke` |

**進行中的筆畫不經過 `document`。** 鐵律 #3 管的是狀態變更，而 `T_wet` 依定義不是持久狀態
——取消一筆只是清掉它，`T_paint` 從未被污染（`§4.3` #4）。`document.apply` 只在抬筆時被呼叫一次。

### 2.1 `StrokeBuilder` 與 `generate_dabs` 必須等價

兩者是同一套邏輯的串流版與批次版。**這是可測的不變式**：

```
StrokeBuilder（餵入全部真實樣本）.finish()  ==  generate_dabs(全部真實樣本, preset, seed)
```

沒有這條，串流版會慢慢漂移，而 golden test 只守得住批次版——手感回歸就測不出來。

### 2.2 必答：`pressure` 與 `radius` 誰說了算

FFI 的 `InputSample` 同時有 `pressure` 與 `radius`（S0 已定）。判準：

```
radius > 0.0  →  手指。走自適應正規化（§5），忽略 pressure
radius == 0.0 →  觸控筆。直接用 pressure
```

**iOS 端負責在 `touch.type == .pencil` 時把 `radius` 設 0**，即使 `UITouch.majorRadius`
對觸控筆也有值。這是 Bridge 的責任，歸 `E1-input`。

不改 FFI 加一個 `source` 欄位，是因為這個約定用既有欄位就表達得完整，
而 `contracts.md` 的修正窗口在 E2／E3。**但它是語意條款，看簽章看不出來**——
列入回寫清單，補為 `contracts.md` C9。

---

## 3. `generate_dabs` 契約

```rust
pub fn generate_dabs(samples: &[InputSample], preset: &BrushPreset, size: f32, seed: u32) -> Vec<Dab>;
```

型別照 `architecture.md §5.2`，**只多一個 `size`**（決議 E，§14）。`seed` 讓 jitter 可重現，
否則 E3 的縮時重播會與原作不同。

**`samples` 必須已經濾除 `predicted: true`。** 純函式不知道預測是什麼。

---

## 4. 輸入處理鏈

```
真實樣本 → One-Euro（位置、radius 各一組）→ 向心 Catmull-Rom → 弧長取樣 → Dab
```

### 4.1 One-Euro filter

參數（實機調校，列入 `E1-perf`）：

| | 位置 | radius |
|---|---|---|
| `min_cutoff` | 1.0 Hz | 0.5 Hz |
| `beta` | 0.05 | 0.0 |
| `d_cutoff` | 1.0 Hz | 1.0 Hz |

- **radius 用更低的 cutoff、`beta = 0`**：接觸半徑本身就抖，而它驅動的是筆寬，
  抖動在視覺上比位置抖動更明顯。`beta = 0` 表示不隨速度放寬——radius 沒有「快速移動時
  要更跟手」的需求。
- **`dt` 必須從 `InputSample.t` 取，不可假設固定**。coalesced touch 的間隔不均勻，
  用固定 dt 會讓濾波強度隨取樣率浮動。

### 4.2 向心 Catmull-Rom（`alpha = 0.5`）

不是均勻參數化。均勻版在樣本間距差異大時會 overshoot 與打結——手指快速轉向時
必然發生。向心版沒有 cusp 與自交，代價只是多一個 `pow`。

**需要 4 個控制點才能畫出 `p1`–`p2` 之間的段**，所以樣條天生落後一個樣本。
這一格延遲由 `predictedTouches` 補（§9）。

### 4.3 弧長取樣

沿樣條累積弧長，每前進 `spacing × dab_size` 就放一個 dab。
**累積量跨 segment 保留**，否則每段接縫處都會多一個或少一個 dab，
在慢速筆畫上看得出串珠。

`dab_size` 隨壓感變化，所以間距也隨之變化——這是預期行為，`spacing` 的單位是筆尖直徑比。

---

## 5. `majorRadius` → `pressure` 的自適應正規化

演算法在 `stroke`（純 CPU、可測），資料來源在 iOS（`E1-input`）。

```
per-stroke running baseline：
  r_min ← min(r_min, r)   起始值 = 第一個樣本的 r
  r_max ← max(r_max, r)
  pressure = clamp((r - r_min) / max(r_max - r_min, R_EPS), 0, 1)
```

`R_EPS` 防止筆畫剛開始時分母為 0——此時 `pressure` 應為中值而非 0 或 1。
初值 `R_EPS = 4.0`（點），實機調校。

**per-stroke 而非固定 min/max**（`architecture.md §10.2`）：`majorRadius` 的絕對值
因手指大小而異。**已知限制**：一筆之內若使用者力道單調遞增，`r_min` 永遠是起筆值，
壓感範圍會被壓縮。使用者層級的長期基線是更好的解，但它需要跨 session 的狀態
——**E1 只做 per-stroke，跨 session 基線列為 E2 之後的候選**。

---

## 6. `BrushPreset`

十四個欄位逐字照 `architecture.md §4.6`。`Curve` 該節未定義，**本文定義它**：

```rust
pub struct Curve { pub min: f32, pub max: f32, pub gamma: f32 }
// out = min + (max - min) * p.powf(gamma)
```

三個參數、無編輯器、完全決定性。不用 LUT 或貝茲：`prd.md` 的 Don't Have
禁止使用者編輯筆刷參數，所以曲線只需要「表達得出五支 preset 的差異」，不需要可編輯性。

### 軟圓筆（E1 唯一實作的一支）

| 欄位 | 值 |
|---|---|
| `tip` | `SoftRound` |
| `spacing` | 0.05 |
| `pressure_to_size` | `{ min: 0.35, max: 1.0, gamma: 1.0 }` |
| `pressure_to_opacity` | `{ min: 0.40, max: 1.0, gamma: 1.0 }` |
| `velocity_to_size` / `tilt_to_size` | 0.0（E2） |
| `jitter_pos` / `jitter_size` / `jitter_angle` | 0.0 |
| `blend` | `Normal` |
| `flow` | 1.0 |
| `opacity` | 0.85 |
| `build_up` | `false` |
| `edge_boost` | 0.0 |

其餘四支照 `§4.6` 的表登記，**E1 不實作**——選到它們時 fallback 到軟圓筆並記一次 log。

### 6.1 Tip 貼圖

`TipId` → `texture_2d_array<f32>` 的 layer index，256×256 R8。
**程序生成，不進 `.colorpack` 也不進 app bundle**：軟圓是解析式的徑向衰減，
一行 shader 或一次 CPU 填表就有。E2 的顆粒／蠟筆紋才需要真的貼圖資產。

從第一天就用 array 而非單張，是為了讓 E2 加 tip 不動 bind group layout。

---

## 7. Pass 1 — Stroke

```
instanced quad × dab_count → T_wet
scissor：本 frame 新增 dab 的 bbox（不是整筆的 bbox）
```

每個 dab 一個 instance，vertex shader 依 `pos` / `size` / `angle` 展開四個頂點。
Fragment：`coverage = tip[tip_id].sample(uv) * dab.alpha`，其中
`dab.alpha = preset.flow × pressure_to_opacity(p)`。

**blend 依 `build_up`**（`architecture.md §4.2`）：

| `build_up` | wgpu blend | 效果 |
|---|---|---|
| `false` | `src: One, dst: One, op: Max` | 同筆內不疊暗 |
| `true` | `src: OneMinusDst, dst: One, op: Add` | `over` 累積（噴槍／水彩，E2） |

**scissor 用增量 bbox**：每 frame 只有 10–30 個新 dab，用整筆 bbox 會讓 scissor
隨筆畫變長而失去意義。整筆 bbox 另外累積，Pass 2 才用。

**instance 數上限**：單次 draw 上限 `MAX_DABS_PER_DRAW = 4096`，超過就分批。
快速長筆畫在一 frame 內產生上千 dab 是正常的，靜默截斷會變成「畫太快就斷線」。

---

## 8. Pass 2 — Commit（路徑 (a)）

抬筆時一次，scissor 至**整筆 bbox**。

```wgsl
let a = textureLoad(T_wet, cc, 0).r * u.opacity * mask(id);
out   = vec4(u.color.rgb * a, a);      // premultiplied
```

blend：`src: One, dst: OneMinusSrcAlpha`（premultiplied over）。

> **`T_paint` 存 premultiplied alpha。** `E1-composite.md §3` 第 ③ 層的 `over()`
> 因此是 `p.rgb + c * (1 - p.a)`，不是 straight-alpha 的版本。列入回寫。

`u.opacity` 是 `Tool::Brush.opacity` 覆寫值，`None` 時取 `preset.opacity`
（`architecture.md §6` Boundary 1）。**遮罩在這裡算一次，不是每個 dab 算**（`§4.3` #3）。

### 8.1 收尾

```
1. clear T_wet（scissor 至整筆 bbox，不是全畫布）
2. E3 才有：dirty tiles → Undo pool
```

E1 沒有 undo，所以收尾只有第 1 步。

---

## 9. `predicted` 樣本

預測點只影響當前 frame 的 `T_wet`，不進 oplog（`contracts.md` C4）。
但 `T_wet` 是累積的，`Max` blend 下已畫上去的預測 dab 抹不掉——
若直接 commit，筆畫的尾端會比使用者實際抬筆處長出一截。

**解法：`end_stroke` 時先重建 `T_wet`。**

```
end_stroke:
  1. clear T_wet（整筆 bbox）
  2. 以 generate_dabs(全部真實樣本) 重跑 Pass 1
  3. Pass 2 commit
```

成本是每筆抬筆時多跑一次 Pass 1，與 commit 同一個數量級，一筆一次。
換到的是三件事：尾端精確、`T_paint` 只含真實樣本、
以及 **`T_wet` 在 commit 前恰好等於 §2.1 那條不變式的右邊**——它讓那條不變式變得可觀測。

筆畫進行中的預測誤差（1–2 frame，數 px）不處理：`Max` blend 讓誤差只會在前緣多出覆蓋，
而前緣正在手指底下，看不見。若 D3 顯示仍有可見痕跡，退路是把預測 dab 畫進一張
bbox 大小的暫存層、每 frame 清掉，再與 `T_wet` 一起合成。

---

## 10. Golden test

`stroke` 是全專案唯一能在 CI 防手感回歸的地方（`architecture.md §5.2`）。

- fixture：`Vec<InputSample>` 的軌跡 ＋ preset ＋ seed → 期望的 `Vec<Dab>`，存 JSON
- E1 立三條：直線慢速、快速轉向（測向心 Catmull-Rom 不 overshoot）、原地停留
- **E1 不設為 CI gate**——參數還在調，每次調校都會讓 fixture 全紅。
  E2 參數定案後才 gate（`E1.md` 的驗收沒有這條，是 E2 的事）
- §2.1 的串流／批次等價**現在就設為 gate**：它與參數值無關，只與實作一致性有關

---

## 11. 已否決

| 做法 | 為何不 |
|---|---|
| 進行中的樣本走 `document.apply` | `T_wet` 不是持久狀態；每 frame 呼叫 apply 會讓「apply = 狀態變更」失去意義（§2） |
| 只做 `generate_dabs`，串流時每 frame 全量重算 | 一筆數千樣本 × 每 frame，且 `Max` blend 下重畫整筆是浪費（§2.1 用等價測試取代） |
| 均勻參數化 Catmull-Rom | 手指快速轉向時 overshoot 與打結（§4.2） |
| 固定 min/max 正規化 `majorRadius` | 手指大小因人而異（`architecture.md §10.2`） |
| `Curve` 用 LUT 或貝茲 | 使用者不能編輯筆刷參數，曲線不需要可編輯性（§6） |
| tip 貼圖進 `.colorpack` 或 app bundle | 軟圓是解析式的，程序生成即可（§6.1） |
| scissor Pass 1 到整筆 bbox | 筆畫越長 scissor 越無效（§7） |
| 接受預測樣本留在 `T_paint` 裡 | 筆尾會多出一截。重建 `T_wet` 很便宜（§9） |
| FFI 加 `source` 欄位區分手指／筆 | 既有的 `radius == 0` 就表達得完整，且修正窗口在 E2（§2.2） |

---

## 12. 驗收

- [ ] `cargo test -p stroke` 在無 GPU 環境全綠（Boundary 2）
- [ ] **串流／批次等價**：`StrokeBuilder.finish() == generate_dabs(同一組真實樣本)`
- [ ] 三條 golden fixture 產生穩定輸出；同 `seed` 兩次執行逐位元相同
- [ ] 快速轉向的軌跡不 overshoot、不自交打結
- [ ] 慢速來回塗抹同一處，濃度不隨次數變深（`T_wet` ＋ `Max` blend 的直接驗證）
- [ ] `opacity` 調整後，整筆濃度上限跟著變，而不是每個 dab 的濃度變
- [ ] 抬筆後的筆畫尾端與實際抬筆位置相符（§9 的重建生效）
- [ ] 快速長筆畫（一 frame > 4096 dab）不斷線
- [ ] `cancel_stroke` 之後 `T_paint` 逐像素不變

## 13. 要回寫的既有文件

| 文件 | 改什麼 | |
|---|---|---|
| `architecture.md §4.6` | `Curve` 的定義（§6）；補軟圓筆的初值表 | ✅ |
| `architecture.md §5.2` | `generate_dabs` 多一個 `size` 參數（決議 E） | ✅ |
| `architecture.md §5.3` | 輸入處理鏈補「向心」與「radius 另一組濾波參數」（§4） | ✅ |
| `architecture.md §10.2` | 自適應正規化補 per-stroke 的已知限制與 `R_EPS`（§5）；baseline 初值是 `r ± R_EPS/2` 的帶狀，不是 `r`（決議 F） | ✅ |
| `E1-composite.md §3` | `T_paint` 是 premultiplied alpha，第 ③ 層的 `over()` 要對應（§8） | Pass 2 落地時 |
| `contracts.md` ③ | 補 C9：`radius == 0` 表示觸控筆，`> 0` 表示手指（§2.2） | 修正窗口在 E2／E3 |

---

## 14. 執行期決議（交接）

實作 §3–§6 時遇到、spec 沒答的問題。**後續 Agent 直接照這節做，不要重新發明。**

| | 問題 | 決議 |
|---|---|---|
| A | `app_state::BrushPreset`（五支 enum，＝筆刷 ID）與本文的 `BrushPreset`（十四欄參數 struct）**同名不同物** | 兩個都留。`stroke` 是參數的唯一出處；`stroke` **不依賴 `app-state`**（它是同層 crate，不在 `stroke` 的下游），所以 enum → 參數的對應寫在 `engine` |
| B | `architecture.md §5.2` 的 `Vec2` 全 workspace 不存在，也沒有數學 crate | 定義在 `stroke::math`。不引 glam：只需要 add/sub/mul/lerp/length |
| C | `engine` → `stroke` 不在 `deps-policy.toml`，而 §2 把扁平 → `Vec2` 的轉換派給 `engine` | **本段不動 policy**。轉換與 `begin/append/end_stroke` 的接線延到 Pass 1/2 落地時一併做（改 policy ＝ 改架構，要單獨一次） |
| D | §10「E1 不設為 CI gate」需要一個機制 | 三條 golden 標 `#[ignore]`；`UPDATE_GOLDEN=1 cargo test -p colorlull-stroke --test golden -- --ignored` 重新產生。§2.1 的等價測試與「同 seed 逐位元相同」**不標**，現在就是 gate |
| E | `spacing` 是筆尖直徑比、`pressure_to_size` 是 0.35–1.0 倍率，但筆刷直徑住在 `AppState.size`，不在 `generate_dabs` 的簽章裡——弧長門檻算不出 px | 加 `size: f32` 參數，`Dab.size` 是 px 直徑。列入 §13 回寫 |
| F | §5 的 baseline 初值寫「＝第一個樣本的 `r`」，但這樣起筆 `pressure` 恆為 0，與同段「此時應為中值」矛盾 | 公式不動，**初值改成 `r_min = r₀ - R_EPS/2`、`r_max = r₀ + R_EPS/2`**。起筆得 0.5，且 min/max 照樣單調外擴 |
| G | `Dab` 要當 GPU instance 資料，會想加 `repr(C)` ＋ `bytemuck` | **不加**。`render` 自己定 `DabInstance` 與轉換，`stroke` 保持零版面配置知識（Boundary 2） |
| H | 筆畫進行中 predicted 樣本也要產 dab，但 §2.1 的不變式只認真實樣本 | `StrokeBuilder::push` 只吃真實樣本；predicted 走 `predicted_dabs(&self, &[InputSample])`——複製一份 builder 狀態算完就丟，committed 狀態不被污染，§9 的重建因此不必特別處理 |
