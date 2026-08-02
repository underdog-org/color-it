# Color It - Changelog

## [Unreleased]


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