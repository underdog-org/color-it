//
//  LevelTicks.swift
//  ColorApp
//
//  設計稿：`Draw · Tools v2` 的 `Brush Size Level` / `Brush Opacity Level`，
//  五個狀態見 `Draw · Tool States`。
//

import SwiftUI

/// 貼在紙張右緣的直立刻度。九格、離散、可拖曳。
struct LevelTicks: View {
    /// 刻度的語意。差別只有 marker 的長相——大小是實心三角，透明度是半填圓。
    enum Kind {
        case size
        case opacity
    }

    let kind: Kind
    /// `0` 在**最上面**＝最大。往下遞減。
    @Binding var level: Int
    var isEnabled: Bool = true

    static let count = 9
    
    private static let dot: CGFloat = 3
    private static let marker: CGFloat = 14
    private static let gap: CGFloat = 16
    private static let trackWidth: CGFloat = 44
    private static let verticalPadding: CGFloat = 20

    /// 一格佔多高。marker 那一格較高，但拖曳映射用平均值就夠準——
    /// 誤差最多半格，而拖曳本來就會 snap。
    private static var stride: CGFloat {
        (CGFloat(count - 1) * (dot + gap) + marker) / CGFloat(count)
    }

    var body: some View {
        VStack(spacing: Self.gap) {
            ForEach(0..<Self.count, id: \.self) { index in
                tick(at: index)
            }
        }
        .frame(width: Self.trackWidth)
        .padding(.vertical, Self.verticalPadding)
        .background(
            // `#FCFBF9`：比 `paper` 再白一點的軌道底，讓它在紙張與畫布底色上都看得出邊界。
            Capsule().fill(Color(.sRGB, red: 0.988, green: 0.984, blue: 0.976))
                .shadow(color: DS.Color.ink.opacity(0.22), radius: 10, x: -7, y: 0)
        )
        .opacity(isEnabled ? 1 : 0.4)
        .allowsHitTesting(isEnabled)
        .contentShape(Capsule())
        .gesture(drag)
        .accessibilityElement()
        .accessibilityLabel(kind == .size ? "筆刷大小" : "不透明度")
        .accessibilityValue("\(Self.count - level) / \(Self.count)")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: level = max(0, level - 1)
            case .decrement: level = min(Self.count - 1, level + 1)
            @unknown default: break
            }
        }
    }

    @ViewBuilder
    private func tick(at index: Int) -> some View {
        if index == level {
            switch kind {
            case .size:
                // 設計稿是 `polygon`（三角）指向紙張，把視線帶回畫布。
                Triangle()
                    .fill(DS.Color.ink)
                    .frame(width: Self.marker, height: Self.marker)
                    .rotationEffect(.degrees(-90))
            case .opacity:
                Image(systemName: "circle.lefthalf.filled")
                    .font(.system(size: Self.marker + 1))
                    .foregroundStyle(DS.Color.ink)
            }
        } else {
            Circle()
                .fill(DS.Color.ink.opacity(0.28))
                .frame(width: Self.dot, height: Self.dot)
        }
    }

    /// 拖曳把 y 座標換算成格號。`height` 從內容推得，不量 geometry——
    /// 這個元件的高度完全由格數決定，量 geometry 只會多一層 `GeometryReader`。
    private var drag: some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                let usable = CGFloat(Self.count) * Self.stride
                let y = value.location.y - Self.verticalPadding
                let index = Int((y / usable * CGFloat(Self.count)).rounded(.down))
                let clamped = min(max(index, 0), Self.count - 1)
                if clamped != level { level = clamped }
            }
    }
}

/// SwiftUI 沒有內建三角形。只給 `LevelTicks` 用，所以 `fileprivate`。
private struct Triangle: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.midX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

#Preview("兩排刻度 · 可用／停用") {
    @Previewable @State var size = 2
    @Previewable @State var opacity = 4

    HStack(spacing: 40) {
        VStack(spacing: 12) {
            LevelTicks(kind: .size, level: $size)
            Text("可用").font(DS.Typography.caption).foregroundStyle(DS.Color.muted)
        }
        VStack(spacing: 12) {
            LevelTicks(kind: .opacity, level: $opacity)
            Text("可用").font(DS.Typography.caption).foregroundStyle(DS.Color.muted)
        }
        VStack(spacing: 12) {
            LevelTicks(kind: .size, level: $size, isEnabled: false)
            Text("停用").font(DS.Typography.caption).foregroundStyle(DS.Color.muted)
        }
    }
    .padding(40)
    .background(DS.Color.bg)
}
