# E1 · Composite Pass

> 狀態：草案（2026-08-03）｜里程碑：[E1](../roadmap/E1.md)｜計畫：[E1-spec-plan](./E1-spec-plan.md)
>
> 資源型別、格式、pass ↔ 資源矩陣見 [E1-wgpu](./E1-wgpu.md)，本文只定 Pass 3 的內容。

## 涵蓋 `E1.md` 的哪幾條

| `E1.md` 實作清單 | 本文 |
|---|---|
| WGSL composite shader 六層 | §3 |
| `PAPER_WHITE` 常數 | §3.1 |
| `set_viewport`：畫布 transform ＋ full-screen triangle | §4 |
| 擴散動畫：composite shader 內 `smoothstep`；`fill_origin` / `max_radius` per-tap | §5 |
| Mask Mode A／B 兩種都實作，可即時切換 | §6 |

不涵蓋：Pass 1／2（`E1-stroke`）、`Fill` 的 CPU 側與動畫推進（`E1-bucket`）。

---

## 1. 範圍

每 frame 一個 full-screen triangle，把七個資源合成到 surface。**Composite 沒有任何條件分支
是關於「使用者在做什麼」**——它只讀狀態、不改狀態。

---

## 2. 必答：色彩空間 = **sRGB 編碼空間，不是 linear**

**全部合成在 sRGB 編碼值上直接做。** 所有資源用非 sRGB 的 `Unorm` 格式，
硬體不做任何 decode／encode。

### 為什麼不是 linear

物理上 linear 才對，但**有一個更強的約束：畫布必須跟 baker 產出的縮圖長一樣。**

`tools/baker/src/thumb.rs` 的合成是 u8 整數乘法：

```rust
v = base[c] as u32 * shade[..] / 255;
v = v * lineart_white[..] / 255;
```

`compose.rs::over_white` 的 alpha 合成同樣在編碼值上做。`thumb.jpg` 是 Gallery 的卡片圖，
也是 `architecture.md §8.4` 在資產取不到時的唯讀顯示。**若 runtime 在 linear 空間合成，
同一張圖在 Gallery 與 Canvas 會是兩個顏色。**

三個佐證指向同一邊：

1. **baker 已經定死了**（上述），且它有測試守著
2. **繪師的 `reference.png` 也是 sRGB 工具產出的**——`assets-spec` 的驗收基準
3. `T_line` 的抗鋸齒灰階是繪師的工具在 sRGB 空間 rasterize 的。在 linear 空間相乘
   不會還原繪師看到的邊緣，只會得到另一種邊緣

### 代價（已接受）

軟筆刷的 coverage 在 gamma 空間混合，邊緣會比 linear 略「薄」。
**這是所有 8-bit 繪圖 App 的預設行為**，使用者的預期正是校準在那上面。
若 D3／D5 盲測認為軟邊不對，退路是只把 **Pass 2 commit 的 `T_wet` → `T_paint`**
改到 linear 做，composite 維持 sRGB——那是一個 pass 的改動，不動資源格式。

### 對 `E1-wgpu.md` 的修正

| 資源 | 原寫 | 改為 |
|---|---|---|
| surface | `Bgra8UnormSrgb` | **`Bgra8Unorm`** |
| `T_line` `T_shade` `T_paint` | `Rgba8UnormSrgb` | **`Rgba8Unorm`** |
| `Buf_palette` | linear `vec4<f32>` | **編碼值 `vec4<f32>`**（`Rgba.r as f32 / 255`，不做 linearize） |

`T_erase` / `T_wet` 不變（`R8Unorm`，它們是 coverage 不是顏色）。

> `E1-wgpu.md` 原本用「8 bit 存 linear 在暗部有階梯」論證 sRGB 格式——**那個論證是錯的**：
> 兩種格式儲存的 bits 相同，差別只在硬體要不要在 blend 前後做轉換，精度完全一樣。

---

## 3. WGSL 六層

逐行對照 `architecture.md §4.2 Pass 3`，差異已標註。

```wgsl
let cc      = canvas_coord(in.uv);            // §4：screen → canvas 整數座標
let id      = textureLoad(T_region, cc, 0).r;
let erased  = textureLoad(T_erase,  cc, 0).r;

// ① 油漆桶底色 ＋ 擴散動畫（§5）
let f       = fill[id];
let d       = distance(vec2<f32>(cc), f.origin);
let t       = smoothstep(d - FILL_EDGE, d, f.progress * f.max_radius);
let base    = mix(f.prev_color, palette[id], t);

// ② 未上色 = 白紙，且底色可被局部擦除
var color   = mix(PAPER_WHITE, base.rgb, base.a);
color       = mix(color, PAPER_WHITE, erased);

// ③ 已提交的筆刷
color       = over(color, textureLoad(T_paint, cc, 0));

// ④ 進行中的筆畫
color       = over(color, tint(textureLoad(T_wet, cc, 0).r, brush_color) * mask(id));

// ⑤⑥ Multiply，線稿蓋頂
color       = color * textureSample(T_shade, samp, in.uv).rgb;
color       = color * textureSample(T_line,  samp, in.uv).rgb;
```

**與 `§4.2` 的三處差異**：

1. **`erased` 的套用位置**。`§4.2` 寫 `mix(palette[id], PAPER_WHITE, erased)`，
   但那樣「從未填色」與「填了又擦掉」無法區分。改成先用 `palette.a` 決定有沒有底色（②），
   再套 `erased`。`Buf_palette` 的 `a == 0` 表未填色是 `E1-wgpu.md §4.1` 定的。
2. **擴散動畫併進第 ① 層**，不是額外一層。`§4.5` 把它寫成獨立片段，但它算的就是底色。
3. **`T_region` / `T_paint` / `T_erase` / `T_wet` 用 `textureLoad`**（整數座標、無 sampler），
   `T_line` / `T_shade` 用 `textureSample`（linear filter，畫布縮放時需要）。
   `T_region` 必須 `textureLoad`——`R16Uint` 綁不了 sampler（`E1-wgpu.md §5.2`）。

### 3.1 `PAPER_WHITE`

`vec3<f32>(1.0, 1.0, 1.0)`，編譯期常數。**未上色 = 白紙，不是透明**（`prd.md §4.1`）。

不做成 uniform：它不是設定項，是產品語意。深色模式在 `v1 不做` 清單上，
真要做也是換一整套渲染語意，不是改一個常數。

---

## 4. `set_viewport` 與 full-screen triangle

`Transform { scale: f32, tx: f32, ty: f32 }`（S0 已定，`core/engine/src/ffi.rs`）。

**Composite 的成本由螢幕解析度決定，不是畫布解析度。** full-screen triangle
rasterize 的是 surface 的像素（約 3.0 M），不是 2048²（4.19 M）。
`canvas_coord()` 把 screen UV 反變換回畫布整數座標：

```wgsl
fn canvas_coord(uv: vec2<f32>) -> vec2<i32> {
    let p = (uv * screen_size - vec2(vp.tx, vp.ty)) / vp.scale;
    return vec2<i32>(floor(p));
}
```

**畫布外的像素**（縮小時的四周）：`p` 落在 `[0, canvas_size)` 之外時直接輸出
背景色並 `return`，不讀任何貼圖。背景色是 UI 的 canvas 底色，由 `set_viewport`
一併帶入 uniform——**不要用 `PAPER_WHITE`**，否則使用者分不出畫布邊界在哪。

E1 的 `scale` 恆為 fit-to-screen，**縮放平移是 E2**。但反變換現在就寫對，
E2 只是讓 `Transform` 開始變動。

---

## 5. 擴散動畫的 buffer 佈局

```wgsl
struct FillAnim {
    origin:     vec2<f32>,   // 點擊處，畫布像素座標
    max_radius: f32,         // region bbox 對角線（保守略大，寧可早結束）
    progress:   f32,         // 0..1，CPU 每 frame 更新
    prev_color: vec4<f32>,   // 這次填色之前該區域的顏色
}
@group(0) @binding(N) var<storage, read> fill: array<FillAnim>;
```

長度 `manifest.region_count`，32 bytes／筆，65535 區上限 = 2 MB。

**`prev_color` 是 `§4.5` 沒寫但必要的欄位。** 沒有它，重複填同一區時動畫的起點無從得知——
只能從新顏色跳變，或錯誤地從白紙淡入。有了它，三種情況用同一條式子涵蓋：

| 情況 | `prev.a` | `palette.a` | 視覺 |
|---|---|---|---|
| 從未填色 | 0 | 0 | 恆為 `PAPER_WHITE` |
| 首次填色 | 0 | 1 | 白紙 → 顏色淡入 |
| 重新填色 | 1 | 1 | 舊色 → 新色交叉淡出 |

**per-tap 而非 per-region**（`§4.5`）：`origin` 與 `max_radius` 每次 tap 都重寫。
同一區被連點兩次，第二次的動畫從第一次的**當前插值結果**起算——
`E1-bucket` 負責把它寫進 `prev_color`。

**更新成本**：CPU 每 frame 只對**進行中**的 entry 做 `queue.write_buffer`（32 bytes ×
進行中筆數），不是整個 2 MB。`progress` 到 1 之後停止寫入。動畫推進的時間軸歸 `E1-bucket`。

`FILL_EDGE`：擴散前緣的柔邊寬度，畫布像素。初值 **24**，實機調校（列入 `E1-perf`）。

---

## 6. Mask Mode：Mode B 的重新定義

```wgsl
fn mask(id: u32) -> f32 {
    // mode 0 = A 嚴格；1 = B 寬鬆
    return select(1.0, f32(id == m.active_region_id), m.mode == 0u);
}
```

**Mode B 恆回 1.0，也就是完全不遮罩。**

`architecture.md §4.4` 寫的是 `id != REGION_LINEART`，但 baker 產出的 ID map
是滿的、沒有保留 ID——線稿覆蓋帶的像素全部被重新分配給相鄰區域
（`baker-core-design.md §2.5`）。**`REGION_LINEART` 不存在，該條件恆為真。**

`§4.4` 說「`id != REGION_LINEART` 已提供足夠的無害性：怎麼擦都不會擦掉線稿」——
**這個保證仍然成立，但來源是第 ⑥ 層**：`T_line` 永遠 Multiply 蓋在最頂，
使用者畫的任何東西都在它底下。與 mask 無關。

D4 要比較的兩件事因此沒有改變：A = 只能塗當前區域，B = 到處都能塗。
`MaskUniform` 佈局見 `E1-wgpu.md §7.1`；切 mode 不重建 pipeline。

---

## 7. 每 frame 成本

| | |
|---|---|
| 片段數 | 螢幕解析度，約 3.0 M（**不是畫布的 4.19 M**） |
| 每片段 | 4 次 `textureLoad` ＋ 2 次 `textureSample` ＋ 2 次 storage buffer 讀 |
| 畫布外片段 | early return，只寫背景色 |

Composite 是頻寬受限而非算術受限。真正的成本在 Pass 1，而它只畫 stroke bbox
（`architecture.md §4.2`）。**若 p99 frame time 不達標，先量這個 pass 再談優化**——
不要預先做 tile culling，`E1-perf` 會給數字。

---

## 8. 已否決

| 做法 | 為何不 |
|---|---|
| linear 空間合成 | 與 baker 的 `thumb.jpg` 不一致，Gallery 與 Canvas 會是兩個顏色（§2） |
| `PAPER_WHITE` 做成 uniform | 它是產品語意不是設定；深色模式在 v1 不做清單上（§3.1） |
| 擴散動畫另開一個 pass | 它算的就是底色，併進第 ① 層零成本（§3） |
| 動畫 buffer 每 frame 整份上傳 | 2 MB／frame。只寫進行中的 entry（§5） |
| 沿用 `§4.2` 的 `mix(palette, PAPER_WHITE, erased)` | 分不出「從未填色」與「填了又擦掉」（§3 差異 1） |
| 畫布外用 `PAPER_WHITE` | 使用者看不出畫布邊界在哪（§4） |
| 預先做 tile culling | 沒有數字支撐的優化（§7） |

---

## 9. 驗收

- [ ] **與 `thumb.jpg` 一致**：把 `palette` 設為各區的 `suggested_color`、無使用者筆刷，
      composite 輸出降採樣至長邊 512，與 `.colorpack` 內的 `thumb.jpg`
      在 JPEG 容差內相符。**這是色彩空間唯一的客觀驗收**
- [ ] 無 `shade` 的文件與有 `shade` 的文件走同一個 pipeline，輸出正確
- [ ] 未填色區域顯示 `PAPER_WHITE`；填色後擦除一塊，該塊回到 `PAPER_WHITE`
- [ ] 首次填色是白紙淡入，重新填色是交叉淡出，連點兩次不跳變
- [ ] Mask A／B 即時切換，畫面立即反映，無 pipeline 重建
- [ ] 畫布外區域顯示背景色，與畫布邊界清晰可辨
- [ ] p99 frame time 記錄進 `docs/perf-baseline.md`（`E1-perf`）

## 10. 要回寫的既有文件

| 文件 | 改什麼 |
|---|---|
| `E1-wgpu.md §4 §6 §9` | 資源格式改非 sRGB 變體；刪掉錯誤的 banding 論證（§2） |
| `architecture.md §4.2 Pass 3` | `erased` 的套用位置（§3 差異 1）；補色彩空間一句 |
| `architecture.md §4.4` | Mode B 不是 `id != REGION_LINEART`，是無條件通過（§6） |
| `architecture.md §4.5` | 擴散動畫補 `prev_color` 欄位（§5） |
| `roadmap/E1.md` 第 61 行 | Mode B 的描述同上 |
