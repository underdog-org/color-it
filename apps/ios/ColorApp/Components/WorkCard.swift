//
//  WorkCard.swift
//  ColorApp
//
//  設計稿：`Work Card` 元件、`Card States · Work` 的六格。
//

import EngineBridge
import SwiftUI

/// 我的作品分頁的卡片。
///
/// **進度永遠是縮圖下緣的線性條，不是環**（`Card States · Work` 的 head sub）——
/// 卡片與 Canvas Top Bar 用同一種進度表示，`prd.md §5.1`／`§5.2` 的進度環已作廢。
struct WorkCard: View {
    let item: GalleryItem
    let action: () -> Void

    private static let thumbHeight: CGFloat = 176
    /// 軌道左右各內縮 12，因為縮圖是 `radiusMd` 的圓角——不內縮的話角落曲率會吃掉
    /// 頭尾各約 12px，3% 完全看不見、97% 與 100% 分不出來（`Progress bar finding`）。
    private static let trackInset: CGFloat = 12
    /// 填充的最小長度。同一份 finding：短到看不見的填充等於沒有進度。
    private static let minimumFill: CGFloat = 8

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.space3) {
            thumb
            meta
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: action)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(item.title)，完成 \(percentText)")
    }

    private var thumb: some View {
        ZStack(alignment: .bottom) {
            ArtworkThumbnail(assetID: item.assetID)
            progressBar
                .padding(.horizontal, Self.trackInset)
                .padding(.bottom, 6)
        }
        .frame(height: Self.thumbHeight)
        .clipShape(RoundedRectangle(cornerRadius: DS.Radius.md, style: .continuous))
    }

    private var progressBar: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            let filled = max(width * progress, Self.minimumFill)
            ZStack(alignment: .leading) {
                Capsule().fill(DS.Color.ink.opacity(0.12))
                Capsule().fill(DS.Color.accent).frame(width: filled)
            }
        }
        .frame(height: 4)
    }

    private var meta: some View {
        VStack(alignment: .leading, spacing: DS.Space.space1) {
            Text(item.title)
                .font(DS.Typography.body)
                .fontWeight(.semibold)
                .foregroundStyle(DS.Color.ink)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(subtitle)
                .font(DS.Typography.caption)
                .foregroundStyle(DS.Color.muted)
                .lineLimit(1)
        }
    }

    // MARK: -

    private var progress: Double { item.work?.progress ?? 0 }

    private var percentText: String { "\(Int((progress * 100).rounded()))%" }

    /// 已分享的卡片改講「已分享」而不是「編輯於」——那是這張卡跟使用者最後一次互動的事。
    private var subtitle: String {
        let suffix: String
        if case .shared = item.work {
            suffix = "已分享"
        } else if let date = item.lastEditedAt {
            suffix = Self.relative.localizedString(for: date, relativeTo: .now)
        } else {
            suffix = "尚未編輯"
        }
        return "\(percentText) · \(suffix)"
    }

    private static let relative: RelativeDateTimeFormatter = {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter
    }()
}

#Preview("Card States · Work") {
    let works = FixtureCatalog(scenario: .populated).myWorks

    ScrollView {
        LazyVGrid(
            columns: Array(repeating: GridItem(.fixed(167), spacing: DS.Space.space4), count: 2),
            spacing: DS.Space.space6
        ) {
            ForEach(works) { item in
                WorkCard(item: item, action: {})
            }
        }
        .padding(DS.Space.space6)
    }
    .background(DS.Color.bg)
}
