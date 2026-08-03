//
//  ArtworkThumbnail.swift
//  ColorApp
//

import EngineBridge
import SwiftUI

/// 卡片縮圖。**S1 沒有真的縮圖**——它跟著下載的 pack 走，而下載器是 S1 後續子系統。
///
/// 在那之前畫一張由 `assetID` 決定的程序性佔位圖：同一個 asset 永遠得到同一張，
/// 所以卡片狀態矩陣看起來像一整組作品而不是一片灰。**刻意不是骨架灰**——
/// 骨架灰是 `.loading` 的語意，佔位圖不該跟它撞。
struct ArtworkThumbnail: View {
    let assetID: String

    private static let tints: [Color] = [
        DS.Color.brandAmber, DS.Color.brandTeal, DS.Color.brandPeri, DS.Color.brandBlush,
    ]

    var body: some View {
        let seed = abs(assetID.hashValue)
        let base = Self.tints[seed % Self.tints.count]
        let accent = Self.tints[(seed / 7 + 1) % Self.tints.count]

        LinearGradient(
            colors: [
                base.mix(with: DS.Color.paper, by: 0.55),
                accent.mix(with: DS.Color.paper, by: 0.72),
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
        .overlay {
            // 一顆偏移的圓，讓不同 asset 的佔位圖一眼分得出來。
            Circle()
                .fill(base.opacity(0.28))
                .scaleEffect(0.55)
                .offset(
                    x: CGFloat(seed % 40) - 20,
                    y: CGFloat((seed / 3) % 40) - 20
                )
        }
        .clipped()
    }
}

/// 難度的三格訊號柱。設計稿用的是 lucide 的 `signal-low/medium/high`，
/// SF Symbols 沒有等價符號，所以直接畫——三根柱子比找一個近似符號誠實。
struct DifficultyBars: View {
    let difficulty: Difficulty

    private var filled: Int {
        switch difficulty {
        case .easy: 1
        case .medium: 2
        case .focus: 3
        }
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 1.5) {
            ForEach(0..<3, id: \.self) { index in
                Capsule()
                    .fill(DS.Color.muted.opacity(index < filled ? 1 : 0.3))
                    .frame(width: 2, height: 4 + CGFloat(index) * 3)
            }
        }
        .frame(height: 11, alignment: .bottom)
    }
}

extension Difficulty {
    /// 副標**不顯示區數**（`Card States · Download` 的 note）——一個知覺性的詞就好。
    var displayName: String {
        switch self {
        case .easy: "輕鬆"
        case .medium: "適中"
        case .focus: "專注"
        }
    }
}
