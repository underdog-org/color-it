//
//  GalleryCatalogTests.swift
//  EngineBridgeTests
//
//  `docs/specs/S1-ios-ui.md §5` 的四條。它們釘的都是**衍生規則**，不是渲染——
//  卡片長什麼樣由 SwiftUI Preview 對照設計稿，不在這裡。
//

import XCTest

@testable import EngineBridge

final class GalleryCatalogTests: XCTestCase {

    // MARK: - isLocked 的衍生規則

    /// 付費 × 未訂閱 × 無本機文件才鎖，其餘皆不鎖。
    func testIsLockedOnlyForPaidUnsubscribedWithoutLocalDocument() {
        for combination in StateCombination.allCases {
            for isSubscribed in [false, true] {
                let item = Self.item(for: combination)
                let expected = combination.entitlement == .paid
                    && !isSubscribed
                    && combination.work == .notStarted
                XCTAssertEqual(
                    item.isLocked(isSubscribed: isSubscribed),
                    expected,
                    "\(combination) / isSubscribed=\(isSubscribed)"
                )
            }
        }
    }

    // MARK: - 三個不可能組合

    /// 「未下載 × 進行中」——`init` 的 assert 判的述詞。
    ///
    /// 測述詞而不是測 `init`：assert 失敗直接 trap，XCTest 攔不到
    func testNotDownloadedWithLocalDocumentIsImpossible() {
        XCTAssertTrue(
            GalleryItem.isImpossible(download: .notDownloaded, work: .inProgress(progress: 0.5))
        )
        XCTAssertTrue(
            GalleryItem.isImpossible(download: .notDownloaded, work: .shared(progress: 1.0))
        )

        // 反面：其餘每一種下載狀態配上任何文件狀態都是合法的。
        for download: DownloadState in [
            .downloading(0.4), .downloaded, .failed(reason: nil),
        ] {
            for work: WorkState? in [nil, .inProgress(progress: 0.5), .shared(progress: 1.0)] {
                XCTAssertFalse(
                    GalleryItem.isImpossible(download: download, work: work),
                    "\(download) × \(String(describing: work))"
                )
            }
        }
        XCTAssertFalse(GalleryItem.isImpossible(download: .notDownloaded, work: nil))
    }

    /// 「鎖定 × 進行中」「鎖定 × 已分享」——**建構不出來**。
    func testLockedNeverCoexistsWithALocalDocument() {
        let itemsWithWork = (FixtureCatalog.combinationCards + FixtureCatalog.edgeCases)
            .filter { $0.work != nil }
        XCTAssertFalse(itemsWithWork.isEmpty, "樣本本身要有東西可測")

        for item in itemsWithWork {
            for isSubscribed in [false, true] {
                XCTAssertFalse(
                    item.isLocked(isSubscribed: isSubscribed),
                    "\(item.assetID) work=\(String(describing: item.work))"
                )
            }
        }
    }

    // MARK: - Fixture 涵蓋率

    /// `.populated` 涵蓋每一個合法狀態組合——以組合列舉逐一比對，不是目測。
    func testPopulatedCoversEveryLegalCombination() {
        let catalog = FixtureCatalog(scenario: .populated)
        let covered = Set(catalog.items.map(\.stateCombination))
        let expected = Set(StateCombination.allCases)

        XCTAssertEqual(expected.count, 20, "20 ＝ 2 × 4 × 3 扣掉 4 個「未下載 × 有文件」")
        XCTAssertTrue(
            expected.subtracting(covered).isEmpty,
            "沒被涵蓋的組合：\(expected.subtracting(covered))"
        )
        XCTAssertTrue(
            covered.subtracting(expected).isEmpty,
            "fixture 造出了不該存在的組合：\(covered.subtracting(expected))"
        )
    }

    /// 三個難度都要出現，否則 `Card States · Download` 的難度列對不上。
    func testPopulatedCoversEveryDifficulty() {
        let catalog = FixtureCatalog(scenario: .populated)
        XCTAssertEqual(
            Set(catalog.items.map(\.difficulty)), Set(Difficulty.allCases)
        )
    }

    // MARK: - 兩個分頁的投影

    func testMyWorksIsSortedByLastEditedDescending() {
        let catalog = FixtureCatalog(scenario: .populated)
        let dates = catalog.myWorks.map { $0.lastEditedAt ?? .distantPast }
        XCTAssertEqual(dates, dates.sorted(by: >))
        XCTAssertTrue(catalog.myWorks.allSatisfy { $0.work != nil })
    }

    func testMyWorksEmptyScenarioHasNoLocalDocuments() {
        let catalog = FixtureCatalog(scenario: .myWorksEmpty)
        XCTAssertTrue(catalog.myWorks.isEmpty)
        XCTAssertFalse(catalog.items.isEmpty, "探索分頁仍然要有東西")
    }

    func testBrowseLoadingScenarioIsLoadingAndEmpty() {
        let catalog = FixtureCatalog(scenario: .browseLoading)
        XCTAssertEqual(catalog.loadState, .loading)
        XCTAssertTrue(catalog.items.isEmpty)
    }

    func testSearchNoResultsScenarioSeedsAQueryThatMatchesNothing() throws {
        let catalog = FixtureCatalog(scenario: .searchNoResults)
        let query = try XCTUnwrap(catalog.initialSearchQuery)
        let matches = catalog.items.filter {
            $0.title.localizedCaseInsensitiveContains(query)
        }
        XCTAssertTrue(matches.isEmpty, "場景的前提就是這個查詢比不到東西")
    }

    func testExploreSectionsPartitionTheWholeCatalog() {
        let catalog = FixtureCatalog(scenario: .populated)
        let sections = catalog.exploreSections
        XCTAssertEqual(sections.reduce(0) { $0 + $1.items.count }, catalog.items.count)
        XCTAssertEqual(
            Set(sections.map(\.categoryID)), Set(catalog.items.map(\.categoryID))
        )
    }

    // MARK: -

    /// 把組合變回一張卡。數值不重要，只有 case 標籤要對。
    private static func item(for combination: StateCombination) -> GalleryItem {
        let download: DownloadState = switch combination.download {
        case .notDownloaded: .notDownloaded
        case .downloading: .downloading(0.4)
        case .downloaded: .downloaded
        case .failed: .failed(reason: nil)
        }
        let work: WorkState? = switch combination.work {
        case .notStarted: nil
        case .inProgress: .inProgress(progress: 0.4)
        case .shared: .shared(progress: 1.0)
        }
        return GalleryItem(
            assetID: "t-\(combination.hashValue)",
            title: "T",
            categoryID: "animals",
            difficulty: .easy,
            regionCount: 24,
            entitlement: combination.entitlement,
            download: download,
            work: work,
            lastEditedAt: work == nil ? nil : Date()
        )
    }
}
