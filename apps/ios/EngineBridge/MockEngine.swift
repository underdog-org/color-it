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
    /// `AppState::default()` 的 `color`。
    private static let defaultColor = Rgba(r: 0x1a, g: 0x1a, b: 0x1a, a: 0xff)

    /// Rust 端從 `.colorpack` 讀 region 數（E1 起），Mock 沒有 pack——所以由呼叫端給。
    /// 預設值只是「一張圖大概有幾個區」，差分測試會傳 fixture 的真實值進來。
    private let totalRegions: UInt32

    public private(set) var state: UiState

    // MARK: `AppState` 的鏡射

    private var toolKind: ToolKind = .brush(.softRound)
    private var color: Rgba = MockEngine.defaultColor
    private var size: Float = 24.0
    private var opacity: Float?
    private var coloredRegions: UInt32 = 0

    /// stroke 狀態機**不在 `UiState`** 裡，所以維護它不會 emit。
    private var strokeActive = false

    /// `tap` 從 E1 起需要 GPU 上的 `region_ids`（`E1-wgpu §5.1`），沒有 surface
    /// 就落空——Mock 沒有 region 的概念，但**這條時序必須一致**，否則差分測試
    /// 會在「還沒 attach 就 tap」的情境下分歧。
    private var attached = false

    public init(totalRegions: UInt32 = 24) {
        self.totalRegions = totalRegions
        state = UiState(
            tool: .brush(
                preset: .softRound,
                color: MockEngine.defaultColor,
                size: 24.0,
                opacity: nil
            ),
            canUndo: false,
            canRedo: false,
            progress: Progress(colored: 0, total: totalRegions)
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
            progress: Progress(colored: coloredRegions, total: totalRegions)
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

    // MARK: Surface 生命週期
    //
    // 只記「有沒有」，不碰 GPU。記這一個 bool 的理由見 `attached` 的註解。

    public func attachSurface(_ handle: SurfaceHandle) throws {
        mutate { $0.attached = true }
    }

    public func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float) {}

    public func detachSurface() {
        mutate { $0.attached = false }
    }

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

    /// 推進 `colored`，飽和於 `totalRegions`。
    ///
    /// 沒有 surface 就落空——Rust 端的理由是 `region_ids` 還沒配置（`E1-bucket §4.3`），
    /// Mock 沒有那份資料，但要的是**同一條時序**。
    public func tap(x: Float, y: Float) {
        mutate { mock in
            guard mock.attached, mock.coloredRegions < mock.totalRegions else { return }
            mock.coloredRegions += 1
        }
    }

    // MARK: 歷史與渲染（v0 全 no-op）

    public func undo() {}
    public func redo() {}
    public func render() {}
    public func setViewport(_ transform: Transform) {}

    // MARK: Debug

    /// Mock 沒有遮罩可切。記下來只為了讓差分測試看得出「兩邊都不 emit」——
    /// mask mode 不在 `UiState` 裡，切它不該產生任何狀態回呼。
    public private(set) var maskMode: MaskMode = .strict

    public func setMaskMode(_ mode: MaskMode) {
        mutate { $0.maskMode = mode }
    }

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
