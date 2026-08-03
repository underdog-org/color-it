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
