# Colorlull - Changelog

## [Unreleased]

**Render（E1）**
- `core/render` 落地 wgpu 起手：`Gpu`（Metal-only、零 optional feature）、`RenderContext` 的 attach／resize／detach 狀態機、`DocumentResources` 七資源、`MaskBinding`
- `T_region` 上傳 round-trip 逐值相等（`E1-wgpu` 驗收第 4 條）；detach 後 `T_paint` 內容不變（第 3 條）；`T_shade` 缺席綁 1×1 白 dummy（第 5 條）
- `E1-wgpu.md` §4：`T_line`／`T_shade`／`T_region` 一律加 `COPY_SRC`（唯一能驗證上傳內容的機制）；§3.1 補 wgpu 30 的 `color_space: Auto`
- 開發機確認拿得到 Metal adapter，`render` 的測試真的在 GPU 上跑；CI runner 待驗
- Pass 3 Composite 落地：WGSL 六層、`PAPER_WHITE`、full-screen triangle ＋ `Transform` 反變換、擴散動畫的 `Buf_fill`（32 bytes／區，含 `prev_color`）、Mask A／B 切換不重建 pipeline
- 8 條 offscreen 比對測試，涵蓋 `E1-composite` 驗收五條；`thumb.jpg` 逐像素比對卡在 M0 沒有 `.colorpack`，先以「直接驗 `thumb.rs` 的整數算術」頂替（不降採樣、不過 JPEG，更嚴格）
- **修正 spec**：`T_line`／`T_shade` 以畫布 UV 取樣而非螢幕 UV（letterbox 時才會顯形，已用左右異色線稿釘住）
- 回寫 `architecture.md`：§4.2 `erased` 的套用位置＋色彩空間、§4.4 Mode B 改為無條件通過（`REGION_LINEART` 不存在）、§4.5 擴散動畫補 `prev_color`

**Bucket（E1）**
- `core/document` 落地最小 apply：`Op::Fill`／`Effect::Filled`（帶 `prev` 與 `bbox`）、palette `a == 0` 即未填色、`colored_regions` 成為進度的真相。純 CPU，`deps-policy` 不動
- `core/render` 補上 `Fill` 那一列：`Transform::canvas_pos`＋`DocumentResources::region_at`（O(1)、畫布外不 clamp）、`ErasePass`（scissor 至 bbox、以 `T_region` 為 mask、`discard`）、`FillAnimator`（ease-out cubic 180ms、連點取當前插值色）、`render_with_dt` 與 `render` 的 wrapper 關係
- **修正既有實作**：composite shader 的 `canvas_pos` 改吃 `@builtin(position)`——UV 再乘一次 `screen_size` 差一個 ulp，邊界像素會 floor 到隔壁區；由 20×12 螢幕逐點比對 Rust 孿生體釘住
- `Buf_palette`／`Buf_fill` 加 `COPY_SRC`（`E1-bucket §10` 的「逐位元不變」要讀得回來）
- 回寫 `architecture.md` §4.5（`max_radius` 取四角最大距離、曲線與時長）與 §4.7（進度真相在 `document`）、`E1-composite.md §5`、`contracts.md` ②＋新增 C9（座標單位＝螢幕像素）
- `engine` 的 `tap` 接線仍是 S0 mock，歸 `E1-input`

**Input／FrameDriver（E1）**
- `RustEngine` 不再是 S0 mock：`new` 真的解析 `.colorpack`（`total_regions` 與 `region_ids` 都從它來）、`attach_surface` 真的建 device／surface／`DocumentResources`、`render` 走 Pass 3、`tap` 走 `canvas_pos → region_at → document.apply → RenderContext::fill`
- `EngineError::Surface` 新增——`attach_surface` 從「永遠 `Ok`」變成真的會失敗（`E1-wgpu §2.2`）；資產包壞了與 surface 建不起來，使用者能做的事不同
- `deps-policy.toml` 加 `engine → colorpack`（方向仍只向下）
- iOS 端 `FrameDriver`（weak proxy 破 `CADisplayLink` 的 retain cycle、runloop mode `.common`、80–120 Hz）與 `InputAdapter`（coalesced 在前預測在後、預測點每 frame 覆寫不累積、`t` 相對筆畫起點歸零、`radius`／`pressure` 兩條來源含 `maximumPossibleForce == 0` 的 NaN 防線）
- `EngineCanvasView` 收全部 touch：`maximumDrawableCount = 2`、`onFrame` 順序 flush → `appendSamples` → `render`（**touch handler 內零次 render**）、attach 失敗顯示錯誤態而不 crash
- **修正既有實作**：`attach()` 只在 `didMoveToWindow` 呼叫，而 SwiftUI 先掛上再排版——那一刻 `bounds` 是 0，於是永遠不 attach；`layoutSubviews` 補做。S0 沒發現是因為 `render()` 本來就是 no-op
- `CanvasScreen.onTapGesture` 移除：它送未縮放的 UIKit point（`E1-bucket §4.1` 要求螢幕像素），且與 view 自己的 touch handling 競爭。`MockEngine` 的 `totalRegions` 改為建構參數、`tap` 未 attach 時落空（與 Rust 同一條時序）
- iOS 測試 fixture `.colorpack` 進 git，由 `regenerate_ios_fixture` 產生；schema 漂移由 Rust 側的 `ios_fixture_still_matches_the_current_schema` 擋著
- Rust 16 條、iOS 24 條測試全綠。`E1-input §10` 剩兩條要真機（ProMotion 120 Hz 實測、cancel 後 `T_paint` 逐像素不變）
- 回寫 `architecture.md` §10.3（`MTKView` 待決項結案）、`contracts.md` ②／③（C10–C12）／⑤（`EngineError::Surface` 遷移記錄）、`E1-stroke.md` §9（濾波器狀態排除預測點）；八條實作期決議記在 `E1-input.md §12`

**Stroke（E1）**
- `core/stroke` 落地 `E1-stroke` §3–§6／§10：`Vec2`／`InputSample`／`Dab`／`Curve`／`BrushPreset` 十四欄＋五支 preset 登記、One-Euro（位置與 radius 各一組參數）、向心 Catmull-Rom（`alpha = 0.5`）、跨 segment 保留累積量的弧長取樣、`majorRadius` per-stroke 自適應正規化
- **`StrokeBuilder`（串流）與 `generate_dabs`（批次）共用同一份實作**，§2.1 的等價因此由建構保證；等價與「同 seed 逐位元相同」設為 gate
- 預測點走 `predicted_dabs`：複製一份 builder 狀態算完就丟，committed 狀態不被污染，§9 的抬筆重建因此不必特別處理
- 32 條測試無 GPU 全綠（Boundary 2）。三條 golden fixture 標 `#[ignore]`（參數調校中，E2 定案後才 gate）；不 overshoot／不打結／原地停留只一個 dab／`opacity` 不烘進 dab 改以性質測試守，與參數值無關
- **修正 spec**：`generate_dabs` 多一個 `size` 參數（`spacing × dab_size` 需要 px 基準）；`majorRadius` baseline 初值改為 `r ± R_EPS/2` 的帶狀，否則起筆壓感恆為 0，與 spec 自述的「應為中值」矛盾
- 回寫 `architecture.md`：§4.6 `Curve` 定義＋軟圓筆初值表、§5.2 簽章、§5.3 補「向心」與 radius 濾波參數表、§10.2 補 `R_EPS` 與 per-stroke 的已知限制
- 實作期間的八條決議記在 `E1-stroke.md §14`（交接用）

- 繪師交付改為「線稿＋色標圖」，刪除 flats/reference，區域改由線稿封閉區 flood fill 推導；`.colorpack` 與 App 端零改動。Phase 0 需先驗證線稿封閉性，否決則退回「保留 flats ＋ baker 端量化 snap」

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
