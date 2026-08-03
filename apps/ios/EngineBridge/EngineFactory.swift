//
//  EngineFactory.swift
//  EngineBridge
//

import Foundation

/// 建引擎的**唯一**入口。Shell 呼叫它，然後就只認得 `any EngineProtocol`。
///
/// 為什麼選擇邏輯在 Bridge 而不在 `ColorApp.swift`（`specs/ios-scaffold.md §6` 的原始寫法）：
/// 驗收要求「App Shell 端沒有任何一行直接引用 `RustEngine`」，而 `§9` 的 `lint-ios`
/// 是純文字檢查。`RustEngineAdapter` 這個字面上就含有 `RustEngine`——只要 Shell 寫得出
/// 這個型別名，機械檢查就不可能同時是嚴格的又是通得過的。
///
/// 把 switch 收進來之後兩邊都成立：lint 保持最笨最嚴格的形式，而 Shell 連「有一個 Rust 實作」
/// 這件事都不需要知道。`-engine rust` 這條路仍然是「換引擎 Shell 零修改」的可執行證明。
public enum EngineFactory {
    /// `-engine rust` 切到真的 FFI；其餘（含未指定）一律 `MockEngine`。
    ///
    /// 預設是 Mock 而不是 Rust：S0 的 `RustEngine` 需要一個存在的 pack 檔，
    /// 而 `.colorpack` 格式要到 M1 才有。
    public static func make(packPath: String?) -> any EngineProtocol {
        guard UserDefaults.standard.string(forKey: "engine") == "rust" else {
            return MockEngine()
        }
        guard let packPath else {
            assertionFailure("-engine rust 需要一個 pack 路徑")
            return MockEngine()
        }
        do {
            return try RustEngineAdapter(packPath: packPath)
        } catch {
            assertionFailure("建立 Rust 引擎失敗：\(error)")
            return MockEngine()
        }
    }
}
