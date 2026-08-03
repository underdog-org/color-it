//
//  GalleryScreen.swift
//  ColorApp
//

import SwiftUI

/// Root。S0 只有一張假卡片 ＋ 兩個 modal 入口。
struct GalleryScreen: View {
    @Binding var sheet: Sheet?

    var body: some View {
        List {
            NavigationLink(value: Route.canvas(assetID: "mock")) {
                Text("Mock lineart")
            }
        }
        .navigationTitle("Gallery")
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Settings") { sheet = .settings }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("Subscribe") { sheet = .subscription }
            }
        }
    }
}
