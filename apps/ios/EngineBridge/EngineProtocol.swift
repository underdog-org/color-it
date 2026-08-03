//
//  EngineProtocol.swift
//  EngineBridge
//
//  契約版本：**v0**。整個 FFI 表面標記為 v0，預告兩次修正窗口（`docs/contracts.md ①`）：
//
//  - **E2**（工具集完整）：`Tool` 與 `BrushId` 可能增值，`opacity` 的語意複審
//  - **E3**（Undo／持久化接上）：`canUndo` / `canRedo` 由據實回報取代 mock 的常數 `false`，
//    `save` / `exportPNG` / `exportTimelapse` 真的實作
//
//  在那之前 Shell 不該假設任何欄位是最終形狀。
//

import UIKit

/// App Shell 對引擎的**唯一**依賴。逐項對照基準是 `docs/contracts.md ②`，不是肉眼比檔案。
///
/// 型別全部來自 uniffi 生成的 `colorlull_engine.swift`，由本 module 轉出——
/// Bridge **不重新定義任何 DTO**，那會違反「一份契約只能存在一次」（`architecture.md §7`）。
///
/// 三個記名的例外（驗收「逐一對照無缺漏」會把它們誤判成缺漏，所以寫在這裡）：
///
/// | FFI | 為什麼不在 protocol |
/// |---|---|
/// | `new(pack_path, doc_path)` | 建構不是抽象的一部分——`MockEngine()` 沒有 pack。對應 `RustEngineAdapter.init(packPath:docPath:)` |
/// | `set_state_listener(opt)` | 它是**實作 `state` 的手段**，不是 Shell 的介面。Shell 要的是「狀態會自己更新」 |
/// | `makeCanvasView(pickMode:)` | 反向：Bridge 有、FFI 沒有（`docs/contracts.md` C7 已授權） |
public protocol EngineProtocol: AnyObject {
    /// 對應 FFI 的 `state()`。兩個實作都是 `@Observable`，所以 view 經
    /// `any EngineProtocol` 讀這個欄位就會被 observation tracking 記錄，
    /// 不需要 Combine——`architecture.md §10.1` 的 `AnyPublisher` 草案是 Combine 時代的寫法。
    var state: UiState { get }

    // MARK: Surface 生命週期
    //
    // 留在 protocol 是驗收明文要求，但 **Shell 一行都不呼叫**——
    // 只有 `EngineCanvasView` 呼叫它們（`docs/contracts.md` C5：attach／detach 是正常路徑）。

    func attachSurface(_ handle: SurfaceHandle) throws
    func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float)
    func detachSurface()

    // MARK: 工具與顏色

    func setTool(_ tool: Tool)
    func pickColor(x: Float, y: Float) -> Rgba

    // MARK: 輸入
    //
    // `appendSamples` 走批次：一 frame 一次，**不得出現 per-touch 呼叫**
    // （`docs/contracts.md` C3／`architecture.md §6` Boundary 1 紅線 2）。

    func beginStroke(_ s: InputSample)
    func appendSamples(_ s: [InputSample])
    func endStroke()
    func cancelStroke()
    func tap(x: Float, y: Float)

    // MARK: 歷史與渲染

    func undo()
    func redo()

    /// 由 FrameDriver 每 frame 呼叫，所以刻意 infallible——Shell 不會想每 frame `try`。
    func render()
    func setViewport(_ transform: Transform)

    // MARK: Debug（D4 拍板後整組移除）

    /// Mask Mode A／B 即時切換（`docs/specs/E1-perf.md §5`）。
    ///
    /// **不是正式 UI**：D4 之後留一個開關在畫面上會被誤觸，所以它只出現在 Debug
    /// 建置的選單裡。決策寫回 `prd.md §4.1` 之後，這支方法與兩個實作一起刪掉。
    func setMaskMode(_ mode: MaskMode)

    // MARK: 持久化與匯出（v0 全數丟 `EngineError.NotImplemented`）

    func save() throws
    func exportPNG() throws -> Data
    func exportTimelapse() throws -> Data

    // MARK: Bridge 專屬

    /// `docs/contracts.md` C7：不在 FFI 表面上，對照表上不算缺漏。
    func makeCanvasView(pickMode: CanvasPickMode) -> UIView
}
