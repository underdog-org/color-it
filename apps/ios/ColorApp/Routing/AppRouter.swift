//
//  AppRouter.swift
//  ColorApp
//

import SwiftUI

/// 五條路由的接線處。`GalleryScreen` 當 root。
///
/// S0 只證明導航跑得通——五個畫面都是只有標題與必要入口的空殼，
/// 實際 UI（工具列、色盤、卡片、付費牆內容）是 S1 之後的事。
struct AppRouter: View {
    @State private var path = NavigationPath()
    @State private var sheet: Sheet?

    var body: some View {
        NavigationStack(path: $path) {
            GalleryScreen(sheet: $sheet)
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .canvas(let assetID):
                        CanvasScreen(assetID: assetID)
                    case .share:
                        ShareScreen()
                    }
                }
        }
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case .settings: SettingsScreen()
            case .subscription: SubscriptionScreen()
            }
        }
    }
}
