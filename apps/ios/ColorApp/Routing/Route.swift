//
//  Route.swift
//  ColorApp
//

import Foundation

/// 只有**推進堆疊**的目的地在這裡。
///
/// `Settings` 與 `Subscription` 不在其中——它們走 `.sheet`：付費牆是 modal，
/// 設定也是進去就出來，兩者都不該堆在 Gallery → Canvas 的返回堆疊上
/// （`specs/ios-scaffold.md §6`）。
enum Route: Hashable {
    case canvas(assetID: String)
    case share
}

/// 兩個 modal。分開成 enum 是為了讓 `.sheet(item:)` 一次只認一個。
enum Sheet: String, Identifiable {
    case settings
    case subscription

    var id: String { rawValue }
}
