# 介面缺陷清單

S0 預告、S1 驗收要求的那份清單（`roadmap/S1.md`：「從 Mock 切換到 `RustEngine` 時 Shell 端
程式碼零修改，若需修改，記錄為介面缺陷清單」）。

**收錄標準**：Shell 為了繞過 FFI 表面的缺口而多寫的東西。純粹的 UI 決策不算。
修正窗口見 `EngineProtocol` 檔頭與 `contracts.md ①`。

| # | 缺陷 | 繞法 | 修正窗口 |
|---|---|---|---|
| 1 | `Tool.eraser(size:)` 沒有 `color` 欄位，切到橡皮擦後 Shell 從 `UiState` 讀不回當前色（Rust 端與 `MockEngine` 內部都留著） | Shell 自己保存一份當前色（`CanvasToolState.color`） | E3 |

## 1. `UiState.tool` 投影不出跨工具共用的顏色

`Tool` 是「工具 ＋ 該工具的參數」的合體，而 `AppState` 裡的 `color` 是**跨工具共用**的。
橡皮擦不吃顏色，所以 `Tool.eraser` 不帶它——投影因此有損。

後果：Shell 必須自己維護當前色，否則「筆刷 → 橡皮擦 → 筆刷」會把顏色弄丟。
色環與色票列的「當前色」標示也只能讀 Shell 那份，不能讀 `state`。

修正方向（E3 決定）：把 `color` 提到 `UiState` 上，與 `tool` 平行；或讓 `Tool.eraser`
也帶 `color`。前者比較誠實——它本來就不屬於任何單一工具。
