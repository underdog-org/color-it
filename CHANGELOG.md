# Colorlull - Changelog

## [Unreleased]

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