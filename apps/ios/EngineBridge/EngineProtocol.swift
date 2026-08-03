//
//  EngineProtocol.swift
//  EngineBridge

import UIKit

/// App Shell 對引擎的**唯一**依賴。逐項對照基準是 `docs/contracts.md ②`，不是肉眼比檔案。
public protocol EngineProtocol: AnyObject {
    var state: UiState { get }

    // MARK: Surface 生命週期
    func attachSurface(_ handle: SurfaceHandle) throws
    func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float)
    func detachSurface()

    // MARK: 工具與顏色

    func setTool(_ tool: Tool)
    func pickColor(x: Float, y: Float) -> Rgba

    // MARK: 輸入
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

    /// **不是正式 UI**：D4 之後留一個開關在畫面上會被誤觸，所以它只出現在 Debug
    func setMaskMode(_ mode: MaskMode)

    // MARK: 持久化與匯出（v0 全數丟 `EngineError.NotImplemented`）

    func save() throws
    func exportPNG() throws -> Data
    func exportTimelapse() throws -> Data

    // MARK: Bridge 專屬

    func makeCanvasView(pickMode: CanvasPickMode) -> UIView
}
