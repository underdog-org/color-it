//
//  ColorApp.swift
//  ColorApp
//

import EngineBridge
import SwiftUI

@main
struct ColorApp: App {
    /// 引擎建一次，經 `.environment` 傳下去。
    ///
    /// Shell 只認得 `any EngineProtocol`——選哪個實作是 `EngineFactory` 的事，
    /// 這裡看不到也不需要看到（`specs/ios-scaffold.md §6`，驗收「Shell 沒有任何一行
    /// 直接引用 `RustEngine`」）。
    @State private var engine: any EngineProtocol = EngineFactory.make(packPath: mockPackPath)

    var body: some Scene {
        WindowGroup {
            AppRouter()
                .environment(\.engine, engine)
        }
    }

    /// S0 的 `-engine rust` 只需要「一個存在的檔」——引擎不解析內容（M1 才有格式）。
    private static var mockPackPath: String? {
        Bundle.main.path(forResource: "mock-lineart", ofType: "png")
    }
}

/// 用 environment 而不是 `@Observable` 的 singleton：測試與 preview 能各自塞自己的實作。
extension EnvironmentValues {
    @Entry var engine: any EngineProtocol = MockEngine()
}
