//! Colorlull 的建置協調。指令清單見 `docs/architecture.md §12.1`。

mod metadata;
mod policy;

use std::path::PathBuf;

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
    /// 執行 baker
    Bake {
        /// 素材來源目錄
        dir: PathBuf,
    },
    /// 產生 .xcframework ＋ Swift binding → apps/ios/Generated/
    Ios,
    /// CI gate：檢查 uniffi binding 是否為最新
    VerifyGenerated,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::LintDeps => lint_deps(),
        Command::Bake { .. } => {
            bail!("cargo xtask bake 尚未實作（M1）。流程見 docs/architecture.md §9.2")
        }
        Command::Ios => bail!("cargo xtask ios 尚未實作（S0）。見 docs/architecture.md §12.1"),
        Command::VerifyGenerated => {
            bail!("cargo xtask verify-generated 尚未實作（S0）。見 docs/architecture.md §12.1")
        }
    }
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
