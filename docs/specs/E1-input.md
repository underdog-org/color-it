# E1 · 輸入與 FrameDriver

> 狀態：草案（2026-08-03）｜里程碑：[E1](../roadmap/E1.md)｜計畫：[E1-spec-plan](./E1-spec-plan.md)
>
> 唯一一份寫 Swift 的 E1 spec。演算法一律不在這裡——
> `majorRadius` 正規化見 [E1-stroke](./E1-stroke.md) §5，逆變換與查表見 [E1-bucket](./E1-bucket.md) §4。

## 涵蓋 `E1.md` 的哪幾條

| `E1.md` 實作清單 | 本文 |
|---|---|
| iOS `CADisplayLink`——渲染由 FrameDriver 驅動，不由輸入事件驅動 | §2 §3 |
| `InputAdapter`：`coalescedTouches` ＋ `predictedTouches`；預測點標記 `predicted: true` | §4 |
| `majorRadius` → `pressure` 的自適應正規化（**資料來源側**） | §6 |
| `cancel_stroke`：palm rejection 事後判定失敗時直接清 `T_wet` | §7 |

不涵蓋：正規化演算法本身（`E1-stroke §5`）、`Dab` 生成（`E1-stroke`）、
螢幕像素 → region ID（`E1-bucket §4`）。

---

## 1. 範圍 ＋ present 路徑定案

**定案：`CAMetalLayer` ＋ 自建 `CADisplayLink`。`MTKView` 是退路，E1 不用。**

`architecture.md §10.3` 的待決項在此結案。三個理由：

1. `§10.3` 規定渲染由 FrameDriver 驅動而非輸入事件驅動，而 `MTKView` 自帶一套 draw loop
   ——兩者是競爭機制，不是互補
2. wgpu 本來就吃 `CAMetalLayer`（`E1-wgpu §3`），`MTKView` 只是多包一層
3. S0 的 `EngineCanvasView` 已經是 `layerClass = CAMetalLayer` 這條路

退路的觸發條件：若 D3 顯示 latency 不達標且**已排除** `maximumDrawableCount`
與輸入批次時機（§3.1），才值得換 `MTKView` 比對——它與自建 loop 的差別只在誰呼叫
`nextDrawable`，不會憑空少一格。

---

## 2. FrameDriver

```swift
final class FrameDriver {
    private var link: CADisplayLink?
    private weak var target: EngineCanvasView?
}
```

| 項目 | 值 | 理由 |
|---|---|---|
| runloop mode | `.common` | 預設的 `.default` 在使用者滑動 UI 時會停掉 display link |
| `preferredFrameRateRange` | `CAFrameRateRange(minimum: 80, maximum: 120, preferred: 120)` | ProMotion 全速。非 ProMotion 裝置自動落到 60，不需要分支 |
| `isPaused` | 綁 attach／detach | 沒有 surface 時 `render()` 是純浪費 |

**`CADisplayLink` 會 retain target。** 直接把 `EngineCanvasView` 當 target 會讓 view
永遠不釋放，而 view 持有 engine，於是整份文件洩漏。用一個持 weak reference 的 proxy 物件轉發。
這是 `CADisplayLink` 的既知陷阱，不是可以「之後再說」的細節——洩漏的症狀是
「畫幾張圖之後 App 越來越慢」，會被誤判成渲染效能問題。

### 2.1 每 frame 做什麼

```
1. adapter.flush()  → engine.appendSamples([...])   （有累積樣本才呼叫）
2. engine.render()
```

**順序不可換。** 先送輸入再渲染，這一 frame 的手指位置才畫得進去；反過來會固定多一格延遲。

`appendSamples` 一 frame 一次，是 C3 的紅線（`contracts.md`）。沒有累積樣本時不呼叫——
空陣列的呼叫要跨一次 FFI，而它什麼也不做。

---

## 3. begin／end 立刻送，append 壓 frame

C3 只約束 `append_samples`。判準：

| 呼叫 | 時機 | 理由 |
|---|---|---|
| `beginStroke(s)` | `touchesBegan` 內立刻 | 它建立 stroke 狀態。壓到下一 frame 會讓第一批 `appendSamples` 沒有可附著的筆畫 |
| `appendSamples([s])` | 每 frame 一次 | C3 |
| `endStroke()` | `touchesEnded` 內立刻，**但先 flush 剩餘樣本** | 抬筆要重建 `T_wet` 並 commit（`E1-stroke §9`），漏掉最後幾個樣本 = 筆尾少一截 |
| `cancelStroke()` | `touchesCancelled` 內立刻，**丟棄未送出的樣本** | 取消不需要完整性（§7） |

所以 `touchesEnded` 的順序是 `flush()` → `endStroke()`，兩次 FFI 呼叫。
一筆一次，不在 C3 的約束範圍內。

### 3.1 這是 latency 的第二根調節桿

`touchesBegan` 到第一次 `render()` 之間隔多久，取決於 touch 事件落在 display link
callback 的哪一側。UIKit 的 touch delivery 與 `CADisplayLink` 都掛在 main runloop，
順序不保證。**這件事量得到**：`E1-perf` 的 motion-to-photon 逐格分析會看到它，
第一根桿是 `maximumDrawableCount`（`E1-wgpu §3.1`）。

不預先做任何處理（例如在 touch handler 裡直接觸發一次 render）——那會退回
「渲染由輸入驅動」，正是 `§10.3` 禁止的。

---

## 4. `InputAdapter`

```swift
func handle(_ touch: UITouch, event: UIEvent?, in view: UIView) {
    for t in event?.coalescedTouches(for: touch) ?? [touch] {
        pending.append(sample(from: t, predicted: false))
    }
    for t in event?.predictedTouches(for: touch) ?? [] {
        pending.append(sample(from: t, predicted: true))
    }
}
```

**順序是真實在前、預測在後**，且預測點永遠在該批的尾端——`stroke` 依賴這個順序做弧長取樣。

**預測點每 frame 重新產生，不累積。** UIKit 每次 touch 事件都給一組全新的預測，
上一批的預測沒有任何保存價值。

> **`StrokeBuilder` 必須把 `predicted: true` 的樣本排除在濾波器狀態之外。**
> One-Euro 是有狀態的，讓預測點更新 `x_prev` / `dx_prev` 會讓下一個真實樣本的濾波
> 建立在猜測上，而預測誤差會就此留在筆畫裡。`E1-stroke §9` 只寫了 `T_wet` 的重建，
> 沒寫濾波器狀態——列入回寫。

### 4.1 時間戳：**必須相對筆畫起點歸零**

`UITouch.timestamp` 是 `systemUptime`，開機幾天後是數十萬秒。
`InputSample.t` 是 `f32`（`core/engine/src/ffi.rs`），有效位數約 7 位——
`432000.0` 的下一個可表示值差 0.03 秒。**One-Euro 的 `dt` 會直接爛掉**
（`E1-stroke §4.1` 明文要求 `dt` 從 `t` 取）。

所以 `t = Float(touch.timestamp - strokeStartTimestamp)`，`strokeStartTimestamp`
在 `touchesBegan` 記下。一筆撐不到幾十秒，f32 在這個範圍內是微秒精度。

不改 FFI 把 `t` 換成 `f64`：修正窗口在 E2／E3，而歸零用既有欄位就解決得完整。

---

## 5. 座標系

```swift
let p = touch.preciseLocation(in: view)          // UIKit point
let px = Float(p.x * view.layer.contentsScale)   // 螢幕像素
```

- **`preciseLocation` 而非 `location`**：Pencil 有次像素精度，`location` 會捨去
- **乘 `contentsScale` 在 Swift 端做**，`tap` 與 `InputSample` 一律送**螢幕像素**
  （`E1-bucket §4.1` 定案）。`contentsScale` 是 layer 的屬性，Rust 不該去猜它
- **畫布逆變換不在 Swift 端做**。Swift 送螢幕像素，Rust 用 `Transform::canvas_pos`
  轉。`Transform` 的真相在 Rust（`set_viewport` 送進去的那份），Swift 端另存一份就會漂移

---

## 6. `pressure` 的兩條來源

演算法在 `E1-stroke §5`，本文只定**送什麼進去**。判準是 `E1-stroke §2.2` 的
`radius == 0 → 觸控筆`，而**「把 Pencil 的 radius 設 0」是 Bridge 的責任**：

```swift
let isPencil = touch.type == .pencil
sample.radius   = isPencil ? 0 : Float(touch.majorRadius)
sample.pressure = isPencil ? Float(touch.force / touch.maximumPossibleForce) : 0
sample.tiltX / tiltY = ...   // E1 不使用，如實填以免 E2 才發現沒接
```

`UITouch.majorRadius` 對 Pencil 也有值，所以這個 0 是**主動寫入的語意**，不是缺值。

**`maximumPossibleForce == 0` 的裝置**（不支援 force 的手指觸控）：走 radius 分支。
判斷條件因此是 `touch.type == .pencil && touch.maximumPossibleForce > 0`，
不是只看 `type`——除以 0 會產生 `NaN`，而 `NaN` 會沿著 One-Euro 傳染整筆
（`contracts.md` C8 的已知瑕疵也是同一種病）。

### 6.1 estimated properties：E1 不處理

Pencil 的 `force` 初次送達時是估計值，稍後由 `touchesEstimatedPropertiesUpdated`
補正。**E1 忽略補正**——採納它需要用 `estimationUpdateIndex` 回頭修改已送出的樣本，
而樣本已經濾過波、已經生成 dab、可能已經 commit。

代價：Pencil 的壓感在筆畫最初幾個樣本可能略有偏差。E1 的主要測試對象是手指
（`E1.md` 驗收：**必須用手指測**），這條偏差不在關鍵路徑上。
真要修是 E2 的事，且正確的做法是延遲一格再送，不是回頭改。

---

## 7. `cancelStroke` 與 palm rejection

`touchesCancelled` → 丟棄 `pending` → `engine.cancelStroke()`。

`T_wet` 直接清掉，`T_paint` 從未被污染（`E1-stroke §2`）——這正是「進行中的筆畫不經過
`document`」換到的東西：取消是零成本的，不需要 undo。

**多指：只追蹤第一根。** `touchesBegan` 時若已有 active touch，新的直接忽略
（不呼叫 `beginStroke`）。E1 沒有縮放平移手勢（`E1-composite §4`），所以第二根手指
唯一的合理語意就是「不是在畫畫」。

系統的 palm rejection 之外不自建判定邏輯——Apple 的實作吃得到 UIKit 拿不到的資料。

---

## 8. `EngineCanvasView` 的增修

現況 92 行（`apps/ios/EngineBridge/EngineCanvasView.swift`），E1 要動四處：

| 位置 | 改什麼 |
|---|---|
| `:71` 的 `assertionFailure` | 換成真的錯誤處理。`attach_surface` 在 E1 起真的會失敗（`E1-wgpu §2.2`）——顯示錯誤態，**不 crash**：使用者的畫作還在 engine 裡 |
| `attach()` / `detach()` | 加 FrameDriver 的 start／pause；`attach()` 內設 `layer.maximumDrawableCount = 2`（`E1-wgpu §3.1` 的記名例外） |
| 新增 touch handlers | `touchesBegan/Moved/Ended/Cancelled`，轉發給 `InputAdapter`（§3 的時機表） |
| 類別註解 `:14-17` | 「已列為 E1 的待決項」→ 結案，改指向本文 §1 |

`isMultipleTouchEnabled` 維持預設的 `false`——它讓 §7 的「只追蹤第一根」由 UIKit 保證。

---

## 9. 已否決

| 做法 | 為何不 |
|---|---|
| `MTKView` | 自帶 draw loop，與 `§10.3` 的 FrameDriver 是競爭機制（§1） |
| 在 touch handler 裡直接觸發 render | 退回「渲染由輸入驅動」（§3.1） |
| `CADisplayLink` 直接以 view 為 target | retain cycle，整份文件洩漏（§2） |
| runloop mode 用 `.default` | 滑動 UI 時 display link 停掉（§2） |
| 累積上一批的 `predictedTouches` | UIKit 每次都給全新一組，舊的無保存價值（§4） |
| `t` 直接送 `touch.timestamp` | f32 在 `systemUptime` 的量級只剩 0.03 秒精度，`dt` 爛掉（§4.1） |
| 把 `t` 改成 `f64` 擴 FFI | 歸零用既有欄位就解決得完整，且修正窗口在 E2（§4.1） |
| `location(in:)` | 捨去 Pencil 的次像素精度（§5） |
| Swift 端做畫布逆變換 | `Transform` 會有兩份，必然漂移（§5） |
| 只用 `touch.type == .pencil` 判斷壓感來源 | `maximumPossibleForce == 0` 會除出 `NaN`（§6） |
| 採納 `touchesEstimatedPropertiesUpdated` 的補正 | 要回頭改已 commit 的樣本；E1 以手指為主要測試對象（§6.1） |
| 自建 palm rejection | Apple 的實作吃得到 UIKit 拿不到的資料（§7） |

---

## 10. 驗收

- [ ] `render()` 由 FrameDriver 呼叫，**touch handler 內零次** render 呼叫（以 log 或斷點驗）
- [ ] 一 frame 內多個 touch 事件只產生一次 `appendSamples`（C3）
- [ ] `EngineCanvasView` 移出 window 後 deinit——FrameDriver 無 retain cycle
- [ ] Pencil 的樣本 `radius == 0`；手指的樣本 `radius > 0`
- [ ] 不支援 force 的裝置上，Pencil 分支不產生 `NaN`
- [ ] 連續畫 60 秒，`InputSample.t` 單調遞增且相鄰差值 > 0（f32 精度未塌陷）
- [ ] 畫到一半按下 Home／來電 → `touchesCancelled` → `T_paint` 逐像素不變
- [ ] 第二根手指落下不中斷第一筆，也不產生第二筆
- [ ] ProMotion 裝置上 `CADisplayLink` 實測 120 Hz（`E1-perf` 取數）

## 11. 要回寫的既有文件

| 文件 | 改什麼 |
|---|---|
| `E1-stroke.md §9` | `StrokeBuilder` 的濾波器狀態必須排除 `predicted` 樣本（§4） |
| `contracts.md ②` | `attach_surface` 的 v0 狀態失效；補 C9（`radius == 0` = 觸控筆，`E1-stroke §2.2`）；補 C10（`InputSample.t` 相對筆畫起點，§4.1） |
| `architecture.md §10.1` | `MTKView` → `CAMetalLayer` ＋ 自建 `CADisplayLink`（§1） |
| `architecture.md §10.3` | 待決項結案（§1） |
| `roadmap/E1.md` | 產出物與實作清單的「iOS 端 `MTKView`」→ `CAMetalLayer`（§1） |
| `apps/ios/EngineBridge/EngineCanvasView.swift:14-17` | 註解的待決項結案（§8） |
