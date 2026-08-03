//
//  CanvasScreen.swift
//  ColorApp
//

import EngineBridge
import SwiftUI

/// 空殼 ＋ 引擎給的畫布 view。
///
/// **輸入由 `EngineCanvasView` 自己收**（`E1-input §3`／`§8`），Shell 不接手勢：
/// `tap` 與 `InputSample` 一律送螢幕像素，而乘 `contentsScale` 需要 layer——
/// S0 的 `.onTapGesture` 送的是未縮放的 UIKit point，E1 起是錯的。
///
/// 進度那行仍然是「`tap()` 能驅動 progress 變化並反映到 UI」的驗收點——
/// `engine.state` 讀取被 observation tracking 記錄，引擎更新 `state` 時這個 view 自己重畫。
struct CanvasScreen: View {
    let assetID: String

    @Environment(\.engine) private var engine

    var body: some View {
        VStack(spacing: 16) {
            EngineCanvas(engine: engine)

            #if DEBUG
                DebugToolBar(engine: engine)
            #endif

            Text("\(engine.state.progress.colored) / \(engine.state.progress.total)")
                .monospacedDigit()

            NavigationLink("Share", value: Route.share)
        }
        .padding()
        .navigationTitle(assetID)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { MaskModeToggle(engine: engine) }
    }
}

#if DEBUG

    /// E1 真機測試的 harness——**不是產品 UI**（產品的工具列在 `S1`）。
    ///
    /// 為什麼非有不可：`EngineCanvasView.touchesBegan` 要 `state.tool` 是 `.bucket`
    /// 才走 `tap()`，而預設是 `Brush`。沒有這排按鈕，油漆桶那條路在真機上點不到，
    /// 於是 D4 的 Mask Mode 比較與擴散動畫調校（E1 驗收兩項）都做不了。
    ///
    /// **刻意不做**：筆刷切換（E1 只有一支軟圓筆）、size／opacity 調整與縮放平移
    /// （都在 `roadmap/E2.md`）。這裡只放剛好夠跑完 E1 驗收的東西。
    ///
    /// 真相在 Rust 的 `AppState`，不在這裡——選中狀態一律從 `engine.state.tool` 推導，
    /// 不留 `@State` 副本。這跟 `MaskModeToggle` 不同，因為 mask mode 不在 `UiState` 裡。
    private struct DebugToolBar: View {
        let engine: any EngineProtocol

        /// 六個色票。要能看出「相鄰區填不同色」，深淺各半——擴散動畫在淺色上比較看得出頭尾。
        private static let swatches: [Rgba] = [
            Rgba(r: 0x1A, g: 0x1A, b: 0x1A, a: 0xFF),
            Rgba(r: 0xE5, g: 0x39, b: 0x35, a: 0xFF),
            Rgba(r: 0xFB, g: 0xC0, b: 0x2D, a: 0xFF),
            Rgba(r: 0x43, g: 0xA0, b: 0x47, a: 0xFF),
            Rgba(r: 0x1E, g: 0x88, b: 0xE5, a: 0xFF),
            Rgba(r: 0xF8, g: 0xBB, b: 0xD0, a: 0xFF),
        ]

        var body: some View {
            VStack(spacing: 12) {
                Picker("工具", selection: bucketBinding) {
                    Text("筆刷").tag(false)
                    Text("油漆桶").tag(true)
                }
                .pickerStyle(.segmented)

                HStack(spacing: 12) {
                    ForEach(Self.swatches, id: \.self) { swatch in
                        Button {
                            apply(color: swatch)
                        } label: {
                            Circle()
                                .fill(Color(swatch))
                                .frame(width: 32, height: 32)
                                .overlay {
                                    Circle().strokeBorder(
                                        .primary,
                                        lineWidth: swatch == color ? 3 : 0
                                    )
                                }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }

        // MARK: 從 `UiState` 推導

        private var color: Rgba {
            switch engine.state.tool {
            case .brush(_, let color, _, _): return color
            case .bucket(let color): return color
            case .eraser: return Self.swatches[0]
            }
        }

        /// 切工具時要沿用目前筆刷大小，否則每次切回筆刷都會跳回預設值——
        /// 手感盲測中途變粗細會直接毀掉那一輪。
        private var size: Float {
            switch engine.state.tool {
            case .brush(_, _, let size, _): return size
            case .eraser(let size): return size
            // 油漆桶沒有 size，而 `AppState.size` 這時仍保有筆刷的值——但 `Tool`
            // 投影不出來。回預設值 24（`AppState::default`）是唯一誠實的答案。
            case .bucket: return 24
            }
        }

        private var isBucket: Bool {
            if case .bucket = engine.state.tool { return true }
            return false
        }

        private var bucketBinding: Binding<Bool> {
            Binding(get: { isBucket }, set: { apply(bucket: $0) })
        }

        // MARK: 寫回引擎

        private func apply(bucket: Bool) {
            apply(bucket: bucket, color: color)
        }

        private func apply(color: Rgba) {
            apply(bucket: isBucket, color: color)
        }

        /// `opacity: nil` ＝ 沿用 preset 的整筆上限（`Tool.brush` 的契約）。
        /// E1 不讓使用者覆寫它，那是 `E2`。
        private func apply(bucket: Bool, color: Rgba) {
            engine.setTool(
                bucket
                    ? .bucket(color: color)
                    : .brush(preset: .softRound, color: color, size: size, opacity: nil)
            )
        }
    }

    extension Color {
        /// `Rgba` 是 sRGB 8-bit，而 `Color(red:green:blue:)` 也是 sRGB——不需要轉換。
        fileprivate init(_ rgba: Rgba) {
            self.init(
                red: Double(rgba.r) / 255,
                green: Double(rgba.g) / 255,
                blue: Double(rgba.b) / 255,
                opacity: Double(rgba.a) / 255
            )
        }
    }

#endif

/// D4 的比較開關（`docs/specs/E1-perf.md §5`）。**Debug 建置限定**——
/// Release 裡整個 toolbar item 不存在，不是 disabled。
///
/// `mask mode` 不在 `UiState` 裡（切它不改任何業務狀態），所以真相在這個
/// `@State` 與 Rust 的 `Inner` 各一份。**Shell 這份只是 toggle 的位置**，
/// 畫面上的實際遮罩永遠以 Rust 那份為準——D4 之後兩份一起刪掉。
private struct MaskModeToggle: ToolbarContent {
    let engine: any EngineProtocol

    @State private var strict = true

    var body: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            #if DEBUG
                Toggle("Mask A", isOn: $strict)
                    .toggleStyle(.button)
                    .onChange(of: strict) { _, isStrict in
                        engine.setMaskMode(isStrict ? .strict : .loose)
                    }
            #endif
        }
    }
}

/// 唯一一處 UIKit 膠水。引擎那側是 `UIView`（`docs/contracts.md` C7），
/// 而它的內容由誰畫是引擎的事——Shell 只負責把它擺進 SwiftUI 階層。
private struct EngineCanvas: UIViewRepresentable {
    let engine: any EngineProtocol

    func makeUIView(context: Context) -> UIView { engine.makeCanvasView() }
    func updateUIView(_ uiView: UIView, context: Context) {}
}
