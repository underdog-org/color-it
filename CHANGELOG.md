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