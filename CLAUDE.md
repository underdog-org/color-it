# Color It

iOS 著色 App。核心是「**受區域約束的塗抹**」——不是繪圖軟體。
單人開發，v1 只有 iOS，38 週零 buffer。

**當前**：M0 專案基建（W0–1，見 `docs/roadmap/M0.md`）。尚無程式碼。

## Bootstrapping

**不要整份讀 prd / architecture / roadmap。** 本檔已含所有恆常約束，其餘按需載入：

- 當前任務與 DoD → `docs/roadmap/<里程碑>.md`（單檔 40–90 行）
- 找某個主題 → 用 `docs/README.md` 定位章節，`grep -n '^## 4\.'` 取行號後 `Read` 帶 offset/limit
- 跨文件查證、要掃多節 → 派 `Explore` subagent，**只讓結論回主 context**
- 原始碼結構、符號、呼叫鏈 → codebase-memory MCP（M0 尚無程式碼，暫不適用）

**判準：能用 subagent 或圖查詢回答的，不要把原文搬進主 context。**

## 技術選型

Rust + uniffi ／ wgpu + WGSL ／ iOS：SwiftUI + MTKView ／ 資產：`tools/baker` ／ 建置：`cargo xtask`
Android（v1 不做，目錄先固定）：Compose + SurfaceView

**已否決，不要重提**：Skia、Flutter/Impeller、Kotlin Multiplatform、libmypaint（`architecture.md §1`）

## 鐵律（違反＝架構錯誤，不是風格問題）

1. `core/render` 是**唯一** import wgpu 的 crate
2. `core/stroke` 純 CPU、零 GPU 依賴（有 golden test）
3. **單一寫入口**：所有狀態變更走 `document` 的 apply
4. **Undo ≠ OpLog**，是兩套東西（`architecture.md §6.6`）
5. `apps/` 不放可重用邏輯（→ `core/`）；`core/` 不碰任何平台 SDK
6. uniffi 產物 gitignore，CI 檢查 freshness

`core/` colorpack stroke render document history oplog app-state engine｜
`contracts/` 只放 uniffi 生不出來的契約｜`tools/baker`｜
`apps/` ios android web（Platform Bridge 各自獨立 target/module）｜
`assets/` source=LFS、packs 不進 git

## 別提議

圖層編輯、自訂筆刷或參數、匯入自己的線稿、社群功能、跨平台同步、帳號登入、廣告、遊戲化、AI 上色（`prd.md §9`）
v1 不做但已知想做：Android、深色模式、Pencil 進階、iPad 佈局

**這些不是 bug**：麥克筆疊色後改底色不更新、Undo 不跨 session、留白作品不觸發完成動畫（全表 `prd.md §10` T1–T6）

## 慣例

mise 管工具鏈｜建置一律走 `cargo xtask`｜commit `type(scope): subject`｜里程碑推進時更新本檔「當前」

## 常查

判斷提案是否違反產品定位 → `prd.md §3`｜某邏輯該放哪一層 → `architecture.md §6`｜
「這看起來像 bug」→ 先查 `prd.md §10`｜**完整索引 → `docs/README.md`**
