# FFI 契約規格（S0）

> 涵蓋 S0 的 **Rust 契約面**：`core/engine` facade、uniffi 型別與方法表、`xtask ios` /
> `verify-generated`、`docs/contracts.md` 首版。
> 對應 `architecture.md §5.1 §6 §7 §12.1`、`roadmap/S0.md`。
>
> **不涵蓋** Xcode 專案、`EngineProtocol.swift`、`MockEngine.swift`、五條路由空殼——另開一份。
>
> 文中裸寫的 `§n` 一律指 `architecture.md`；本規格內部互引寫成「第 n 節」。

本規格定義的一切標記為 **v0**，預告在 E2（工具集完整）與 E3（Undo／持久化接上）各有一次修正。

## 1. 撰寫形式：proc-macro，不是 UDL

`S0.md` 與 `§12.1` 原本寫「uniffi UDL」。**改為 proc-macro**（`#[uniffi::export]` ＋
`uniffi::setup_scaffolding!()`），理由是 UDL 與 Rust 實作是兩處要手動保持一致，
與 `§7`「一份契約只能存在一次」直接衝突；且 UDL 對帶欄位的 enum 表達力落後，
而 `Tool` 正是帶欄位的 enum。

uniffi 仍從 S0 導入、v1 仍只生成 Swift，變的只有撰寫形式。
proc-macro 模式**必須**走 library mode bindgen（從編譯好的庫讀 metadata）。

## 2. 型別

### 住哪

新增 `core/engine/src/ffi.rs`，所有 `#[derive(uniffi::*)]` 型別都在這裡，
**與 core crate 的原生型別是兩組**，`engine` 負責轉換。

`§7` 說 FFI 的 SSOT 是 `core/engine`——這是那句話的字面實現。附帶三個好處：
`core/stroke` 完全不知道 uniffi 存在，`§5.2` 的 golden test 契約不被污染；
FFI 表面能為跨界最佳化；內部重構不等於 FFI major bump，`§7` 的 semver 表才有著力點。

### 定義

```rust
#[derive(uniffi::Record)]
pub struct InputSample {
    pub x: f32, pub y: f32,
    pub t: f32,
    pub pressure: f32,
    pub radius: f32,          // ★ 手指模式的動態來源（§10.2）
    pub tilt_x: f32, pub tilt_y: f32,
    pub predicted: bool,      // 預測點不進 oplog
}

#[derive(uniffi::Record)]
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }   // sRGB 8-bit

#[derive(uniffi::Enum)]
pub enum BrushId { SoftRound, Marker, Crayon, Airbrush, Watercolor }

#[derive(uniffi::Enum)]
pub enum Tool {
    Brush { preset: BrushId, color: Rgba, size: f32, opacity: Option<f32> },
    Eraser { size: f32 },
    Bucket { color: Rgba },
}

#[derive(uniffi::Record)]
pub struct Transform { pub scale: f32, pub tx: f32, pub ty: f32 }

#[derive(uniffi::Record)]
pub struct Progress { pub colored: u32, pub total: u32 }

#[derive(uniffi::Record)]
pub struct UiState {
    pub tool: Tool,
    pub can_undo: bool,       // §4.1.1：據實回報 pool 內容，不是假設步數
    pub can_redo: bool,
    pub progress: Progress,
}

#[derive(uniffi::Record)]
pub struct SurfaceHandle {
    pub layer_ptr: u64,       // CAMetalLayer 位址
    pub width_px: u32, pub height_px: u32,
    pub scale: f32,           // contentsScale
}
```

四個決定：

- **`Vec2` 攤平成 `x`／`y`**——uniffi 沒有 SIMD 概念，而 iOS 的 InputAdapter 本來就是從
  `UITouch` 逐欄位讀出來的，攤平反而少一層包裝。
- **`BrushId` 是 enum 不是字串**——`§4.6` 的五支 preset 是程式碼常數，不是資產驅動，
  工具列也是固定五顆按鈕。enum 換到 Swift 端是 exhaustive switch。
- **`Transform` 沒有 rotation**——畫布操作只有縮放平移。加 rotation 是 major bump。
- **`UiState` 只有四個欄位**。`§5.1` 授權「工具、顏色、大小、進度」，`§4.1.1` 額外要求
  undo 可用狀態據實回報。刻意不加 `is_dirty` / `doc_revision`——那是在猜 E3。

### 懸而未決：`engine → stroke`

`ffi::InputSample` → `stroke::InputSample` 的轉換需要 `engine` 認識 `stroke`，
但 `§5.1` 的依賴圖上沒有這條邊。

**S0 不解這題**——headless mock 沒有東西消費 samples，S0 只定義 DTO、不寫這條轉換。
處理方式與 `build-infra.md §2` 對 `document → history` 的處理一致：等 E1 真的要連時，
「必須先改 `deps-policy.toml`」正是逼這個決策浮上檯面的機制。

## 3. 方法表

```rust
#[uniffi::export]
impl RustEngine {
    #[uniffi::constructor]
    fn new(pack_path: String, doc_path: Option<String>) -> Result<Arc<RustEngine>, EngineError>;

    fn attach_surface(&self, handle: SurfaceHandle) -> Result<(), EngineError>;
    fn resize_surface(&self, width_px: u32, height_px: u32, scale: f32);
    fn detach_surface(&self);

    fn set_tool(&self, tool: Tool);
    fn pick_color(&self, x: f32, y: f32) -> Rgba;

    fn begin_stroke(&self, s: InputSample);
    fn append_samples(&self, s: Vec<InputSample>);   // ★ 唯一高頻路徑，批次
    fn end_stroke(&self);
    fn cancel_stroke(&self);
    fn tap(&self, x: f32, y: f32);

    fn undo(&self);
    fn redo(&self);

    fn render(&self);
    fn set_viewport(&self, transform: Transform);   // v0：no-op，transform 不保存（見下）

    fn state(&self) -> UiState;
    fn set_state_listener(&self, listener: Option<Arc<dyn StateListener>>);

    fn save(&self) -> Result<(), EngineError>;
    fn export_png(&self) -> Result<Vec<u8>, EngineError>;
    fn export_timelapse(&self) -> Result<Vec<u8>, EngineError>;
}
```

`StateListener` 用 foreign trait（`#[uniffi::export(with_foreign)]`），
不用已被取代的 `callback_interface`。

**簽章 ≠ v0 行為。** `set_viewport` 收下 `Transform` 但 v0 直接丟棄，`Inner` 沒有 viewport
欄位——viewport 要能被消費得先有 render pass（E1）。各方法的 v0 實際行為一律以第 5 節的
行為表為準，不要從簽章推論。

### 對 `§6 Boundary 1` 的三處修正

**1. `Engine::new(surface, …)` 拆成兩段。**
uniffi 傳不了 raw pointer 是表面理由，真正的理由有兩個：`MTKView` 的 layer 在 view
生命週期中會重建，兩段式讓 re-attach 是正常路徑，而不是重建 `RustEngine`——重建 `RustEngine`
等於丟掉 undo stack 與未存檔狀態，那是 bug 不是設計；其次，`new` 因此能在無 GPU 的環境跑，
這是 headless mock 與 CI 單元測試的前提。

**2. `subscribe(Box<dyn StateListener>)` → `set_state_listener(Option<Arc<…>>)`。**
名字誠實反映語意（單一 listener、後設覆蓋前設），`Option` 給了明確的 detach 路徑——
否則 Swift 端的 retain cycle 沒有解。Bridge 層只需要一個 listener，再用 Combine 廣播。

**3. `new` / `attach_surface` / `save` / `export_*` fallible，其餘一律 infallible。**
這條界線是契約的一部分，不能因為 S0 是 mock 就臨時挪動——`render()` 每 frame 呼叫，
Swift 端不會想每 frame `try`。

### 一個標記但不在 S0 解的張力

`§6` 把 `pick_color` 定義成同步回傳 `Rgba`，但同節的實作註記說它走 async readback ring
buffer——兩者不相容。**v0 維持同步簽章**，接受抬筆時約一 frame 的 stall（低頻操作）。
`pick_color` 排在 S1；若屆時實測不可接受，那是一次 major bump，仍在預告的修正窗口內。

### 不進 FFI

`makeCanvasView()` 只存在於 Swift 側——它是 Bridge 包 `MTKView` 並呼叫 `attach_surface`
的地方。`contracts.md` 要明講這個歸屬，否則「逐一對照無缺漏」的驗收會誤判它是缺漏。

## 4. `core/engine` 內部形狀

### Cargo.toml

```toml
[lib]
name = "engine"
crate-type = ["lib", "staticlib", "cdylib"]
```

三種都要：`staticlib` 給 xcframework、`cdylib` 給 library-mode bindgen、`lib` 給
workspace 內的測試。加 `uniffi`、`thiserror` 兩個外部依賴。

**`deps-policy.toml` 不用改**——`uniffi` 不在 `banned-external`，一般外部依賴依
`build-infra.md §3` 不需登記；內部依賴邊也不變。

### 模組佈局

```
core/engine/src/
  lib.rs        uniffi::setup_scaffolding!("colorlull_engine");
  ffi.rs        Record / Enum 型別（§2）
  error.rs      EngineError
  listener.rs   StateListener foreign trait
  engine.rs     pub struct RustEngine { inner: Mutex<Inner> }  ← 只有生命週期與 DTO 轉換
  inner.rs      Inner  ← 狀態機
```

**namespace 必須覆寫。** 預設用 `lib.name`（`engine`），那會產出一個叫 `engine` 的
Swift module，太泛、遲早撞名。

### 狀態住 `app-state`，不住 `engine`

鐵律說 `engine` 不負責任何業務邏輯，`§5.1` 把「目前工具、顏色、筆刷大小、進度、
`UiState` 投影」明確劃給 `app-state`。所以 S0 就給 `core/app-state` 最小骨架：

```rust
pub enum BrushPreset { SoftRound, Marker, Crayon, Airbrush, Watercolor }
pub enum ToolKind { Brush(BrushPreset), Eraser, Bucket }

pub struct AppState {
    pub tool: ToolKind, pub color: [u8; 4], pub size: f32, pub opacity: Option<f32>,
    pub colored_regions: u32, pub total_regions: u32,
    pub can_undo: bool, pub can_redo: bool,
}
```

欄位由已定案的 `UiState` 反推。`engine` 只做 `From<&AppState> for ffi::UiState`。

**`ToolKind::Brush` 必須帶 `BrushPreset`。** 本規格初版的 `AppState` 只有一個裸的
`tool: ToolKind`，但 `ffi::Tool::Brush` 要求 `preset: BrushId`——沒有這個欄位，
投影就得憑空生一個 preset 出來，`From` 根本寫不出來。

反過來 `ToolKind` **不帶** color / size / opacity：那三個在 `AppState` 是跨工具共用的
欄位（切到橡皮擦不該遺失使用者選的顏色），`ffi::Tool` 才把兩者組合起來。

這讓「engine 無業務邏輯」從第一天成立，不必在 E1 搬家；也讓第 2 節的 DTO 模式**在 S0 就被
真的行使一次**——`InputSample` 那條轉換要延到 E1，`UiState` 這條現在就能證明模式站得住。

### 鎖與 emit：靠結構，不靠紀律

`RustEngine` 是 uniffi Object，必須 `Send + Sync`，內部用 `Mutex<Inner>`。
「發送前先釋放鎖」如果靠紀律維持一定會被打破，所以做成唯一的變更入口：

```rust
impl RustEngine {
    fn mutate(&self, f: impl FnOnce(&mut Inner)) {
        let (snapshot, listener) = {
            let mut inner = self.lock();
            f(&mut inner);
            (UiState::from(&inner.app), inner.listener.clone())
        };                                   // ← 鎖在此釋放
        if let Some(l) = listener { l.on_state(snapshot); }
    }
}
```

所有會改狀態的方法都走 `mutate`，沒有第二條路。`lock()` 順手處理 poisoning
（`unwrap_or_else(PoisonError::into_inner)`），不引入 `parking_lot`。

高頻路徑一 frame 一次鎖——`append_samples` 是批次的、`render` 一 frame 一次，非競爭熱點。

**`mutate` 只在投影結果真的改變時才 emit。** 上面的骨架是無條件發送，但
`append_samples` 依 C3 是一 frame 一次，120Hz 下等於每秒 120 次內容完全相同的
`UiState` 回呼——而 stroke 狀態機根本不在 `UiState` 裡。
`Inner` 因此保存 `last_emitted: Option<UiState>`，只有 `!=` 時才送。

這是**結構**而不是紀律：不需要「哪些方法該走 mutate、哪些不該」這種每次都會被打破的判斷，
所有方法照樣走同一條路。代價是 `ffi::UiState` 與其成員都要 `PartialEq`。
語意寫進 `contracts.md` 的 C8，不要讓它變成只存在於程式碼裡的行為。

### `EngineError`

```rust
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum EngineError {
    #[error("尚未實作：{feature}（排程 {milestone}）")]
    NotImplemented { feature: String, milestone: String },
    #[error("資產包載入失敗：{detail}")]
    Pack { detail: String },
    #[error("I/O 失敗：{detail}")]
    Io { detail: String },
}
```

`NotImplemented` 帶 `milestone`，與 `xtask` 既有的 `bail!("尚未實作（S0）")` 慣例一致。
**但它是有期限的**：E3 結束時必須從 enum 移除，寫進 `contracts.md` 當 v1 的 exit
criteria，否則它會安靜地活到上架。

## 5. Headless mock 與測試

### 行為表

| 方法 | S0 行為 |
|---|---|
| `new` | 檢查 `pack_path` 存在即可，不解析 `.colorpack`（M1 才有格式）。`total_regions` 給固定值 |
| `attach_surface` / `resize_surface` / `detach_surface` | 記下 handle，不碰 GPU |
| `set_tool` | 真的寫進 `AppState`，emit |
| `tap` | 推進 `colored_regions`（上限 `total_regions`），emit |
| `begin_stroke` / `append_samples` / `end_stroke` / `cancel_stroke` | 維護 `stroke_active` 狀態機，樣本丟棄 |
| `undo` / `redo` / `render` / `set_viewport` | no-op ＋ 一次性 log |
| `pick_color` | 回傳 `AppState.color`（可預期，且讓 Swift 端能驗證資料流）。不是「目前工具的顏色」——`Eraser` 沒有顏色，那句話對三個工具中的一個沒有定義 |
| `state` | 投影 `AppState` |
| `save` / `export_png` / `export_timelapse` | `Err(NotImplemented)` |

未實作的 **infallible** 方法是 no-op ＋ 一次性 log，不是 panic、不是回傳錯誤。

### 測試清單

全部不需 GPU、不需模擬器，跑在既有的 `ubuntu-latest` CI 上：

1. `set_tool` → `state()` round-trip 一致
2. 設了 listener 後 `set_tool` 觸發一次回呼，內容 == `state()`
3. **listener 回呼中呼叫 `engine.state()` 不死鎖**
4. `set_state_listener(None)` 後不再收到回呼
5. `tap()` 推進 progress 並 emit；到達 `total_regions` 後不再增加
6. 未 `begin_stroke` 就 `append_samples` 是 no-op 不 panic；`cancel` 後 `end` 是 no-op
7. `save` / `export_*` 回傳 `NotImplemented`；`undo` / `render` 不 panic
8. 用相同的 `Tool` 連續 `set_tool` 兩次只觸發一次回呼；`append_samples` 不觸發回呼

第 3 條把 C2 條款（第 7 節）的重入語意變成可執行的驗證，第 8 條對 C8 做同一件事。

**這是選 headless mock 的主要紅利**：S0 的契約正確性不依賴「xcframework 有沒有接通」
這件高風險的事。`S0.md` 的風險註記說 uniffi 若吃掉 3 天以上就要切軌——切軌後這 8 條
測試仍然綠燈，契約仍然被驗證過。

## 6. `xtask ios` 與 `verify-generated`

### 先解掉文件裡的一個矛盾

`§7` 說「重新生成 binding 後，若 `apps/ios/Generated/` 出現 diff 則 CI 失敗」，
但 `§12.2` 同時說 `Generated/` 是 gitignore 的。**沒有基準就沒有 diff 可比**，
這個 gate 照字面實作是空的。

解法：**指紋檔進 git，產物不進**。

```toml
# core/engine/ffi-lock.toml —— 由 cargo xtask ios 產生
uniffi = "0.29.5"
swift_sources_sha256 = "…"     # 見下方「hash 涵蓋什麼」
```

**hash 只涵蓋 bindgen 的文字產物**：`colorlull_engine.swift`、`colorlull_engineFFI.h`、
`module.modulemap`，依相對路徑排序後把「路徑 ＋ 內容」一起餵進 SHA-256。

初版寫的是「生成目錄下所有檔案」——那是錯的。`apps/ios/Generated/` 底下還有
`ColorlullEngine.xcframework/`，裡面是編譯產物：不可重現，而且 Linux 上根本產不出來。
照字面實作的話 `verify-generated` 永遠不可能通過。

放 `core/engine/` 而非 `contracts/`——`§7` 說 `contracts/` 只放 uniffi 生不出來的東西，
而這是契約的**指紋**不是契約本身；放在 FFI 定義旁邊，改 FFI 的 commit 自然帶著它，
且既有的 `core/**` CI 觸發路徑已經涵蓋。

這順帶讓 `§7` 的 semver 規則第一次有執行力：`ffi-lock.toml` 的 diff 就是「FFI 表面變了」
的可見信號，逼你在同一個 PR 裡確認 `docs/contracts.md` 有沒有跟上。

### 兩個 gate，切在成本線上

macOS runner 的分鐘數是 Linux 的十倍，而這是零預算的單人專案：

| Gate | 在哪跑 | 驗什麼 |
|---|---|---|
| `cargo xtask verify-generated` | **Linux**（既有 CI，零額外成本） | 重新生成 Swift binding，比對 `ffi-lock.toml` |
| xcframework 建置 | macOS，只在 `core/engine/**`／`apps/ios/**` 變更時 | 能不能真的編出來、連得起來 |

成立的關鍵前提：**uniffi 的 metadata 與 target 無關**——library-mode bindgen 讀的是編譯進
二進位的 metadata，用 host target 的 `cdylib` 就能生成 Swift binding，只有打包
`.xcframework` 需要 Apple toolchain。

> ✅ **已驗證**（uniffi 0.29.5，2026-08-03）。拿同一份 crate 分別編出
> host `libengine.dylib`、`aarch64-apple-ios/libengine.a`、`aarch64-apple-ios-sim/libengine.a`，
> 三者跑 bindgen 的產物**逐位元相同**。靜態庫也在支援範圍內
> （`macro_metadata::extract_from_archive`）。切到 macOS job 的退路用不到了。

**但兩個 gate 一律從 host cdylib 生成。** 上面證明了「可以」，這裡選擇「總是」：
`xtask ios` 也用 host cdylib 產 Swift binding，iOS 的兩個 `.a` 只餵給
`xcodebuild -create-xcframework`。這讓 `verify-generated` 從「比對兩條路徑的產物是否碰巧相同」
變成「同一條路徑跑兩次」——未來 uniffi 改了行為也不會變成 CI 上的謎題。
成本是 `xtask ios` 多編一次 host 庫，而 `cargo test` 本來就要編。

### `cargo xtask ios` 的步驟

```
1. cargo build -p colorlull-engine --release                              (host cdylib)
2. cargo build -p colorlull-engine --release --target aarch64-apple-ios
3. cargo build -p colorlull-engine --release --target aarch64-apple-ios-sim
4. generate_swift_bindings(host cdylib) → Generated/{Sources,Headers}/
5. xcodebuild -create-xcframework → apps/ios/Generated/ColorlullEngine.xcframework
6. 重算 hash → 寫回 core/engine/ffi-lock.toml
```

四個決定：

- **只做 arm64 device ＋ arm64 simulator，不做 x86_64 模擬器。** 建置時間減半，
  代價是 Intel Mac 上無法跑模擬器——開發機是 Apple Silicon，代價為零；
  真需要時加一個 target ＋ 一次 `lipo` 就補回來。
- **當 library 連進 `xtask`，不 shell out。** 但**不是** `uniffi-bindgen-swift`——
  那不是一個 library crate。`uniffi` 開 `cli` feature 只給得到
  `uniffi::uniffi_bindgen_swift()`，它自己解析 `argv` 又在出錯時 `process::exit(1)`，
  在 `xtask` 裡不能用。真正的 API 在 **`uniffi_bindgen`** crate：
  `uniffi_bindgen::bindings::generate_swift_bindings(SwiftBindingsOptions) -> Result<()>`。
- **`uniffi` 與 `uniffi_bindgen` 兩個都在 `[workspace.dependencies]` 用 `=` 硬 pin**
  在同一版本。生成端與 scaffolding 端版本不一致會產出能編譯但行為錯誤的 binding；
  只 pin 其中一個等於沒 pin。
- **`module_name` 必須明確傳 `"colorlull_engineFFI"`。** ★ 見下。

### `module_name` 不傳會編不起來

namespace 覆寫只管到 Swift 原始碼與 header 的檔名。**modulemap 的 module 名是另一條路徑
決定的**——`generate_swift_bindings` 從 `library_path` 的檔名推導（去掉 `lib` 前綴與副檔名），
於是 `lib.name = "engine"` 會產出：

```
framework module engine { header "colorlull_engineFFI.h" ... }
```

而生成的 Swift 裡寫的是 `import colorlull_engineFFI`。兩者對不上，
且因為那行包在 `#if canImport(...)` 裡，**import 會靜默失效**，
一路到 link 階段才炸成 undefined symbol。

所以必須傳 `module_name = Some("colorlull_engineFFI")`，
規則是 **`<namespace>FFI`，一個字元都不能差**。
（`modulemap_filename` 一併固定成 `module.modulemap`，`-create-xcframework` 要的就是這個名字。）

`.xcframework` 的**檔名**仍是 `ColorlullEngine.xcframework`——那只是檔名，與 module 名無關。

### Apple targets 寫進 `rust-toolchain.toml`

```toml
targets = ["aarch64-apple-ios", "aarch64-apple-ios-sim"]
```

代價是 Linux CI 也會裝兩份用不到的 std（幾十 MB）。換來本機少一個「沒寫在任何地方的
前置步驟」——`build-infra.md §6` 已把 toolchain 的 SSOT 定在這個檔案。

### 產物佈局

```
apps/ios/Generated/               ← gitignore（既有規則已涵蓋）
  ColorlullEngine.xcframework/    ← ios-arm64/ ＋ ios-arm64-simulator/，各含 libengine.a
  Headers/colorlull_engineFFI.h ＋ module.modulemap
  Sources/colorlull_engine.swift
```

`EngineBridge` framework target 引用這裡；App Shell 不直接引用。

### 兩個只有編譯器抓得到的坑（S0 已解，記錄理由）

兩件事的共同點：**`cargo xtask ios` 都會 exit 0**，錯誤只在 Swift 端才浮現，
而 CI 的 macOS gate 只建 xcframework、不編 Swift。所以兩件都必須寫下來。

**1. `SwiftBindingsOptions::xcframework` 必須是 `false`。**

那個旗標唯一的作用是在 modulemap 前面加上 `framework` 關鍵字，而 `framework module`
只有在**真的以 framework 佈局**被消費時才成立——我們用 `-create-xcframework -library`
產的是 library slice，不是 framework。

實測（拿生成的 `colorlull_engine.swift` 跑 `swiftc -typecheck`）：

| modulemap | 結果 |
|---|---|
| `framework module colorlull_engineFFI` | **220 errors**（型別全找不到） |
| `module colorlull_engineFFI` | **0 errors** |

`import` 那行包在 `#if canImport(...)` 裡，所以對不上時不會報「找不到 module」，
而是整批型別憑空消失——這就是它 exit 0 卻編不起來的原因。

**2. Rust 的 Object 叫 `RustEngine`，不叫 `Engine`。**

uniffi 會生一個 `<ObjectName>Protocol`。叫 `Engine` 就生出 `EngineProtocol`——
與 `S0.md` 要求手寫的 `apps/ios/EngineBridge/EngineProtocol.swift` **同名**，
而且兩者會被編進同一個 module（生成的 `.swift` 是 `EngineBridge` target 的一員）。
那不是命名曖昧，是 invalid redeclaration。

uniffi 的 Object **沒有 rename 機制**（`ObjectItem::name()` 直接取 Rust ident），
所以唯一的解是改 Rust 這邊。改成 `RustEngine` 後生成 `RustEngine` ＋ `RustEngineProtocol`，
正好就是 `§10.1` 與 `S0.md` 一直在講的那個具體類別（「把 `MockEngine` 換成 `RustEngine`」
「App Shell 端沒有任何一行直接引用 `RustEngine`」）——文件裡的名字第一次和生成物對上了。

## 7. `docs/contracts.md` 首版

**它不是 FFI 的真相。** 真相是 `core/engine` 的 Rust proc-macro 標註——`§7` 的核心原則
不允許第二份。`contracts.md` 只放三種**程式碼表達不出來**的東西：語意條款、semver 判定、
遷移記錄。

目標 100 行上下，六節：

**① SSOT 宣告與 v0 標記** — 一段話，含兩次預告的修正窗口（E2、E3）。

**② FFI 表面速查表** — `方法 | fallible | 實際實作於 | v0 狀態`。
這張表就是 S0 驗收「`EngineProtocol.swift` 與 Rust FFI 表面逐一對照無缺漏」的對照基準；
沒有它，那條驗收只能靠肉眼比對兩個檔案。

**③ 語意條款**

| | 條款 |
|---|---|
| C1 | listener 在呼叫端 thread 同步觸發；hop 到 main queue 由 Bridge 負責 |
| C2 | 發送 `UiState` 前必先釋放內部鎖（重入安全）— 已有測試守 |
| C3 | `append_samples` 一 frame 一次；**不得出現 per-touch 呼叫**（Boundary 1 紅線 2） |
| C4 | `predicted: true` 的樣本只影響當前 frame，不進 oplog |
| C5 | `RustEngine` 生命週期長於 surface；`attach`／`detach` 是正常路徑，重建 `RustEngine` 等於丟失狀態 |
| C6 | `pick_color` 同步回傳，接受抬筆時約一 frame 的 stall（S1 實作時複審） |
| C7 | `makeCanvasView()` 屬 Bridge，不在 FFI——對照表上不算缺漏 |
| C8 | `UiState` 回呼只在投影結果**真的改變**時發送；連續兩次相同狀態只會收到一次 |

C1／C2／C5 是「生成的 Swift 簽章完全看不出來、但 E1 一定會踩」的那三條。

**④ semver 規則** — 引用 `§7` 的表，不複製。補一句執行機制：`ffi-lock.toml` 的 diff
是 FFI 變更的可見信號。

**⑤ 遷移記錄格式**

```markdown
## v0 → v1（E2，YYYY-MM-DD）
**變更**：<哪個方法或欄位>
**分類**：major｜minor（依 §7 規則：<哪一條>）
**Rust**：<改了什麼>
**Swift Bridge**：<要跟著改什麼>
**驗證**：<怎麼確認兩端一致>
```

首版只有格式與一個空的 `## v0（S0，YYYY-MM-DD）初版` 條目。

**⑥ 有期限的東西** — `NotImplemented` 必須在 E3 結束時移除。

## 8. 回寫清單

S0 收尾時必須完成，對應驗收「本里程碑期間改變的決策已寫回文件」：

| 檔案 | 改什麼 |
|---|---|
| `roadmap/S0.md` | 「uniffi UDL」→「uniffi proc-macro」（產出物 ＋ 實作清單兩處） |
| `architecture.md §12.1` | 同上措辭 |
| `architecture.md §6` | 三處修正（第 3 節）；加註「當前實際簽章見 `docs/contracts.md`；不一致時以 `core/engine` 為準」；`pick_color` 的同步／async 張力 |
| `architecture.md §7` | CI 守門：`Generated/` diff → `ffi-lock.toml` 指紋比對 |
| `architecture.md §10.1` | Swift protocol 對齊新簽章（歸 iOS 那份 spec，此處只標記相依） |
| `specs/build-infra.md §4` | xtask 指令表：`ios`／`verify-generated` 狀態改為已實作 |
| `specs/build-infra.md §6` | `rust-toolchain.toml` 加 `targets` |
| `specs/build-infra.md §2` | `engine → stroke` 的懸而未決，與 `document → history` 並列 |
| `docs/README.md` | 文件地圖加一列本規格 |
| `CLAUDE.md` | 「當前」推進到 S0 |
| `CHANGELOG.md` | 本里程碑條目 |

## 不做

- Xcode 專案、`EngineProtocol.swift`、`MockEngine.swift`、五條路由空殼（另一份 spec）
- Kotlin binding（v1 不做，`§12.1`）
- `ffi::InputSample` → `stroke::InputSample` 的轉換（E1）
- x86_64 模擬器 target
- 「改 FFI 但沒改 `contracts.md` 就 CI 失敗」的檢查——那需要語意判斷，
  做成機械規則只會逼出無意義的樣板條目。`ffi-lock.toml` 的 diff 已提供信號
- `contracts/` 底下的任何 schema（`colorpack` 在 M1、`oplog` 在 E3）
