# Colorlull — 文件索引

> **這份索引是給 agent 與人類共用的導航表。**
> 三份主文件都很長（PRD 567 行、Architecture 1445 行），**不要整份載入**——
> 用下方的「何時讀 → 讀哪裡」定位到章節，只讀那一節。

## 文件地圖

| 文件 | 回答什麼問題 | 大小 |
|---|---|---|
| [prd.md](./prd.md) | **要做什麼、為什麼** — 產品定位、原則、體驗、範圍 | 567 行 |
| [architecture.md](./architecture.md) | **怎麼做** — 技術選型、分層、渲染、契約、持久化 | 1445 行 |
| [roadmap/](./roadmap/README.md) | **什麼時候做、做完怎麼算** — 12 個里程碑 ＋ 決策點 | 每份 38–90 行 |
| [specs/assets-spec.md](./specs/assets-spec.md) | 繪師交付規格（PNG ＋ meta.json 硬性要求） | v1.1 |
| [specs/baker-seeds.md](./specs/baker-seeds.md) | 色標交付設計：`flats`／`reference` → `seeds.png`，區域改由線稿封閉區推導（**草案，待 Phase 0 驗證**） | 短 |
| [specs/build-infra.md](./specs/build-infra.md) | workspace 佈局、依賴 lint 規則、CI 形狀（M0 基建） | 短 |
| [specs/naming.md](./specs/naming.md) | 產品名稱決策記錄（為何不叫 Color It）＋ 上架前必補的查證項 | 短 |
| [specs/ffi-contract.md](./specs/ffi-contract.md) | uniffi 型別與方法表、headless mock、`xtask ios`（S0 Rust 契約**設計**） | 509 行 |
| [specs/ios-scaffold.md](./specs/ios-scaffold.md) | Xcode 專案佈局、`EngineProtocol` / `MockEngine` / `RustEngineAdapter`、五條路由、Swift 測試與 CI gate（S0 iOS 側） | 短 |
| [specs/E1-spec-plan.md](./specs/E1-spec-plan.md) | E1 六份 spec 的拆分依據、型別歸屬、撰寫約束（**不是 spec**） | 短 |
| [specs/E1-wgpu.md](./specs/E1-wgpu.md) | `RenderContext`、`DocumentResources` 七資源、pass ↔ 資源矩陣、mask uniform（**其餘四份只引用**） | 短 |
| [specs/E1-composite.md](./specs/E1-composite.md) | Pass 3 六層 WGSL、色彩空間、`set_viewport`、擴散動畫 buffer、Mask Mode | 短 |
| [specs/E1-stroke.md](./specs/E1-stroke.md) | `generate_dabs` 契約、One-Euro ＋ Catmull-Rom、`BrushPreset`、Pass 1／2 | 短 |
| [specs/E1-bucket.md](./specs/E1-bucket.md) | `document.apply(Op)` 最小版、`tap` → region ID、擴散動畫 CPU 側 | 短 |
| [specs/E1-input.md](./specs/E1-input.md) | present 路徑定案、FrameDriver、`InputAdapter`、座標系、`cancelStroke` | 短 |
| [specs/E1-perf.md](./specs/E1-perf.md) | motion-to-photon 流程、記憶體對帳、D2／D3／D4 劇本、調校項總表 | 短 |
| [perf-baseline.md](./perf-baseline.md) | **實測數字**（流程在 `E1-perf`）：各裝置的 m2p／frame p99／記憶體，調校記錄 | 短 |
| [contracts.md](./contracts.md) | FFI 的**現況**：表面速查表、語意條款 C1–C13、semver 判定、遷移記錄 | 短 |

`roadmap/` 已全部拆檔，一次只讀需要的那一份。
六份 `E1-*` 也是一組，**先讀 `E1-wgpu`**——其餘五份以它為共同輸入。

---

## 何時讀 → 讀哪裡

### 產品面（prd.md）

| 何時 | 章節 |
|---|---|
| 判斷某提案是否違反產品定位 | `§3` 核心原則 P1–P4 ★ 最常用 |
| 某功能該長什麼樣、互動細節 | `§4` 核心體驗 |
| 某畫面有什麼、路由怎麼走 | `§5` 路由與畫面 |
| 訂閱、定價、付費牆 | `§7` 商業模式 |
| 資料存哪、隱私邊界 | `§8` 資料與隱私 |
| 這功能屬於 v1 嗎 | `§9` 範圍（Must / Should / Later / Don't） |
| 「這看起來像 bug」 | `§10` 已接受的取捨 T1–T6 — **先查這裡再回報** |

### 技術面（architecture.md）

| 何時 | 章節 |
|---|---|
| 選型理由 / 為什麼不用某方案 | `§1` |
| 這段邏輯該放哪一層 | `§2` 系統分層、`§6` 邊界拆分 ★ |
| 新增檔案放哪個目錄 | `§3` Repo 結構 |
| 動渲染、blend、tile、dab | `§4` 渲染模型 |
| 動 core crate 的介面 | `§5` Core crate 設計 |
| 動 FFI / uniffi / JSON schema | `§7` 契約層 ＋ `specs/ffi-contract.md` |
| 動存檔、oplog、Undo、崩潰復原 | `§8` 狀態與持久化 |
| 動 baker、`.colorpack` 格式 | `§9` 資產管線 ＋ `specs/assets-spec.md` |
| 繪師交付要交哪幾張圖、為什麼不再交 `flats` | `specs/baker-seeds.md` |
| 動 iOS 整合、手勢、frame pacing | `§10` 平台整合 |
| 動 R2、備份、雲端 | `§11` 雲端 |
| 動 CI / 建置流程 | `§12` 建置與 CI ＋ `specs/build-infra.md` |
| 動 workspace 骨架、依賴 lint、xtask 指令 | `specs/build-infra.md` |
| 產品叫什麼、名稱還有哪些沒查 | `specs/naming.md` |
| 動 FFI 型別／方法簽章、uniffi 生成、`ffi-lock.toml` | `specs/ffi-contract.md` ★ |
| 動 Xcode 專案、`EngineBridge`、五條路由、Swift 測試 | `specs/ios-scaffold.md` ＋ `apps/ios/README.md` |
| 某方法現在到底做了什麼、Swift Bridge 能假設什麼 | `contracts.md` ② ③ ★ |
| FFI 改動算 major 還是 minor、遷移怎麼記 | `contracts.md` ④ ⑤ |
| 效能目標、量測 | `§13` 效能觀測 ＋ `specs/E1-perf.md` |
| 風險與退路 | `§14` |

### E1 面（specs/E1-*.md）

| 何時 | 檔案 |
|---|---|
| 動 wgpu 資源、格式、pass 的讀寫權 | `E1-wgpu.md` ★ 六份的共同輸入 |
| 動 composite、色彩空間、viewport、Mask Mode | `E1-composite.md` |
| 動筆刷、濾波、插值、`BrushPreset`、Pass 1／2 | `E1-stroke.md` |
| 動油漆桶、`document.apply`、擴散動畫的推進 | `E1-bucket.md` |
| 動 iOS 輸入、FrameDriver、座標系 | `E1-input.md` |
| 要量什麼、怎麼量、D2／D3／D4 怎麼跑 | `E1-perf.md` |
| 現在量到多少、某個參數是量出來的還是猜的 | `perf-baseline.md` |
| 為什麼是這樣切、哪份擁有哪個型別 | `E1-spec-plan.md` |

### 排程面（roadmap/）

| 何時 | 檔案 |
|---|---|
| 當前該做什麼、DoD 是什麼 | `roadmap/<當前里程碑>.md` ★ |
| 週次總表、里程碑相依圖 | `roadmap/README.md` |
| D1–D8 該量什麼、外部前置 lead time、單人專案的風險 | `roadmap/checkpoints.md` |
| 為什麼是這個排法（繪師為何延到 W16、S0 為何還要做） | `roadmap/strategy.md` |
| Android 何時做、v1 之後 | `roadmap/beyond-v1.md` |

---

## 讀法

```bash
grep -n '^## 4\.' docs/architecture.md      # → 得到起始行號
```
再 `Read(docs/architecture.md, offset=<行號>, limit=<該節長度>)`。

跨文件查證（例如「這個設計違反哪條原則」）交給 subagent，
讓長文件在子 context 燒掉，主 session 只收結論。

## 維護規則

- **決策改變時必須即時寫回文件**（`architecture.md §14` R9：單人專案的 bus factor）
- 里程碑推進時更新 `CLAUDE.md` 的「當前狀態」
- 新增文件時同步更新本索引與 `CLAUDE.md` 的文件索引表
