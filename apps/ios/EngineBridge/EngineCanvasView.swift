//
//  EngineCanvasView.swift
//  EngineBridge
//
//  `docs/contracts.md` C7 ＋ `docs/specs/E1-input.md §8` 的實作。
//

import QuartzCore
import UIKit

/// 承載 surface 生命週期與輸入的 `UIView`。
///
/// **`CAMetalLayer` ＋ 自建 `CADisplayLink`，`MTKView` 是退路**（`E1-input §1`
/// 結案，`architecture.md §10.3` 的待決項在那裡）。三個理由：`§10.3` 規定渲染由
/// FrameDriver 驅動而非輸入事件驅動，而 `MTKView` 自帶一套 draw loop——兩者是
/// 競爭機制；wgpu 本來就吃 `CAMetalLayer`；S0 已經走在這條路上。
public final class EngineCanvasView: UIView, FrameDriverTarget {
    public override class var layerClass: AnyClass { CAMetalLayer.self }

    private let engine: any EngineProtocol
    private let input = InputAdapter()
    private var driver: FrameDriver?
    private var attached = false

    /// surface 建不起來時的錯誤態。**不 crash**——使用者的畫作還在 engine 裡
    /// （`E1-wgpu §2.2`／`E1-input §8`）。
    private var errorLabel: UILabel?

    public init(engine: any EngineProtocol) {
        self.engine = engine
        super.init(frame: .zero)
        isOpaque = true
        // §7 的「只追蹤第一根」由 UIKit 保證，`InputAdapter` 是第二道防線。
        isMultipleTouchEnabled = false
        driver = FrameDriver(target: self)
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
        errorLabel?.frame = bounds
        guard attached else {
            // `didMoveToWindow` 那一刻 `bounds` 還是 0 是常態（SwiftUI 先掛上再排版），
            // 而 attach 需要真實尺寸——所以第一次量得到的時候要補做，否則永遠不 attach。
            if window != nil { attach() }
            return
        }
        let size = drawableSize
        engine.resizeSurface(widthPx: size.width, heightPx: size.height, scale: Float(scale))
    }

    // MARK: FrameDriver

    /// **順序不可換**（`E1-input §2.1`）：先送輸入再渲染，這一 frame 的手指位置
    /// 才畫得進去；反過來會固定多一格延遲。
    func onFrame() {
        let batch = input.flush()
        // 空陣列的呼叫要跨一次 FFI，而它什麼也不做——所以有樣本才呼叫（C3）。
        if !batch.isEmpty {
            engine.appendSamples(batch)
        }
        engine.render()
    }

    // MARK: 輸入
    //
    // 時機表在 `E1-input §3`。**這裡一次 `render()` 都不呼叫**——那會退回
    // 「渲染由輸入驅動」，正是 `architecture.md §10.3` 禁止的。

    public override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = touches.first else { return }

        // 油漆桶不是筆畫：O(1) 的一次填色，不經過 stroke 狀態機。
        // 座標乘 `contentsScale` 在這裡做——`tap` 收**螢幕像素**（`E1-bucket §4.1`）。
        if case .bucket = engine.state.tool {
            let point = touch.preciseLocation(in: self)
            // 乘 `layer.contentsScale` 而不是 `scale`：`InputAdapter` 用的是前者，
            // 而 `tap` 與 `InputSample` 必須落在同一個座標系。`attach()` 已經把
            // 兩者對齊（`layer.contentsScale = scale`）。
            let pixelScale = layer.contentsScale
            engine.tap(x: Float(point.x * pixelScale), y: Float(point.y * pixelScale))
            return
        }

        // `beginStroke` 建立 stroke 狀態，壓到下一 frame 會讓第一批 `appendSamples`
        // 沒有可附著的筆畫，所以在這裡立刻送（§3）。
        guard let first = input.begin(touch, in: self) else { return }
        engine.beginStroke(first)
    }

    public override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = touches.first, input.isTracking else { return }
        input.append(touch, event: event, in: self)
    }

    /// 順序是 `flush()` → `endStroke()`，兩次 FFI 呼叫（§3）。抬筆要重建 `T_wet`
    /// 並 commit，漏掉最後幾個樣本 ＝ 筆尾少一截。
    public override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = touches.first, input.end(touch) else { return }
        let batch = input.flush()
        if !batch.isEmpty {
            engine.appendSamples(batch)
        }
        engine.endStroke()
    }

    /// palm rejection 事後判定失敗、來電、Home——`T_wet` 直接清掉，
    /// `T_paint` 從未被污染，所以取消是零成本的，不需要 undo（§7）。
    public override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = touches.first, input.cancel(touch) else { return }
        engine.cancelStroke()
    }

    // MARK: Surface 生命週期

    private func attach() {
        guard !attached else { return }
        let size = drawableSize
        guard size.width > 0, size.height > 0 else { return }

        let layer = layer as! CAMetalLayer
        layer.contentsScale = scale
        // `E1-wgpu §3.1` 的記名例外：預設 3 會多一格 latency，而 E1 的整條路
        // 就是為了 motion-to-photon。wgpu 不暴露它，只能在 layer 上設。
        layer.maximumDrawableCount = 2

        let handle = SurfaceHandle(
            layerPtr: UInt64(UInt(bitPattern: Unmanaged.passUnretained(layer).toOpaque())),
            widthPx: size.width,
            heightPx: size.height,
            scale: Float(scale)
        )
        do {
            try engine.attachSurface(handle)
            attached = true
            clearError()
            driver?.start()
        } catch {
            // **不 crash。** adapter 取不到、device 建不出來、surface 格式不支援
            // 都會走到這裡（`E1-wgpu §2.2`），而使用者的畫作還在 engine 裡——
            // 下次進 window 會重試。
            showError(error)
        }
    }

    private func detach() {
        guard attached else { return }
        // 先停 driver 再丟 surface：反過來的話那一 frame 會對著已經沒了的
        // surface 呼叫 `render()`。
        driver?.pause()
        engine.detachSurface()
        attached = false
    }

    // MARK: 錯誤態

    private func showError(_ error: Error) {
        let label = errorLabel ?? {
            let label = UILabel()
            label.numberOfLines = 0
            label.textAlignment = .center
            label.backgroundColor = .systemBackground
            label.textColor = .secondaryLabel
            label.font = .preferredFont(forTextStyle: .footnote)
            addSubview(label)
            errorLabel = label
            return label
        }()
        label.frame = bounds
        label.text = "\(error.localizedDescription)\n\n作品沒有遺失。"
    }

    private func clearError() {
        errorLabel?.removeFromSuperview()
        errorLabel = nil
    }

    // MARK: 尺寸

    /// `contentsScale` 而非 `UIScreen.main.scale`——後者在 iPad 分割視窗與外接顯示器上是錯的。
    private var scale: CGFloat {
        window?.screen.nativeScale ?? traitCollection.displayScale
    }

    private var drawableSize: (width: UInt32, height: UInt32) {
        let pixels = bounds.size.applying(CGAffineTransform(scaleX: scale, y: scale))
        return (UInt32(max(0, pixels.width.rounded())), UInt32(max(0, pixels.height.rounded())))
    }
}
