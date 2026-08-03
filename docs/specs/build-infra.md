# 建置基礎建設規格（M0）

> 涵蓋 M0「基建」三項：workspace 骨架、邊界 lint、CI。
> 對應 `architecture.md §3 §5.1 §12`、`roadmap/M0.md`。

## 1. Workspace 佈局

```
Cargo.toml              [workspace] members ＋ [workspace.dependencies] 集中版本
rust-toolchain.toml     pin 1.97.1 ＋ rustfmt / clippy components（edition 2024）
core/{colorpack,stroke,render,document,history,oplog,app-state,engine}/
tools/baker/            bin ＋ lib（`[lib] name = "baker"`）
xtask/                  bin
```

M0 的 crate 全是空殼：`Cargo.toml` ＋ 空 `lib.rs`，**但依賴邊要照 §5.1 連好**——
依賴方向 lint 從第一天就有東西可驗，E1 開工時不必回頭改結構。

**package name 用 `colorlull-*`，`lib.name` 用短名。**
`render`、`document`、`history` 在 crates.io 都有人佔用。path dependency 本身不會解析錯，
但 `cargo add` 一類的操作容易踩到。`colorlull-render` ＋ `lib.name = "render"`
讓程式碼裡仍是 `use render::…`。

## 2. 依賴矩陣

| crate | 內部依賴 |
|---|---|
| `colorpack` | — |
| `stroke` | colorpack |
| `oplog` | colorpack |
| `render` | colorpack, stroke |
| `document` | colorpack, oplog |
| `history` | colorpack, oplog |
| `app-state` | — |
| `engine` | app-state, document, history, render |
| `baker` | colorpack |
| `xtask` | baker |

**`baker` 同時是 bin 與 lib，`xtask` 依賴它而不 shell out。** `bake` 與 `gen-torture` 都直接呼叫 `baker::` 的函式——shell out 拿不回結構化的錯誤，只拿得到 stdout。代價是 `xtask` 的編譯時間掛上 baker 的依賴樹（`png`、`zip`、`jpeg-encoder`）。

**`render/Cargo.toml` 現在就真的列 wgpu**，即使還沒有程式碼。
代價是 CI cold build 多兩三分鐘（有 cache 後不痛），
換來「wgpu 只在 render」這條規則從第一天就有正例，而不是一條從未被行使的規則。

### 兩條懸而未決的邊

| 邊 | 為什麼可能需要 | 何時揭曉 |
|---|---|---|
| `document → history` | §5.1 的依賴圖上沒有，但同節內文說 `document` 是協調者、要把 `UndoEntry` 送進 `history` | E3 |
| `engine → stroke` | `ffi::InputSample` → `stroke::InputSample` 的轉換需要 `engine` 認識 `stroke`，依賴圖上同樣沒這條邊 | E1 |

兩條都不預先開通——policy 檔照 §5.1 的圖寫。
等真的需要連的時候，「必須先改 policy 檔」正是逼這個決策浮上檯面的機制。
S0 沒踩到第二條：headless mock 沒有東西消費 samples，只定義 DTO、不寫轉換。

## 3. `xtask lint-deps`

全 external 白名單維護不起來（serde / thiserror 到處都是），所以拆成三層：

```toml
# xtask/deps-policy.toml（節錄；實際每個 member 都要有自己一節）
[workspace]
banned-external = ["wgpu"]      # 預設全禁，需逐 crate 解禁

[crates.stroke]
internal = ["colorpack"]

[crates.render]
internal = ["colorpack", "stroke"]
external = ["wgpu"]             # ← 唯一解禁 wgpu 的地方
```

`external` 只用來解禁 `banned-external` 的項目——一般外部依賴（serde、thiserror）
不需登記。全 external 白名單維護不起來，這是刻意的取捨。

檢查邏輯——讀 `cargo metadata --no-deps`，**只看直接宣告的依賴**
（wgpu 經由 render 傳遞到 engine 是合法的，符合 M0 驗收「不得在 `Cargo.toml` 列入 wgpu」的字面）。
**不區分 normal / dev / build**：`stroke` 就算只在 dev-dependencies 列 wgpu，
也違反「純 CPU、零 GPU 依賴」。

1. 每個 workspace member 都必須在 `[crates.*]` 有一節——新增 crate 忘了登記就失敗
2. 內部依賴不在該 crate 的 `internal` → 失敗
3. `banned-external` 的項目不在該 crate 的 `external` → 失敗
4. `[crates.*]` 有一節但沒有對應的 member → 失敗（crate 改名或刪除後的殘留）

違規一次全部回報，不在第一個就停。

M0 驗收「刻意在 `stroke` 加 wgpu 依賴時 lint 會失敗」**做成 xtask 的單元測試**：
用 fixture manifest 餵給 policy engine，驗它會拒絕。
比在 CI 裡真的去改 `stroke/Cargo.toml` 乾淨，且每次跑 test 都會驗。

## 4. xtask 指令位

| 指令 | 狀態 |
|---|---|
| `lint-deps` | M0 實作（alias 定義在 `.cargo/config.toml`） |
| `lint-ios` | **S0 實作**。純文字檢查 `apps/ios/ColorApp/**` 不得出現 `RustEngine`、不得 `import colorlull_engine`。把驗收「App Shell 沒有一行直接引用 `RustEngine`」從目視變成機械檢查，跑在 Linux 上零成本 |
| `gen-torture` | M0 實作。決定性產生 `assets/source/torture-01/`，重跑逐位元相同（否則 LFS 每跑一次胖一份） |
| `bake <dir>` | 指令位，body 回傳「M1 實作」錯誤訊息 |
| `gen-torture` | M0 實作、**M1 移進 `baker::synth`**。決定性產生 `assets/source/torture-01/`，重跑逐位元相同（否則 LFS 每跑一次胖一份）。順帶寫出 `synth-lock.json`，讓「改了生成器卻沒重跑」在 CI 失敗 |
| `bake <dir> [--out] [--report]` | **M1 實作**。直接呼叫 `baker::bake()`。exit code：0 通過（可含警告）／1 有 Error／2 baker 自身故障 |
| `ios` | **S0 實作**。host cdylib 生 Swift binding ＋ 兩個 iOS `.a` 打包 `.xcframework`，重算 `core/engine/ffi-lock.toml` |
| `verify-generated` | **S0 實作**。重新生成後比對 `ffi-lock.toml` 指紋，跑在 Linux（見 `specs/ffi-contract.md §6`） |

用 clap 定義。**不用 `todo!()`**——要的是明確錯誤訊息，不是 panic。

## 5. CI

單一 `.github/workflows/ci.yml`。M0 階段 workspace 還小，
按 §12.3 分流 job 省不到時間，反而讓設定提早複雜化；要分流隨時可分。

- 觸發於 `push`（`main`）與 `pull_request`
- `runs-on: ubuntu-latest`，checkout `lfs: false`，`Swatinem/rust-cache`
- steps：`fmt --check` → `clippy -D warnings` → `xtask lint-deps` → `xtask lint-ios` → `xtask verify-generated` → `test --workspace` → `build --workspace`
- macOS job（paths-filter 節流）：`xtask ios` → `ffi-lock.toml` 應無 diff → `xcodebuild build-for-testing`。
  **不 boot 模擬器**——花編譯時間、不花啟動時間，就能守住 modulemap／link／protocol 對齊這三類
  只有 Swift 編譯器抓得到的錯。測試本身在本機跑

觸發路徑除了 §12.3 的 `core/**`、`tools/baker/**`、`contracts/**`，
**再加 `xtask/**`、`Cargo.toml`、`Cargo.lock` 與 workflow 檔自身**——
否則改了 policy 檔或 CI 設定本身不會被驗，這是實際會咬人的漏洞。

## 6. Git 與 toolchain

- `.gitattributes`：`assets/source/** filter=lfs diff=lfs merge=lfs -text`，
  但 `assets/source/**/*.json` 以 `!filter !diff !merge text` 排除——`meta.json` 是 150 bytes 的文字檔，
  進 LFS 會讓 diff 不可讀，且 CI 用 `lfs: false` checkout 時只拿得到 pointer，讀不出 `category`
- `.gitignore`：`/target`、`assets/packs/`、`apps/*/Generated/`、`apps/*/generated/`、`.DS_Store`
- git-lfs 加進 `mise.toml`（目前未安裝），跟 kotlin / rust 一致由 mise 管
- **toolchain 版本的 SSOT 是 `rust-toolchain.toml`**，mise 只負責把 rustup 裝起來——避免兩邊各 pin 一次
- S0 起 `rust-toolchain.toml` 多一行 `targets = ["aarch64-apple-ios", "aarch64-apple-ios-sim"]`。
  代價是 Linux CI 也會裝兩份用不到的 std（幾十 MB），換來本機少一個「沒寫在任何地方的前置步驟」

## 不做

cargo-deny、CI path filter 分流 job、建 GitHub repo、
iOS / Android 相關的一切、`contracts/` 的 schema 內容（M1）。
