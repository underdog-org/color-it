//
//  CanvasScreen.swift
//  ColorApp
//

import EngineBridge
import SwiftUI

/// 空殼 ＋ 引擎給的畫布 view。
///
/// `tap` 是 S0 唯一會改變 `UiState` 的操作，所以進度那行就是驗收
/// 「`tap()` 能驅動 progress 變化並反映到 UI」的地方——`engine.state` 讀取被
/// observation tracking 記錄，Mock 或 Adapter 更新 `state` 時這個 view 自己重畫。
struct CanvasScreen: View {
    let assetID: String

    @Environment(\.engine) private var engine

    var body: some View {
        VStack(spacing: 16) {
            EngineCanvas(engine: engine)
                .onTapGesture { point in
                    engine.tap(x: Float(point.x), y: Float(point.y))
                }

            Text("\(engine.state.progress.colored) / \(engine.state.progress.total)")
                .monospacedDigit()

            NavigationLink("Share", value: Route.share)
        }
        .padding()
        .navigationTitle(assetID)
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// 唯一一處 UIKit 膠水。引擎那側是 `UIView`（`docs/contracts.md` C7），
/// 而它的內容由誰畫是引擎的事——Shell 只負責把它擺進 SwiftUI 階層。
private struct EngineCanvas: UIViewRepresentable {
    let engine: any EngineProtocol

    func makeUIView(context: Context) -> UIView { engine.makeCanvasView() }
    func updateUIView(_ uiView: UIView, context: Context) {}
}
