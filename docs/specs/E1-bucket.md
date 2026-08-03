# E1 · 油漆桶與 `document.apply`

> 狀態：草案（2026-08-03）｜里程碑：[E1](../roadmap/E1.md)｜計畫：[E1-spec-plan](./E1-spec-plan.md)
>
> 資源與 pass ↔ 資源矩陣見 [E1-wgpu](./E1-wgpu.md)；`FillAnim` 佈局與六層合成見
> [E1-composite](./E1-composite.md) §5。

## 涵蓋 `E1.md` 的哪幾條

| `E1.md` 實作清單 | 本文 |
|---|---|
| `tap(x, y)` → `T_region` 取 ID → `Buf_palette[id] = color`（O(1)，不做泛洪） | §4 §5 |
| `Fill` 順帶清該區域的 `T_erase`（scissor 至 region bbox，以 `T_region` 為 mask） | §6 |
| 擴散動畫的 CPU 推進（shader 側歸 `E1-composite`） | §7 |
| `core/document` 最小 apply（`E1.md` 尚未列，由 `E1-spec-plan` 的第三個拍板決定補上） | §2 §3 |

不涵蓋：`FillAnim` 的 WGSL 佈局與第 ① 層算式（`E1-composite §5`）、`Op::BrushStroke`
的內容（`E1-stroke`）、`tap` 的座標從哪來（`E1-input`）。

---

## 1. 範圍

油漆桶是 E1 最便宜的一條垂直切片：O(1) 查表 ＋ 一次 buffer 寫入，**完全不碰 stroke 管線**，
卻能驗證 `T_region` / `Buf_palette` / composite 三者接對了。順帶把 `core/document` 立起來
——鐵律 #3 的「單一寫入口」需要有個入口存在。E1 的 `document` 不接 oplog、不接 history、沒有 undo。

---

## 2. `document::apply(Op)` 最小版

```rust
pub enum Op {
    Fill { region_id: u32, color: [u8; 4] },
    BrushStroke { color: [u8; 4], opacity: f32, /* E1 只定型不使用 */ },
}

pub struct Document {
    /// 長度 `manifest.region_count`，索引即 region ID（§8）。
    /// **`a == 0` 表示未填色**，與 `Buf_palette` 同一個約定（`E1-wgpu §4.1`）。
    palette: Vec<[u8; 4]>,
}

impl Document {
    pub fn apply(&mut self, op: Op) -> Effect;
    pub fn palette(&self) -> &[[u8; 4]];
    pub fn colored_regions(&self) -> u32;   // palette 中 a != 0 的筆數
}
```

顏色用**編碼值 `[u8; 4]`**，與 `AppState.color` 同型（`core/app-state`），
不在 `document` 引入第三種顏色型別。linear 轉換不存在——全案在 sRGB 編碼值上合成
（`E1-composite §2`）。

**`colored_regions` 的真相從此在 `document`。** S0 是 `AppState::tap()` 自己遞增到
飽和（`contracts.md ②`），E1 起改由 `engine` 把 `document.colored_regions()` 投影進
`AppState`。列入回寫。

---

## 3. `document` 不得依賴 `render`

`xtask/deps-policy.toml` 已經把這件事寫死：`[crates.document] internal = ["colorpack", "oplog"]`，
而 `[crates.engine]` 同時看得到 `document` 與 `render`。**改這個檔案等於改架構。**

所以 `apply` 不畫任何東西，它回傳一個描述性的 `Effect`，由 `engine` 翻譯成 GPU 動作：

```rust
pub enum Effect {
    /// `prev` 是這次填色之前的顏色，直接餵給 `FillAnim.prev_color`（§7）。
    Filled { region_id: u32, color: [u8; 4], prev: [u8; 4], bbox: [u32; 4] },
    /// 同色重填、座標落在畫布外——狀態沒變，engine 什麼都不做。
    None,
}
```

```
tap(x, y) ─engine─▶ 逆變換 ＋ 查表（§4）─▶ Op::Fill ─▶ document.apply ─▶ Effect::Filled
                                                                            │
              engine ──┬──▶ render：write_palette ＋ write_fill ＋ 清 T_erase ◀┘
                       └──▶ AppState.colored_regions → UiState emit
```

鐵律 #3 守的是「狀態變更只有一個入口」，不是「入口自己畫」。E3 只是在 `apply` 裡多接
oplog 與 undo 兩條線，`Effect` 這一層不動。

---

## 4. `tap(x, y)` → region ID

### 4.1 座標單位定案：**螢幕像素**

FFI 的 `tap(x, y)` 在 S0 忽略座標（`contracts.md ②`）。E1 定死它收**螢幕像素**，
不是 UIKit point——`SurfaceHandle` 的 `width_px` / `height_px` 已經是像素，
一個 FFI 表面裡混兩種單位是 bug 溫床。乘 `contentsScale` 是 Bridge 的責任（`E1-input`）。

### 4.2 逆變換

```rust
impl Transform {
    /// screen px → 畫布像素（浮點）。整數化與邊界判斷由呼叫端做。
    pub fn canvas_pos(&self, screen: [f32; 2]) -> [f32; 2] {
        [(screen[0] - self.tx) / self.scale, (screen[1] - self.ty) / self.scale]
    }
}
```

**這是 `composite.wgsl:70` 同名函式的孿生體**，兩者不可能共用程式碼（一個 Rust 一個 WGSL），
所以**用測試釘住**：同一組 `Transform` ＋ 同一組螢幕座標，Rust 的結果與 shader readback
的整數座標逐點相等。漂移的症狀是「點到隔壁區」，縮放平移進 E2 之後才浮出來，
而那時它看起來像手感問題。

### 4.3 查表

```rust
let id = res.region_ids()[y as usize * w + x as usize];   // O(1)
```

`region_ids: Vec<u16>` 由 `DocumentResources` 持有（`E1-wgpu §5.1` 的必答：CPU 側保留副本）。
**不得另開一份。** 落在 `[0, canvas_size)` 之外 → `Effect::None`，不 clamp——
clamp 會讓畫布外的誤觸填到邊緣區域。

### 4.4 `active_region_id` 也走這條

Mask Mode A 需要 `active_region_id`（`E1-wgpu §7.1`），它由 `begin_stroke` 的**第一個
真實樣本**落點決定，取法與 `tap` 完全相同。因此 §4.2 ＋ §4.3 是兩者共用的路徑，
歸屬本文——`E1-stroke` 與 `E1-input` 都只引用。

---

## 5. `Buf_palette` 的更新

```rust
res.write_palette(gpu, region_id, [r as f32 / 255.0, g / 255.0, b / 255.0, 1.0]);
```

- **編碼值，不 linearize**（`E1-wgpu §6`）
- **alpha 恆為 1.0**：填色即不透明。`a == 0` 只用來表示「從未填過」，
  它是狀態旗標不是使用者可調的透明度
- 單筆 16 bytes 的 `write_buffer`，不重傳整份

`Tool::Bucket { color }` 的 `color.a` 被忽略——FFI 的 `Rgba` 有 alpha 是因為它是通用型別。
若 E2 要半透明填色，那是 `Op::Fill` 的語意變更，不是這裡放行。

---

## 6. `Fill` 清 `T_erase`

矩陣裡的 `Fill` 一列（`E1-wgpu §7`）：讀 `T_region`、clear `T_erase`、寫 `Buf_palette`。
清除是一個小 render pass：

```
scissor  : regions[id].bbox（`RegionEntry.bbox` = [x, y, w, h]）
fragment : if textureLoad(T_region, cc, 0).r != id { discard; } else { out = 0.0; }
```

`discard` 而非寫回原值——`T_erase` 是 render attachment，讀寫同一張圖要 ping-pong，
而 discard 零成本。

> **E1 的可觀測效果是零。** 橡皮擦是 E2，`T_erase` 在 E1 全程恆為 0。
> 現在做是因為它是 `Fill` 的語意一部分（填色要蓋掉先前的擦除痕跡），事後補會變成
> 「為什麼填了色還是白的」這種難查的 bug。驗收只能用注入非零 pattern 的 offscreen 測試。

---

## 7. 擴散動畫的 CPU 側

### 7.1 狀態放 `render`，不放 `document`

動畫需要時間，而 `document` 是純狀態、可測、無時鐘的一層——時間進去會讓 `apply`
從「狀態轉移」變成「狀態轉移 ＋ 副作用排程」。因此 `render` 持有：

```rust
struct FillAnimator { active: Vec<Active> }   // 動畫結束就移除
struct Active { region_id: u32, elapsed: f32, anim: FillAnim }
```

`document` 對動畫一無所知——它只在 `Effect::Filled` 裡交出 `prev`。

### 7.2 時間來源

`render()` 沒有時間參數，且**不擴 FFI**（每 frame 都要傳的東西進 FFI 只會變成 Bridge 的
另一個出錯點）。`RenderContext::render` 內部持有 `Instant`，取兩次呼叫的差為 `dt`。
代價是 `render` 沾上時鐘：`render_with_dt(frame, dt)` 為真正的實作、`render(frame)`
是它的 wrapper，測試呼叫前者，不需要 mock 框架。

### 7.3 時間軸

| | 值 |
|---|---|
| 時長 | **180 ms**（初值，實機調校 → `E1-perf`） |
| 曲線 | ease-out cubic：`p = 1 - (1 - t)³`，`t = elapsed / 180ms` |
| 每 frame 成本 | 進行中筆數 × 32 bytes `write_buffer`，`p` 到 1 之後停止寫入 |

多筆同時進行不特別處理：各自獨立推進，各自寫自己的 entry。180 ms 內人手點得出來的
筆數是個位數，不值得為它做批次上傳。

### 7.4 `max_radius`：**不是 bbox 對角線**

`E1-composite §5` 寫的是「region bbox 對角線」。**那個值在 origin 靠近 bbox 角落時不夠大**
——擴散會在動畫結束時仍未覆蓋對角的另一端，視覺上是「填到一半就停了」。

正確值是 origin 到 bbox 四個角的**最大距離**：

```rust
let max_radius = corners(bbox).iter().map(|c| distance(origin, *c)).fold(0.0, f32::max);
```

它天生 ≥ 對角線的一半、≤ 對角線，且恆定足夠。列入回寫。

### 7.5 連點同一區：`prev_color` 取當前插值結果

`E1-composite §5` 指名這件事歸本文。若該區的動畫還在進行（`p < 1`），
新一筆的 `prev_color` 不是舊的 `palette[id]`，而是**此刻畫面上的顏色**：

```rust
let prev = mix(old.prev_color, old_palette, ease_out(old.elapsed / DURATION));
```

CPU 與 shader 用同一條 `mix`，所以接得上，不跳變。動畫已結束時這條式子退化成
`old_palette`，不需要分支。

`origin` 與 `max_radius` 一律用**新這次**的 tap 重算——per-tap 而非 per-region。

---

## 8. `REGION_LINEART`：查證結果 = **不存在**

`E1-spec-plan` 要求對 `tools/baker` 查證，不得自行約定。查證結果：

| 事實 | 出處 |
|---|---|
| `entries` 是 `(0..regions.count).map(\|id\| RegionEntry { id, .. })` | `tools/baker/src/lib.rs:182` |
| 載入時驗 `region_ids` 內無 `id >= region_count` | `core/colorpack/src/lib.rs:157` |
| 線稿覆蓋帶的像素全部重新分配給相鄰區域，無保留 ID | `baker-core-design.md §2.5` |

**結論：region ID 是 0-based dense，`regions[id].id == id`。**
`palette[id]`、`fill[id]`、`regions[id]` 三者可直接以 ID 索引，不需要任何映射表。
`E1-composite §6` 的「Mode B 恆為真」由此成立。

---

## 9. 已否決

| 做法 | 為何不 |
|---|---|
| `document` 直接呼叫 `render` | `deps-policy.toml` 禁止；且 `document` 會失去純 CPU 可測性（§3） |
| 泛洪填充（flood fill） | `T_region` 已經是預先算好的區域圖，泛洪是把 baker 做過的事在 runtime 重做一次（`E1.md`） |
| `tap` 收 UIKit point | 同一個 FFI 表面混兩種座標單位（§4.1） |
| 畫布外的 tap 做 clamp | 誤觸會填到邊緣區域（§4.3） |
| 動畫狀態放 `document` | 時間進 `document` 會讓 `apply` 不只是狀態轉移（§7.1） |
| `render()` 加 dt 參數擴 FFI | 每 frame 都要傳的東西進 FFI 只會變成 Bridge 的出錯點（§7.2） |
| `max_radius` 用 bbox 對角線 | origin 在角落時不夠大，動畫會提早停（§7.4） |
| 連點時 `prev_color` 取 `palette[id]` | 第二次動畫從舊色跳變起算（§7.5） |
| 為 region ID 另做一張映射表 | ID 就是索引，baker 已保證（§8） |

---

## 10. 驗收

- [ ] `cargo test -p document` 在無 GPU 環境全綠（`document` 不依賴 `render`）
- [ ] `cargo xtask lint-deps` 通過：`document` 未新增依賴
- [ ] Rust 的 `Transform::canvas_pos` 與 shader 的同名函式，在同一組 transform 下逐點相等
- [ ] 畫布外的 `tap` 不改變任何狀態（`Effect::None`，`Buf_palette` 逐位元不變）
- [ ] 填色後 `colored_regions` 遞增；同色重填不遞增
- [ ] `Fill` 之後該區域的 `T_erase` 為 0（測試預先注入非零 pattern）
- [ ] 連點同一區兩次不同色，第二次動畫從當前畫面顏色起算（§7.5）
- [ ] `max_radius` 足以覆蓋整個 bbox：動畫結束時該區域無殘留舊色（origin 取 bbox 四角逐一驗）
- [ ] 65535 區的合成文件，`tap` 的耗時與區域數無關（O(1)）

## 11. 要回寫的既有文件

| 文件 | 改什麼 |
|---|---|
| `E1-composite.md §5` | `max_radius` 不是 bbox 對角線，是 origin 到四角的最大距離（§7.4） |
| `contracts.md ②` | `tap(x, y)` 的 v0 狀態（推進 `colored_regions` 到飽和）在 E1 失效；補座標單位＝螢幕像素 |
| `architecture.md §4.5` | 擴散動畫補 ease-out cubic 與 `max_radius` 的取法（§7.3 §7.4） |
| `architecture.md §4.7` | 進度計算的真相移至 `document.colored_regions()`（§2） |
| `roadmap/E1.md` | 實作清單補一條 `core/document` 最小 apply（`E1-spec-plan` 的第三個拍板） |
