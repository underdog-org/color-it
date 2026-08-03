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
- **直式比例正名 `4:5` → `3:4`，runtime `1536×1920` → `1536×2048`**。母帶 3072×4096 本來就是 3:4；
  3072→1536 是 ÷2 而 4096→1920 是 ÷2.133，兩軸倍率不同會壓扁畫面，也讓 `architecture §9.1`
  「4096→2048 是整數倍降採樣」在直式路徑上不成立。像素數 3.15M 仍低於 1:1 的 4.19M，
  「以 1:1 為記憶體上界」的論證不變。回寫 `prd §5.3 §6 §9`、`architecture §4.1.1 §9.1 §9.3`、
  `assets-spec §3 §5 §7`、`roadmap/M0 M1`。匯出 letterbox 的 4:5（IG 直式）是社群平台目標比例，**不受影響**
- **階段內 fail-fast 收窄到「算不下去的那兩條」**：`size-mismatch`（逐像素會越界）與
  `unique-color-overflow`（CC 會切出幾百萬區）。原本 `canvas-size` 被綁進 structural failure、
  `reserved-color`/`tiny-color-area` 被綁進 flats broken，結果是繪師改完尺寸重交才第一次看到縫隙問題
  ——正是 `§4.2` 那條「來回從 N 天變 1 天」要消除的情境
- `flats` 唯一色數改掛 `Report` 而非 `Summary`：`§2.6` 要求一律列出，而被退件的素材才是最需要它的地方。
  快篩命中時掃描提前中止，該值是下界，渲染成 `≥N` 而不是假裝是實際值
- `region_count` / `difficulty` 改依**輸出解析度**的存活區域數判定，不再靠 `region-count-drift`
  是 Error 維持的不變式
- zip 的 unix 權限與 host system 由「不寫」改為**顯式釘死** `0o644` / `System::Unix`：zip 格式沒有
  「不寫」這個選項，而 crate 預設的 system 依 `cfg!(windows)` 分歧，會讓「同輸入位元相同」
  只在同一個 OS 家族內成立
- **`content_hash` 加凍結向量**（兩層：`hash::content_hash` 最小輸入 ＋ 完整 sample pack）。
  在此之前只驗「改內容會換 hash」，把長度前綴改成 BE 或調換 `ENTRY_ORDER` 測試都會全綠——
  而 `§3.3` 的整段立論是 hash 漂移一次等於全世界的使用者作品同時失效
- `ColorPack::open()` 補**重算 hash 比對**與 `region_ids < region_count` 驗證；
  `check_schema_version` 收嚴成契約的 `^[0-9]+\.[0-9]+$`（原本 `"1"`、`"1.0.0"` 都會通過）
- **`torture-01` 首次有自動化證據**：一條 e2e 直接烘它（生成器現場產生，不讀 LFS），斷言通過且
  唯一診斷是 `region-count-range`。另補 `synth-lock.json` drift 守門——改了 `synth.rs` 卻忘了跑
  `gen-torture`，測試會失敗。hash 取原始 RGBA 而非 PNG bytes，避免綁到 `png` crate 的 deflate 實作
- `unassigned-pixel` 由「Master ＋ Output」降回**純 Master**：輸出階段恆真（majority 必定產出既有 ID），
  留一條永不觸發的檢查不如降級，實作端只保留 `debug_assert`
- `architecture §9.2` 補降採樣對象是 ID map、majority 平手規則、4-連通、膨脹的精確語意；
  `§9.4` 釐清 `palette[]`（去重色票給 UI）與 `suggested_color`（逐區）的分工。
  `specs/baker-core-design.md` 升 v1.2，依實作對齊 §1／§3.2／§4.1／§4.2／§5
- `tools/baker` 管線落地：source → 母帶檢查 → connected components → 合成白底 → 降採樣
  → 膨脹 → 輸出檢查 → 逐區取建議色 → 縮圖 → `.colorpack`。`baker` 同時是 bin 與 lib
  （`bake(dir, opts) -> Report` 要被 xtask 當 library 呼叫，不 shell out，錯誤訊息才帶得回來）
- **色彩空間判定不看 iCCP 名稱，改解 ICC tag table 比對 colorant**（不引第三方 crate）。
  實測 `kirby-demo-1` 的 `flats`/`reference` 帶的是泛用名 `ICC Profile`，照名稱判會誤退合格素材。
  同時發現：這四張圖的 `wtpt` 存的是 **D65**（ICC v2 慣例存實際白點）而不是 PCS 的 D50，
  且 Display P3 的 `wtpt` 同樣是 D65——**白點根本分不出 sRGB 與 P3**，真正的判準只有
  `rXYZ`/`gXYZ`/`bXYZ`（兩者差 0.08，容差取 0.02）。白點只用來擋「白點根本不是這兩個」的怪 profile
- 判定順序定為 `iCCP` → `sRGB` chunk → `gAMA`+`cHRM` → 皆無即通過（`assets-spec §3` 明列）。
  iCCP 存在但解不出 colorant 時**往下一個訊號走而非拒收**——寧可漏放也不要誤退合格素材
- `shade` 的 luma < 60 判定跑在**合成白底之後**：交透明底的 `shade` 在合成前 RGB 是 0，
  照原值判會把每一張合規的透明底 `shade` 都退掉
- 新增 `source-incomplete` 檢查碼（`§4.1` 清冊同步更新）：缺 `lineart`/`flats`/`reference` 時
  若走 exit 2（baker 自身故障）會把繪師的交付疏漏誤報成工具壞掉
- `core/colorpack` 落地：`.colorpack` 容器的讀寫、`manifest` / `regions.json` 型別、R16 RLE、
  正規化 `content_hash`。**reader 與 writer 同時做**（不留到 E1）——round-trip 是驗證 writer
  最便宜的手段，而 reader 只是 zip central directory 解析加 RLE 解碼
- `content_hash` 定義為「未壓縮內容的正規化串流 SHA-256」而非 hash 整個 zip 檔：
  `architecture §8.4` 規定文件永遠指向它原本的 `asset_hash`，hash 若受 zip crate 版本或
  deflate 實作影響，升級一個依賴就會讓全世界的使用者作品失效
- `.colorpack` 的壓縮方式**由副檔名決定，無例外**：二進位 Stored（runtime 可 mmap 零拷貝取 slice）、
  JSON Deflate（高區域數的 `regions.json` 到 MB 級）。mtime 固定 zip epoch、deflate level 固定，
  同輸入重跑位元相同
- 新增 `contracts/colorpack.schema.json`（SSOT）＋ Rust 端三條對照測試，其中一條刻意餵壞資料，
  防止 schema 寫鬆之後變成永遠通過的假綠燈
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