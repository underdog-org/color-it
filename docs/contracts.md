# FFI 契約（v0）

> **這份文件不是 FFI 的真相。** 真相是 `core/engine` 的 Rust proc-macro 標註，
> Swift 端由 uniffi 生成（`architecture.md §7`：一份契約只能存在一次）。
> 這裡只放**程式碼表達不出來**的三種東西：語意條款、semver 判定、遷移記錄。
> 設計理由與被否決的選項見 `docs/specs/ffi-contract.md`。

## ① SSOT 與 v0 標記

SSOT 是 `core/engine/src/` 的 `ffi.rs`（型別）、`error.rs`、`listener.rs`、`engine.rs`（方法表）。
本文與原始碼不一致時，**以原始碼為準，要改的是本文**。

整個表面標記為 **v0**，預告兩次修正窗口：

- **E2**（工具集完整）：`Tool` 與 `BrushId` 可能增值，`opacity` 的語意複審
- **E3**（Undo／持久化接上）：`can_undo` / `can_redo` 由據實回報取代 mock 的常數 `false`，
  `save` / `export_*` 真的實作

在那之前 Swift Bridge 不該假設任何欄位是最終形狀。

## ② FFI 表面速查表

S0 驗收「`EngineProtocol.swift` 與 Rust FFI 表面逐一對照無缺漏」以本表為對照基準。

| 方法 | fallible | 實際實作於 | v0 狀態 |
|---|---|---|---|
| `new(pack_path, doc_path)` | ✅ | `engine.rs` | **E1 起真的解析 `.colorpack`**：`total_regions` 與 `region_ids` 都從它來，開不了或 hash 不符回 `Pack`。`doc_path` 仍忽略（E3） |
| `attach_surface(handle)` | ✅ | `engine.rs` | **v0 的「永遠 `Ok`」已失效**：E1 起真的建 device／surface／`DocumentResources`（`maximumDrawableCount = 2`，`present_mode = Fifo`），失敗回新的 `EngineError::Surface`。Bridge 顯示錯誤態，**不得 crash**——畫作還在 engine 裡 |
| `resize_surface(w, h, scale)` | — | `engine.rs` | 重設 surface configuration 並重算 fit-to-screen 的 `Transform`；未 attach 時 no-op |
| `detach_surface()` | — | `engine.rs` | **只丟 surface**，device 與文件資源留著（C5） |
| `set_tool(tool)` | — | `ffi.rs` → `app-state` | 真的寫進 `AppState`，emit |
| `pick_color(x, y)` | — | `engine.rs` | 回傳 `AppState.color`，座標忽略（見 C6） |
| `begin_stroke(s)` | — | `engine.rs` → `stroke` / `render` | 起 `StrokeBuilder`、取起筆處的 `active_region_id`、Pass 1 畫第一個 dab。**非筆刷工具時什麼都不做** |
| `end_stroke()` | — | `engine.rs` → `render` / `document` | 重建 `T_wet`（丟掉預測點）→ Pass 2 commit → `document.apply(Op::BrushStroke)`（E1 回 `Effect::None`） |
| `cancel_stroke()` | — | `engine.rs` → `render` | 只清 `T_wet`，**`T_paint` 從未被污染** |
| `append_samples(s)` | — | `engine.rs` → `stroke` / `render` | 真實樣本進 builder、預測點走 `predicted_dabs`（不污染濾波器狀態）；兩者都畫進 `T_wet`。不 emit（stroke 狀態不在 `UiState` 裡） |
| `tap(x, y)` | — | `engine.rs` → `document` | `Transform::canvas_pos` → `region_at` → `document.apply(Op::Fill)` → `RenderContext::fill`；`colored_regions` 由 `document` 投影。畫布外與同色重填都落空。**未 attach 時 no-op**——`region_ids` 住在 `DocumentResources` |
| `undo()` / `redo()` | — | `engine.rs` | no-op ＋ 一次性 log（E3） |
| `render()` | — | `engine.rs` → `render` | 推進擴散動畫 ＋ Pass 3 Composite。infallible：掉 frame 與取不到 drawable 都不是錯誤。無 surface 時什麼都不做 |
| `set_viewport(transform)` | — | `engine.rs` | 覆寫 `Inner::transform`。E1 的 transform 由 attach／resize 自算 fit-to-screen，這支是 E2 縮放平移的入口 |
| `set_mask_mode(mode)` | — | `engine.rs` → `render` | **Debug 專用**，D4 的 A／B 比較（劇本見 `perf-baseline.md`）。一次 `write_buffer`，不重建 pipeline，筆畫進行中切也不掉 frame。不 emit。**決策拍板後與 Swift 端的 toggle 一起移除** |
| `state()` | — | `ffi.rs` | `From<&AppState> for UiState` 投影 |
| `set_state_listener(opt)` | — | `engine.rs` | 單一 listener，後設覆蓋前設；`None` 是 detach 路徑 |
| `save()` | ✅ | `engine.rs` | `Err(NotImplemented { milestone: "E3" })` |
| `export_png()` | ✅ | `engine.rs` | `Err(NotImplemented { milestone: "E1" })` |
| `export_timelapse()` | ✅ | `engine.rs` | `Err(NotImplemented { milestone: "E3" })` |

fallible 的界線是契約的一部分，不因 S0 是 mock 而挪動——`render()` 每 frame 呼叫，
Swift 端不會想每 frame `try`。表上沒有 `makeCanvasView(pickMode:)`，那是 Bridge 的東西（見 C7）。

**Swift 對應**：上表除三項外全部一對一出現在 `apps/ios/EngineBridge/EngineProtocol.swift`
（名稱照 uniffi 的 camelCase，`export_png` → `exportPNG`）。三個記名的例外——
驗收「逐一對照無缺漏」會把它們誤判成缺漏，所以記在這裡：

| FFI | Swift 對應 |
|---|---|
| `new(pack_path, doc_path)` | `RustEngineAdapter.init(packPath:docPath:)`。建構不是抽象的一部分——`MockEngine()` 沒有 pack |
| `set_state_listener(opt)` | 無。它是**實作 `state` 的手段**，不是 Shell 的介面；Shell 要的是「狀態會自己更新」 |
| 無 | `makeCanvasView(pickMode:)`。反向：Bridge 有、FFI 沒有（C7） |

**生成的 Swift 名字**：`RustEngine`（class）＋ `RustEngineProtocol`（uniffi 自動生的）。
後者與手寫的 `EngineProtocol.swift` 是**兩個不同的東西**，前者簽章跟著 Rust 走、
後者是 Shell 依賴的抽象。Rust 端的 Object 不可改名回 `Engine`——那會與手寫的
`EngineProtocol` 在同一個 module 裡 invalid redeclaration（`specs/ffi-contract.md §6`）。

## ③ 語意條款

生成的 Swift 簽章完全看不出來，但 E1 一定會踩到的東西。

| | 條款 |
|---|---|
| C1 | listener 在**呼叫端 thread** 同步觸發；hop 到 main queue 由 Bridge 負責 |
| C2 | 發送 `UiState` 前必先釋放內部鎖（回呼中可安全再呼叫 `RustEngine`）— 已有測試守 |
| C3 | `append_samples` 一 frame 一次；**不得出現 per-touch 呼叫**（`§6` Boundary 1 紅線 2） |
| C4 | `predicted: true` 的樣本只影響當前 frame，不進 oplog |
| C5 | `RustEngine` 生命週期長於 surface；`attach` / `detach` 是正常路徑，重建 `RustEngine` 等於丟失狀態 |
| C6 | `pick_color` 同步回傳，接受抬筆時約一 frame 的 stall（S1 實作時複審） |
| C7 | `makeCanvasView(pickMode:)` 屬 Bridge（包 `CAMetalLayer` 並呼叫 `attach_surface`），不在 FFI——對照表上不算缺漏。`CanvasPickMode` 同理：吸管待命不改引擎狀態，所以不進 `UiState`（S1） |
| C8 | `UiState` 回呼只在投影結果**真的改變**時發送；連續兩次相同狀態只會收到一次 |
| C9 | `tap` / `begin_stroke` / `pick_color` 的座標單位是**螢幕像素**，不是 UIKit point——乘 `contentsScale` 是 Bridge 的責任 |
| C10 | `InputSample.radius == 0` 表示**觸控筆**、`> 0` 表示手指。Pencil 的 `majorRadius` 也有值，所以那個 0 是 Bridge **主動寫入的語意**，不是缺值 |
| C11 | `InputSample.t` 相對**筆畫起點**歸零，單位秒。`UITouch.timestamp` 是 `systemUptime`，直接送 `f32` 只剩 0.03 秒解析度，One-Euro 的 `dt` 會爛掉 |
| C12 | `InputSample.radius` 的單位是**點**，不是螢幕像素——這是 C9 的記名例外。`R_EPS = 4.0`（點）是絕對量，換單位不會被自適應正規化約掉 |
| C13 | `set_mask_mode` 是**排定要移除的方法**，不是 v0 表面的一部分——Shell 不得依賴它。D4 拍板寫回 `prd.md §4.1` 之後，移除它**不算 major bump** |

C8 的兩個後果，Bridge 必須知道：

- **attach listener 不會拿到初始快照。** `Inner::new` 就把初始投影寫進 `last_emitted`，
  所以 `set_state_listener` 自己不觸發回呼——第一個值要 Bridge 呼叫 `state()` 取。
  反過來若不 seed，光是設 listener 就會送出一次假的「狀態變更」。
- **已知 v0 瑕疵**：去重用 `PartialEq` 比 `f32`，而 `NaN != NaN`。Swift 端若把 `NaN`
  送進 `size` / `opacity`，之後每次投影都判定為「變了」，去重失效。不在 v0 修——
  該擋的位置是 Bridge 的輸入驗證，不是在 `PartialEq` 上疊特例。

## ④ semver 規則

判定規則見 `architecture.md §7`「FFI semver 規則」表，不在此複製。

執行機制：`core/engine/ffi-lock.toml` 由 `cargo xtask ios` 產生，
其 diff 就是「FFI 表面變了」的可見信號——看到它動，就要確認本文有沒有跟上。
`cargo xtask verify-generated` 在 CI 比對這個指紋。

## ⑤ 遷移記錄

major bump 必須在此留一條，格式：

```markdown
### v0 → v1（E2，YYYY-MM-DD）
**變更**：<哪個方法或欄位>
**分類**：major｜minor（依 §7 規則：<哪一條>）
**Rust**：<改了什麼>
**Swift Bridge**：<要跟著改什麼>
**驗證**：<怎麼確認兩端一致>
```

### v0 → v0.1（E1，2026-08-03）`EngineError::Surface`

**變更**：`EngineError` 新增 `Surface { detail }` 變體
**分類**：**major**（依 §7：Swift 端對 `EngineError` 的 exhaustive switch 會壞）
**Rust**：`attach_surface` 從「永遠 `Ok`」變成真的會失敗；surface 專屬的錯誤不與資產包錯誤混用——使用者能做的事不同
**Swift Bridge**：`EngineCanvasView.attach()` 的 `assertionFailure` 換成錯誤態顯示，**不 crash**
**驗證**：`ffi-lock.toml` 的 hash 已隨之更動；`cargo xtask verify-generated` 通過

> 記為 v0.1 而非 v1：整個表面仍在 `①` 宣告的 v0 修正窗口內，E2／E3 的複審照舊。

### v0（S0，2026-08-03）初版

無前置版本可遷移。相對 `architecture.md §6` 原始草案的三處修正
（`new` 拆兩段、`subscribe` → `set_state_listener`、fallible 界線）已直接寫回 `§6`。

## ⑥ 有期限的東西

- **`EngineError::NotImplemented` 必須在 E3 結束時從 enum 移除**，這是 v1 的 exit criteria。
  它帶 `milestone` 欄位是為了讓 Swift 端能顯示排程，不是為了讓它長住——
  沒有這條期限它會安靜地活到上架。
