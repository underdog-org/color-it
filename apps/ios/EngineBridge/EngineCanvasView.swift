//
//  EngineCanvasView.swift
//  EngineBridge
//
//  `docs/contracts.md` C7 的實作。
//

import QuartzCore
import UIKit

/// 承載 surface 生命週期的 `UIView`。**S0 不畫任何東西**——`render()` 本來就是 no-op，
/// 畫面會是空的。這個 view 的用途是先把 attach／resize／detach 這條路跑通。
///
/// **刻意不用 `MTKView`，儘管 `architecture.md §10.1` 提到它。**
/// `§10.3` 規定渲染由 `CADisplayLink` 驅動而非輸入事件驅動，而 `MTKView` 自帶一套
/// draw loop，兩者是競爭機制。這個取捨該由 E1 拿著真的 render pass 決定，
/// S0 不預先綁死——**已列為 E1 的待決項**。
public final class EngineCanvasView: UIView {
    public override class var layerClass: AnyClass { CAMetalLayer.self }

    private let engine: any EngineProtocol
    private var attached = false

    public init(engine: any EngineProtocol) {
        self.engine = engine
        super.init(frame: .zero)
        isOpaque = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("EngineCanvasView 只由程式碼建立")
    }

    /// `docs/contracts.md` C5：`attach` / `detach` 是正常路徑，不是錯誤處理。
    /// 引擎的生命週期長於 surface——離開 window 只是丟掉 surface，不丟狀態。
    public override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil {
            detach()
        } else {
            attach()
        }
    }

    public override func layoutSubviews() {
        super.layoutSubviews()
        guard attached else { return }
        let size = drawableSize
        engine.resizeSurface(widthPx: size.width, heightPx: size.height, scale: Float(scale))
    }

    private func attach() {
        guard !attached else { return }
        let size = drawableSize
        guard size.width > 0, size.height > 0 else { return }

        let layer = layer as! CAMetalLayer
        layer.contentsScale = scale

        let handle = SurfaceHandle(
            layerPtr: UInt64(UInt(bitPattern: Unmanaged.passUnretained(layer).toOpaque())),
            widthPx: size.width,
            heightPx: size.height,
            scale: Float(scale)
        )
        do {
            try engine.attachSurface(handle)
            attached = true
        } catch {
            // S0 的 `attach_surface` 永遠回 `Ok`（不碰 GPU）。真的走到這裡代表
            // E1 之後引擎開始驗 surface 了，那時這個分支要換成真的錯誤處理。
            assertionFailure("attachSurface 失敗：\(error)")
        }
    }

    private func detach() {
        guard attached else { return }
        engine.detachSurface()
        attached = false
    }

    /// `contentsScale` 而非 `UIScreen.main.scale`——後者在 iPad 分割視窗與外接顯示器上是錯的。
    private var scale: CGFloat {
        window?.screen.nativeScale ?? traitCollection.displayScale
    }

    private var drawableSize: (width: UInt32, height: UInt32) {
        let pixels = bounds.size.applying(CGAffineTransform(scaleX: scale, y: scale))
        return (UInt32(max(0, pixels.width.rounded())), UInt32(max(0, pixels.height.rounded())))
    }
}
