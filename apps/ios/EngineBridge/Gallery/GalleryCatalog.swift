//
//  GalleryCatalog.swift
//  EngineBridge
//
//  圖庫的**資料層契約**（`docs/specs/S1-ios-ui.md §2`）。
//  S1 全程對著 `FixtureCatalog`；真正的目錄來源（R2 版本化 JSON ＋ 下載器）另走一輪 spec。
//

import Foundation
import Observation

// MARK: - DTO

/// 圖庫裡的一張線稿。**這是投影，不是儲存格式**——`.colorpack` 是不可變的資產，
/// 而 `download` / `work` 是本機狀態，兩者在這裡合流成 UI 要的一個型別。
public struct GalleryItem: Identifiable, Hashable, Sendable {
    public var id: String { assetID }

    public let assetID: String
    public let title: String
    /// 線稿作者。免費包可能沒有署名，所以是 optional。
    public let credit: String?
    public let categoryID: String
    public let difficulty: Difficulty
    /// 封閉區數量。**不上卡片**——`Card States · Download` 的 note 明講副標不顯示數字，
    /// 改用一個知覺性的難度詞。留著是因為它是 pack 的真實欄位，之後的篩選會用到。
    public let regionCount: UInt32
    public let entitlement: Entitlement
    public let download: DownloadState
    /// `nil` ＝ 從未開始。有值就代表本機存在一份文件。
    public let work: WorkState?
    public let lastEditedAt: Date?

    public init(
        assetID: String,
        title: String,
        credit: String? = nil,
        categoryID: String,
        difficulty: Difficulty,
        regionCount: UInt32,
        entitlement: Entitlement,
        download: DownloadState,
        work: WorkState? = nil,
        lastEditedAt: Date? = nil
    ) {
        assert(
            !Self.isImpossible(download: download, work: work),
            "未下載 × 進行中：本機有文件就必然下載過。assetID=\(assetID)"
        )
        self.assetID = assetID
        self.title = title
        self.credit = credit
        self.categoryID = categoryID
        self.difficulty = difficulty
        self.regionCount = regionCount
        self.entitlement = entitlement
        self.download = download
        self.work = work
        self.lastEditedAt = lastEditedAt
    }
}

extension GalleryItem {
    public static func isImpossible(download: DownloadState, work: WorkState?) -> Bool {
        download == .notDownloaded && work != nil
    }
}

public enum Difficulty: String, CaseIterable, Hashable, Sendable {
    case easy
    case medium
    case focus
}

public enum Entitlement: String, CaseIterable, Hashable, Sendable {
    case free
    case paid
}

/// 資產在本機的下載狀態。
public enum DownloadState: Hashable, Sendable {
    case notDownloaded
    /// `0...1`。
    case downloading(Double)
    case downloaded
    case failed(reason: String?)
}

/// 本機文件的進度。`nil`（沒有 `WorkState`）＝ 從未開始，這是第三種情況，不是這個 enum 的成員。
public enum WorkState: Hashable, Sendable {
    /// `0...1`。
    case inProgress(progress: Double)
    /// 已經分享過。仍然可以繼續畫，所以照樣帶進度。
    case shared(progress: Double)

    public var progress: Double {
        switch self {
        case .inProgress(let p), .shared(let p): p
        }
    }
}

// MARK: - 鎖定是衍生值

extension GalleryItem {
    public func isLocked(isSubscribed: Bool) -> Bool {
        entitlement == .paid && !isSubscribed && work == nil
    }
}

// MARK: - Catalog

public enum LoadState: Hashable, Sendable {
    case loading
    case ready
    case failed(String)
}

/// Shell 對圖庫目錄的唯一依賴。
///
/// 實作為 `@Observable`，與 `MockEngine` 同一套觀察機制——View 讀 `items` / `loadState`
/// 會被 observation tracking 記錄，不需要 Combine。
public protocol GalleryCatalog: AnyObject {
    var items: [GalleryItem] { get }
    var loadState: LoadState { get }
    func refresh() async
}

/// 兩個分頁是**同一份 `items` 的兩種投影**，不做兩份資料源。
/// 投影規則放這裡而不是各 View，是為了讓「排序依據」只有一處可改。
extension GalleryCatalog {
    public var exploreSections: [(categoryID: String, items: [GalleryItem])] {
        var order: [String] = []
        var buckets: [String: [GalleryItem]] = [:]
        for item in items {
            if buckets[item.categoryID] == nil { order.append(item.categoryID) }
            buckets[item.categoryID, default: []].append(item)
        }
        return order.map { ($0, buckets[$0] ?? []) }
    }
    public var myWorks: [GalleryItem] {
        items
            .filter { $0.work != nil }
            .sorted { ($0.lastEditedAt ?? .distantPast) > ($1.lastEditedAt ?? .distantPast) }
    }
}
