//
//  SubscriptionScreen.swift
//  ColorApp
//

import SwiftUI

/// 空殼。走 `.sheet`——付費牆是 modal。內容是 S1 之後的事。
struct SubscriptionScreen: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Text("Subscription")
                .navigationTitle("Subscription")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Close") { dismiss() }
                    }
                }
        }
    }
}
