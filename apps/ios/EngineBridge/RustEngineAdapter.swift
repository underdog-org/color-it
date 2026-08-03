//
//  RustEngineAdapter.swift
//  EngineBridge
//
//  契約版本：**v0**，跟著 `EngineProtocol` 一起在 E2 / E3 修正。
//

import Observation
import UIKit

/// 把 uniffi 生成的 `RustEngine` 包成 `EngineProtocol`。
///
/// 這個檔案是 `docs/contracts.md ③` 語意條款在 Swift 側的落點——生成的簽章
/// 完全看不出這些事，但少做任何一項都會在 E1 咬人。
@Observable
public final class RustEngineAdapter: EngineProtocol {
    public private(set) var state: UiState

    @ObservationIgnored private let engine: RustEngine
    @ObservationIgnored private let listenerBox: ListenerBox

    /// 對應 FFI 的 `new(pack_path, doc_path)`。建構不在 `EngineProtocol` 裡——
    /// `MockEngine()` 沒有 pack，那不是同一種抽象。
    public init(packPath: String, docPath: String? = nil) throws {
        engine = try RustEngine(packPath: packPath, docPath: docPath)
        listenerBox = ListenerBox()

        // `docs/contracts.md` C8：`Inner::new` 已把初始投影寫進 `last_emitted`，
        // 所以設 listener **不會**收到第一個值——要自己 seed。
        // 順序固定：先讀 `state()` 再設 listener。反過來會漏掉這中間發生的變更。
        state = engine.state()
        listenerBox.owner = self
        engine.setStateListener(listener: listenerBox)
    }

    deinit {
        // `docs/contracts.md` C5 說 `Option` 給了明確的 detach 路徑，這裡就是那條路徑存在的理由：
        // Rust 端持有 `Arc<dyn StateListener>`，不解掉就是一個跨 FFI 的參照環。
        engine.setStateListener(listener: nil)
        listenerBox.owner = nil
    }

    /// 反指回 Adapter 的 **weak** box。
    ///
    /// 讓 `RustEngineAdapter` 自己 conform `StateListener` 會是：
    /// Adapter → `RustEngine` →（Rust 的 `Arc`）→ Adapter。那個環跨 FFI，ARC 看不見，
    /// `deinit` 永遠不會跑。
    ///
    /// uniffi 生成的 `StateListener` 是 `Sendable`（回呼可能來自任何 thread），
    /// 而 `owner` 是可變的 weak 參照，所以要自己上鎖，不能靠編譯器推導。
    private final class ListenerBox: StateListener, @unchecked Sendable {
        private let lock = NSLock()
        private weak var _owner: RustEngineAdapter?

        var owner: RustEngineAdapter? {
            get { lock.withLock { _owner } }
            set { lock.withLock { _owner = newValue } }
        }

        /// `docs/contracts.md` C1：在**呼叫端 thread** 同步觸發，hop 到 main 是 Bridge 的責任。
        func onState(state: UiState) {
            guard let owner else { return }

            // 無條件 `DispatchQueue.main.async` 會讓 `tap()` → 進度更新慢一個 runloop turn，
            // 而 Shell 幾乎都在 main 上呼叫。所以已經在 main 就直接賦值。
            if Thread.isMainThread {
                owner.state = state
            } else {
                DispatchQueue.main.async { owner.state = state }
            }
        }
    }

    // MARK: Surface 生命週期

    public func attachSurface(_ handle: SurfaceHandle) throws {
        try engine.attachSurface(handle: handle)
    }

    public func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float) {
        engine.resizeSurface(widthPx: widthPx, heightPx: heightPx, scale: scale)
    }

    public func detachSurface() {
        engine.detachSurface()
    }

    // MARK: 工具與顏色

    /// `docs/contracts.md ③` 點名的 **NaN 防線**。
    ///
    /// Rust 端的 C8 去重用 `PartialEq` 比 `f32`，而 `NaN != NaN`——一旦 `NaN` 進到
    /// `size` / `opacity`，之後每次投影都判定為「變了」，去重就永久失效。文件明說
    /// 該擋的位置是 Bridge 的輸入驗證，不是在 `PartialEq` 上疊特例，所以擋在這裡。
    public func setTool(_ tool: Tool) {
        engine.setTool(tool: sanitized(tool))
    }

    private func sanitized(_ tool: Tool) -> Tool {
        switch tool {
        case .brush(let preset, let color, let size, let opacity):
            .brush(
                preset: preset,
                color: color,
                size: finite(size, fallback: 24.0),
                opacity: opacity.map { finite($0, fallback: 1.0) }
            )
        case .eraser(let size):
            .eraser(size: finite(size, fallback: 24.0))
        case .bucket(let color):
            .bucket(color: color)
        }
    }

    private func finite(_ value: Float, fallback: Float) -> Float {
        value.isFinite ? value : fallback
    }

    public func pickColor(x: Float, y: Float) -> Rgba {
        engine.pickColor(x: x, y: y)
    }

    // MARK: 輸入

    public func beginStroke(_ s: InputSample) {
        engine.beginStroke(s: s)
    }

    /// C3：一 frame 一次。呼叫端是 FrameDriver，不是 touch handler。
    public func appendSamples(_ s: [InputSample]) {
        engine.appendSamples(s: s)
    }

    public func endStroke() {
        engine.endStroke()
    }

    public func cancelStroke() {
        engine.cancelStroke()
    }

    public func tap(x: Float, y: Float) {
        engine.tap(x: x, y: y)
    }

    // MARK: 歷史與渲染

    public func undo() { engine.undo() }
    public func redo() { engine.redo() }
    public func render() { engine.render() }

    public func setViewport(_ transform: Transform) {
        engine.setViewport(transform: transform)
    }

    // MARK: Debug

    /// 一次 `write_buffer`，不重建 pipeline（`E1-wgpu §7.1`）——所以筆畫進行中切
    /// 也不會掉 frame，這正是 D4 要的「同一筆兩種模式對照」。
    public func setMaskMode(_ mode: MaskMode) {
        engine.setMaskMode(mode: mode)
    }

    // MARK: 持久化與匯出

    public func save() throws { try engine.save() }
    public func exportPNG() throws -> Data { try engine.exportPng() }
    public func exportTimelapse() throws -> Data { try engine.exportTimelapse() }

    // MARK: Bridge 專屬

    public func makeCanvasView() -> UIView {
        EngineCanvasView(engine: self)
    }
}
