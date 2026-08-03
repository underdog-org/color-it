//
//  ToolBar.swift
//  ColorApp
//
//  設計稿：`Tool Bar` 元件、`Draw · Tool States` 的五格。
//

import EngineBridge
import SwiftUI

/// 畫布下緣那條 64pt 的橫列：左邊三個工具，右邊 Undo／Redo。
///
/// **Undo／Redo 為什麼在這裡而不在 Top Bar**（`Draw · Design Notes`）：Undo 是高頻操作，
/// 放 Top Bar 等於要求單手時伸到螢幕頂端。放在同一條橫列的右端，與三工具同一個拇指弧內。
///
/// **無可復原步驟時只降階為停用，不隱藏**——避免按鈕位置跳動。
struct ToolBar: View {
    let state: CanvasToolState
    let canUndo: Bool
    let canRedo: Bool
    let onSelect: (CanvasToolState.Kind) -> Void
    let onUndo: () -> Void
    let onRedo: () -> Void

    private static let button: CGFloat = 48
    private static let icon: CGFloat = 21

    var body: some View {
        HStack {
            pill {
                toolButton(.brush, systemImage: "paintbrush.pointed.fill", label: "筆刷")
                toolButton(.eraser, systemImage: "eraser.fill", label: "橡皮擦")
                // SF Symbols 沒有油漆桶。`drop.fill` 是最接近「把這一區填滿」的既有符號，
                // 與設計稿的 lucide `paint-bucket` 不同形但同義。
                toolButton(.bucket, systemImage: "drop.fill", label: "油漆桶")
            }

            Spacer(minLength: 0)

            pill {
                historyButton(
                    systemImage: "arrow.uturn.backward",
                    label: "復原", isEnabled: canUndo, action: onUndo
                )
                historyButton(
                    systemImage: "arrow.uturn.forward",
                    label: "重做", isEnabled: canRedo, action: onRedo
                )
            }
        }
        .padding(.horizontal, DS.Space.space4)
        .frame(height: 64)
    }

    // MARK: -

    /// 兩組按鈕外面那顆白色膠囊。同一個外觀出現兩次，所以抽出來。
    private func pill<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        HStack(spacing: 0) { content() }
            .padding(DS.Space.space1)
            .background(
                Capsule()
                    .fill(DS.Color.surface)
                    .shadow(color: DS.Color.ink.opacity(0.15), radius: 7, x: 0, y: 4)
            )
    }

    private func toolButton(
        _ kind: CanvasToolState.Kind, systemImage: String, label: String
    ) -> some View {
        let isSelected = state.kind == kind
        return Button {
            onSelect(kind)
        } label: {
            Image(systemName: systemImage)
                .font(.system(size: Self.icon))
                .foregroundStyle(isSelected ? DS.Color.ink : DS.Color.muted)
                .frame(width: Self.button, height: Self.button)
                .background(
                    Circle().fill(isSelected ? DS.Color.ink.opacity(0.08) : .clear)
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }

    private func historyButton(
        systemImage: String, label: String, isEnabled: Bool, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: Self.icon))
                .foregroundStyle(isEnabled ? DS.Color.ink : DS.Color.muted.opacity(0.45))
                .frame(width: Self.button, height: Self.button)
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .accessibilityLabel(label)
    }
}

/// 筆刷 preset 的展開層。**再次點擊已選中的筆刷才展開**，點畫布任一處即收起。
///
/// 浮在工具列上方、不佔色環的位置——色環常駐即完整色盤，展開層不該把它擠掉。
///
/// 吃一份 `[BrushId]` 渲染，**不寫死格數**（`S1-ios-ui.md §4.3`）。
struct BrushPresetLayer: View {
    let presets: [BrushId]
    let selected: BrushId
    let onSelect: (BrushId) -> Void

    var body: some View {
        HStack(spacing: DS.Space.space1) {
            ForEach(presets, id: \.self) { preset in
                let dot = BrushCatalog.dot(for: preset)
                Button {
                    onSelect(preset)
                } label: {
                    Circle()
                        .fill(DS.Color.ink.opacity(dot.opacity))
                        .frame(width: dot.diameter, height: dot.diameter)
                        .frame(width: 48, height: 48)
                        .background(
                            Circle().fill(
                                preset == selected ? DS.Color.ink.opacity(0.08) : .clear
                            )
                        )
                }
                .buttonStyle(.plain)
                .accessibilityLabel(BrushCatalog.name(for: preset))
                .accessibilityAddTraits(preset == selected ? [.isSelected] : [])
            }
        }
        .padding(DS.Space.space1)
        .background(
            Capsule()
                .fill(DS.Color.surface)
                .shadow(color: DS.Color.ink.opacity(0.15), radius: 7, x: 0, y: 4)
        )
    }
}

#Preview("ToolBar 五態") {
    @Previewable @State var brush = CanvasToolState()
    @Previewable @State var eraser = {
        let s = CanvasToolState()
        s.kind = .eraser
        return s
    }()
    @Previewable @State var bucket = {
        let s = CanvasToolState()
        s.kind = .bucket
        return s
    }()

    VStack(alignment: .leading, spacing: DS.Space.space6) {
        labelled("筆刷選中（基準）") {
            ToolBar(state: brush, canUndo: true, canRedo: false, onSelect: { _ in }, onUndo: {}, onRedo: {})
        }
        labelled("橡皮擦選中") {
            ToolBar(state: eraser, canUndo: true, canRedo: true, onSelect: { _ in }, onUndo: {}, onRedo: {})
        }
        labelled("油漆桶選中") {
            ToolBar(state: bucket, canUndo: true, canRedo: false, onSelect: { _ in }, onUndo: {}, onRedo: {})
        }
        labelled("Undo 不可用") {
            ToolBar(state: brush, canUndo: false, canRedo: false, onSelect: { _ in }, onUndo: {}, onRedo: {})
        }
        labelled("筆刷展開層（五支 preset）") {
            VStack(alignment: .leading, spacing: DS.Space.space2) {
                BrushPresetLayer(
                    presets: BrushCatalog.presets, selected: .marker, onSelect: { _ in }
                )
                .padding(.leading, DS.Space.space4)
                ToolBar(state: brush, canUndo: true, canRedo: false, onSelect: { _ in }, onUndo: {}, onRedo: {})
            }
        }
    }
    .padding(.vertical, DS.Space.space6)
    .background(DS.Color.bg)
}

@ViewBuilder
private func labelled<Content: View>(
    _ title: String, @ViewBuilder content: () -> Content
) -> some View {
    VStack(alignment: .leading, spacing: DS.Space.space2) {
        Text(title)
            .font(DS.Typography.caption)
            .foregroundStyle(DS.Color.muted)
            .padding(.horizontal, DS.Space.space4)
        content()
    }
}
