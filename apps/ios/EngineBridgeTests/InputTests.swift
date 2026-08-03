//
//  InputTests.swift
//  EngineBridgeTests
//
//  `docs/specs/E1-input.md §10` 裡驗得到的那幾條。
//
//  驗不到的（列在這裡以免被當成漏測）：ProMotion 實測 120 Hz、motion-to-photon、
//  `touchesCancelled` 之後 `T_paint` 逐像素不變——三條都要真機 ＋ 真 GPU，
//  歸 `E1-perf` 與人工驗收。
//

import UIKit
import XCTest

@testable import EngineBridge

final class InputTests: XCTestCase {
    // MARK: §10 第 1 條：touch handler 內零次 render

    func testTouchHandlersNeverRender() {
        let (view, spy) = makeView()

        view.touchesBegan([touch(at: CGPoint(x: 10, y: 10))], with: nil)
        view.touchesMoved([touch(at: CGPoint(x: 12, y: 12), timestamp: 0.01)], with: nil)
        view.touchesEnded([touch(at: CGPoint(x: 14, y: 14), timestamp: 0.02)], with: nil)

        XCTAssertEqual(spy.renderCount, 0, "渲染只能由 FrameDriver 驅動")
    }

    /// FrameDriver 的那一 frame 才 render，且**先送輸入再渲染**（§2.1 順序不可換）。
    func testFrameDriverSendsInputBeforeRendering() {
        let (view, spy) = makeView()
        let first = touch(at: CGPoint(x: 10, y: 10))

        view.touchesBegan([first], with: nil)
        view.touchesMoved([first.moved(to: CGPoint(x: 12, y: 12), timestamp: 0.01)], with: nil)
        view.onFrame()

        XCTAssertEqual(spy.calls, ["beginStroke", "appendSamples", "render"])
    }

    // MARK: §10 第 2 條：一 frame 一次 appendSamples（C3）

    func testMultipleTouchEventsInOneFrameProduceOneAppend() {
        let (view, spy) = makeView()
        let first = touch(at: CGPoint(x: 10, y: 10))
        view.touchesBegan([first], with: nil)

        for i in 1...5 {
            let t = TimeInterval(i) * 0.002
            view.touchesMoved([first.moved(to: CGPoint(x: 10 + CGFloat(i), y: 10), timestamp: t)], with: nil)
        }
        view.onFrame()

        XCTAssertEqual(spy.appendedBatches.count, 1)
        XCTAssertEqual(spy.appendedBatches.first?.count, 5)
    }

    /// 沒有累積樣本時不呼叫——空陣列的呼叫要跨一次 FFI，而它什麼也不做（§2.1）。
    func testIdleFrameDoesNotCallAppendSamples() {
        let (view, spy) = makeView()

        view.onFrame()

        XCTAssertEqual(spy.appendedBatches.count, 0)
        XCTAssertEqual(spy.renderCount, 1)
    }

    // MARK: §10 第 3 條：FrameDriver 無 retain cycle

    func testViewDeallocatesDespiteDisplayLink() {
        weak var weakView: EngineCanvasView?
        autoreleasepool {
            let view = EngineCanvasView(engine: SpyEngine())
            weakView = view
            // 不進 window 也建得起 driver——retain cycle 若存在就在 init 那一刻成形。
            XCTAssertNotNil(weakView)
        }
        XCTAssertNil(weakView, "CADisplayLink 的 proxy 必須持 weak reference")
    }

    // MARK: §10 第 4／5 條：radius / pressure 的兩條來源

    func testFingerSamplesCarryRadiusAndNoPressure() {
        let sample = firstSample(from: touch(at: .zero, majorRadius: 12))

        XCTAssertEqual(sample.radius, 12, accuracy: 0.001)
        XCTAssertEqual(sample.pressure, 0)
    }

    func testPencilSamplesCarryPressureAndZeroRadius() {
        let pencil = touch(at: .zero, majorRadius: 12)
        pencil.stubType = .pencil
        pencil.stubForce = 2.0
        pencil.stubMaximumPossibleForce = 4.0

        let sample = firstSample(from: pencil)

        XCTAssertEqual(sample.radius, 0, "radius == 0 是「這是觸控筆」的主動語意")
        XCTAssertEqual(sample.pressure, 0.5, accuracy: 0.001)
    }

    /// `maximumPossibleForce == 0` 的裝置：走 radius 分支，**不得除出 NaN**——
    /// NaN 會沿著 One-Euro 傳染整筆。
    func testPencilOnADeviceWithoutForceDoesNotProduceNaN() {
        let pencil = touch(at: .zero, majorRadius: 9)
        pencil.stubType = .pencil
        pencil.stubForce = 0
        pencil.stubMaximumPossibleForce = 0

        let sample = firstSample(from: pencil)

        XCTAssertFalse(sample.pressure.isNaN)
        XCTAssertFalse(sample.radius.isNaN)
        XCTAssertEqual(sample.radius, 9, accuracy: 0.001)
    }

    // MARK: §10 第 6 條：`t` 相對筆畫起點歸零

    /// `systemUptime` 的量級直接送 `f32` 會塌成 0.03 秒解析度（§4.1）。
    func testTimestampsAreRelativeToStrokeStartAndStayMonotonic() {
        let adapter = InputAdapter()
        let view = EngineCanvasView(engine: SpyEngine())
        let uptime: TimeInterval = 432_000 // 開機五天

        let first = touch(at: .zero, timestamp: uptime)
        let begin = adapter.begin(first, in: view)
        XCTAssertEqual(begin?.t, 0)

        // 120 Hz 連續 60 秒。
        var previous: Float = 0
        for i in 1...7200 {
            let dt = TimeInterval(i) / 120.0
            adapter.append(first.moved(to: .zero, timestamp: uptime + dt), event: nil, in: view)
            let sample = adapter.flush()[0]
            XCTAssertGreaterThan(sample.t, previous, "f32 精度塌陷：相鄰樣本的 t 不再遞增")
            previous = sample.t
        }
    }

    // MARK: §10 第 8 條：第二根手指

    func testSecondFingerNeitherInterruptsNorStartsAStroke() {
        let (view, spy) = makeView()
        let first = touch(at: CGPoint(x: 10, y: 10))
        let second = touch(at: CGPoint(x: 80, y: 80))

        view.touchesBegan([first], with: nil)
        view.touchesBegan([second], with: nil)
        view.touchesEnded([second], with: nil)

        XCTAssertEqual(spy.calls, ["beginStroke"], "第二根手指不得產生第二筆，也不得結束第一筆")
    }

    // MARK: cancel

    /// 取消**丟棄未送出的樣本**——取消不需要完整性（§7）。
    func testCancelDiscardsPendingSamples() {
        let (view, spy) = makeView()
        let first = touch(at: CGPoint(x: 10, y: 10))

        view.touchesBegan([first], with: nil)
        view.touchesMoved([first.moved(to: CGPoint(x: 20, y: 20), timestamp: 0.01)], with: nil)
        view.touchesCancelled([first], with: nil)
        view.onFrame()

        XCTAssertEqual(spy.calls, ["beginStroke", "cancelStroke", "render"])
    }

    /// 抬筆的順序是 `flush()` → `endStroke()`（§3）：漏掉最後幾個樣本 ＝ 筆尾少一截。
    func testEndFlushesRemainingSamplesBeforeEndStroke() {
        let (view, spy) = makeView()
        let first = touch(at: CGPoint(x: 10, y: 10))

        view.touchesBegan([first], with: nil)
        view.touchesMoved([first.moved(to: CGPoint(x: 20, y: 20), timestamp: 0.01)], with: nil)
        view.touchesEnded([first], with: nil)

        XCTAssertEqual(spy.calls, ["beginStroke", "appendSamples", "endStroke"])
    }

    // MARK: 油漆桶

    /// 油漆桶不是筆畫，而且座標必須乘 `contentsScale`（`E1-bucket §4.1` 送螢幕像素）。
    func testBucketToolTapsInScreenPixels() {
        let (view, spy) = makeView()
        spy.stubTool = .bucket(color: Rgba(r: 0, g: 0, b: 0, a: 255))
        view.layer.contentsScale = 3

        view.touchesBegan([touch(at: CGPoint(x: 10, y: 20))], with: nil)

        XCTAssertEqual(spy.calls, ["tap"], "油漆桶不建立 stroke 狀態")
        XCTAssertEqual(spy.taps.first?.x, 30)
        XCTAssertEqual(spy.taps.first?.y, 60)
    }

    /// `InputSample` 與 `tap` 必須落在同一個座標系（`E1-bucket §4.1`：螢幕像素）。
    func testStrokeSamplesUseTheSamePixelScaleAsTap() {
        let (view, spy) = makeView()
        view.layer.contentsScale = 3

        view.touchesBegan([touch(at: CGPoint(x: 10, y: 20))], with: nil)
        view.onFrame()

        XCTAssertEqual(spy.beganSample?.x, 30)
        XCTAssertEqual(spy.beganSample?.y, 60)
    }

    // MARK: 工具

    private func makeView() -> (EngineCanvasView, SpyEngine) {
        let spy = SpyEngine()
        let view = EngineCanvasView(engine: spy)
        view.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        return (view, spy)
    }

    private func firstSample(from touch: StubTouch) -> InputSample {
        let adapter = InputAdapter()
        let view = EngineCanvasView(engine: SpyEngine())
        return adapter.begin(touch, in: view)!
    }

    private func touch(
        at point: CGPoint,
        timestamp: TimeInterval = 0,
        majorRadius: CGFloat = 10
    ) -> StubTouch {
        let touch = StubTouch()
        touch.stubLocation = point
        touch.stubTimestamp = timestamp
        touch.stubMajorRadius = majorRadius
        return touch
    }
}

/// `UITouch` 沒有公開的建構路徑，也不能設定屬性——只能覆寫。
///
/// `InputAdapter` 用 `==` 比對追蹤中的那一根，而 `UITouch` 走 `NSObject` 的
/// 身分相等，所以「同一根手指的後續事件」在測試裡就是**同一個實例**。
final class StubTouch: UITouch {
    var stubType: UITouch.TouchType = .direct
    var stubLocation: CGPoint = .zero
    var stubTimestamp: TimeInterval = 0
    var stubMajorRadius: CGFloat = 10
    var stubForce: CGFloat = 0
    var stubMaximumPossibleForce: CGFloat = 0

    override var type: UITouch.TouchType { stubType }
    override var timestamp: TimeInterval { stubTimestamp }
    override var majorRadius: CGFloat { stubMajorRadius }
    override var force: CGFloat { stubForce }
    override var maximumPossibleForce: CGFloat { stubMaximumPossibleForce }
    override var altitudeAngle: CGFloat { 0 }

    override func preciseLocation(in view: UIView?) -> CGPoint { stubLocation }
    override func location(in view: UIView?) -> CGPoint { stubLocation }
    override func azimuthAngle(in view: UIView?) -> CGFloat { 0 }

    /// 同一根手指移動到下一個位置。回傳的是 `self`——見類別註解。
    @discardableResult
    func moved(to point: CGPoint, timestamp: TimeInterval) -> StubTouch {
        stubLocation = point
        stubTimestamp = timestamp
        return self
    }
}

/// 只記呼叫順序。`MockEngine` 記的是狀態，這裡要的是**時序**。
final class SpyEngine: EngineProtocol {
    private(set) var calls: [String] = []
    private(set) var appendedBatches: [[InputSample]] = []
    private(set) var taps: [(x: Float, y: Float)] = []
    private(set) var renderCount = 0

    var stubTool: Tool = .brush(
        preset: .softRound,
        color: Rgba(r: 0x1a, g: 0x1a, b: 0x1a, a: 0xff),
        size: 24,
        opacity: nil
    )

    var state: UiState {
        UiState(tool: stubTool, canUndo: false, canRedo: false, progress: Progress(colored: 0, total: 1))
    }

    func attachSurface(_ handle: SurfaceHandle) throws { calls.append("attachSurface") }
    func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float) {}
    func detachSurface() { calls.append("detachSurface") }

    func setTool(_ tool: Tool) { stubTool = tool }
    func pickColor(x: Float, y: Float) -> Rgba { Rgba(r: 0, g: 0, b: 0, a: 255) }

    private(set) var beganSample: InputSample?

    func beginStroke(_ s: InputSample) {
        calls.append("beginStroke")
        beganSample = s
    }

    func appendSamples(_ s: [InputSample]) {
        calls.append("appendSamples")
        appendedBatches.append(s)
    }

    func endStroke() { calls.append("endStroke") }
    func cancelStroke() { calls.append("cancelStroke") }

    func tap(x: Float, y: Float) {
        calls.append("tap")
        taps.append((x, y))
    }

    func undo() {}
    func redo() {}

    func render() {
        calls.append("render")
        renderCount += 1
    }

    func setViewport(_ transform: Transform) {}

    func save() throws {}
    func exportPNG() throws -> Data { Data() }
    func exportTimelapse() throws -> Data { Data() }

    func makeCanvasView() -> UIView { UIView() }
}
