//
//  MockEngine.swift
//  EngineBridge
//
//  契約版本：**v0**，跟著 `EngineProtocol` 一起在 E2 / E3 修正。
//

import Observation
import UIKit

/// 不是「隨便回點東西」——它是 S1 換引擎時 Shell 零修改的保險。
///
/// 行為逐條複製 `docs/contracts.md ②` 的 v0 狀態欄，且**內部形狀鏡射 `core/app-state` 的
/// `AppState`**，而不是直接存一個 `Tool` enum。理由有兩個，都是差分測試會抓到的：
///
/// - `pickColor` 要回傳**跨工具共用**的目前顏色。`Tool.eraser` 沒有顏色欄位，
///   存 enum 就會在切到橡皮擦之後把顏色弄丟，而 Rust 端是留著的。
/// - `Tool.apply_to` 只寫入該工具帶的欄位（橡皮擦不動 `color` / `opacity`，
///   油漆桶不動 `size` / `opacity`）。存 enum 表達不出這件事。
///
/// 初始值也必須逐欄位等於 `AppState::default()`——差分測試比對的是**整串** `UiState`，
/// 包含第一個。
@Observable
public final class MockEngine: EngineProtocol {
    /// `core/engine/src/inner.rs` 的 `MOCK_TOTAL_REGIONS`。M1 有 `.colorpack` 之後改成從資產包讀。
    private static let mockTotalRegions: UInt32 = 24

    /// `AppState::default()` 的 `color`。
    private static let defaultColor = Rgba(r: 0x1a, g: 0x1a, b: 0x1a, a: 0xff)

    public private(set) var state: UiState

    // MARK: `AppState` 的鏡射

    private var toolKind: ToolKind = .brush(.softRound)
    private var color: Rgba = MockEngine.defaultColor
    private var size: Float = 24.0
    private var opacity: Float?
    private var coloredRegions: UInt32 = 0

    /// stroke 狀態機**不在 `UiState`** 裡，所以維護它不會 emit。
    private var strokeActive = false

    public init() {
        state = UiState(
            tool: .brush(
                preset: .softRound,
                color: MockEngine.defaultColor,
                size: 24.0,
                opacity: nil
            ),
            canUndo: false,
            canRedo: false,
            progress: Progress(colored: 0, total: MockEngine.mockTotalRegions)
        )
    }

    /// `Tool.apply_to` 在 Swift 這側的對應：橡皮擦不動 `color`，油漆桶不動 `size`。
    private enum ToolKind {
        case brush(BrushId)
        case eraser
        case bucket
    }

    /// `From<&AppState> for UiState` 的對應。
    private var projected: UiState {
        let tool: Tool = switch toolKind {
        case .brush(let preset):
            .brush(preset: preset, color: color, size: size, opacity: opacity)
        case .eraser:
            .eraser(size: size)
        case .bucket:
            .bucket(color: color)
        }
        return UiState(
            tool: tool,
            canUndo: false,
            canRedo: false,
            progress: Progress(colored: coloredRegions, total: MockEngine.mockTotalRegions)
        )
    }

    /// **唯一**的狀態變更入口，形狀對齊 `RustEngine::mutate`。
    ///
    /// `docs/contracts.md` C8：只在投影**真的改變**時才更新。`@Observable` 對相同的值
    /// 一樣會通知 observer，所以這個 `!=` 不是最佳化，是契約。
    private func mutate(_ body: (MockEngine) -> Void) {
        body(self)
        let next = projected
        if next != state {
            state = next
        }
    }

    // MARK: Surface 生命週期（記都不記——S0 的 Mock 沒有 surface 概念）

    public func attachSurface(_ handle: SurfaceHandle) throws {}
    public func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float) {}
    public func detachSurface() {}

    // MARK: 工具與顏色

    public func setTool(_ tool: Tool) {
        mutate { mock in
            switch tool {
            case .brush(let preset, let color, let size, let opacity):
                mock.toolKind = .brush(preset)
                mock.color = color
                mock.size = size
                mock.opacity = opacity
            case .eraser(let size):
                mock.toolKind = .eraser
                mock.size = size
            case .bucket(let color):
                mock.toolKind = .bucket
                mock.color = color
            }
        }
    }

    /// 座標忽略（`docs/contracts.md` C6／v0 狀態欄）。回傳的是共用顏色，不是「目前工具的顏色」。
    public func pickColor(x: Float, y: Float) -> Rgba {
        color
    }

    // MARK: 輸入

    public func beginStroke(_ s: InputSample) {
        mutate { $0.strokeActive = true }
    }

    /// 樣本丟棄；不 emit（stroke 狀態不在 `UiState` 裡）。
    public func appendSamples(_ s: [InputSample]) {
        mutate { _ in }
    }

    public func endStroke() {
        mutate { $0.strokeActive = false }
    }

    public func cancelStroke() {
        mutate { $0.strokeActive = false }
    }

    /// 推進 `colored`，飽和於 `mockTotalRegions`——對應 `AppState::mark_region_colored`。
    public func tap(x: Float, y: Float) {
        mutate { mock in
            guard mock.coloredRegions < MockEngine.mockTotalRegions else { return }
            mock.coloredRegions += 1
        }
    }

    // MARK: 歷史與渲染（v0 全 no-op）

    public func undo() {}
    public func redo() {}
    public func render() {}
    public func setViewport(_ transform: Transform) {}

    // MARK: 持久化與匯出
    //
    // 三個都丟同一種錯，`milestone` 與 Rust 端一致——Shell 顯示排程時兩個實作不能有落差。

    public func save() throws {
        throw EngineError.NotImplemented(feature: "save", milestone: "E3")
    }

    public func exportPNG() throws -> Data {
        throw EngineError.NotImplemented(feature: "export_png", milestone: "E1")
    }

    public func exportTimelapse() throws -> Data {
        throw EngineError.NotImplemented(feature: "export_timelapse", milestone: "E3")
    }

    // MARK: Bridge 專屬

    /// 靜態圖，不是渲染。素材由 App target 提供（`ColorApp/Resources/mock-lineart.png`），
    /// 所以查的是 `Bundle.main`；unit test bundle 裡找不到是正常的，退回純色底。
    public func makeCanvasView() -> UIView {
        let view = UIImageView(image: UIImage(named: "mock-lineart"))
        view.contentMode = .scaleAspectFit
        view.backgroundColor = .systemBackground
        return view
    }
}
