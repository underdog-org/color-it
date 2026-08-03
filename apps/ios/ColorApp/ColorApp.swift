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
    /// 這裡看不到也不需要看到（`cargo xtask lint-ios` 守著「Shell 沒有任何一行
    /// 直接引用 `RustEngine`」，見 apps/ios/README.md）。
    @State private var engine: any EngineProtocol = EngineFactory.make(packPath: devPackPath)

    var body: some Scene {
        WindowGroup {
            AppRouter()
                .environment(\.engine, engine)
        }
    }

    private static var devPackPath: String? {
        Bundle.main.path(forResource: "dev", ofType: "colorpack")
    }
}

/// 用 environment 而不是 `@Observable` 的 singleton：測試與 preview 能各自塞自己的實作。
extension EnvironmentValues {
    @Entry var engine: any EngineProtocol = MockEngine()
}
