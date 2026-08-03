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
    },
    /// 產生 .xcframework ＋ Swift binding → apps/ios/Generated/
    Ios,
    /// CI gate：檢查 uniffi binding 是否為最新
    VerifyGenerated,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::LintDeps => lint_deps(),
        Command::LintIos => lint_ios::run(&metadata::load()?.root),
        Command::GenTorture => gen_torture(),
        Command::Bake { dir, out, report } => bake(&dir, out, report),
        Command::Ios => ios::run(&metadata::load()?),
        Command::VerifyGenerated => ios::verify_generated(&metadata::load()?),
    }
}

fn gen_torture() -> Result<()> {
    let dir = baker::synth::write_torture(&metadata::load()?.root)?;
    println!("gen-torture：→ {}", dir.display());
    Ok(())
}

fn bake(dir: &Path, out: Option<PathBuf>, report_json: Option<PathBuf>) -> Result<()> {
    let root = metadata::load()?.root;
    let opts = baker::BakeOptions {
        out_dir: out.unwrap_or_else(|| root.join("assets/packs")),
        report_json,
        // `cargo xtask bake` 一律走契約預設值；要調參數請直接跑 baker --set。
        params: baker::Params::default(),
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
