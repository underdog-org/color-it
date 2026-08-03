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
    /// 過渡用：從舊契約的 reference.png 反推 seeds.png
    ///
    /// 繪師依 baker-seeds.md §2 重交 seeds.png 之後，這個指令與 baker::migrate
    /// 都該刪掉。
    SeedsFromReference {
        /// 素材來源目錄（要有 lineart.png 與 reference.png）
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
        Command::LintIos => lint_ios::run(&metadata::load()?.root),
        Command::GenTorture => gen_torture(),
        Command::Bake { dir, out, report } => bake(&dir, out, report),
        Command::SeedsFromReference { dir } => seeds_from_reference(&dir),
        Command::Ios => ios::run(&metadata::load()?),
        Command::VerifyGenerated => ios::verify_generated(&metadata::load()?),
    }
}

fn gen_torture() -> Result<()> {
    let dir = baker::synth::write_torture(&metadata::load()?.root)?;
    println!("gen-torture：→ {}", dir.display());
    Ok(())
}

fn seeds_from_reference(dir: &Path) -> Result<()> {
    use baker::image::{Image, PngOptions};

    let lineart = Image::load(&dir.join("lineart.png"))?;
    let reference = Image::load(&dir.join("reference.png"))?;
    if (lineart.width, lineart.height) != (reference.width, reference.height) {
        bail!(
            "lineart {}×{} 與 reference {}×{} 尺寸不一致",
            lineart.width,
            lineart.height,
            reference.width,
            reference.height
        );
    }

    let p = baker::Params::default();
    let derived = baker::migrate::seeds_from_reference(
        &lineart.rgba,
        &reference.rgba,
        lineart.width,
        lineart.height,
        p.line_threshold,
        p.min_orphan_area,
    );

    let path = dir.join("seeds.png");
    let bytes = baker::image::encode_rgba(
        &derived.seeds,
        lineart.width,
        lineart.height,
        PngOptions {
            srgb: true,
            icc: None,
            compression: png::Compression::High,
        },
    )?;
    std::fs::write(&path, bytes).with_context(|| format!("寫入 {} 失敗", path.display()))?;
    println!(
        "seeds-from-reference：{} 個封閉區點了色標，{} 個碎片跳過 → {}",
        derived.seeded,
        derived.skipped,
        path.display()
    );
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
