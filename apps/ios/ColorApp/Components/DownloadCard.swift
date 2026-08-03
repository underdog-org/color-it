//
//  DownloadCard.swift
//  ColorApp
//
//  設計稿：`Download Card` 元件、`Card States · Download` 的七格。
//

import EngineBridge
import SwiftUI

/// 探索分頁的卡片。徽章與圓鈕是**兩個獨立的維度**：徽章講「這張圖跟你的關係」，
/// 圓鈕講「現在能對它做什麼」。三個不可能組合不畫 fallback UI，型別上也做不出來。
struct DownloadCard: View {
    let item: GalleryItem
    let isSubscribed: Bool
    let action: () -> Void

    private static let thumbHeight: CGFloat = 186

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.space3) {
            thumb
            meta
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: action)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(item.title)，\(item.difficulty.displayName)")
    }

    private var thumb: some View {
        ZStack(alignment: .topLeading) {
            ArtworkThumbnail(assetID: item.assetID)

            // 下載中：整張卡面壓一層紙色的薄膜，讓進度圈是唯一還在動的東西。
            if case .downloading = item.download {
                DS.Color.paper.opacity(0.72)
            }

            badge
                .padding(DS.Space.space2)

            actionButton
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                .padding(DS.Space.space2)
        }
        .frame(height: Self.thumbHeight)
        .clipShape(RoundedRectangle(cornerRadius: DS.Radius.md, style: .continuous))
    }

    // MARK: - 徽章

    /// 「有本機文件」蓋過付費與否——`prd.md §5.1` 的鎖定規則在視覺上的樣子。
    private var badge: some View {
        let (text, background, foreground): (String, Color, Color) =
            if item.work != nil {
                ("CONTINUE", DS.Color.ink, DS.Color.surface)
            } else if item.entitlement == .paid {
                ("PAID", DS.Color.accent, DS.Color.surface)
            } else {
                ("FREE", DS.Color.surface.opacity(0.9), DS.Color.ink)
            }

        return Text(text)
            .font(DS.Typography.caption)
            .fontWeight(.bold)
            .kerning(0.8)
            .foregroundStyle(foreground)
            .padding(.horizontal, DS.Space.space2)
            .frame(height: 22)
            .background(Capsule().fill(background))
    }

    // MARK: - 圓鈕

    /// 已下載且不鎖定時什麼都不畫——整張卡就是點擊區，不需要多一顆按鈕。
    @ViewBuilder
    private var actionButton: some View {
        if item.isLocked(isSubscribed: isSubscribed) {
            circle(fill: DS.Color.surface.opacity(0.9)) {
                Image(systemName: "lock.fill")
                    .font(.system(size: 14))
                    .foregroundStyle(DS.Color.ink)
            }
        } else {
            switch item.download {
            case .notDownloaded:
                circle(fill: DS.Color.ink.opacity(0.9)) {
                    Image(systemName: "arrow.down.to.line")
                        .font(.system(size: 14))
                        .foregroundStyle(DS.Color.surface)
                }
            case .downloading(let fraction):
                circle(fill: DS.Color.ink.opacity(0.9)) {
                    ZStack {
                        Circle()
                            .stroke(DS.Color.surface.opacity(0.24), lineWidth: 3)
                        Circle()
                            .trim(from: 0, to: max(fraction, 0.02))
                            .stroke(
                                DS.Color.surface,
                                style: StrokeStyle(lineWidth: 3, lineCap: .round)
                            )
                            .rotationEffect(.degrees(-90))
                    }
                    .frame(width: 22, height: 22)
                }
            case .failed:
                circle(fill: DS.Color.surface.opacity(0.9)) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 14))
                        .foregroundStyle(DS.Color.accent)
                }
            case .downloaded:
                EmptyView()
            }
        }
    }

    private func circle<Content: View>(
        fill: Color, @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .frame(width: 32, height: 32)
            .background(Circle().fill(fill))
    }

    // MARK: - 標題列

    private var meta: some View {
        VStack(alignment: .leading, spacing: DS.Space.space1) {
            Text(item.title)
                .font(DS.Typography.body)
                .fontWeight(.semibold)
                .foregroundStyle(DS.Color.ink)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: DS.Space.space1) {
                DifficultyBars(difficulty: item.difficulty)
                Text(item.difficulty.displayName)
                    .font(DS.Typography.caption)
                    .foregroundStyle(DS.Color.muted)
            }
        }
    }
}

#Preview("Card States · Download") {
    let base = FixtureCatalog(scenario: .populated).items

    ScrollView {
        LazyVGrid(
            columns: Array(repeating: GridItem(.fixed(150), spacing: DS.Space.space6), count: 3),
            spacing: DS.Space.space6
        ) {
            ForEach(base) { item in
                VStack(alignment: .leading, spacing: DS.Space.space2) {
                    DownloadCard(item: item, isSubscribed: false, action: {})
                    Text(String(describing: item.stateCombination))
                        .font(DS.Typography.caption)
                        .foregroundStyle(DS.Color.muted)
                }
            }
        }
        .padding(DS.Space.space6)
    }
    .background(DS.Color.bg)
}
