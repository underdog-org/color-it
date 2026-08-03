//! 資產烘焙 CLI。clap 薄殼，管線在 `baker::bake`。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "baker",
    about = "Colorlull 資產烘焙（docs/specs/baker-core-design.md）"
)]
struct Cli {
    /// 素材來源目錄（資料夾名即 id）
    src_dir: PathBuf,
    /// 輸出目錄。預設 assets/packs/（gitignore，走 R2）
    #[arg(long, default_value = "assets/packs")]
    out: PathBuf,
    /// 額外把報告寫成 JSON
    #[arg(long, value_name = "PATH")]
    report: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let opts = baker::BakeOptions {
        out_dir: cli.out,
        report_json: cli.report,
    };
    match baker::bake(&cli.src_dir, &opts) {
        Ok(report) => {
            print!("{}", report.to_text());
            if report.has_error() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        // exit 2 保留給 baker 自身故障（§4.2）。
        Err(e) => {
            eprintln!("baker 故障：{e:#}");
            ExitCode::from(2)
        }
    }
}
