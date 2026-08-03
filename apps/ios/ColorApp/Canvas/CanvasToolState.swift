//
//  CanvasToolState.swift
//  ColorApp
//
//  `docs/specs/S1-ios-ui.md §4.1`：`Tool` 是「工具 ＋ 該工具的參數」的合體，
//  所以 Shell 得自己留一份 UI 選擇狀態，每次切換組出完整的 `Tool` 再 `setTool`。
//

import EngineBridge
import Observation
import SwiftUI

/// 工具列的 UI 選擇狀態。**不是真相的另一份副本**——引擎那側的 `UiState.tool`
/// 仍然是唯一權威，這裡存的是「使用者選了什麼」，包含 `Tool` 投影不出來的部分。
@Observable
final class CanvasToolState {
    enum Kind: Hashable {
        case brush
        case eraser
        case bucket
    }

    var kind: Kind = .brush
    var preset: BrushId = .softRound
    /// 筆刷與油漆桶共用。橡皮擦不吃顏色，但切回來時要拿得到。
    var color: Rgba = Rgba(r: 0x1A, g: 0x1A, b: 0x1A, a: 0xFF)

    var sizeLevel: Int = 3
    var opacityLevel: Int = 0

    var isBrushLayerExpanded = false

    // MARK: - 檔位 → 引擎的值

    /// 九格對應的筆刷直徑。等比而不是等差——小筆刷差 2pt 看得出來，大筆刷差 2pt 看不出來。
    static let sizes: [Float] = [64, 48, 36, 24, 18, 14, 10, 7, 4]

    var size: Float { Self.sizes[min(max(sizeLevel, 0), Self.sizes.count - 1)] }

    var opacity: Float? {
        opacityLevel == 0 ? nil : 1.0 - Float(opacityLevel) * 0.1
    }

    /// 油漆桶不吃大小與透明度，所以兩排刻度整體停用。
    var levelsEnabled: Bool { kind != .bucket }

    // MARK: - 組出 `Tool`

    var tool: Tool {
        switch kind {
        case .brush: .brush(preset: preset, color: color, size: size, opacity: opacity)
        case .eraser: .eraser(size: size)
        case .bucket: .bucket(color: color)
        }
    }
}

/// v1 的五支 preset，順序就是展開層由細到粗的順序（`Draw · Tool States`／筆刷展開層）。
///
/// **這份清單是與 E2 的唯一耦合點**（`S1-ios-ui.md §4.3`）：展開層吃它渲染，不寫死格數。
/// E2 若把水彩砍掉（`E2.md` 的時間盒退路），這裡少一個元素，版面與程式都不動。
enum BrushCatalog {
    static let presets: [BrushId] = [.softRound, .marker, .crayon, .airbrush, .watercolor]

    /// 展開層每格中央那顆點的直徑與不透明度。它畫的是**筆觸的感覺**，不是筆刷大小——
    /// 大小由右緣刻度決定，兩者刻意不共用數字。
    static func dot(for preset: BrushId) -> (diameter: CGFloat, opacity: Double) {
        switch preset {
        case .softRound: (10, 1)
        case .marker: (14, 1)
        case .crayon: (18, 1)
        case .airbrush: (22, 0.55)
        case .watercolor: (26, 0.3)
        }
    }

    static func name(for preset: BrushId) -> String {
        switch preset {
        case .softRound: "軟圓"
        case .marker: "麥克筆"
        case .crayon: "蠟筆"
        case .airbrush: "噴槍"
        case .watercolor: "水彩"
        }
    }
}

extension Color {
    /// `Rgba` 是 sRGB 8-bit，`Color(.sRGB, …)` 也是——不需要色彩空間轉換。
    init(_ rgba: Rgba) {
        self.init(
            .sRGB,
            red: Double(rgba.r) / 255,
            green: Double(rgba.g) / 255,
            blue: Double(rgba.b) / 255,
            opacity: Double(rgba.a) / 255
        )
    }
}
