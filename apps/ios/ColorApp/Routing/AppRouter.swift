//
//  AppRouter.swift
//  ColorApp
//

import SwiftUI

/// 五條路由的接線處。`GalleryScreen` 當 root。
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
