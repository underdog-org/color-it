//! `cargo xtask ios` 與 `cargo xtask verify-generated`。規格見 `docs/specs/ffi-contract.md §6`。
//!
//! 兩個指令共用 [`generate`]：`verify-generated` 不是「另一條路徑的產物碰巧相同」，
//! 而是**同一條路徑跑第二次**。uniffi 未來改了行為時，這會直接反映成 hash 變動，
//! 而不是變成 CI 上的謎題。代價只是 `xtask ios` 多編一次 host 庫。

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::metadata::Workspace;

const PACKAGE: &str = "colorlull-engine";
/// `[lib] name`，決定產物檔名 `libengine.{a,dylib,so}`。
const LIB_NAME: &str = "engine";
/// uniffi namespace，即 `setup_scaffolding!("colorlull_engine")` 的參數。
const NAMESPACE: &str = "colorlull_engine";

/// modulemap 的 module 名，規則是 `<namespace>FFI`，一個字元都不能差。
const MODULE_NAME: &str = "colorlull_engineFFI";

/// `-create-xcframework` 認的就是這個檔名，不能用 bindgen 預設的 `engine.modulemap`。
const MODULEMAP_FILE: &str = "module.modulemap";

/// 只做 arm64 device ＋ arm64 simulator。不做 x86_64 模擬器——開發機是 Apple Silicon。
const IOS_TARGETS: [&str; 2] = ["aarch64-apple-ios", "aarch64-apple-ios-sim"];

const LOCK_FILE: &str = "core/engine/ffi-lock.toml";
const GENERATED_DIR: &str = "apps/ios/Generated";
const XCFRAMEWORK_NAME: &str = "ColorlullEngine.xcframework";

/// `core/engine/ffi-lock.toml`：**指紋進 git，產物不進**。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FfiLock {
    uniffi: String,
    swift_sources_sha256: String,
}

/// `cargo xtask ios`：產出 `.xcframework` ＋ Swift binding，並回寫指紋。
pub fn run(workspace: &Workspace) -> Result<()> {
    let root = utf8(&workspace.root)?;
    let target_dir = utf8(&workspace.target_dir)?;

    let staging = target_dir.join("xtask/ios-bindings");
    let bindings = generate(&root, &target_dir, &staging)?;

    for target in IOS_TARGETS {
        cargo_build(&root, &["--target", target])?;
    }

    let generated = root.join(GENERATED_DIR);
    let headers = generated.join("Headers");
    let sources = generated.join("Sources");
    for dir in [&headers, &sources] {
        fs::create_dir_all(dir).with_context(|| format!("建立 {dir} 失敗"))?;
    }
    copy(&staging.join(swift_file()), &sources.join(swift_file()))?;
    copy(&staging.join(header_file()), &headers.join(header_file()))?;
    copy(&staging.join(MODULEMAP_FILE), &headers.join(MODULEMAP_FILE))?;

    create_xcframework(
        &root,
        &target_dir,
        &headers,
        &generated.join(XCFRAMEWORK_NAME),
    )?;

    let uniffi = uniffi_version(&root)?;
    let lock_path = root.join(LOCK_FILE);
    write_lock(&lock_path, &uniffi, &bindings)?;

    println!("ios：{generated} 已更新（uniffi {uniffi}）");
    println!("ios：{LOCK_FILE} = {bindings}");
    Ok(())
}

/// `cargo xtask verify-generated`：CI gate，跑在 Linux。
pub fn verify_generated(workspace: &Workspace) -> Result<()> {
    let root = utf8(&workspace.root)?;
    let target_dir = utf8(&workspace.target_dir)?;

    let lock_path = root.join(LOCK_FILE);
    let source = fs::read_to_string(&lock_path).with_context(|| {
        format!("讀取 {LOCK_FILE} 失敗；這個檔由 `cargo xtask ios` 產生，應該進 git")
    })?;
    let lock: FfiLock = toml::from_str(&source).with_context(|| {
        format!("解析 {LOCK_FILE} 失敗（格式見 docs/specs/ffi-contract.md §6）")
    })?;

    let uniffi = uniffi_version(&root)?;
    if lock.uniffi != uniffi {
        bail!(
            "verify-generated：{LOCK_FILE} 記錄 uniffi {}，實際編進去的是 {uniffi}；\
             請重跑 `cargo xtask ios` 並確認 docs/contracts.md 有跟上",
            lock.uniffi
        );
    }

    let actual = generate(
        &root,
        &target_dir,
        &target_dir.join("xtask/verify-bindings"),
    )?;
    if actual != lock.swift_sources_sha256 {
        bail!(
            "verify-generated：Swift binding 與 {LOCK_FILE} 不符\n  \
             記錄：{}\n  實際：{actual}\n\
             FFI 表面變了。請跑 `cargo xtask ios` 並把 {LOCK_FILE} 一起 commit",
            lock.swift_sources_sha256
        );
    }

    println!("verify-generated：Swift binding 與 {LOCK_FILE} 相符（uniffi {uniffi}）");
    Ok(())
}

/// 編 host cdylib、生成三個文字產物到 `out_dir`，回傳指紋。
fn generate(root: &Utf8Path, target_dir: &Utf8Path, out_dir: &Utf8Path) -> Result<String> {
    cargo_build(root, &[])?;

    let library = target_dir.join("release").join(host_cdylib()?);
    if !library.exists() {
        bail!(
            "找不到 host cdylib {library}；請確認 core/engine/Cargo.toml 仍宣告 crate-type = cdylib"
        );
    }

    // 殘留檔會讓 hash 反映上一次的產物，而不是這一次的。
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).with_context(|| format!("清除 {out_dir} 失敗"))?;
    }

    std::env::set_current_dir(root).with_context(|| format!("切換工作目錄到 {root} 失敗"))?;

    uniffi_bindgen::bindings::generate_swift_bindings(
        uniffi_bindgen::bindings::SwiftBindingsOptions {
            generate_swift_sources: true,
            generate_headers: true,
            generate_modulemap: true,
            library_path: library.clone(),
            out_dir: out_dir.to_owned(),
            xcframework: false, // 必須用 false
            module_name: Some(MODULE_NAME.to_string()),
            modulemap_filename: Some(MODULEMAP_FILE.to_string()),
            metadata_no_deps: false,
            link_frameworks: vec![],
        },
    )
    .with_context(|| format!("從 {library} 生成 Swift binding 失敗"))?;

    fingerprint(out_dir)
}

/// bindgen 三個文字產物的 SHA-256。
///
/// **只涵蓋文字產物**，刻意不含 `.xcframework`：那是編譯產物，不可重現，
/// 而且 Linux 上根本產不出來——照「生成目錄下所有檔案」實作的話，
/// `verify-generated` 永遠不可能通過。
fn fingerprint(dir: &Utf8Path) -> Result<String> {
    let mut names = [swift_file(), header_file(), MODULEMAP_FILE.to_string()];
    names.sort(); // 依相對路徑排序，讓 hash 與檔案系統的列舉順序無關

    let mut hasher = Sha256::new();
    for name in &names {
        let path = dir.join(name);
        let content = fs::read(&path)
            .with_context(|| format!("讀取 bindgen 產物 {path} 失敗（bindgen 沒產出這個檔？）"))?;
        // 路徑與內容都進 hash，且用 NUL 分隔：否則「檔名尾巴搬到內容開頭」不會改變 hash。
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(&content);
        hasher.update([0]);
    }

    Ok(hasher
        .finalize()
        .iter()
        .fold(String::new(), |mut acc, byte| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{byte:02x}");
            acc
        }))
}

fn swift_file() -> String {
    format!("{NAMESPACE}.swift")
}

fn header_file() -> String {
    format!("{NAMESPACE}FFI.h")
}

/// host cdylib 的檔名。`xtask ios` 跑在 macOS、`verify-generated` 跑在 Linux，
/// 同一段程式要在兩邊都定位得到同一個 library。
fn host_cdylib() -> Result<String> {
    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "linux") {
        "so"
    } else {
        bail!("不支援的 host 平台：library-mode bindgen 需要 cdylib，目前只驗過 macOS 與 Linux");
    };
    Ok(format!("lib{LIB_NAME}.{ext}"))
}

// ---------------------------------------------------------------------------
// 外部工具
// ---------------------------------------------------------------------------
fn cargo_build(root: &Utf8Path, extra: &[&str]) -> Result<()> {
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["build", "-p", PACKAGE, "--release"])
        .args(extra)
        .status()
        .context("執行 cargo build 失敗")?;

    if !status.success() {
        bail!(
            "cargo build -p {PACKAGE} --release {} 回傳非零",
            extra.join(" ")
        );
    }
    Ok(())
}

fn create_xcframework(
    root: &Utf8Path,
    target_dir: &Utf8Path,
    headers: &Utf8Path,
    output: &Utf8Path,
) -> Result<()> {
    // xcodebuild 遇到已存在的輸出會直接失敗，不會覆蓋。
    if output.exists() {
        fs::remove_dir_all(output).with_context(|| format!("清除既有的 {output} 失敗"))?;
    }

    let mut command = Command::new("xcodebuild");
    command.current_dir(root).arg("-create-xcframework");
    for target in IOS_TARGETS {
        let library = target_dir
            .join(target)
            .join("release")
            .join(format!("lib{LIB_NAME}.a"));
        if !library.exists() {
            bail!("找不到 {library}；`cargo build --target {target}` 應該產出 staticlib");
        }
        command.args(["-library", library.as_str(), "-headers", headers.as_str()]);
    }
    command.args(["-output", output.as_str()]);

    let status = command
        .status()
        .context("執行 xcodebuild 失敗（需要 Xcode command line tools）")?;
    if !status.success() {
        bail!("xcodebuild -create-xcframework 回傳非零");
    }
    Ok(())
}

#[derive(Deserialize)]
struct CargoLock {
    #[serde(default)]
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
}

fn uniffi_version(root: &Utf8Path) -> Result<String> {
    let path = root.join("Cargo.lock");
    let source = fs::read_to_string(&path).with_context(|| format!("讀取 {path} 失敗"))?;
    let lock: CargoLock = toml::from_str(&source).with_context(|| format!("解析 {path} 失敗"))?;

    let find = |name: &str| {
        lock.package
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.version.clone())
            .with_context(|| format!("Cargo.lock 沒有 {name}——core/engine 應該依賴它"))
    };

    let uniffi = find("uniffi")?;
    let bindgen = find("uniffi_bindgen")?;
    if uniffi != bindgen {
        bail!(
            "uniffi {uniffi} 與 uniffi_bindgen {bindgen} 版本不一致；\
             兩者都要在 [workspace.dependencies] 用 `=` 硬 pin 在同一版"
        );
    }
    Ok(uniffi)
}

fn write_lock(path: &Utf8Path, uniffi: &str, sha256: &str) -> Result<()> {
    let content = format!(
        "# 由 `cargo xtask ios` 產生，請勿手改。見 docs/specs/ffi-contract.md §6。\n\
         # 產物是 gitignore 的，這個檔是它唯一的基準：hash 涵蓋 bindgen 的三個文字產物\n\
         # （.swift / FFI.h / module.modulemap），不含 .xcframework。\n\
         # 這裡的 diff 即「FFI 表面變了」，請順手確認 docs/contracts.md 有跟上。\n\
         uniffi = \"{uniffi}\"\n\
         swift_sources_sha256 = \"{sha256}\"\n"
    );
    fs::write(path, content).with_context(|| format!("寫入 {path} 失敗"))
}

/// uniffi_bindgen 的 API 收 camino 路徑，而 `cargo metadata` 給的是 [`std::path::PathBuf`]。
fn utf8(path: &Path) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path.to_owned())
        .map_err(|p| anyhow::anyhow!("路徑不是合法的 UTF-8：{}", p.display()))
}

fn copy(from: &Utf8Path, to: &Utf8Path) -> Result<()> {
    fs::copy(from, to)
        .with_context(|| format!("複製 {from} → {to} 失敗"))
        .map(|_| ())
}
