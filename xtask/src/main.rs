//! Colorlull 的建置協調。指令清單見 `docs/architecture.md §12.1`。

mod ios;
mod lint_ios;
mod metadata;
mod policy;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

const POLICY_FILE: &str = "xtask/deps-policy.toml";

#[derive(Parser)]
#[command(name = "xtask", about = "Colorlull 建置協調")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 檢查 crate 依賴是否符合 xtask/deps-policy.toml
    LintDeps,
    /// 檢查 App Shell 沒有直接引用 RustEngine（跑在 Linux，不需要 Xcode）
    LintIos,
    /// 決定性產生 torture test 素材 → assets/source/torture-01/
    GenTorture,
    /// 執行 baker
    Bake {
        /// 素材來源目錄
        dir: PathBuf,
        /// 輸出目錄。預設 <repo>/assets/packs
        #[arg(long)]
        out: Option<PathBuf>,
        /// 額外把報告寫成 JSON
        #[arg(long, value_name = "PATH")]
        report: Option<PathBuf>,
        /// 產出退件附件到指定目錄（baker-seeds.md §5）
        #[arg(long = "debug-out", value_name = "DIR")]
        debug_out: Option<PathBuf>,
    },
    /// 產生 .xcframework ＋ Swift binding → apps/ios/Generated/，並附帶 dev-pack
    Ios,
    /// bake 一顆 pack 進 iOS bundle，讓 `-engine rust` 有東西可讀
    DevPack {
        /// 素材來源目錄。預設 assets/source/kirby-demo-1
        dir: Option<PathBuf>,
    },
    /// CI gate：檢查 uniffi binding 是否為最新
    VerifyGenerated,
}

/// `cargo xtask dev-pack` 的預設素材。挑 kirby 是因為它 1:1、有 shade、57 區——
/// 手感測試要的就是「區夠多、夠雜」。
const DEV_PACK_SOURCE: &str = "assets/source/kirby-demo-1";

/// bundle 裡的固定檔名。**gitignore**——`assets/packs/` 不進 git
/// （`architecture.md §12.2`），複製進 app 的這顆沒有理由破例。
const DEV_PACK_DEST: &str = "apps/ios/ColorApp/Resources/dev.colorpack";

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::LintDeps => lint_deps(),
        Command::LintIos => lint_ios::run(&metadata::load()?.root),
        Command::GenTorture => gen_torture(),
        Command::Bake {
            dir,
            out,
            report,
            debug_out,
        } => bake(&dir, out, report, debug_out),
        // dev-pack 綁在 `ios` 裡而不是獨立一步：忘記跑它的懲罰是 App 靜默退回
        // `MockEngine`，而那個症狀（「畫得動但沒有線稿」）要花很久才會被認出來。
        Command::Ios => {
            let workspace = metadata::load()?;
            ios::run(&workspace)?;
            dev_pack(&workspace.root, None)
        }
        Command::DevPack { dir } => dev_pack(&metadata::load()?.root, dir),
        Command::VerifyGenerated => ios::verify_generated(&metadata::load()?),
    }
}

/// bake 素材並複製成 `apps/ios/ColorApp/Resources/dev.colorpack`。
///
/// Xcode 16 的 synchronized group 會自動把它收進 bundle，所以除了「檔案存在」
/// 之外不需要動 `project.pbxproj`。
fn dev_pack(root: &Path, dir: Option<PathBuf>) -> Result<()> {
    let source = dir.unwrap_or_else(|| root.join(DEV_PACK_SOURCE));
    let out_dir = root.join("assets/packs");
    let report = baker::bake(
        &source,
        &baker::BakeOptions {
            out_dir: out_dir.clone(),
            report_json: None,
            params: baker::Params::default(),
            debug_out: None,
        },
    )?;
    if report.has_error() {
        print!("{}", report.to_text());
        bail!("dev-pack：{} 未通過驗證", report.id);
    }

    let dest = root.join(DEV_PACK_DEST);
    let parent = dest
        .parent()
        .context("DEV_PACK_DEST 沒有上層目錄，這是常數寫錯")?;
    std::fs::create_dir_all(parent).with_context(|| format!("建立 {} 失敗", parent.display()))?;
    let baked = out_dir.join(format!("{}.colorpack", report.id));
    std::fs::copy(&baked, &dest)
        .with_context(|| format!("複製 {} → {} 失敗", baked.display(), dest.display()))?;

    println!("dev-pack：{} → {DEV_PACK_DEST}", report.id);
    Ok(())
}

fn gen_torture() -> Result<()> {
    let dir = baker::synth::write_torture(&metadata::load()?.root)?;
    println!("gen-torture：→ {}", dir.display());
    Ok(())
}

fn bake(
    dir: &Path,
    out: Option<PathBuf>,
    report_json: Option<PathBuf>,
    debug_out: Option<PathBuf>,
) -> Result<()> {
    let root = metadata::load()?.root;
    let opts = baker::BakeOptions {
        out_dir: out.unwrap_or_else(|| root.join("assets/packs")),
        report_json,
        // `cargo xtask bake` 一律走契約預設值；要調參數請直接跑 baker --set。
        params: baker::Params::default(),
        debug_out,
    };
    let report = baker::bake(dir, &opts)?;
    print!("{}", report.to_text());
    if report.has_error() {
        bail!("bake：{} 未通過驗證", report.id);
    }
    Ok(())
}

fn lint_deps() -> Result<()> {
    let workspace = metadata::load()?;

    let path = workspace.root.join(POLICY_FILE);
    let source =
        std::fs::read_to_string(&path).with_context(|| format!("讀取 {} 失敗", path.display()))?;
    let policy: policy::Policy =
        toml::from_str(&source).with_context(|| format!("解析 {} 失敗", path.display()))?;

    let violations = policy::check(&policy, &workspace.manifests);

    if violations.is_empty() {
        println!(
            "lint-deps：{} 個 crate 全數符合 {POLICY_FILE}",
            workspace.manifests.len()
        );
        return Ok(());
    }

    for violation in &violations {
        eprintln!("  ✗ {violation}");
    }
    bail!(
        "lint-deps：{} 項依賴違規（規則見 docs/specs/build-infra.md §3）",
        violations.len()
    )
}
