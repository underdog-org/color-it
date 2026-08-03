# iOS 骨架規格（S0）

> 涵蓋 S0 的 **iOS 側**：Xcode 專案、`EngineBridge` framework target、`EngineProtocol` ＋
> `MockEngine` ＋ `RustEngineAdapter`、五條路由空殼、Swift 端的測試與 CI gate。
> 對應 `architecture.md §10`、`roadmap/S0.md`、`docs/contracts.md`。
>
> **不涵蓋** Rust 契約面（`specs/ffi-contract.md`）、InputAdapter、FrameDriver、任何真的渲染。
>
> 文中裸寫的 `§n` 一律指 `architecture.md`；`C1`–`C8` 指 `docs/contracts.md ③`。

環境基準：Xcode 26.6、iOS SDK 26.5、uniffi 0.29.5。

## 1. 兩個文件裡的警告已經不成立

接手前先消掉這兩條，否則會照著解決不存在的問題：

**`S0.md` 的「⚠️ 接手 iOS 骨架時第一件事」（modulemap 的 `framework` 關鍵字）已在 Rust 側繞開。**
`xtask/src/ios.rs` 寫死 `xcframework: false`，實際產出是 `module colorlull_engineFFI`，
沒有 `framework` 關鍵字，那個 220-error 的情境不會發生。剩下的真風險縮小成
「Xcode target 怎麼正確引用一個 library-slice 的 xcframework」——由第 7 節的測試 1 守。

**`ffi-contract.md §6` 的「`EngineProtocol` 撞名」不成立。**
uniffi 0.29.5 生成的 protocol 叫 `RustEngineProtocol`（Object 名 ＋ `Protocol`），
不是 `EngineProtocol`。手寫的 `EngineProtocol` 這個名字是空的。

## 2. 專案佈局

```
apps/ios/
  ColorApp.xcodeproj/          ← 進 git，唯一手工維護的檔
  ColorApp/                    ← target 1：App Shell
    ColorApp.swift             @main ＋ 引擎注入
    Routing/{Route.swift, AppRouter.swift}
    Gallery/GalleryScreen.swift
    Canvas/CanvasScreen.swift
    Share/ShareScreen.swift
    Subscription/SubscriptionScreen.swift
    Settings/SettingsScreen.swift
    Resources/{Assets.xcassets, mock-lineart.png}
  EngineBridge/                ← target 2：framework
    EngineProtocol.swift
    MockEngine.swift
    RustEngineAdapter.swift
    EngineCanvasView.swift
  EngineBridgeTests/           ← target 3：unit test bundle
  Generated/                   ← gitignore；`cargo xtask ios` 產生
  README.md                    ← 前置步驟
```

依賴鏈：`ColorApp` → `EngineBridge.framework` → `ColorlullEngine.xcframework`
（static library slices，**link 但不 embed**）。

四個決定：

- **每個 target 一個資料夾，全部用 file-system synchronized group**
  （`PBXFileSystemSynchronizedRootGroup`，Xcode 16+）。這是手工維護 `.xcodeproj` 能撐住的前提：
  新增 Swift 檔完全不動 `project.pbxproj`，pbxproj 只在改 build setting 時才進 diff。
  `Generated/Sources` 掛成第三個 synchronized group，歸 target 2。
- **`xcuserdata/` 必須 gitignore。** Xcode 預設會把 `UserInterfaceState.xcuserstate` 之類
  的每次開關都在變的二進位檔放進專案目錄，它們不是專案設定。
- **uniffi 生成的型別由 `EngineBridge` 轉出。** `Generated/Sources/colorlull_engine.swift`
  直接編進 target 2，所以 `Tool` / `UiState` / `Rgba` / `InputSample` / `Transform` /
  `SurfaceHandle` / `BrushId` / `EngineError` 這些 `public` 型別自然成為 `EngineBridge`
  module 的一部分。Shell 只寫 `import EngineBridge`，永不 `import colorlull_engine`。
  **Bridge 不重新定義任何 DTO**——那會違反 `§7`「一份契約只能存在一次」。
- **bootstrap 不代跑 cargo。** `EngineBridge` 加一個排在編譯前的 Run Script phase，
  只做「`Generated/` 不存在就以人話訊息 fail」。自動跑 `cargo xtask ios` 會讓每次 Xcode build
  都可能變成一趟 release 編譯，build 時間不可預測。前置步驟寫在 `apps/ios/README.md`；
  CI 上 `cargo xtask ios` 本來就排在 `xcodebuild` 前面，天然滿足。

### Mock 素材不走 `assets/source/`

那裡是 Git LFS，而 CI 兩個 job 都是 `lfs: false`——直接引用會讓 macOS build 拿到 pointer 檔
而失敗。改放一張長邊 1024 的 `mock-lineart.png` 進 `ColorApp/Resources/`。
來源是 `assets/source/kirby-demo-1/lineart.png`（避開 `adventure-time-demo-1`，
它的 flats 還待重做）。`.gitattributes` 不用改——LFS 規則本來就只轄
`assets/source/**`，放在 `apps/ios/` 底下的 PNG 自動是普通 git 檔。

S0 只需要證明畫布路徑會顯示東西，不需要原始解析度；M1 之後這張圖被真的 `.colorpack` 取代。

## 3. `EngineProtocol`

以 `contracts.md ②` 為對照基準逐項對應。全用 uniffi 生成的型別。

```swift
public protocol EngineProtocol: AnyObject {
    var state: UiState { get }                                  // ← state()

    func attachSurface(_ handle: SurfaceHandle) throws
    func resizeSurface(widthPx: UInt32, heightPx: UInt32, scale: Float)
    func detachSurface()

    func setTool(_ tool: Tool)
    func pickColor(x: Float, y: Float) -> Rgba

    func beginStroke(_ s: InputSample)
    func appendSamples(_ s: [InputSample])
    func endStroke()
    func cancelStroke()
    func tap(x: Float, y: Float)

    func undo(); func redo()
    func render()
    func setViewport(_ transform: Transform)

    func save() throws
    func exportPNG() throws -> Data
    func exportTimelapse() throws -> Data

    func makeCanvasView() -> UIView                             // C7，不在 FFI
}
```

`state` 用 `@Observable`（兩個實作都標），不用 `AnyPublisher`——`§10.1` 的草案是
Combine 時代的寫法，Shell 因此連 `import Combine` 都不需要，view 經
`any EngineProtocol` 存取仍能觸發 observation tracking。`§10.1` 要回寫。

### 三個記名的例外

驗收「逐一對照無缺漏」會把這三項誤判成缺漏，所以寫成契約的一部分：

| FFI | 為什麼不在 protocol |
|---|---|
| `new(pack_path, doc_path)` | 建構不是抽象的一部分——`MockEngine()` 沒有 pack。對應 `RustEngineAdapter.init(packPath:docPath:) throws` |
| `set_state_listener(opt)` | 它是**實作 `state` 的手段**，不是 Shell 的介面。Shell 要的是「狀態會自己更新」，不是訂閱機制 |
| `makeCanvasView()` | 反向：Bridge 有、FFI 沒有（C7 已授權） |

`attachSurface` / `resizeSurface` / `detachSurface` 留在 protocol（驗收明文要求），
但 **Shell 一行都不呼叫**——只有 `EngineCanvasView` 呼叫它們。

## 4. `RustEngineAdapter` 的三個非顯而易見處

**① listener 必須用 weak box。** Adapter 持有 `RustEngine`，Rust 端持有
`Arc<dyn StateListener>` 反指回 Swift 物件——讓 Adapter 自己 conform `StateListener`
就是一個跨 FFI 的 retain cycle，`deinit` 永遠不跑。做成內部
`final class ListenerBox: StateListener { weak var owner: RustEngineAdapter? }`，
並在 `deinit` 呼叫 `setStateListener(nil)`。C5 說 `Option` 給了明確的 detach 路徑，
這裡就是那條路徑存在的理由。

**② init 時必須自己 seed（C8）。** `Inner::new` 已把初始投影寫進 `last_emitted`，
所以設 listener 不會拿到第一個值。順序固定為 `state = engine.state()` →
`engine.setStateListener(box)`，反過來會漏掉中間的變更。

**③ hop 到 main 要有 fast path（C1）。** listener 在呼叫端 thread 同步觸發，
而 `@Observable` 的賦值得在 main。無條件 `DispatchQueue.main.async` 會讓
`tap()` → 進度更新慢一個 runloop turn，而 Shell 幾乎都在 main 上呼叫。
所以 `Thread.isMainThread` 就直接賦值，否則才 hop。

另外落實 `contracts.md ③` 點名的 **NaN 防線**：`setTool` 在進 FFI 前驗 `size` /
`opacity` 是有限值。文件明說該擋的位置是 Bridge 的輸入驗證，S0 就補上。

## 5. `MockEngine` 逐條複製 v0 行為表

它不是「隨便回點東西」，是 S1 換引擎時 Shell 零修改的保險。照 `contracts.md ②`：

| | 行為 |
|---|---|
| 初始 | `tool` = SoftRound 筆刷、`progress` = 0 / 24、`canUndo` / `canRedo` = `false` |
| `tap` | 推進 `colored`，飽和於 24 |
| `setTool` | 真的寫入 |
| `pickColor` | 回傳**跨工具共用**的目前顏色，不是「目前工具的顏色」（Eraser 沒有顏色） |
| `save` / `exportPNG` / `exportTimelapse` | 丟同一個 `EngineError.notImplemented` |
| C8 去重 | 賦值前先 `!=` 比對；連續兩次相同狀態只更新一次 |
| 其餘 | no-op |
| `makeCanvasView()` | 貼著 `mock-lineart.png` 的 `UIImageView`（aspect fit） |

## 6. 路由與注入

`GalleryScreen` 當 root，`NavigationStack(path:)` 配
`enum Route { case canvas(assetID: String); case share }`；`Settings` 與 `Subscription`
走 `.sheet`——付費牆是 modal，設定也是進去就出來，兩者都不該堆在 Gallery → Canvas
的返回堆疊上。五個畫面都是只有標題與必要入口的空殼。

引擎在 `ColorApp.swift` 建一次，經 `.environment` 傳下去。預設 `MockEngine`；
加一個 launch argument（`-engine rust`）切到 `RustEngineAdapter`。這讓你能單獨驗 FFI
這條路而 Shell 程式碼一行不改——正是「換引擎 Shell 零修改」的可執行證明。

## 7. `EngineCanvasView`（C7 的實作）

`UIView` 子類，`layerClass = CAMetalLayer.self`。進 window 時把 layer 位址、`bounds`
的像素尺寸、`contentsScale` 組成 `SurfaceHandle` 呼叫 `attachSurface`；`layoutSubviews`
呼叫 `resizeSurface`；離開 window 呼叫 `detachSurface`（C5：這是正常路徑）。
S0 不畫任何東西，`render()` 本來就是 no-op，畫面會是空的——這個 view 的用途是把
surface 生命週期先跑通。

**不用 `MTKView`，儘管 `§10.1` 提到它。** `§10.3` 規定渲染由 `CADisplayLink` 驅動
而非輸入事件驅動，而 `MTKView` 自帶一套 draw loop，兩者是競爭機制。這個選擇該由 E1
拿著真的 render pass 決定，S0 不預先綁死。**列為 E1 的待決項。**

## 8. 測試（`EngineBridgeTests`）

| | 測什麼 |
|---|---|
| 1 | `RustEngineAdapter.init` 成功——這條就是「xcframework 在 Xcode 連得起來」的證明 |
| 2 | 走真 Rust：`tap` ×3 → `state.progress.colored == 3` |
| 3 | listener 回呼後 `state` 已更新，且在 main thread |
| 4 | Adapter 釋放後不 crash、不洩漏（detach 路徑有效） |
| 5 | **差分測試**：同一串操作序列餵給 `MockEngine` 與 `RustEngineAdapter`，`UiState` 序列逐一相同 |
| 6 | 兩個實作的 `save` / `exportPNG` / `exportTimelapse` 都丟 `notImplemented` |

前 4 條驗「接得起來」，第 5 條驗「換得掉」——它是唯一防止 `MockEngine` 慢慢漂移的機制。

## 9. CI 兩處改動

| Job | 加什麼 |
|---|---|
| macOS（既有 paths-filter 節流） | `cargo xtask ios` 之後接 `xcodebuild build-for-testing -scheme ColorApp -destination 'generic/platform=iOS Simulator'` |
| Linux | `cargo xtask lint-ios`：純文字檢查 `apps/ios/ColorApp/**` 不得出現 `RustEngine`、不得 `import colorlull_engine` |

`build-for-testing` 編 App ＋ EngineBridge ＋ test target 但**不 boot 模擬器**——
花編譯時間、不花啟動時間，就能持續守住 modulemap／link／protocol 對齊這三類錯誤。
測試本身在本機跑。

`lint-ios` 把驗收「App Shell 端沒有任何一行直接引用 `RustEngine`」從人工目視變成機械檢查，
且跑在 Linux 上零成本。

## 10. 回寫清單

| 檔案 | 改什麼 |
|---|---|
| `architecture.md §10.1` | protocol 草案換成第 3 節的版本（`@Observable`、attach／resize／detach、fallible export）；移除已過期的 ⚠ 註記 |
| `architecture.md §10.3` | 加註 MTKView vs `CADisplayLink` ＋ `CAMetalLayer` 是 E1 待決項 |
| `specs/ffi-contract.md §6` | 「兩件留給 iOS spec 的事」標記為已消解（見第 1 節） |
| `contracts.md ②` | 速查表加一欄「Swift 對應」，記三個例外 |
| `roadmap/S0.md` | 勾選 iOS 骨架五項與驗收；刪掉已不成立的 ⚠️ 註記 |
| `specs/build-infra.md §4` | xtask 指令表加 `lint-ios` |
| `docs/README.md` | 文件地圖加一列本規格 |
| `CLAUDE.md` | 「當前」推進 |
| `CHANGELOG.md` | 本里程碑條目 |

## 不做

- InputAdapter（`coalescedTouches` / `predictedTouches` / `majorRadius`）與 FrameDriver — E1
- 任何真的渲染、`MTKView` 的取捨 — E1
- 五個畫面的實際 UI（工具列、色盤、卡片、付費牆內容）— S1 之後
- 在 CI 上 boot 模擬器跑測試 — 成本不划算，見第 9 節
- Shell 對 `attachSurface` 系列的呼叫 — 那是 `EngineCanvasView` 的事
