# Colorlull

iOS 著色 App。核心是「**受區域約束的塗抹**」——不是繪圖軟體。
單人開發，v1 只有 iOS，38 週零 buffer。

**當前**：M0／M1／S0／E1 已完成；S1 進行中（第一塊「設計系統 ＋ Gallery ＋ Canvas UI」已落地，見 `specs/S1-ios-ui.md`）

**真機測試**：`cargo xtask ios` 會順便 bake `apps/ios/ColorApp/Resources/dev.colorpack`（gitignore），Xcode 選 `ColorApp (rust)` scheme 跑真 FFI；S1 起畫布用的是**產品 UI**（`DebugToolBar` 已刪），Gallery 頂端另有 Debug 限定的假資料場景切換列。模擬器不行——`attach_surface` 要真的 `CAMetalLayer`。做法見 `apps/ios/README.md`。

**M0 色標交付（`specs/baker-seeds.md`）已完成**：交付改成 `lineart` ＋ `seeds.png`，區域由線稿封閉區決定；`flats.png`／`reference.png` 與 `migrate.rs` 已刪，`assets-spec.md` 升到 v2.0（§0 可整段複製進 JD）。

> 產品原名 `Color It`，2026-08-03 因商標衝突改名 **Colorlull**（`docs/specs/naming.md`）。

## Bootstrapping

**不要整份讀 prd / architecture / roadmap。** 本檔已含所有恆常約束，其餘按需載入：

- 當前任務與 DoD → `docs/roadmap/<里程碑>.md`（單檔 40–90 行）
- 找某個主題 → 用 `docs/README.md` 定位章節，`grep -n '^## 4\.'` 取行號後 `Read` 帶 offset/limit
- 原始碼結構、符號、呼叫鏈 → **codebase-memory MCP**

## 技術選型

- Rust + uniffi ／ wgpu + WGSL ／ iOS：SwiftUI + `CAMetalLayer` ＋ 自建 `CADisplayLink`（`MTKView` 是退路）／ 資產：`tools/baker` ／ 建置：`cargo xtask`
- Android（v1 不做，目錄先固定）：Compose + SurfaceView

## 架構準則

1. `core/render` 是**唯一** import wgpu 的 crate
2. `core/stroke` 純 CPU、零 GPU 依賴（有 golden test）
3. **單一寫入口**：所有狀態變更走 `document` 的 apply
4. **Undo ≠ OpLog**，是兩套東西（`architecture.md §6.6`）
5. `apps/` 不放可重用邏輯（→ `core/`）；`core/` 不碰任何平台 SDK
6. uniffi 產物 gitignore，CI 檢查 freshness

`core/` colorpack stroke render document history oplog app-state engine｜
`contracts/` 只放 uniffi 生不出來的契約｜`tools/baker`｜
`apps/` ios android web（Platform Bridge 各自獨立 target/module）｜
`assets/` source PNG=LFS（`meta.json` 除外）、packs 不進 git

## 別提議

圖層編輯、自訂筆刷或參數、匯入自己的線稿、社群功能、跨平台同步、帳號登入、廣告、遊戲化、AI 上色（`prd.md §9`）
v1 不做但已知想做：Android、深色模式、Pencil 進階、iPad 佈局


## 慣例

- mise 管工具鏈｜建置一律走 `cargo xtask`｜commit `type(scope): subject`｜里程碑推進時更新本檔「當前」｜
- Xcode 的 `xcuserdata/` 不進 git，要進 git 的 scheme 放 `xcshareddata/xcschemes/`｜
- 開 Xcode 前先跑 `cargo xtask ios`——`apps/ios/Generated/` 是 gitignore 的產物（`apps/ios/README.md`）

## 常查

判斷提案是否違反產品定位 → `prd.md §3`｜某邏輯該放哪一層 → `architecture.md §6`｜
「這看起來像 bug」→ 先查 `prd.md §10`｜FFI 某方法現在做什麼、改動算不算 major → `docs/contracts.md`｜
**完整索引 → `docs/README.md`**
