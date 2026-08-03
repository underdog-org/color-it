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
| [specs/assets-spec.md](./specs/assets-spec.md) | 繪師交付規格（PNG ＋ meta.json 硬性要求） | v2.0 |
| [specs/baker-core-design.md](./specs/baker-core-design.md) | **`tools/baker` ＋ `core/colorpack` 的 SSOT**：色標交付管線、`.colorpack` 容器、檢查清冊、測試（**已實作**） | 長 |
| [specs/naming.md](./specs/naming.md) | 產品名稱決策記錄（為何不叫 Color It）＋ 上架前必補的查證項 | 短 |
| [specs/ffi-contract.md](./specs/ffi-contract.md) | uniffi 型別與方法表、headless mock、`xtask ios`（S0 Rust 契約**設計**） | 509 行 |
| [specs/E2-spec-plan.md](./specs/E2-spec-plan.md) | E2 四份 spec 的拆分依據與撰寫契約（**里程碑期間有效，收尾後刪除**） | 220 行 |
| [specs/E2-brush.md](./specs/E2-brush.md) | 三張 tip、五支 preset 的差異軸、velocity 啟用、RNG 契約與 golden（**里程碑期間有效，收尾後刪除**） | 264 行 |
| [perf-baseline.md](./perf-baseline.md) | **效能量測方法 ＋ 實測數字**：m2p 流程、對帳、D2／D3／D4 劇本、調校記錄 | 短 |
| [contracts.md](./contracts.md) | FFI 的**現況**：表面速查表、語意條款 C1–C13、semver 判定、遷移記錄 | 短 |

`roadmap/` 已全部拆檔，一次只讀需要的那一份。

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
| 動 baker、`.colorpack` 格式 | `§9` 資產管線 ＋ `specs/baker-core-design.md` |
| 繪師交付要交哪幾張圖 | `specs/assets-spec.md`（為什麼這樣改 → `specs/baker-core-design.md §1`） |
| 動 iOS 整合、手勢、frame pacing | `§10` 平台整合 |
| 動 R2、備份、雲端 | `§11` 雲端 |
| 動 CI / 建置流程 | `§12` 建置與 CI |
| 動 workspace 骨架、依賴 lint、xtask 指令 | `§3` repo 結構 ＋ `architecture.md §12` |
| 產品叫什麼、名稱還有哪些沒查 | `specs/naming.md` |
| 動 FFI 型別／方法簽章、uniffi 生成、`ffi-lock.toml` | `specs/ffi-contract.md` ★ |
| 動 Xcode 專案、`EngineBridge`、五條路由、Swift 測試 | `apps/ios/README.md` |
| 某方法現在到底做了什麼、Swift Bridge 能假設什麼 | `contracts.md` ② ③ ★ |
| FFI 改動算 major 還是 minor、遷移怎麼記 | `contracts.md` ④ ⑤ |
| 效能目標、量測 | `§13` 效能觀測 ＋ `perf-baseline.md` |
| 風險與退路 | `§14` |

### 渲染／筆刷／油漆桶（已落地，回寫進 architecture.md）

E1 期把渲染、筆刷、油漆桶、輸入定案為六份 spec；里程碑收尾時決策已回寫進 `architecture.md` 與
`contracts.md`，spec 已刪除。現在查這幾塊的設計 → `architecture.md §4`／`§5`／`§10` 與 `contracts.md`
語意條款；某個參數是量出來的還是猜的 → `perf-baseline.md` 調校記錄。

> **原始碼註解裡還有約 150 處 `E1-*.md §N` 引用。** 那六份 ＋ 拆分計畫的原文在 git 歷史，
> 取用：`git show 8089d0e:docs/specs/E1-stroke.md`（其餘同理）。**這些引用是待清理的債**——
> 正確做法是把被引用的那句理由就地寫進註解，而不是改指 `architecture.md §N`（章節號一樣會漂）。
> 新寫的 spec 不得再被原始碼註解引用，見 `specs/E2-spec-plan.md`「撰寫約束」。

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
