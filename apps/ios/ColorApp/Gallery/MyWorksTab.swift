//
//  MyWorksTab.swift
//  ColorApp
//
//  設計稿：`Gallery States` 的「My works · empty」。
//

import EngineBridge
import SwiftUI

/// 我的作品：`work != nil`，依 `lastEditedAt` 降冪。沒有搜尋與分類。
struct MyWorksTab: View {
    let catalog: FixtureCatalog
    let onBrowse: () -> Void
    let onOpenSettings: () -> Void

    private static let cardWidth: CGFloat = 167

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DS.Space.space5) {
                GalleryHeader(title: "我的作品", onAvatarTap: onOpenSettings)

                if catalog.myWorks.isEmpty {
                    empty
                } else {
                    grid
                }
            }
            .padding(.horizontal, DS.Space.space5)
            .padding(.bottom, DS.Space.space6)
        }
    }

    private var grid: some View {
        LazyVGrid(
            columns: [
                GridItem(.flexible(), spacing: DS.Space.space4),
                GridItem(.flexible(), spacing: DS.Space.space4),
            ],
            alignment: .leading,
            spacing: DS.Space.space6
        ) {
            ForEach(catalog.myWorks) { item in
                NavigationLink(value: Route.canvas(assetID: item.assetID)) {
                    WorkCard(item: item, action: {})
                }
                .buttonStyle(.plain)
            }
        }
    }

    /// 沒有 onboarding，所以這一頁得自己解釋它是什麼。
    private var empty: some View {
        GalleryPlaceholder(
            systemImage: "paintpalette",
            message: "你上過色的作品都收在這裡——先去圖庫挑一張線稿開始。"
        ) {
            Button(action: onBrowse) {
                HStack(spacing: DS.Space.space2) {
                    Text("去圖庫看看")
                        .font(DS.Typography.body)
                        .fontWeight(.semibold)
                    Image(systemName: "arrow.right").font(.system(size: 15))
                }
                .foregroundStyle(DS.Color.surface)
                .padding(.horizontal, DS.Space.space5)
                .frame(height: 46)
                .background(Capsule().fill(DS.Color.ink))
            }
            .buttonStyle(.plain)
        }
        .padding(.top, DS.Space.space6 * 2)
    }
}
