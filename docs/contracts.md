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
| `new(pack_path, doc_path)` | ✅ | `engine.rs` | 只檢查 `pack_path` 存在，不解析 `.colorpack`（M1）；`doc_path` 忽略 |
| `attach_surface(handle)` | ✅ | `engine.rs` | 記下 handle，不碰 GPU；永遠 `Ok` |
| `resize_surface(w, h, scale)` | — | `engine.rs` | 更新已記下的 handle；未 attach 時 no-op |
| `detach_surface()` | — | `engine.rs` | 清掉 handle |
| `set_tool(tool)` | — | `ffi.rs` → `app-state` | 真的寫進 `AppState`，emit |
| `pick_color(x, y)` | — | `engine.rs` | 回傳 `AppState.color`，座標忽略（見 C6） |
| `begin_stroke(s)` / `end_stroke()` / `cancel_stroke()` | — | `engine.rs` | 只維護 `Inner::stroke_active`，樣本丟棄 |
| `append_samples(s)` | — | `engine.rs` | 樣本丟棄；不 emit（stroke 狀態不在 `UiState` 裡） |
| `tap(x, y)` | — | `app-state` | 推進 `colored_regions`，到 `total_regions`（S0 固定 24）飽和 |
| `undo()` / `redo()` | — | `engine.rs` | no-op ＋ 一次性 log（E3） |
| `render()` | — | `engine.rs` | no-op ＋ 一次性 log（E1） |
| `set_viewport(transform)` | — | `engine.rs` | no-op ＋ 一次性 log；`Transform` 丟棄，`Inner` 沒有 viewport 欄位（E1） |
| `state()` | — | `ffi.rs` | `From<&AppState> for UiState` 投影 |
| `set_state_listener(opt)` | — | `engine.rs` | 單一 listener，後設覆蓋前設；`None` 是 detach 路徑 |
| `save()` | ✅ | `engine.rs` | `Err(NotImplemented { milestone: "E3" })` |
| `export_png()` | ✅ | `engine.rs` | `Err(NotImplemented { milestone: "E1" })` |
| `export_timelapse()` | ✅ | `engine.rs` | `Err(NotImplemented { milestone: "E3" })` |

fallible 的界線是契約的一部分，不因 S0 是 mock 而挪動——`render()` 每 frame 呼叫，
Swift 端不會想每 frame `try`。表上沒有 `makeCanvasView()`，那是 Bridge 的東西（見 C7）。

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
| C7 | `makeCanvasView()` 屬 Bridge（包 `MTKView` 並呼叫 `attach_surface`），不在 FFI——對照表上不算缺漏 |
| C8 | `UiState` 回呼只在投影結果**真的改變**時發送；連續兩次相同狀態只會收到一次 |

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

### v0（S0，2026-08-03）初版

無前置版本可遷移。相對 `architecture.md §6` 原始草案的三處修正
（`new` 拆兩段、`subscribe` → `set_state_listener`、fallible 界線）已直接寫回 `§6`。

## ⑥ 有期限的東西

- **`EngineError::NotImplemented` 必須在 E3 結束時從 enum 移除**，這是 v1 的 exit criteria。
  它帶 `milestone` 欄位是為了讓 Swift 端能顯示排程，不是為了讓它長住——
  沒有這條期限它會安靜地活到上架。
