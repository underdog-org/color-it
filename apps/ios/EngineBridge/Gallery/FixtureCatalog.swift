//
//  FixtureCatalog.swift
//  EngineBridge
//
//  **`.populated` 的建構方式是**逐一列舉合法狀態組合**，
//

import Foundation
import Observation

/// 卡片的狀態組合。**驗收清單本身就是這個型別的值域**。
public struct StateCombination: Hashable, Sendable, CaseIterable {
    public let entitlement: Entitlement
    public let download: DownloadKind
    public let work: WorkKind

    /// `DownloadState` 的 case 標籤。組合空間要的是「哪一種」，不是進度數值。
    public enum DownloadKind: String, CaseIterable, Hashable, Sendable {
        case notDownloaded, downloading, downloaded, failed
    }

    /// `WorkState?` 的 case 標籤，含 `nil`（＝從未開始）那一格。
    public enum WorkKind: String, CaseIterable, Hashable, Sendable {
        case notStarted, inProgress, shared
    }

    public init(entitlement: Entitlement, download: DownloadKind, work: WorkKind) {
        self.entitlement = entitlement
        self.download = download
        self.work = work
    }

    public static var allCases: [StateCombination] {
        var result: [StateCombination] = []
        for entitlement in Entitlement.allCases {
            for download in DownloadKind.allCases {
                for work in WorkKind.allCases {
                    guard !(download == .notDownloaded && work != .notStarted) else { continue }
                    result.append(
                        StateCombination(entitlement: entitlement, download: download, work: work)
                    )
                }
            }
        }
        return result
    }
}

extension GalleryItem {
    /// 反向投影：一張卡片落在哪一格。測試拿它比對涵蓋率。
    public var stateCombination: StateCombination {
        let downloadKind: StateCombination.DownloadKind = switch download {
        case .notDownloaded: .notDownloaded
        case .downloading: .downloading
        case .downloaded: .downloaded
        case .failed: .failed
        }
        let workKind: StateCombination.WorkKind = switch work {
        case nil: .notStarted
        case .inProgress: .inProgress
        case .shared: .shared
        }
        return StateCombination(
            entitlement: entitlement, download: downloadKind, work: workKind
        )
    }
}

/// 假目錄。切換 `Scenario` 就換一整份 `items` ＋ `loadState`。
///
/// **切換入口是 Debug build 限定**，不是產品 UI（`S1-ios-ui.md §2.4`）。
@Observable
public final class FixtureCatalog: GalleryCatalog {
    public enum Scenario: String, CaseIterable, Sendable {
        /// 涵蓋**每一個**合法狀態組合，S1 驗收的主力。
        case populated
        case myWorksEmpty
        /// 搜尋無結果。查詢字串由 `initialSearchQuery` 帶出來——
        /// 目錄本身沒有「搜尋」的概念，是畫面的狀態。
        case searchNoResults
        /// 探索載入中骨架。
        case browseLoading

        public var displayName: String {
            switch self {
            case .populated: "完整狀態"
            case .myWorksEmpty: "我的作品：空"
            case .searchNoResults: "搜尋：無結果"
            case .browseLoading: "探索：載入中"
            }
        }
    }

    public private(set) var scenario: Scenario
    public private(set) var items: [GalleryItem] = []
    public private(set) var loadState: LoadState = .loading

    /// 只有 `.searchNoResults` 會給值。Debug 的場景切換器拿它去 seed 搜尋框。
    public private(set) var initialSearchQuery: String?

    public init(scenario: Scenario = .populated) {
        self.scenario = scenario
        apply(scenario)
    }

    /// Debug 場景切換。產品沒有這條路徑。
    public func select(_ scenario: Scenario) {
        self.scenario = scenario
        apply(scenario)
    }

    /// 假目錄沒有網路，但時序要一致：先回 `.loading`，讓骨架真的被看見。
    public func refresh() async {
        loadState = .loading
        try? await Task.sleep(for: .milliseconds(400))
        apply(scenario)
    }

    private func apply(_ scenario: Scenario) {
        switch scenario {
        case .populated:
            items = Self.combinationCards + Self.edgeCases
            loadState = .ready
            initialSearchQuery = nil
        case .myWorksEmpty:
            items = Self.combinationCards.filter { $0.work == nil }
            loadState = .ready
            initialSearchQuery = nil
        case .searchNoResults:
            items = Self.combinationCards
            loadState = .ready
            initialSearchQuery = "octopus"
        case .browseLoading:
            items = []
            loadState = .loading
            initialSearchQuery = nil
        }
    }

    // MARK: - 組合卡

    /// 每個合法組合一張。標題／分類／難度只是為了讓畫面不至於全部長一樣，
    /// 輪替取值——它們不影響涵蓋率，涵蓋率看的是 `stateCombination`。
    static let combinationCards: [GalleryItem] = {
        let titles = [
            "Sweet Bakery", "Wild Garden", "Moon Mandala", "Paper Bloom",
            "Bakery Cat", "Summer Bloom", "Night Fern", "Tide Pool",
            "Clay Pot", "Winter Hare",
        ]
        let categories = ["animals", "florals", "mandala"]
        let difficulties = Difficulty.allCases

        return StateCombination.allCases.enumerated().map { index, combo in
            let progress = FixtureCatalog.progress(for: index)
            return GalleryItem(
                assetID: "fixture-\(index)",
                title: titles[index % titles.count],
                credit: index.isMultiple(of: 3) ? "Nina Štajner" : nil,
                categoryID: categories[index % categories.count],
                difficulty: difficulties[index % difficulties.count],
                regionCount: UInt32(24 + index * 7),
                entitlement: combo.entitlement,
                download: FixtureCatalog.downloadState(combo.download, progress: progress),
                work: FixtureCatalog.workState(combo.work, progress: progress),
                lastEditedAt: combo.work == .notStarted
                    ? nil
                    : FixtureCatalog.referenceDate.addingTimeInterval(-Double(index) * 3600)
            )
        }
    }()

    /// 設計稿另外畫過、但不屬於組合空間的邊界：進度條的兩個極端與過長標題
    /// （`Card States · Work` 的 note）。
    static let edgeCases: [GalleryItem] = [
        GalleryItem(
            assetID: "edge-progress-min",
            title: "Three Percent",
            credit: nil,
            categoryID: "mandala",
            difficulty: .focus,
            regionCount: 210,
            entitlement: .free,
            download: .downloaded,
            work: .inProgress(progress: 0.03),
            lastEditedAt: referenceDate.addingTimeInterval(-7200)
        ),
        GalleryItem(
            assetID: "edge-progress-max",
            title: "Ninety Seven Percent",
            credit: nil,
            categoryID: "mandala",
            difficulty: .focus,
            regionCount: 198,
            entitlement: .free,
            download: .downloaded,
            work: .inProgress(progress: 0.97),
            lastEditedAt: referenceDate.addingTimeInterval(-10800)
        ),
        GalleryItem(
            assetID: "edge-long-title",
            title: "A Very Long Artwork Title That Has To Wrap Onto A Second Line",
            credit: "Nina Štajner",
            categoryID: "florals",
            difficulty: .medium,
            regionCount: 64,
            entitlement: .paid,
            download: .downloaded,
            work: .shared(progress: 1.0),
            lastEditedAt: referenceDate.addingTimeInterval(-14400)
        ),
    ]

    /// 固定的參考時刻，讓「2 天前」這種相對敘述在 Preview 與測試裡穩定。
    /// 2026-01-01 00:00:00 UTC。
    static let referenceDate = Date(timeIntervalSince1970: 1_767_225_600)

    /// 讓進度散開一點，同時避開 0 與 1——兩個極端由 `edgeCases` 專門覆蓋。
    private static func progress(for index: Int) -> Double {
        0.15 + Double(index % 6) * 0.13
    }

    private static func downloadState(
        _ kind: StateCombination.DownloadKind, progress: Double
    ) -> DownloadState {
        switch kind {
        case .notDownloaded: .notDownloaded
        case .downloading: .downloading(progress)
        case .downloaded: .downloaded
        case .failed: .failed(reason: "network timeout")
        }
    }

    private static func workState(
        _ kind: StateCombination.WorkKind, progress: Double
    ) -> WorkState? {
        switch kind {
        case .notStarted: nil
        case .inProgress: .inProgress(progress: progress)
        case .shared: .shared(progress: progress)
        }
    }
}
