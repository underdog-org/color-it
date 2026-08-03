//
//  EngineBridgeTests.swift
//  EngineBridgeTests
//
//  `specs/ios-scaffold.md §8` 的六條。前四條驗「接得起來」，第五條驗「換得掉」——
//  它是唯一防止 `MockEngine` 慢慢漂離 Rust 行為的機制。
//

import Observation
import XCTest

@testable import EngineBridge

final class EngineBridgeTests: XCTestCase {
    /// S0 的 `RustEngine::new` 只檢查檔案存在，不解析內容（`.colorpack` 格式在 M1）。
    private var packPath: String!

    override func setUpWithError() throws {
        try super.setUpWithError()
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("colorlull-\(UUID().uuidString).colorpack")
        try Data("stub".utf8).write(to: url)
        packPath = url.path
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(atPath: packPath)
        packPath = nil
        try super.tearDownWithError()
    }

    // MARK: 1

    /// 這條就是「xcframework 在 Xcode 連得起來」的證明——`init` 成功代表
    /// modulemap 對上了、static slice 連上了、生成的 Swift 與 header 一致。
    func testAdapterInitSucceeds() throws {
        let adapter = try RustEngineAdapter(packPath: packPath)
        XCTAssertEqual(adapter.state.progress, Progress(colored: 0, total: 24))
    }

    /// 反向：路徑不存在時必須是 `EngineError.Pack`，不是 crash。
    func testAdapterInitRejectsMissingPack() {
        XCTAssertThrowsError(try RustEngineAdapter(packPath: "/nonexistent.colorpack")) { error in
            guard case EngineError.Pack = error else {
                return XCTFail("預期 EngineError.Pack，實際是 \(error)")
            }
        }
    }

    // MARK: 2

    func testTapAdvancesProgressThroughRealRust() throws {
        let adapter = try RustEngineAdapter(packPath: packPath)
        for _ in 0..<3 { adapter.tap(x: 1, y: 2) }
        XCTAssertEqual(adapter.state.progress.colored, 3)
    }

    // MARK: 3

    /// `docs/contracts.md` C1：回呼在呼叫端 thread 同步觸發，hop 到 main 是 Bridge 的責任。
    func testListenerUpdatesStateOnMainThread() throws {
        let adapter = try RustEngineAdapter(packPath: packPath)

        let updated = expectation(description: "state 更新")
        let wasOnMain = LockedBox(false)
        withObservationTracking {
            _ = adapter.state
        } onChange: {
            wasOnMain.value = Thread.isMainThread
            updated.fulfill()
        }

        DispatchQueue.global().async { adapter.tap(x: 1, y: 2) }

        wait(for: [updated], timeout: 5)
        XCTAssertTrue(wasOnMain.value, "從背景 thread 觸發的回呼必須 hop 到 main 才賦值")
        XCTAssertEqual(adapter.state.progress.colored, 1)
    }

    /// 已經在 main 上時走 fast path：同步賦值，不慢一個 runloop turn。
    func testMainThreadCallbackIsSynchronous() throws {
        let adapter = try RustEngineAdapter(packPath: packPath)
        adapter.tap(x: 1, y: 2)
        XCTAssertEqual(
            adapter.state.progress.colored, 1,
            "main 上呼叫應同步更新，不得排到下一個 runloop turn"
        )
    }

    // MARK: 4

    /// detach 路徑有效：Rust 端持有的 `Arc<dyn StateListener>` 必須在 `deinit` 解掉，
    /// 否則那個跨 FFI 的環會讓 Adapter 永遠不釋放。
    func testAdapterDeallocatesWithoutLeak() throws {
        weak var weakAdapter: RustEngineAdapter?
        try autoreleasepool {
            let adapter = try RustEngineAdapter(packPath: packPath)
            adapter.tap(x: 1, y: 2)
            weakAdapter = adapter
            XCTAssertNotNil(weakAdapter)
        }
        XCTAssertNil(weakAdapter, "Adapter 未釋放——listener 的參照環還在")
    }

    // MARK: 5

    /// 差分測試：同一串操作餵給兩個實作，`UiState` 序列必須**逐一**相同。
    ///
    /// 比對包含初始狀態，所以 `MockEngine` 的預設值必須逐欄位等於
    /// `core/app-state` 的 `AppState::default()`。
    func testMockAndRustProduceIdenticalStateSequences() throws {
        let mock = MockEngine()
        let rust = try RustEngineAdapter(packPath: packPath)

        let sample = InputSample(
            x: 1, y: 2, t: 0, pressure: 0.5, radius: 4, tiltX: 0, tiltY: 0, predicted: false
        )
        let operations: [(String, (any EngineProtocol) -> Void)] = [
            ("初始", { _ in }),
            ("tap", { $0.tap(x: 10, y: 10) }),
            ("tap 再一次", { $0.tap(x: 20, y: 20) }),
            ("setTool marker", {
                $0.setTool(.brush(
                    preset: .marker,
                    color: Rgba(r: 0xff, g: 0x00, b: 0x66, a: 0xff),
                    size: 9,
                    opacity: 0.6
                ))
            }),
            // C8：完全相同的 tool 設第二次不得產生新狀態。
            ("setTool marker 重複", {
                $0.setTool(.brush(
                    preset: .marker,
                    color: Rgba(r: 0xff, g: 0x00, b: 0x66, a: 0xff),
                    size: 9,
                    opacity: 0.6
                ))
            }),
            // 橡皮擦沒有顏色欄位——共用顏色必須留著，這是 Mock 存 `Tool` enum 就會漏掉的地方。
            ("setTool eraser", { $0.setTool(.eraser(size: 40)) }),
            ("beginStroke", { $0.beginStroke(sample) }),
            ("appendSamples", { $0.appendSamples([sample, sample]) }),
            ("endStroke", { $0.endStroke() }),
            ("setTool bucket", {
                $0.setTool(.bucket(color: Rgba(r: 0x00, g: 0x88, b: 0xff, a: 0xff)))
            }),
            ("undo", { $0.undo() }),
            ("redo", { $0.redo() }),
            ("render", { $0.render() }),
            ("setViewport", { $0.setViewport(Transform(scale: 2, tx: 3, ty: 4)) }),
        ]

        for (label, operation) in operations {
            operation(mock)
            operation(rust)
            XCTAssertEqual(mock.state, rust.state, "「\(label)」之後兩個實作的 UiState 分歧")
            XCTAssertEqual(
                mock.pickColor(x: 0, y: 0), rust.pickColor(x: 0, y: 0),
                "「\(label)」之後 pickColor 分歧——共用顏色的語意對不上"
            )
        }

        // 飽和行為也要一致。
        for _ in 0..<30 {
            mock.tap(x: 1, y: 1)
            rust.tap(x: 1, y: 1)
        }
        XCTAssertEqual(mock.state, rust.state)
        XCTAssertEqual(mock.state.progress.colored, 24, "應飽和於 total")
    }

    // MARK: 6

    func testBothImplementationsThrowNotImplemented() throws {
        let mock = MockEngine()
        let rust = try RustEngineAdapter(packPath: packPath)

        for engine in [mock as any EngineProtocol, rust as any EngineProtocol] {
            assertNotImplemented("save") { try engine.save() }
            assertNotImplemented("exportPNG") { _ = try engine.exportPNG() }
            assertNotImplemented("exportTimelapse") { _ = try engine.exportTimelapse() }
        }
    }

    private func assertNotImplemented(
        _ label: String,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ body: () throws -> Void
    ) {
        XCTAssertThrowsError(try body(), label, file: file, line: line) { error in
            guard case EngineError.NotImplemented = error else {
                return XCTFail("\(label) 預期 NotImplemented，實際是 \(error)", file: file, line: line)
            }
        }
    }
}

/// `withObservationTracking` 的 `onChange` 是 `@Sendable`，而它要寫的旗標活在測試的 stack 上。
private final class LockedBox<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: Value

    init(_ value: Value) { storage = value }

    var value: Value {
        get { lock.withLock { storage } }
        set { lock.withLock { storage = newValue } }
    }
}
