# S1｜iOS UI & Design

> **里程碑期間有效，S1 收尾後刪除。**
> 上游：`docs/roadmap/S1.md`、`design/mobile-design.pen`、`prd.md §5`。

S1 是五個彼此獨立的子系統。這份 spec 只涵蓋第一塊——**設計系統 ＋ Gallery ＋ Canvas 的 iOS UI**，
全程對著假資料。其餘四塊（R2 與圖庫目錄、下載器與快取、i18n、`MockEngine`→`RustEngine`）
各自走一輪 spec，不在本檔。

Share / Settings / Subscription 三個畫面維持 S0 佔位，本輪不碰。

---

## 0. 設計稿是 SSOT

`design/mobile-design.pen` 是版面與 token 的唯一真相。實作一律以 **`Draw · Tools v2`** 為準——
`Draw · Main Screen`（DJYF7，舊的 Mode Toggle 版）已作廢。

**已知風險**：`design/` 在 `.gitignore` 裡，`.pen` 又是加密格式，因此設計稿**沒有版本歷史、
只有一份**。本輪不解決，但要知道它是單點失效。

### 0.1 開工前的 `.pen` 收尾

| # | 項目 |
|---|---|
| 1 | 補畫「建議色 ＋ 最近使用」列：各四格、中間分隔線、當前色以 `$ink` 環標示。紙張 416→368，連帶更新 `Draw · Tools v2`、`吸管取色中`、`完成建議` 三張的版面 |
| 2 | `Work Card` 元件的進度條套修正：軌道左右內縮 12px、pill 端、填充最小 8px（依 `Card States · Work` 的 note） |
| 3 | `Draw · Main Screen`（DJYF7）標記作廢 |
| 4 | `Draw · 最近使用色`（aAil2）目前沒畫出那一列——第 1 項做完後補成真正的示範，或併回 `Tools v2` 後刪除 |

### 0.2 要回寫的既有文件

roadmap 驗收明列「不得只存在於腦中」，以下三處是本輪推翻的：

- `prd.md §5.1`／`§5.2`：**進度環 → 線性進度條**。卡片與 Canvas Top Bar 一致用進度條，
  完成建議的動畫改為「進度條填滿」
- `prd.md §5.2`：色盤的「完整色盤入口」由**常駐色環**滿足，不另設入口。建議色與最近使用
  改為色環上方的一列
- `prd.md §5`：Canvas **沒有** Settings 入口。Top Bar 那顆 sliders icon 是筆刷參數，
  Settings 只從 Gallery 進

---

## 1. 檔案落點

```
apps/ios/EngineBridge/Gallery/
    GalleryCatalog.swift      protocol ＋ DTO
    FixtureCatalog.swift      假資料，含場景切換
apps/ios/ColorApp/DesignSystem/
    DesignTokens.swift        .pen 變數的逐項對譯
    Fonts/                    Fraunces、Inter（Info.plist 的 UIAppFonts）
apps/ios/ColorApp/Components/
    DownloadCard.swift  WorkCard.swift  ToolBar.swift
    LevelTicks.swift    ColorWheel.swift  SwatchRow.swift
apps/ios/ColorApp/Gallery/
    GalleryScreen.swift  ExploreTab.swift  MyWorksTab.swift
apps/ios/ColorApp/Canvas/
    CanvasScreen.swift        改寫
```

資料層放 `EngineBridge` 而不是 `ColorApp`：`ColorApp` 不放可重用邏輯是既有準則
（`architecture.md §6`），`MockEngine` 也在那。`cargo xtask lint-ios` 只檢查
`ColorApp/**` 不出現 `RustEngine`，不受影響。

---

## 2. 資料層契約

### 2.1 DTO

```swift
public struct GalleryItem: Identifiable, Hashable {
    public let assetID: String
    public let title: String
    public let credit: String?
    public let categoryID: String
    public let difficulty: Difficulty      // .easy / .medium / .focus
    public let regionCount: UInt32
    public let entitlement: Entitlement    // .free / .paid
    public let download: DownloadState     // .notDownloaded / .downloading(Double) / .downloaded
    public let work: WorkState?            // nil＝未開始
    public let lastEditedAt: Date?
}

public enum WorkState: Hashable {
    case inProgress(progress: Double)
    case shared(progress: Double)
}
```

### 2.2 鎖定是衍生值，不是欄位

```swift
extension GalleryItem {
    func isLocked(isSubscribed: Bool) -> Bool {
        entitlement == .paid && !isSubscribed && work == nil
    }
}
```

`prd.md §5.1` 那條「一旦某 `asset_id` 有對應本機文件，該 asset 在所有 UI 不再顯示鎖定」
在型別上收斂成這一行，而不是散在各 View 的 if。

三個不可能組合（清單見 `Card States · Download` 的 note）一律**不畫 fallback UI**，
但它們的排除機制不同，不要一律塞進 `init`：

| 組合 | 排除機制 |
|---|---|
| 未下載 × 進行中 | `init` 的 `assert`——`download == .notDownloaded && work != nil` 直接爆 |
| 鎖定 × 進行中 | **由 `isLocked` 的定義保證**，不需斷言：`work != nil` 時它恆為 `false` |
| 鎖定 × 已分享 | 同上（`.shared` 是 `work != nil` 的子集） |

`init` 拿不到訂閱狀態，所以後兩條在 `init` 裡根本表達不出來——它們是**建構即成立**的
不變式，用單元測試釘住，不是用 `assert`。

### 2.3 Catalog

```swift
public protocol GalleryCatalog: AnyObject {
    var items: [GalleryItem] { get }
    var loadState: LoadState { get }   // .loading / .ready / .failed(String)
    func refresh() async
}
```

實作為 `@Observable`，與 `MockEngine` 同一套觀察機制，View 不需要 Combine。

兩個分頁是**同一份 `items` 的兩種投影**，不做兩份資料源：

| 分頁 | 投影 |
|---|---|
| 探索 | 全部，依 `categoryID` 分組 |
| 我的作品 | `work != nil`，依 `lastEditedAt` 降冪 |

### 2.4 `FixtureCatalog`

以 `Scenario` enum 切換，對應 `Gallery States` 畫的三張加一張完整態：

| Scenario | 用途 |
|---|---|
| `.populated` | 涵蓋**每一個**合法狀態組合，S1 驗收的主力 |
| `.myWorksEmpty` | 我的作品空狀態 |
| `.searchNoResults` | 搜尋無結果 |
| `.browseLoading` | 探索載入中骨架 |

切換入口是 Debug build 限定，不是產品 UI。

---

## 3. Design Tokens

`.pen` 變數逐項對譯，**手工同步**——`.pen` 是加密格式、只能透過 MCP 讀，做不出產生器。
`DesignTokens.swift` 檔頭註明來源；改 `.pen` 變數時必須同步改 Swift，這是紀律不是工具。

| 群組 | 對譯 |
|---|---|
| 顏色 | `bg #E7E3DD`、`surface #FFFFFF`、`ink #16130F`、`muted #8C857C`、`line #E3DED6`、`accent #F4614C`、`paper #FBF8F3` |
| 品牌色 | `brandAmber #F2A93B`、`brandTeal #3FA89B`、`brandPeri #7B85E0`、`brandBlush #EE9BB4` |
| 間距 | `space1 4`、`space2 8`、`space3 12`、`space4 16`、`space5 20`、`space6 24` |
| 圓角 | `radiusSm 6`、`radiusMd 20`、`radiusLg 32`、`radiusPill 999` |
| 字級 | `textCaption 11`、`textBody 13`、`textTitle 19`、`textDisplay 32` |
| 字型 | `fontDisplay "Fraunces"`、`fontBody "Inter"`（皆 OFL，bundle 進 App） |

字級一律走 `.custom(_, size:relativeTo:)`，隨系統 Dynamic Type 縮放。**大字級與日文的破版
檢查不在本輪驗收**，留給 i18n 那一輪。

---

## 4. Canvas 與引擎

### 4.1 `Tool` 是工具與其參數的合體

```
.brush(preset: BrushId, color: Rgba, size: Float, opacity: Float?)
.eraser(size: Float)
.bucket(color: Rgba)
```

Shell 自行維護一份 UI 選擇狀態（目前工具、目前 preset、共用色、共用大小、透明度），
每次切換組出完整的 `Tool` 再 `setTool`。這與 Design Notes 的版面規則是同一件事的兩面：

- 筆刷／橡皮擦**共用同一個大小值**，切到橡皮擦時大小刻度不消失也不歸零
- 油漆桶不吃大小與透明度，兩排刻度整體降到 40% 不透明度的**停用態，保留原位**，
  避免版面跳動

### 4.2 介面缺陷（記錄，本輪不修）

切到橡皮擦後 `UiState.tool` 是 `.eraser(size:)`，**沒有 `color` 欄位**，Shell 從 `state`
讀不回當前色（Rust 端與 `MockEngine` 內部都留著）。本輪由 Shell 自行保存當前色。

這是 S1 驗收要求的「介面缺陷清單」第一條，留給 E3 的契約修正窗口
（`EngineProtocol` 檔頭已預告該窗口）。

### 4.3 與 E2 的唯一耦合點

筆刷展開層**資料驅動**——吃一份 `[BrushId]` 清單渲染，不寫死格數。E2 若把水彩砍成四支
（`E2.md` 的時間盒退路），這裡不改版面也不改程式。

### 4.4 `DebugToolBar` 的交界

`CanvasScreen.DebugToolBar` 在 Canvas 改寫的**同一步**替換掉，不提前刪——E2 在那之前
還要靠它在真機上點到油漆桶（`touchesBegan` 要 `state.tool` 是 `.bucket`）。

`MaskModeToggle` 保留（Debug build 限定），它的生命週期綁 D4 不綁本輪。

### 4.5 其餘接線

| UI | FFI |
|---|---|
| 三工具切換、preset、大小、透明度 | `setTool` |
| Undo / Redo 按鈕與停用態 | `undo()` / `redo()` / `state.canUndo` / `state.canRedo` |
| 吸管 | `pickColor(x:y:)` |
| Top Bar 進度條 | `state.progress` |

Undo/Redo 無可復原步驟時**只降階為停用，不隱藏**——避免按鈕位置跳動。

---

## 5. 測試

**單元測試**（`EngineBridgeTests`）

- `isLocked` 的衍生規則：付費 × 未訂閱 × 無本機文件才鎖，其餘皆不鎖
- 「未下載 × 進行中」會觸發 `init` 的 `assert`
- 「鎖定 × 進行中」「鎖定 × 已分享」建構不出來：對任意 `work != nil` 的 item，
  `isLocked` 恆為 `false`（property test，不是逐例舉）
- `FixtureCatalog.populated` 涵蓋每一個合法狀態組合（以組合列舉逐一比對，不是目測）

**SwiftUI Preview**

- 卡片狀態矩陣，對照 `Card States · Download` 與 `· Work`
- `ToolBar` 五態，對照 `Draw · Tool States` 那五格

既有的差分測試與 `InputTests` 不動。

---

## 6. 完成定義

- [ ] `.pen` 四項收尾完成（§0.1）
- [ ] App 內可切 `Scenario`，看到全部合法卡片狀態組合與三個空狀態
- [ ] 三工具切換、兩排刻度、Undo/Redo 停用態、吸管取色全部走 `EngineProtocol`
- [ ] `DebugToolBar` 已刪，真機測試改用產品 UI
- [ ] `DesignTokens.swift` 與 `.pen` 變數逐項一致（§3 的表就是核對清單）
- [ ] Gallery → Canvas → 實際塗抹 → 返回，全流程在真機可用
- [ ] `prd.md` 三處回寫完成（§0.2）
- [ ] 介面缺陷清單開檔，第一條已記錄（§4.2）
