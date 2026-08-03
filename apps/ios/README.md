# apps/ios

Colorlull 的 iOS app。規格見 `docs/specs/ios-scaffold.md`。

## 前置步驟：先跑一次 `cargo xtask ios`

`Generated/` 是 gitignore 的產物（uniffi binding ＋ `.xcframework`），**clone 完不會有**。
開 Xcode 之前先在 repo 根目錄跑：

```bash
cargo xtask ios
```

它產出：

```
Generated/ColorlullEngine.xcframework   ← EngineBridge 連的靜態庫（arm64 device ＋ arm64 sim）
Generated/Sources/colorlull_engine.swift ← 編進 EngineBridge target
Generated/Headers/                       ← modulemap ＋ FFI header
ColorApp/Resources/dev.colorpack         ← 開發用資產（順便 bake，見「換引擎」）
```

沒跑就 build 的話會擋在兩個地方，兩個都是明確錯誤而不是一堆型別找不到：

| 少了什麼 | 誰擋下來 |
|---|---|
| 整個 `Generated/` | Xcode 自己：`There is no XCFramework found at .../Generated/ColorlullEngine.xcframework` |
| 只少 `Generated/Sources/` | `EngineBridge` 的「Generated/ 前置檢查」腳本，訊息直接叫你跑 `cargo xtask ios` |

**bootstrap 不代跑 cargo**（`specs/ios-scaffold.md §2`）：自動跑 `cargo xtask ios` 會讓每次
Xcode build 都可能變成一趟 release 編譯，build 時間不可預測。CI 上 `cargo xtask ios` 本來就
排在 `xcodebuild` 前面，天然滿足。

## 三個 target

```
ColorApp            App Shell。只 import EngineBridge，只依賴 EngineProtocol
EngineBridge        framework。EngineProtocol / MockEngine / RustEngineAdapter / EngineCanvasView
EngineBridgeTests   unit test bundle
```

依賴鏈：`ColorApp` → `EngineBridge.framework` → `ColorlullEngine.xcframework`
（static library slices，**link 但不 embed**）。

每個 target 一個資料夾，全部掛 file-system synchronized group（Xcode 16+），
所以**新增 Swift 檔完全不動 `project.pbxproj`**——它只在改 build setting 時才進 diff。

## 換引擎（＝在真機上實際畫畫）

預設 `MockEngine`。要跑真的 FFI，**選 `ColorApp (rust)` 這支 scheme**——它就是
`ColorApp` 加上 launch argument `-engine rust`，Run-only、沒有 testables。

（`ColorApp` scheme 裡也有同一條 argument，預設關閉。不把它翻開是因為那支 scheme 的
TestAction 帶 `shouldUseLaunchSchemeArgsEnv`，翻下去會連 CI 的 host app 行為一起改。）

`-engine rust` 需要一顆真的 `.colorpack`：E1 起 `Engine::new` 會實際解析格式，
S0 那顆「隨便一個存在的檔」已經餵不進去。`cargo xtask ios` 會順便 bake
`ColorApp/Resources/dev.colorpack`（來源 `assets/source/kirby-demo-1`），要換素材：

```bash
cargo xtask dev-pack assets/source/torture-01
```

這顆 pack 跟 `assets/packs/` 一樣**不進 git**（`architecture.md §12.2`）。
沒 bake 就開 `-engine rust`，`EngineFactory` 的 `assertionFailure` 會直接說。

**必須用真機。** `attach_surface` 要一個真的 `CAMetalLayer`；模擬器上 surface 建不起來，
畫面會停在錯誤態（不 crash，這是 `E1-input §8` 的設計）。

畫布下方那排工具／色票是 **Debug 建置限定的 E1 測試 harness**，不是產品 UI——
`CanvasScreen.DebugToolBar`。沒有它就切不到油漆桶（`touchesBegan` 要 `state.tool`
是 `.bucket` 才走 `tap()`，而預設是 `Brush`）。產品的工具列在 S1。

Shell 程式碼一行不用改——選哪個實作是 `EngineFactory` 的事。這條路由
`cargo xtask lint-ios` 守著：`apps/ios/ColorApp/**` 不得出現 `RustEngine`，
也不得 `import colorlull_engine`。

## 跑測試

```bash
xcodebuild test -project ColorApp.xcodeproj -scheme ColorApp \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro'
```

CI 只跑 `build-for-testing`（不 boot 模擬器）。測試本身在本機跑。

其中 `testMockAndRustProduceIdenticalStateSequences` 是差分測試——同一串操作餵給兩個實作，
`UiState` 序列必須逐一相同。它是唯一防止 `MockEngine` 慢慢漂離 Rust 行為的機制，
改任一邊的行為時它應該要會失敗。
