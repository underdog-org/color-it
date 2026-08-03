# Colorlull - Changelog

## [Unreleased]

- 繪師交付改為「線稿＋色標圖」，刪除 flats/reference，區域改由線稿封閉區 flood fill 推導；`.colorpack` 與 App 端零改動。Phase 0 需先驗證線稿封閉性，否決則退回「保留 flats ＋ baker 端量化 snap」
- baker 新增 golden test：固定素材 → 凍結 `region_ids` 與 `content_hash`，守住 `grow`／`merge_small_orphans`／`close` 的確定性（舊 `label_regions` 是精確色比對，確定性是白送的）
- baker 新增 `--debug-out <dir>`：`preview.png`／`seeds-overlay.png`／`reference-preview.png`／`regions.json` 四件**退件附件**，拒收路徑照樣產出
- 診斷報告加座標聚類（`shade-too-dark` 之類的逐像素症狀不再吐 16 個散落座標）與可疑度排序（`line-coverage` 排在它會解釋掉的色標錯誤之前）

## [v0.2]

**Assets / Baker**
- 繪師交付改為「線稿＋色標圖」，刪除 flats/reference；Phase 0 需先驗證線稿封閉性
- `tools/baker` 管線落地（source → 母帶檢查 → CC → 合成白底 → 降採樣 → 膨脹 → 檢查 → 建議色 → 縮圖 → `.colorpack`）
- fail-fast 收窄到 `size-mismatch`／`unique-color-overflow`；`unassigned-pixel` 降級 debug_assert；新增 `source-incomplete`
- 色彩空間判定改解 ICC tag table 比對 colorant（`iCCP` → `sRGB` → `gAMA+cHRM` → 通過）
- `shade` 的 luma<60 改在合成白底後判定
- `torture-01` 首次有自動化證據；`synth-lock.json` drift 守門
- `assets-spec.md` v1.1：返工成本、顏色數／luma 判準、保留色、碎片門檻 800px、reference 機驗、補 `cartoon`

**Core**
- `core/colorpack` 落地：容器讀寫、manifest/regions、R16 RLE；content_hash 凍結向量
- `.colorpack` 壓縮由副檔名決定、mtime/deflate 固定，同輸入重跑位元相同
- 新增 `contracts/colorpack.schema.json`（SSOT）＋對照測試
- cargo workspace 骨架（8 crate＋baker＋xtask）；`lint-deps` 用 deps-policy.toml；CI 單一 workflow
- `rust-toolchain.toml` 為 toolchain pin 的 SSOT

**Engine / FFI**
- `core/engine` headless mock（20 個 FFI 方法）＋`app-state` 骨架；8 條契約測試不需 GPU
- 新增 `docs/contracts.md`（FFI 速查＋C1–C8）；uniffi 改 proc-macro、FFI 型別獨立層、Boundary 1 三處修正
- `Engine::mutate` 唯一寫入口：發送前釋鎖、UiState 真正改變才 emit
- `EngineError::NotImplemented` 有期限（E3 移除，v1 exit criteria）；CI 守門改 `ffi-lock.toml`

**iOS**
- iOS 骨架：三 target、五條路由、synchronized group（新增 Swift 檔不動 pbxproj）
- `EngineProtocol` v0（`@Observable`）；`RustEngineAdapter` 落實 C1/C5/C8＋NaN 防線；Mock↔Rust 差分測試
- `EngineCanvasView` 改 CAMetalLayer（E1 待決）；引擎選擇搬進 `EngineFactory`；`lint-ios`
- CI 接 `xcodebuild build-for-testing`；modulemap `xcframework: false`；Rust Object 改名 `RustEngine`

**Docs / Naming**
- 改名 Color It → Colorlull（`docs/specs/naming.md`）；crate 前綴 `colorit-*` → `colorlull-*`
- 直式母帶比例更正 3:4（1536×2048），回寫 prd/architecture/assets-spec/roadmap；匯出 letterbox 4:5 不受影響
- 新增 `ios-scaffold.md`；`baker-core-design` 升 v1.2；architecture §6/§7/§9/§10.1/§12.1 回寫

## v0.1 (2026-08-03)

- Roadmap 重排：資產分發／解鎖 Worker／手動匯出分別前移 S1/S2/S2b（新增里程碑 W28），S3 縮為 3 週；全線仍 38 週。補漏排的「完成建議與完成動畫」（Must Have）；M0 拆為 M0＋M1
- 拍板：Settings 升為第五條正式路由；筆刷透明度與吸管都做（回寫 FFI）；iOS 備份採 iCloud Drive ubiquity container；手動匯入／匯出升為 v1 Must Have
- 時程修正：D1 W4–6 → W5–7、繪師徵才 W3 → W1、ASC 帳務 W20 → W15、名稱檢查移到 M0
- 規格：`assets-spec.md` 定稿 v1.0（難度門檻表為唯一依據）；prd/architecture/roadmap 統一 v0.1
- Protocol 補 `t_erase`；水彩描邊路徑定案（commit 時對 `T_wet` 做 unsharp）
- v1 刪除 `contracts/tokens.json`，改用 `DesignTokens.swift`；Logo/branding 從 R1 移到 S2；Android release 移到 v1 之後
- 新增 `CLAUDE.md`（恆常約束）、`docs/README.md`（文件索引）；`roadmap.md`（912 行）拆分為 `docs/roadmap/`（index＋11 里程碑＋checkpoints＋beyond-v1）
