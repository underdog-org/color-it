//
//  FrameDriver.swift
//  EngineBridge
//
//  `docs/specs/E1-input.md §2` 的實作。
//

import QuartzCore

/// FrameDriver 每 frame 回呼的對象。
///
/// 有這個 protocol 不是為了抽象，是為了**測試能在沒有 `CAMetalLayer` 的情況下
/// 驗回呼順序**——`EngineCanvasView` 是唯一的正式實作。
protocol FrameDriverTarget: AnyObject {
    func onFrame()
}

/// `CADisplayLink` 的持有者。**渲染由它驅動，不由輸入事件驅動**
/// （`architecture.md §10.3`）。
///
/// 定案是 `CAMetalLayer` ＋ 自建 `CADisplayLink`，`MTKView` 是退路（`E1-input §1`）：
/// `MTKView` 自帶一套 draw loop，與這裡是競爭機制而非互補。
final class FrameDriver {
    private let proxy: Proxy
    private var link: CADisplayLink?

    init(target: FrameDriverTarget) {
        proxy = Proxy(target: target)
    }

    deinit {
        // runloop 持有 link、link 持有 proxy。不 invalidate 的話兩者都留在 runloop 上
        // 繼續被呼叫（target 已經是 nil，於是每 frame 空轉）。
        link?.invalidate()
    }

    /// attach 時呼叫。第一次建立 link，之後只是解除暫停。
    func start() {
        if let link {
            link.isPaused = false
            return
        }

        let link = CADisplayLink(target: proxy, selector: #selector(Proxy.tick))
        // ProMotion 全速。非 ProMotion 裝置自動落到 60，不需要分支。
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 80, maximum: 120, preferred: 120)
        // `.default` 在使用者滑動 UI 時會停掉 display link；`.common` 不會。
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    /// detach 時呼叫。沒有 surface 時 `render()` 是純浪費。
    func pause() {
        link?.isPaused = true
    }

    /// **`CADisplayLink` 會 retain target。**直接把 `EngineCanvasView` 當 target，
    /// view 就永遠不釋放，而 view 持有 engine——於是整份文件洩漏。
    ///
    /// 這不是「之後再說」的細節：洩漏的症狀是「畫幾張圖之後 App 越來越慢」，
    /// 會被誤判成渲染效能問題。
    private final class Proxy {
        weak var target: FrameDriverTarget?

        init(target: FrameDriverTarget) {
            self.target = target
        }

        @objc func tick() {
            target?.onFrame()
        }
    }
}
