//
//  ExploreTab.swift
//  ColorApp
//
//  設計稿：`Gallery`、`Gallery States` 的「Search · no results」與「Browse · loading」。
//

import EngineBridge
import SwiftUI

/// 探索分頁：搜尋 ＋ 分類 ＋ 依分類分組的卡片，最後接一段「我的作品」的近況。
///
/// 搜尋與分類都是**這一層的篩選**，不是目錄的狀態——`GalleryCatalog` 只負責供給
/// 全量 `items`，怎麼過濾是畫面的事。
struct ExploreTab: View {
    let catalog: FixtureCatalog
    @Binding var query: String
    @Binding var category: String?
    let isSubscribed: Bool
    let onOpenSettings: () -> Void
    let onOpenSubscription: () -> Void

    private static let cardWidth: CGFloat = 150
    private static let workCardWidth: CGFloat = 167

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DS.Space.space5) {
                GalleryHeader(title: "圖庫", onAvatarTap: onOpenSettings)
                SearchRow(query: $query)

                if catalog.loadState == .loading {
                    // 骨架期間 chips 也還沒有真實分類可畫，所以整段跳過。
                    loadingSkeleton
                } else if filtered.isEmpty {
                    // 無結果時 chips 仍要在下方**可點**——它是第二條路（`Gallery States` 第二張）。
                    noResults
                    CategoryChips(categories: categories, selected: $category)
                } else {
                    CategoryChips(categories: categories, selected: $category)
                    sections
                    myWorksPreview
                }
            }
            .padding(.horizontal, DS.Space.space5)
            .padding(.bottom, DS.Space.space6)
        }
        .scrollDismissesKeyboard(.immediately)
    }

    // MARK: - 內容

    private var sections: some View {
        ForEach(sectionsData, id: \.categoryID) { section in
            VStack(alignment: .leading, spacing: DS.Space.space3) {
                SectionHead(title: Self.categoryName(section.categoryID)) {
                    Button(action: onOpenSubscription) {
                        HStack(spacing: 2) {
                            Text("看全部").font(DS.Typography.caption)
                            Image(systemName: "chevron.right").font(.system(size: 10))
                        }
                        .foregroundStyle(DS.Color.accent)
                    }
                    .buttonStyle(.plain)
                }

                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: DS.Space.space4) {
                        ForEach(section.items) { item in
                            NavigationLink(value: Route.canvas(assetID: item.assetID)) {
                                DownloadCard(
                                    item: item, isSubscribed: isSubscribed, action: {}
                                )
                                .frame(width: Self.cardWidth)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    // 卡片切齊左緣，但捲動時要能滑出內容區——所以用 padding 而不是縮 ScrollView。
                    .padding(.horizontal, DS.Space.space5)
                }
                .padding(.horizontal, -DS.Space.space5)
            }
        }
    }

    /// 探索分頁底部的「我的作品」近況。完整清單在另一個分頁，這裡只露最近兩張。
    @ViewBuilder
    private var myWorksPreview: some View {
        let works = Array(catalog.myWorks.prefix(2))
        if !works.isEmpty {
            VStack(alignment: .leading, spacing: DS.Space.space3) {
                SectionHead(title: "我的作品", count: catalog.myWorks.count) {
                    HStack(spacing: 2) {
                        Text("最近").font(DS.Typography.caption)
                        Image(systemName: "arrow.up.arrow.down").font(.system(size: 10))
                    }
                    .foregroundStyle(DS.Color.muted)
                }
                HStack(alignment: .top, spacing: DS.Space.space4) {
                    ForEach(works) { item in
                        NavigationLink(value: Route.canvas(assetID: item.assetID)) {
                            WorkCard(item: item, action: {})
                                .frame(width: Self.workCardWidth)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }

    private var noResults: some View {
        GalleryPlaceholder(
            systemImage: "magnifyingglass",
            title: "沒有符合「\(query)」的圖包",
            message: "換一個短一點的詞，或從下面的分類挑一個。",
            action: { EmptyView() }
        )
        .padding(.vertical, DS.Space.space6)
    }

    private var loadingSkeleton: some View {
        VStack(alignment: .leading, spacing: DS.Space.space3) {
            SectionHead(title: "本週新上架") { EmptyView() }
            HStack(alignment: .top, spacing: DS.Space.space4) {
                ForEach(0..<3, id: \.self) { _ in
                    CardSkeleton(width: Self.cardWidth, thumbHeight: 172)
                }
            }
        }
    }

    // MARK: - 篩選

    private var filtered: [GalleryItem] {
        catalog.items.filter { item in
            let matchesCategory = category == nil || item.categoryID == category
            let matchesQuery = query.isEmpty
                || item.title.localizedCaseInsensitiveContains(query)
            return matchesCategory && matchesQuery
        }
    }

    /// 分組沿用 `GalleryCatalog.exploreSections` 的規則，只是先過濾。
    private var sectionsData: [(categoryID: String, items: [GalleryItem])] {
        var order: [String] = []
        var buckets: [String: [GalleryItem]] = [:]
        for item in filtered {
            if buckets[item.categoryID] == nil { order.append(item.categoryID) }
            buckets[item.categoryID, default: []].append(item)
        }
        return order.map { ($0, buckets[$0] ?? []) }
    }

    /// chips 的來源是**全量** `items`，不是 `filtered`——否則選了一個分類之後
    /// 其他分類就消失，使用者被困在裡面出不來。
    private var categories: [String] {
        var seen: Set<String> = []
        return catalog.items.compactMap { seen.insert($0.categoryID).inserted ? $0.categoryID : nil }
    }

    /// S1 的分類名寫死在這裡。i18n 那一輪會換成字串表，目錄 JSON 給的是 ID。
    static func categoryName(_ id: String) -> String {
        switch id {
        case "animals": "動物"
        case "florals": "花草"
        case "mandala": "曼陀羅"
        default: id
        }
    }
}

// MARK: - 搜尋列與分類

private struct SearchRow: View {
    @Binding var query: String
    @FocusState private var isFocused: Bool

    var body: some View {
        HStack(spacing: DS.Space.space2) {
            HStack(spacing: DS.Space.space2) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 17))
                    .foregroundStyle(isFocused ? DS.Color.ink : DS.Color.muted)
                TextField("搜尋貓咪、花草、曼陀羅", text: $query)
                    .font(DS.Typography.body)
                    .foregroundStyle(DS.Color.ink)
                    .focused($isFocused)
                    .submitLabel(.search)
                if !query.isEmpty {
                    Button {
                        query = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .font(.system(size: 15))
                            .foregroundStyle(DS.Color.muted)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("清除搜尋")
                }
            }
            .padding(.horizontal, DS.Space.space4)
            .frame(height: 46)
            .background(
                Capsule()
                    .fill(DS.Color.surface)
                    // 聚焦時是 accent 環（`Gallery · Controls`）。
                    .overlay(
                        Capsule().strokeBorder(
                            isFocused ? DS.Color.accent : .clear, lineWidth: 1.5
                        )
                    )
            )

            Button {
                // 篩選面板在 S1 之後。按鈕先在版面上，不做假的彈窗。
            } label: {
                Image(systemName: "line.3.horizontal.decrease")
                    .font(.system(size: 18))
                    .foregroundStyle(DS.Color.surface)
                    .frame(width: 46, height: 46)
                    .background(Circle().fill(DS.Color.ink))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("篩選")
        }
    }
}

private struct CategoryChips: View {
    let categories: [String]
    @Binding var selected: String?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: DS.Space.space2) {
                chip(title: "全部", isSelected: selected == nil) { selected = nil }
                ForEach(categories, id: \.self) { id in
                    chip(title: ExploreTab.categoryName(id), isSelected: selected == id) {
                        selected = selected == id ? nil : id
                    }
                }
            }
            .padding(.horizontal, DS.Space.space5)
        }
        .padding(.horizontal, -DS.Space.space5)
    }

    private func chip(
        title: String, isSelected: Bool, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(title)
                .font(DS.Typography.body)
                .foregroundStyle(isSelected ? DS.Color.surface : DS.Color.muted)
                .padding(.horizontal, DS.Space.space4)
                .frame(height: 34)
                .background(Capsule().fill(isSelected ? DS.Color.ink : .clear))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }
}
