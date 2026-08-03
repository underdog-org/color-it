# Colorlull - Changelog

## [Unreleased]

## [v0.2]

- iOS 骨架落地：`apps/ios/ColorApp.xcodeproj` 三個 target（App Shell／`EngineBridge` framework／`EngineBridgeTests`），
  五條路由空殼（Gallery root ＋ Canvas／Share 進堆疊，Settings／Subscription 走 `.sheet`）。
  每個 target 一個 file-system synchronized group，**新增 Swift 檔完全不動 `project.pbxproj`**
- `EngineProtocol` 標記 v0，逐項對照 `docs/contracts.md ②`。`state` 用 `@Observable` 而非
  `AnyPublisher`——`architecture.md §10.1` 的草案是 Combine 時代的寫法，已回寫
- `RustEngineAdapter` 落實三條只有實作時才會踩到的語意：listener 用 weak box ＋ `deinit`
  時 `setStateListener(nil)`（C5 那條 detach 路徑存在的理由——否則是跨 FFI 的參照環，
  ARC 看不見）、init 時自己 seed `state` 再設 listener（C8）、已在 main thread 就直接賦值
  不排 runloop（C1）。另補 NaN 防線：`setTool` 進 FFI 前驗 `size` / `opacity` 是有限值
- `MockEngine` 鏡射 `core/app-state` 的 `AppState` 而非存 `Tool` enum——`pickColor` 要回傳
  **跨工具共用**的顏色，而 `Tool.eraser` 沒有顏色欄位。差分測試（同一串操作餵給兩個實作，
  `UiState` 序列逐一相同）是唯一防止 Mock 慢慢漂離 Rust 行為的機制，已實測改一個 byte 就會紅
- `cargo xtask lint-ios`：把驗收「App Shell 端沒有任何一行直接引用 `RustEngine`」從人工目視
  變成機械檢查。連帶把引擎選擇搬進 `EngineBridge.EngineFactory`——`RustEngineAdapter`
  字面上含有 `RustEngine`，Shell 只要寫得出這個名字，嚴格的純文字檢查就不可能通過
- CI macOS job 接 `xcodebuild build-for-testing`：modulemap／link／protocol 對齊這三類
  **只有 Swift 編譯器抓得到**的錯誤，此前 `xtask ios` 一律 exit 0 看不到。不 boot 模擬器
- `xcuserdata/` ＋ `DerivedData/` 進 `.gitignore`；要進 git 的 scheme 改放 `xcshareddata/xcschemes/`
  ——CI 的 `-scheme` 只讀得到 shared scheme
- `EngineCanvasView` 用 `CAMetalLayer` 而非 `MTKView`：後者自帶 draw loop，與 `§10.3`
  「渲染由 FrameDriver 驅動」是競爭機制。**列為 E1 待決項**並回寫 `architecture.md §10.3`
- **產品改名 `Color It` → `Colorlull`**。原名遭 USPTO 註冊號 5152095（COLORIT，Class 16 著色本／畫材，活體）＋ 營業中的 `colorit.com` 阻擋；候選 Coloree／Colory 有 App Store 撞名，Colorie 的 ASO 被 "calorie" 吞噬。決策與殘留查證項記於新增的 `docs/specs/naming.md`。連帶 crate 前綴 `colorit-*` → `colorlull-*`（`lib.name` 短名不變）
- `cargo xtask gen-torture`：決定性產生 `assets/source/torture-01/`，12 個壓力區塊（細碎區域、單像素縫隙、1px 棋盤、螺旋、貼邊界特徵），重跑逐位元相同
- `assets-spec.md` v1.1：補上原始分層檔的返工成本告知（M0 要求「合作前講明」）、`flats` 顏色數判準（無顏色總面積 < 100px）、`shade` 的 luma < 60 判準、`#FF00FF` 列為保留色；碎片門檻明確為**母帶 800px**（對應輸出 200px，此前未指明解析度）；`reference` 的「幾何完全一致」改寫為兩條可機驗規則
- `category` 補上 `cartoon`——`architecture.md §9.1` 本來就有六個值，`assets-spec.md` 停在五個，兩份文件此前不一致
- `assets/source/**/*.json` 排除於 Git LFS 之外：`meta.json` 進 LFS 會讓 diff 不可讀，且 CI 以 `lfs: false` checkout 時只拿得到 pointer
- cargo workspace 骨架：`core/*` 八個 crate ＋ `tools/baker` ＋ `xtask`，依賴邊照 `architecture.md §5.1` 連好
- `cargo xtask lint-deps`：宣告式 `deps-policy.toml` 強制依賴方向與「wgpu 只在 render」（鐵律 1），違規一次全報
- GitHub Actions CI：單一 workflow，觸發路徑為 §12.3 三條 ＋ `xtask/**` ＋ workspace manifest ＋ workflow 自身
- 新增 `docs/specs/build-infra.md`：workspace 佈局、lint 規則、CI 形狀
- package name 用 `colorlull-*` ＋ 短 `lib.name`，避開 crates.io 撞名
- toolchain 版本 pin 的 SSOT 定為 `rust-toolchain.toml`，mise 只負責裝 rustup
- 新增 `docs/specs/ffi-contract.md`（S0 Rust 契約設計）。四項決策：
  - uniffi 改用 **proc-macro**，不用 UDL——UDL 是第二份真相，與 `§7` 核心原則衝突
  - FFI 型別自成一層住 `core/engine`，與 core crate 原生型別分離；`core/stroke` 不沾 uniffi
  - `§6 Boundary 1` 三處修正：`Engine::new(surface,…)` 拆成 `new` ＋ `attach_surface`、
    `subscribe` → `set_state_listener(Option<Arc<…>>)`、fallible／infallible 界線定死
  - `verify-generated` 改用 `core/engine/ffi-lock.toml` 指紋比對——`Generated/` 是 gitignore 的，
    原本「比 diff」的 gate 照字面實作是空的
- `core/engine` headless mock 落地：20 個 FFI 方法 ＋ `core/app-state` 骨架。8 條契約測試不需 GPU 或模擬器，
  跑在既有的 Linux CI——契約正確性因此不押在「xcframework 有沒有接通」這件高風險的事上
- `Engine::mutate` 是唯一的狀態變更入口：發送前釋放鎖（C2）做成結構而非紀律，並只在 `UiState`
  投影**真的改變**時 emit（C8）——否則 `append_samples` 在 120Hz 下等於每秒 120 次內容相同的回呼
- 新增 `docs/contracts.md`：FFI 表面速查表（S0 驗收「逐一對照無缺漏」的對照基準）、
  語意條款 C1–C8、semver 判定、遷移記錄格式。**它不是 FFI 的真相**，真相是 `core/engine` 的標註
- C8 的兩個後果寫進契約，此前只存在於程式碼裡：`Inner` 建構時就 seed `last_emitted`，
  所以 attach listener **不會**拿到初始快照（Bridge 必須自己呼 `state()`）；去重用 `PartialEq` 比 `f32`，
  Swift 端送進 `NaN` 會讓去重失效（v0 已知瑕疵，該擋的位置是 Bridge 的輸入驗證）
- `EngineError::NotImplemented` 標記為有期限：E3 結束時必須從 enum 移除，列為 v1 的 exit criteria
- `architecture.md §7` CI 守門改寫為 `ffi-lock.toml` 指紋比對——原文「`Generated/` 出現 diff 則失敗」
  與 §12.2 的 gitignore 互相矛盾
- `architecture.md §6` Boundary 1 補上三處修正的實際簽章與理由，並標記 `pick_color` 同步簽章 vs
  async readback 的未解張力；§10.1 的 Swift protocol 標為待對齊（歸 iOS 那份 spec）
- `architecture.md §12.1` 與 `roadmap/S0.md`：「uniffi UDL」→「uniffi proc-macro」
- `cargo xtask ios` / `verify-generated` 由指令位改為實作；`rust-toolchain.toml` 加兩個 Apple target
- `build-infra.md §2`：`engine → stroke` 與 `document → history` 並列為懸而未決的依賴邊，
  都不預先開通——「必須先改 policy 檔」正是逼決策浮上檯面的機制
- 新增 `docs/specs/ios-scaffold.md`（S0 iOS 骨架設計）。順帶消掉兩個文件裡已不成立的警告：
  modulemap 的 `framework` 關鍵字問題早被 `xtask ios` 的 `xcframework: false` 繞開，
  且 uniffi 生的 protocol 叫 `RustEngineProtocol`，與手寫的 `EngineProtocol` 不撞名。四項決策：
  - `.xcodeproj` 手工維護進 git，但每個 target 一個 file-system synchronized group——
    新增 Swift 檔不動 `project.pbxproj`，這是手工路線唯一撐得住的形式
  - `EngineProtocol` 的 `state` 用 `@Observable` 取代 `§10.1` 草案的 `AnyPublisher`；
    `new` / `set_state_listener` / `makeCanvasView` 記名為對照表的三個例外
  - S0 就同時交 `MockEngine` 與 `RustEngineAdapter`，並用**差分測試**釘住兩者行為一致——
    S0 最大的技術風險（首次在 Xcode 連起來）因此在 W4 退掉，而不是拖到 E1
  - CI 補 `xcodebuild build-for-testing`（編但不 boot 模擬器）＋ `cargo xtask lint-ios`，
    後者把「Shell 沒有一行引用 `RustEngine`」從人工目視變成機械檢查
- 兩個「`xtask ios` exit 0 但 Swift 端編不起來」的坑，實測後解掉（CI 的 macOS gate 只建
  xcframework、不編 Swift，兩者都攔不到）：
  - modulemap 的 `xcframework` 旗標固定為 `false`。`framework module` 只在 framework 佈局下成立，
    而 `-create-xcframework -library` 產的是 library slice——實測 `framework module` 讓生成的
    Swift 出 220 個 error，改成 `module` 後 0 個。`import` 包在 `#if canImport` 裡，對不上時
    不會報錯而是整批型別消失
  - Rust 的 Object 由 `Engine` 改名為 **`RustEngine`**。uniffi 生的 `<ObjectName>Protocol` 原本
    叫 `EngineProtocol`，與 `S0.md` 要求手寫的同名檔在同一個 module 裡是 invalid redeclaration；
    uniffi 的 Object 沒有 rename 機制，只能改 Rust 端。改完正好對上 `§10.1` 一直在講的 `RustEngine`

## v0.1 (2026-08-03)

- Roadmap 重排：資產分發前移 S1、解鎖 Worker 前移 S2、手動匯出前移 S2b（新增里程碑 W28）、S3 縮為 3 週；全線仍 38 週
- 補上原本漏排的「完成建議與完成動畫」（Must Have）
- 拍板：Settings 升為第五條正式路由；筆刷透明度與吸管都做（回寫 FFI）
- 時程修正：D1 W4–6 → W5–7、繪師徵才 W3 → W1、ASC 帳務 W20 → W15、名稱檢查移到 M0
- `specs/assets-spec.md` 定稿為 v1.0；難度分級門檻表標記為全專案唯一依據
- prd / architecture / roadmap 三份文件版本統一為 v0.1
- Create basic documents -- architecture.md, prd.md, roadmap.md and CHANEGLOG.md
- Add `t_erase` in protocol
- Move Android release after v1
- Add per-milestone implementation checklists to roadmap (roadmap v0.3)
- Split M0 into M0 (scaffolding / spec / assets) and M1 (baker / contracts / tests)
- Decide iOS backup mechanism: iCloud Drive ubiquity container (was undecided)
- Promote manual export/import to v1 Must Have
- Define watercolor per-stroke edge path: unsharp on `T_wet` at commit
- Drop `contracts/tokens.json` from v1; use a hand-written `DesignTokens.swift`
- Move Logo and branding from R1 to S2 so soft-launch conversion data is trustworthy
- Add `CLAUDE.md` as the always-loaded project context (invariant constraints only, ~54 lines)
- Add `docs/README.md` as the on-demand documentation index ("when to read → what to read")
- Split `roadmap.md` (912 lines) into `docs/roadmap/`: index, 11 milestone files, `checkpoints.md`, `beyond-v1.md`