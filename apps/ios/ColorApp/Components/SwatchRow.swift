//
//  SwatchRow.swift
//  ColorApp
//
//  設計稿：`Swatch Row` 元件（`Draw · Tools v2` 等三張都用它）。
//

import EngineBridge
import SwiftUI
struct SwatchRow: View {
    /// 取自線稿的建議色（`manifest.palette`）。S1 對著假資料，來源是 fixture。
    let suggested: [Rgba]
    /// 最近使用。最新的排最前面。
    let recent: [Rgba]
    let current: Rgba
    let onSelect: (Rgba) -> Void

    /// 各四格。少於四格時留空位而不是擠在一起——列寬固定，避免分隔線左右跳動。
    private static let slots = 4

    var body: some View {
        HStack(spacing: 0) {
            group(label: "建議", colors: suggested)
            Spacer(minLength: DS.Space.space2)
            Rectangle()
                .fill(DS.Color.line)
                .frame(width: 1, height: 20)
            Spacer(minLength: DS.Space.space2)
            group(label: "最近", colors: recent)
        }
        .frame(height: 36)
    }

    private func group(label: String, colors: [Rgba]) -> some View {
        HStack(spacing: DS.Space.space2) {
            Text(label)
                .font(DS.Typography.caption)
                .fontWeight(.semibold)
                .kerning(0.5)
                .foregroundStyle(DS.Color.muted)
            HStack(spacing: 2) {
                ForEach(0..<Self.slots, id: \.self) { index in
                    if index < colors.count {
                        swatch(colors[index])
                    } else {
                        Color.clear.frame(width: 30, height: 30)
                    }
                }
            }
        }
    }

    private func swatch(_ rgba: Rgba) -> some View {
        let isCurrent = rgba == current
        return Button {
            onSelect(rgba)
        } label: {
            Circle()
                .fill(Color(rgba))
                .frame(width: 22, height: 22)
                .overlay(Circle().strokeBorder(DS.Color.ink.opacity(0.08), lineWidth: 1))
                .frame(width: 30, height: 30)
                .overlay(
                    Circle().strokeBorder(
                        isCurrent ? DS.Color.ink : .clear, lineWidth: 1.5
                    )
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text("色票"))
        .accessibilityAddTraits(isCurrent ? [.isSelected] : [])
    }
}

#Preview("Swatch Row") {
    let suggested = [
        Rgba(r: 0xE8, g: 0xA0, b: 0x2E, a: 0xFF),
        Rgba(r: 0xF4, g: 0x61, b: 0x4C, a: 0xFF),
        Rgba(r: 0x3F, g: 0xA8, b: 0x9B, a: 0xFF),
        Rgba(r: 0xEE, g: 0x9B, b: 0xB4, a: 0xFF),
    ]
    let recent = [
        Rgba(r: 0x6C, g: 0x92, b: 0xC6, a: 0xFF),
        Rgba(r: 0x7B, g: 0x85, b: 0xE0, a: 0xFF),
        Rgba(r: 0xF2, g: 0xA9, b: 0x3B, a: 0xFF),
        Rgba(r: 0xC9, g: 0x70, b: 0x4E, a: 0xFF),
    ]

    VStack(spacing: DS.Space.space6) {
        SwatchRow(
            suggested: suggested, recent: recent, current: recent[0], onSelect: { _ in }
        )
        SwatchRow(
            suggested: suggested, recent: Array(recent.prefix(2)),
            current: suggested[1], onSelect: { _ in }
        )
    }
    .frame(width: 344)
    .padding(DS.Space.space5)
    .background(DS.Color.bg)
}
