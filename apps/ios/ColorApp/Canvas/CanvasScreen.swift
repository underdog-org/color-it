//
//  CanvasScreen.swift
//  ColorApp
//
//  設計稿：`Draw · Tools v2`（基準）、`Draw · Tool States`、`Draw · 吸管取色中`、
//  `Draw · 完成建議`。
//

import EngineBridge
import SwiftUI

/// 由上到下：Top Bar（返回／進度條／筆刷參數＋分享）、畫布、工具列、色環。
///
/// 輸入由 `EngineCanvasView` 自己收（`E1-input §3`），Shell 不接手勢。
/// 工具選擇狀態在 `CanvasToolState`，每次變更組出完整 `Tool` 送 `setTool`。
struct CanvasScreen: View {
    let assetID: String

    @Environment(\.engine) private var engine
    @Environment(\.dismiss) private var dismiss

    @State private var toolState = CanvasToolState()
    @State private var pickMode = CanvasPickMode()
    @State private var recentColors: [Rgba] = []
    @State private var isFinishNudgeDismissed = false

    /// S1 對著假資料，還沒有真的 `manifest.palette` 可讀。
    private static let suggestedColors: [Rgba] = [
        Rgba(r: 0xE8, g: 0xA0, b: 0x2E, a: 0xFF),
        Rgba(r: 0xF4, g: 0x61, b: 0x4C, a: 0xFF),
        Rgba(r: 0x3F, g: 0xA8, b: 0x9B, a: 0xFF),
        Rgba(r: 0xEE, g: 0x9B, b: 0xB4, a: 0xFF),
    ]

    var body: some View {
        VStack(spacing: 0) {
            topBar
            canvasArea
            toolBarStack
            ColorWheel(
                color: colorBinding,
                isPicking: pickMode.isArmed,
                onEyedropper: { pickMode.isArmed.toggle() }
            )
        }
        .background(DS.Color.bg)
        .navigationBarBackButtonHidden(true)
        .navigationBarHidden(true)
        .onAppear { engine.setTool(toolState.tool) }
        .onChange(of: pickMode.picked) { _, picked in
            if let picked { apply(color: picked) }
        }
    }

    // MARK: - Top Bar

    private var topBar: some View {
        HStack {
            circleButton(systemImage: "chevron.left", label: "返回") { dismiss() }

            Spacer()
            // 進度是線性條，不是環——卡片與這裡用同一種表示（`S1-ios-ui.md §0.2`）。
            ProgressBar(fraction: progressFraction)
                .frame(width: 120, height: 6)
            Spacer()

            HStack(spacing: 0) {
                // sliders 是**筆刷參數**，不是 Settings。Canvas 沒有 Settings 入口。
                iconButton(systemImage: "slider.horizontal.3", label: "筆刷參數") {
                    select(kind: .brush, forceExpand: true)
                }
                NavigationLink(value: Route.share) {
                    Image(systemName: "square.and.arrow.up")
                        .font(.system(size: 19))
                        .foregroundStyle(DS.Color.ink)
                        .frame(width: 42, height: 42)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("分享")
            }
            .padding(DS.Space.space1)
            .background(
                Capsule()
                    .fill(DS.Color.surface)
                    .shadow(color: DS.Color.ink.opacity(0.15), radius: 7, y: 4)
            )

            #if DEBUG
                MaskModeToggle(engine: engine)
            #endif
        }
        .padding(.horizontal, DS.Space.space4)
        .frame(height: 60)
    }

    // MARK: - 畫布

    private var canvasArea: some View {
        VStack(spacing: DS.Space.space3) {
            ZStack(alignment: .topTrailing) {
                EngineCanvas(engine: engine, pickMode: pickMode)
                    .clipShape(RoundedRectangle(cornerRadius: DS.Radius.sm))
                    .shadow(color: DS.Color.ink.opacity(0.12), radius: 10, y: 6)
                    .padding(.horizontal, DS.Space.space5 + DS.Space.space1)

                // 兩排刻度貼紙張右緣。油漆桶時整體降到停用態、位置不動。
                VStack(spacing: 0) {
                    LevelTicks(
                        kind: .size,
                        level: levelBinding(\.sizeLevel),
                        isEnabled: toolState.levelsEnabled
                    )
                    LevelTicks(
                        kind: .opacity,
                        level: levelBinding(\.opacityLevel),
                        isEnabled: toolState.levelsEnabled && toolState.kind == .brush
                    )
                }
                .padding(.top, DS.Space.space6)
                .padding(.trailing, -DS.Space.space5)
            }

            finishNudge

            SwatchRow(
                suggested: Self.suggestedColors,
                recent: recentColors,
                current: toolState.color,
                onSelect: apply(color:)
            )
            .padding(.horizontal, DS.Space.space5 + DS.Space.space1)
        }
        .padding(.vertical, DS.Space.space5)
    }

    /// 浮在畫布下緣的膠囊，不加遮罩、不攔截觸控；關掉後同一份作品不再出現。
    @ViewBuilder
    private var finishNudge: some View {
        if !isFinishNudgeDismissed, progressFraction >= 0.85 {
            HStack {
                HStack(spacing: DS.Space.space2) {
                    Image(systemName: "sparkles")
                        .font(.system(size: 18))
                        .foregroundStyle(DS.Color.accent)
                    Text("看起來不錯了，要分享嗎？")
                        .font(DS.Typography.body)
                        .foregroundStyle(DS.Color.ink)
                }
                Spacer(minLength: DS.Space.space2)
                NavigationLink(value: Route.share) {
                    Text("分享")
                        .font(DS.Typography.body)
                        .fontWeight(.semibold)
                        .foregroundStyle(DS.Color.paper)
                        .padding(.horizontal, DS.Space.space3)
                        .frame(height: 34)
                        .background(Capsule().fill(DS.Color.ink))
                }
                .buttonStyle(.plain)
                Button {
                    isFinishNudgeDismissed = true
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 16))
                        .foregroundStyle(DS.Color.muted)
                        .frame(width: 34, height: 34)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("關閉")
            }
            .padding(.leading, DS.Space.space4)
            .padding(.trailing, DS.Space.space2)
            .frame(height: 52)
            .background(
                Capsule()
                    .fill(DS.Color.surface)
                    .shadow(color: DS.Color.ink.opacity(0.18), radius: 10, y: 6)
            )
            .padding(.horizontal, DS.Space.space5 + DS.Space.space1)
        }
    }

    // MARK: - 工具列

    private var toolBarStack: some View {
        VStack(alignment: .leading, spacing: DS.Space.space2) {
            if toolState.isBrushLayerExpanded {
                BrushPresetLayer(
                    presets: BrushCatalog.presets,
                    selected: toolState.preset,
                    onSelect: apply(preset:)
                )
                .padding(.leading, DS.Space.space4)
                .transition(.opacity.combined(with: .move(edge: .bottom)))
            }

            ToolBar(
                state: toolState,
                canUndo: engine.state.canUndo,
                canRedo: engine.state.canRedo,
                onSelect: { select(kind: $0) },
                onUndo: { engine.undo() },
                onRedo: { engine.redo() }
            )
        }
        .animation(.snappy(duration: 0.2), value: toolState.isBrushLayerExpanded)
    }

    // MARK: - 寫回引擎

    /// 再次點擊已選中的筆刷才展開 preset 層；切到別的工具一律收起。
    private func select(kind: CanvasToolState.Kind, forceExpand: Bool = false) {
        if kind == .brush, toolState.kind == .brush || forceExpand {
            toolState.kind = .brush
            toolState.isBrushLayerExpanded = forceExpand || !toolState.isBrushLayerExpanded
        } else {
            toolState.isBrushLayerExpanded = false
            toolState.kind = kind
        }
        pickMode.isArmed = false
        engine.setTool(toolState.tool)
    }

    private func apply(preset: BrushId) {
        toolState.preset = preset
        toolState.isBrushLayerExpanded = false
        engine.setTool(toolState.tool)
    }

    private func apply(color: Rgba) {
        toolState.color = color
        recentColors = Array(([color] + recentColors.filter { $0 != color }).prefix(4))
        engine.setTool(toolState.tool)
    }

    /// `UiState.tool` 在橡皮擦時沒有 `color` 欄位，所以當前色只讀 Shell 這份
    /// （`docs/interface-defects.md` 第一條）。
    private var colorBinding: Binding<Rgba> {
        Binding(get: { toolState.color }, set: { apply(color: $0) })
    }

    private func levelBinding(
        _ keyPath: ReferenceWritableKeyPath<CanvasToolState, Int>
    ) -> Binding<Int> {
        Binding(
            get: { toolState[keyPath: keyPath] },
            set: {
                toolState[keyPath: keyPath] = $0
                engine.setTool(toolState.tool)
            }
        )
    }

    private var progressFraction: Double {
        let progress = engine.state.progress
        guard progress.total > 0 else { return 0 }
        return Double(progress.colored) / Double(progress.total)
    }

    // MARK: - 小零件

    private func circleButton(
        systemImage: String, label: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 20))
                .foregroundStyle(DS.Color.ink)
                .frame(width: 42, height: 42)
                .background(
                    Circle()
                        .fill(DS.Color.surface)
                        .shadow(color: DS.Color.ink.opacity(0.15), radius: 7, y: 4)
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    private func iconButton(
        systemImage: String, label: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 19))
                .foregroundStyle(DS.Color.ink)
                .frame(width: 42, height: 42)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

/// Top Bar 與卡片共用的線性進度條。
private struct ProgressBar: View {
    let fraction: Double

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(DS.Color.ink.opacity(0.12))
                Capsule()
                    .fill(DS.Color.ink)
                    .frame(width: max(geometry.size.width * fraction, fraction > 0 ? 8 : 0))
            }
        }
    }
}

#if DEBUG

    /// D4 的比較開關（`E1-perf.md §5`）。Debug 建置限定；D4 拍板後與 Rust 那份一起刪。
    private struct MaskModeToggle: View {
        let engine: any EngineProtocol

        @State private var strict = true

        var body: some View {
            Toggle("Mask A", isOn: $strict)
                .toggleStyle(.button)
                .font(DS.Typography.caption)
                .onChange(of: strict) { _, isStrict in
                    engine.setMaskMode(isStrict ? .strict : .loose)
                }
        }
    }

#endif

/// 唯一一處 UIKit 膠水。引擎那側是 `UIView`（`docs/contracts.md` C7）。
private struct EngineCanvas: UIViewRepresentable {
    let engine: any EngineProtocol
    let pickMode: CanvasPickMode

    func makeUIView(context: Context) -> UIView { engine.makeCanvasView(pickMode: pickMode) }
    func updateUIView(_ uiView: UIView, context: Context) {}
}
