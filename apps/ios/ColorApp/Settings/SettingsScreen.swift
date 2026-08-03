//
//  SettingsScreen.swift
//  ColorApp
//

import SwiftUI

/// 空殼。走 `.sheet`——設定是進去就出來，不該堆在返回堆疊上。
struct SettingsScreen: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Text("Settings")
                .navigationTitle("Settings")
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
        }
    }
}
