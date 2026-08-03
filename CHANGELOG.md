# Colorlull - Changelog

## [Unreleased]

**五支筆刷（E2，`specs/E2-brush.md`）**
- 三張 tip 全部程序生成並填滿 atlas 三層：軟圓（既有）、硬圓（窄過渡 `smoothstep`）、顆粒（value noise × 徑向遮罩）。**推翻 `E2-spec-plan` 原拍板的「顆粒用內嵌 PNG」**——noise 的不規則性由 seed／頻率／對比三個常數控制，可調可重現，而顆粒是最需要反覆調的一張
- `TipId::is_implemented()` 與 `dab.rs` 的 fallback 分支一併移除：三個變體全實作後它恆為真，留著會讓日後「新增第四張 tip 卻忘了填 atlas」靜默退回軟圓
- 五支 preset 從 `..soft_round()` 佔位填成實際初值，每支只覆蓋自己差異軸上的欄位。**是初值，D5 會調**
- `velocity_to_size` 從恆 0 啟用（噴槍 ／ 水彩）：速度取自 One-Euro **濾波後**的位置與樣本時間戳，per-dab 值與 `pressure` 走同一條內插路徑，用固定的 `REFERENCE_SPEED` 正規化而非 per-stroke baseline
- `tilt_to_size` 明確不實作：手指恆無 tilt，Pencil 進階是 v1 不做
- **修正一個 jitter 的位元不決定性**：`jitter_angle = 0` 時抽到負值會得到 `-0.0`，與 `0.0` 數值相等但位元不同，使「jitter 三欄全 0 → 輸出逐位元相同」的契約敗在一個看不見的符號位
- `properties.rs` 加五條 RNG 契約與速度的 property test，**現在就是 CI gate**（與參數值無關，不必等 D5）
- golden fixture 從 3 個擴到 15 個（3 條軌跡 × 5 支），fixture 記下 preset 名。**`#[ignore]` 仍在**——解除排在 D5 參數定案之後，提前解只會再標回去；另加一條不 ignore 的測試守著「15 個檔案都在」
- `architecture.md §4.6` 的十四欄數值表改為定性的差異軸表：數值的唯一真相是 `preset.rs`，寫進文件就是寫完即過期
**Tokens & UI（S1）**
- `DesignTokens.swift`：`.pen` document variables 的逐項對譯（顏色／品牌色／間距／圓角／字級／字型）。手工同步，`.pen` 是加密格式做不出產生器
- Fraunces ＋ Inter（OFL variable font）bundle 進 App。`UIAppFonts` 是陣列型別、沒有 `INFOPLIST_KEY_` 寫法，所以補了 `apps/ios/ColorApp-Info.plist`——刻意放在 `ColorApp/` **之外**，該資料夾是 file-system synchronized group，擺進去會與 `ProcessInfoPlistFile` 撞成 "Multiple commands produce Info.plist"
- `EngineBridge/Gallery/`：`GalleryItem` / `GalleryCatalog` / `FixtureCatalog`。鎖定收斂成 `isLocked(isSubscribed:)` 一行；「未下載 × 有本機文件」由 `init` 的 assert 排除，另兩個不可能組合由 `isLocked` 的定義保證
- `DownloadState` 補 `.failed(reason:)`——設計稿的 `Card States · Download` 畫了重試態，spec §2.1 原本漏了。合法組合因此是 20 個，`FixtureCatalog.populated` 逐一涵蓋並由測試比對
- Gallery 兩分頁、六個元件、Canvas 全面改寫（Top Bar 進度條、工具列、筆刷展開層、兩排刻度、常駐色環、色票列、完成建議膠囊）
- 吸管接上 `pickColor`：新增 Bridge 專屬的 `CanvasPickMode`，`makeCanvasView(pickMode:)` 取代無參數版。待命與否不改引擎狀態，所以不進 `UiState`
- `CanvasScreen.DebugToolBar` 已刪，真機測試改用產品 UI；`MaskModeToggle` 保留（綁 D4 不綁本輪）
- **回寫 `prd.md`**：§5.1／§5.2 進度環 → 線性進度條；§5.2 完整色盤入口由常駐色環滿足；§5.2 Canvas 沒有 Settings 入口
- 新增 `docs/interface-defects.md`，第一條：`Tool.eraser` 沒有 `color` 欄位，Shell 只能自己保存當前色（修正窗口 E3）

**修正（E1）**
- `RenderContext::attach_surface` 建 surface 的那一段抽成 cfg 分岔的 `create_metal_surface`：`SurfaceTargetUnsafe::CoreAnimationLayer` 是 wgpu 的 `#[cfg(metal)]` variant，非 Apple 平台不存在，CI 的 Linux job 因此整個 workspace 編不過。非 Apple 版本回新的 `RenderError::UnsupportedPlatform`。注意 metal 那一支在 CI 上沒有任何 job 會編到（`ios` job 的 paths-filter 不含 `core/render/**`）

**真機測試 harness（E1）**
- `cargo xtask dev-pack [DIR]`：bake 一顆 pack 進 `apps/ios/ColorApp/Resources/dev.colorpack`（預設素材 `kirby-demo-1`），並掛進 `cargo xtask ios`——忘記跑的懲罰是 App 靜默退回 `MockEngine`，那個症狀要花很久才認得出來。產物 gitignore，與 `assets/packs/` 同一條規則
- `ColorApp.swift` 的 pack 路徑從 S0 遺留的 `mock-lineart.png` 改成 `dev.colorpack`——E1 起 `Engine::new` 真的解析格式，PNG 一定被 reject
- 新增 shared scheme `ColorApp (rust)`：`-engine rust` 預設開啟、Run-only 無 testables。不直接翻開 `ColorApp` scheme 那條 argument，因為它的 TestAction 帶 `shouldUseLaunchSchemeArgsEnv`，會連 CI 的 host app 行為一起改
- `CanvasScreen.DebugToolBar`（`#if DEBUG`）：筆刷／油漆桶切換 ＋ 六色票，選中狀態一律從 `engine.state.tool` 推導不留副本。沒有它就切不到油漆桶（`touchesBegan` 要 `state.tool` 是 `.bucket`），D4 與擴散動畫調校都做不了。刻意不做 size／opacity／筆刷切換——那些在 E2

## [v0.3] 2026-08-03

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

**Stroke Pass 1／2 與 Mask Mode（E1）**
- Pass 1 `StrokePass`：instanced quad × dab → `T_wet`，程序生成的軟圓 tip（256×256 R8 array，layer 1／2 留給 E2，未實作的 tip 在 CPU 側 fallback）、`build_up` 兩條預建 pipeline（`Max` vs `OneMinusDst/Add`）、scissor 用**增量** bbox、超過 `MAX_DABS_PER_DRAW` 分批
- Pass 2 `CommitPass`：`T_wet × opacity × mask` → `T_paint`（premultiplied over），收尾清 `T_wet`，兩步都 scissor 至整筆 bbox；`cancel_stroke` 共用清除路徑
- `engine` 接線（`brush.rs`）：`deps-policy` 開 `engine → stroke`（`E1-stroke §14` 決議 C 結案）、筆刷 ID → 十四欄參數的對應、`end_stroke` 走「清 → 以真實樣本重建 → commit」丟掉預測點的尾巴、`brush_color` 的 alpha 改用整筆 opacity（否則抬筆瞬間濃度會跳）
- `set_mask_mode` FFI ＋ `CanvasScreen` 的 `#if DEBUG` toggle（D4 的輸入）；`active_region_id` 取自起筆處的 region。契約新增 C13：它排定要移除，移除不算 major bump
- 15 條 GPU 測試 ＋ 4 條 engine 接線測試；`T_wet` 沒有 `COPY_SRC`，驗證一律從 `T_paint` 讀回
- **分批必須各自 submit**：`Queue::write_buffer` 在 submit 時才依序落地，同一次 submit 裡寫兩批會讓第二批蓋掉第一批——「畫太快只剩最後 4096 個 dab」

**量測（E1）**
- `docs/perf-baseline.md` 第一版：量測條件檢查表、兩台裝置的空表、`§4.1.1` 三步對帳、八項調校初值與出處。**數字全部待實測**
- `architecture.md §4.1.1` 補 swapchain drawable 與 `region_ids` 兩列（標「待實測」，不拿估算值改預算結論）；§13.1 標明 undo pool 那條在 E1 不適用
- `E1-perf §7` 調校表新增 `TIP_FALLOFF`——筆尖是程序生成的，衰減曲線因此是個真的參數

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

**色標交付（M0）**
- 繪師交付改為「線稿＋色標圖」，區域改由線稿封閉區 flood fill 推導；`.colorpack` 與 App 端零改動。Phase 0 實測線稿封閉性通過（62/62），不退回「保留 flats ＋ baker 端量化 snap」
- baker 管線改走 `seeds.png`：線稿二值化 → 色標連通分量 → 逐 seed flood fill（含碰撞偵測）→ 孤兒區偵測 → 測地擴張封閉；四個參數可用 `--set` 覆寫
- baker 新增 golden test：固定素材 → 凍結 `region_ids` 與 `content_hash`，守住 `grow`／`merge_small_orphans`／`close` 的確定性（舊 `label_regions` 是精確色比對，確定性是白送的）
- baker 新增 `--debug-out <dir>`：`preview.png`／`seeds-overlay.png`／`reference-preview.png`／`regions.json` 四件**退件附件**，拒收路徑照樣產出
- `assets-spec.md` v2.0：交付改成 `lineart` ＋ `seeds.png`（＋選配 `shade`），`flats`／`reference` 廢止；`§0` 是可整段複製進 JD 的一頁摘要；線稿封閉性升格為硬性要求；Procreate 現在整套都能用
- 刪除 `assets/source/*/flats.png`、`reference.png`（LFS）與過渡用的 `baker::migrate` ＋ `xtask seeds-from-reference`
- `architecture.md §9`、`roadmap/M0.md`、`docs/README.md` 同步到新契約
- 診斷報告加座標聚類（`shade-too-dark` 之類的逐像素症狀不再吐 16 個散落座標）與可疑度排序（`line-coverage` 排在它會解釋掉的色標錯誤之前）

## [v0.2] 2026-08-03

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

## [v0.1] 2026-08-03

- Roadmap 重排：資產分發／解鎖 Worker／手動匯出分別前移 S1/S2/S2b（新增里程碑 W28），S3 縮為 3 週；全線仍 38 週。補漏排的「完成建議與完成動畫」（Must Have）；M0 拆為 M0＋M1
- 拍板：Settings 升為第五條正式路由；筆刷透明度與吸管都做（回寫 FFI）；iOS 備份採 iCloud Drive ubiquity container；手動匯入／匯出升為 v1 Must Have
- 時程修正：D1 W4–6 → W5–7、繪師徵才 W3 → W1、ASC 帳務 W20 → W15、名稱檢查移到 M0
- 規格：`assets-spec.md` 定稿 v1.0（難度門檻表為唯一依據）；prd/architecture/roadmap 統一 v0.1
- Protocol 補 `t_erase`；水彩描邊路徑定案（commit 時對 `T_wet` 做 unsharp）
- v1 刪除 `contracts/tokens.json`，改用 `DesignTokens.swift`；Logo/branding 從 R1 移到 S2；Android release 移到 v1 之後
- 新增 `CLAUDE.md`（恆常約束）、`docs/README.md`（文件索引）；`roadmap.md`（912 行）拆分為 `docs/roadmap/`（index＋11 里程碑＋checkpoints＋beyond-v1）
