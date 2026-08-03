//
//  GalleryScreen.swift
//  ColorApp
//
//  設計稿：`Gallery`、`Gallery States` 三張。
//

import EngineBridge
import SwiftUI

/// Root。兩個分頁 ＋ 底部 tab bar。
///
/// **首次啟動直接進這裡，沒有 onboarding**（`prd.md §8`）。空狀態必須自己解釋自己，
/// 這就是 `Gallery States` 第一張的前提。
///
/// 目錄與訂閱狀態都握在這一層：兩個分頁是**同一份 `items` 的兩種投影**，
/// 分頁各自持有資料源會讓「下載完成」這種事件要同步兩次。
struct GalleryScreen: View {
    @Binding var sheet: Sheet?

    @State private var catalog = FixtureCatalog()
    @State private var tab: Tab = .explore
    @State private var query = ""
    @State private var category: String?

    /// S1 沒有訂閱後端（D6 在 W25）。這顆開關讓鎖定態在 Debug 下看得到兩面。
    @State private var isSubscribed = false

    enum Tab: Hashable {
        case explore
        case myWorks
    }

    var body: some View {
        VStack(spacing: 0) {
            content
            TabBarRow(tab: $tab)
        }
        .background(DS.Color.bg)
        .navigationBarHidden(true)
        .task { query = catalog.initialSearchQuery ?? "" }
        #if DEBUG
            .safeAreaInset(edge: .top, spacing: 0) {
                DebugScenarioBar(catalog: catalog, isSubscribed: $isSubscribed) {
                    query = catalog.initialSearchQuery ?? ""
                }
            }
        #endif
    }

    @ViewBuilder
    private var content: some View {
        switch tab {
        case .explore:
            ExploreTab(
                catalog: catalog,
                query: $query,
                category: $category,
                isSubscribed: isSubscribed,
                onOpenSettings: { sheet = .settings },
                onOpenSubscription: { sheet = .subscription }
            )
        case .myWorks:
            MyWorksTab(
                catalog: catalog,
                onBrowse: { tab = .explore },
                onOpenSettings: { sheet = .settings }
            )
        }
    }
}

/// 底部兩格。**只有兩格**——五條路由裡 Share 從 Canvas 進，Settings 與 Subscription
/// 是 modal，都不該佔一格 tab。
private struct TabBarRow: View {
    @Binding var tab: GalleryScreen.Tab

    var body: some View {
        HStack(spacing: 0) {
            item(.explore, systemImage: "square.grid.2x2.fill", label: "圖庫")
            item(.myWorks, systemImage: "paintpalette.fill", label: "作品")
        }
        .padding(DS.Space.space1)
        .background(
            Capsule()
                .fill(DS.Color.surface)
                .shadow(color: DS.Color.ink.opacity(0.12), radius: 10, y: 4)
        )
        .padding(.horizontal, DS.Space.space4)
        .padding(.bottom, DS.Space.space2)
    }

    private func item(
        _ value: GalleryScreen.Tab, systemImage: String, label: String
    ) -> some View {
        let isSelected = tab == value
        return Button {
            tab = value
        } label: {
            VStack(spacing: 2) {
                Image(systemName: systemImage).font(.system(size: 20))
                Text(label).font(DS.Typography.caption)
            }
            .foregroundStyle(isSelected ? DS.Color.ink : DS.Color.muted)
            .frame(maxWidth: .infinity)
            .frame(height: 44)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }
}

// MARK: - 兩個分頁共用的零件

/// 標題列。標題文字不同，其餘一樣。
struct GalleryHeader: View {
    let title: String
    let onAvatarTap: () -> Void

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: DS.Space.space1) {
                Text("歡迎回來")
                    .font(DS.Typography.caption)
                    .foregroundStyle(DS.Color.muted)
                Text(title)
                    .font(DS.Typography.display)
                    .foregroundStyle(DS.Color.ink)
            }
            Spacer()
            // 頭像就是 Settings 入口。**Canvas 沒有 Settings 入口**（`S1-ios-ui.md §0.2`）——
            // 那裡的 sliders icon 是筆刷參數。
            Button(action: onAvatarTap) {
                Text("B")
                    .font(DS.Typography.body)
                    .fontWeight(.semibold)
                    .foregroundStyle(DS.Color.surface)
                    .frame(width: 44, height: 44)
                    .background(Circle().fill(DS.Color.ink))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("設定")
        }
    }
}

/// 空狀態與無結果共用的版型：一顆圓形圖示 ＋ 一段句子 ＋ 選配的動作。
struct GalleryPlaceholder<Action: View>: View {
    let systemImage: String
    var title: String?
    let message: String
    @ViewBuilder let action: () -> Action

    var body: some View {
        VStack(spacing: DS.Space.space4) {
            Image(systemName: systemImage)
                .font(.system(size: 28))
                .foregroundStyle(DS.Color.muted)
                .frame(width: 72, height: 72)
                .background(Circle().fill(DS.Color.surface))

            if let title {
                Text(title)
                    .font(DS.Typography.title)
                    .foregroundStyle(DS.Color.ink)
                    .multilineTextAlignment(.center)
            }
            Text(message)
                .font(DS.Typography.body)
                .foregroundStyle(title == nil ? DS.Color.ink : DS.Color.muted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 318)

            action()
        }
        .frame(maxWidth: .infinity)
    }
}

/// 骨架。**只有卡片格是佔位色，chrome 立刻就畫**（`Gallery States` 第三張的 caption）——
/// 整頁一起淡入會顯得比實際更慢。
struct CardSkeleton: View {
    let width: CGFloat
    let thumbHeight: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.space3) {
            RoundedRectangle(cornerRadius: DS.Radius.md, style: .continuous)
                .fill(DS.Color.line)
                .frame(height: thumbHeight)
            VStack(alignment: .leading, spacing: DS.Space.space1) {
                Capsule().fill(DS.Color.line).frame(height: 11)
                Capsule().fill(DS.Color.line).frame(width: 64, height: 9)
            }
        }
        .frame(width: width)
    }
}

/// 區塊標題列。右邊那顆是選配的次要動作。
struct SectionHead<Trailing: View>: View {
    let title: String
    var count: Int?
    @ViewBuilder let trailing: () -> Trailing

    var body: some View {
        HStack(spacing: DS.Space.space2) {
            Text(title)
                .font(DS.Typography.title)
                .foregroundStyle(DS.Color.ink)
            if let count {
                Text("\(count)")
                    .font(DS.Typography.caption)
                    .foregroundStyle(DS.Color.muted)
                    .padding(.horizontal, DS.Space.space2)
                    .frame(height: 20)
                    .background(Capsule().fill(DS.Color.line))
            }
            Spacer()
            trailing()
        }
    }
}

#if DEBUG

    /// 場景切換。**不是產品 UI**（`S1-ios-ui.md §2.4`）——它存在的唯一理由是
    /// S1 驗收要「以注入的假狀態」看過全部卡片狀態組合與三個空狀態。
    private struct DebugScenarioBar: View {
        let catalog: FixtureCatalog
        @Binding var isSubscribed: Bool
        let onChange: () -> Void

        var body: some View {
            HStack(spacing: DS.Space.space2) {
                Picker("場景", selection: scenarioBinding) {
                    ForEach(FixtureCatalog.Scenario.allCases, id: \.self) { scenario in
                        Text(scenario.displayName).tag(scenario)
                    }
                }
                .pickerStyle(.menu)

                Toggle("已訂閱", isOn: $isSubscribed)
                    .toggleStyle(.button)
                    .font(DS.Typography.caption)
            }
            .padding(.horizontal, DS.Space.space4)
            .padding(.vertical, DS.Space.space1)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(DS.Color.line)
        }

        private var scenarioBinding: Binding<FixtureCatalog.Scenario> {
            Binding(
                get: { catalog.scenario },
                set: {
                    catalog.select($0)
                    onChange()
                }
            )
        }
    }

#endif
