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
    /// 預設仍是 Mock：Shell 的 UI 工作、preview 與單元測試都不該要求一顆 bake 好的
    /// pack。真機手感測試（E1 D3／D4）才需要 `-engine rust`，做法見 `apps/ios/README.md`。
    public static func make(packPath: String?) -> any EngineProtocol {
        guard UserDefaults.standard.string(forKey: "engine") == "rust" else {
            return MockEngine()
        }
        // 兩個 `assertionFailure` 都是刻意的：靜默退回 `MockEngine` 的症狀是
        // 「畫得動但沒有線稿」，那要花很久才會被認出來不是引擎壞了。
        guard let packPath else {
            assertionFailure("-engine rust 需要一顆 pack；先跑 `cargo xtask dev-pack`")
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
