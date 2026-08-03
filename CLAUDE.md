# Colorlull

iOS 著色 App。核心是「**受區域約束的塗抹**」——不是繪圖軟體。
單人開發，v1 只有 iOS，38 週零 buffer。

**當前**：M0／M1／S0 完成，進行中 **E1 手感垂直切片**（★最高風險，8 週時間盒，`docs/roadmap/E1.md`）。
六份 E1 spec 全部寫完（`E1-wgpu`／`E1-composite`／`E1-stroke`／`E1-bucket`／`E1-input`／`E1-perf`；先讀 `E1-wgpu`，其餘五份以它為共同輸入）。

**E1 程式面已收尾**：`core/render`（wgpu 起手 ＋ Composite／Stroke／Commit／Erase 各 Pass ＋ `FillAnimator`）、`core/stroke`（One-Euro → 向心 Catmull-Rom → 弧長取樣 → `Dab`）、`core/document`（最小 apply）、`core/engine`（真的解析 `.colorpack`，`tap`／筆畫接到 render／document）、iOS（`FrameDriver` ＋ `InputAdapter`）全部落地，Rust／iOS 測試全綠（`core/stroke` 三條 golden 標 `#[ignore]`，參數調校中）。
**下一步：只剩真機量測**——`docs/perf-baseline.md` 是空表，D2／D3／D4 全部待執行（流程在 `E1-perf`）。

**M0 色標交付（`specs/baker-seeds.md`）已完成**：交付改成 `lineart` ＋ `seeds.png`，區域由線稿封閉區決定；`flats.png`／`reference.png` 與 `migrate.rs` 已刪，`assets-spec.md` 升到 v2.0（§0 可整段複製進 JD）。
**M0 未結**：繪師徵才 JD 待發；兩支 demo 的 `seeds.png` 是從已廢止的 `reference.png` 反推的，繪師依 v2.0 重交後才算真正走過交付流程。

> `E1-input.md §12` 是執行期決議（`tap` 由 `touchesBegan` 依工具分流、`radius` 送點不送像素、`Transform` 由 engine 自算 fit）——動 Bridge 或 `engine` 之前先讀。
> composite shader 的 `canvas_pos` 已改吃 `@builtin(position)`——UV 再乘一次 `screen_size` 會差一個 ulp，邊界像素會 floor 到隔壁區。
> `T_wet` 沒有 `COPY_SRC`（單筆暫存不進 undo），所以 Pass 1 的正確性一律**從 `T_paint` 讀回**驗證。
> `set_mask_mode` 是排定要移除的 FFI（契約 C13），D4 拍板後與 Swift toggle 一起刪，不算 major bump。
> `attach_surface` 需要真的 `CAMetalLayer`，無自動測試——模擬器上 `tap` 會落空（沒有 `region_ids`），真機才驗得到。

> 產品原名 `Color It`，2026-08-03 因商標衝突改名 **Colorlull**（`docs/specs/naming.md`）。
> 目錄名 `color-it/` 尚未改，`architecture.md §3` 的結構圖照現況寫。

## Bootstrapping

**不要整份讀 prd / architecture / roadmap。** 本檔已含所有恆常約束，其餘按需載入：

- 當前任務與 DoD → `docs/roadmap/<里程碑>.md`（單檔 40–90 行）
- 找某個主題 → 用 `docs/README.md` 定位章節，`grep -n '^## 4\.'` 取行號後 `Read` 帶 offset/limit
- 跨文件查證、要掃多節 → 派 `Explore` subagent，**只讓結論回主 context**
- 原始碼結構、符號、呼叫鏈 → codebase-memory MCP

**判準：能用 subagent 或圖查詢回答的，不要把原文搬進主 context。**

## 技術選型

Rust + uniffi ／ wgpu + WGSL ／ iOS：SwiftUI + `CAMetalLayer` ＋ 自建 `CADisplayLink`（`MTKView` 是退路）／ 資產：`tools/baker` ／ 建置：`cargo xtask`
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
`assets/` source PNG=LFS（`meta.json` 除外）、packs 不進 git

## 別提議

圖層編輯、自訂筆刷或參數、匯入自己的線稿、社群功能、跨平台同步、帳號登入、廣告、遊戲化、AI 上色（`prd.md §9`）
v1 不做但已知想做：Android、深色模式、Pencil 進階、iPad 佈局

**這些不是 bug**：麥克筆疊色後改底色不更新、Undo 不跨 session、留白作品不觸發完成動畫（全表 `prd.md §10` T1–T6）

## 慣例

mise 管工具鏈｜建置一律走 `cargo xtask`｜commit `type(scope): subject`｜里程碑推進時更新本檔「當前」｜
Xcode 的 `xcuserdata/` 不進 git，要進 git 的 scheme 放 `xcshareddata/xcschemes/`｜
開 Xcode 前先跑 `cargo xtask ios`——`apps/ios/Generated/` 是 gitignore 的產物（`apps/ios/README.md`）

## 常查

判斷提案是否違反產品定位 → `prd.md §3`｜某邏輯該放哪一層 → `architecture.md §6`｜
「這看起來像 bug」→ 先查 `prd.md §10`｜FFI 某方法現在做什麼、改動算不算 major → `docs/contracts.md`｜
**完整索引 → `docs/README.md`**
