//! `cargo xtask lint-ios`：把 S0 的一條驗收從人工目視變成機械檢查。
//!
//! 驗收原文是「**App Shell 端沒有任何一行直接引用 `RustEngine`**——只依賴 `EngineProtocol`」
//! （`roadmap/S0.md`）。這件事沒有型別系統守得住：Shell 與 Bridge 在同一個專案裡，
//! `import EngineBridge` 就看得到 `RustEngineAdapter`，編譯器不會有意見。
//!
//! 所以用最笨的方式——純文字掃描。它跑在 Linux job 上零成本，也不需要 Xcode。

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// 只轄 App Shell。`EngineBridge/` 當然可以提到 `RustEngine`，那是它的工作。
const SHELL_DIR: &str = "apps/ios/ColorApp";

/// 禁詞與「為什麼」。訊息要能直接告訴人下一步做什麼，不然機械檢查只是換個地方卡住。
const FORBIDDEN: [(&str, &str); 2] = [
    (
        "RustEngine",
        "Shell 只能依賴 EngineProtocol。要換引擎請走 EngineFactory.make(packPath:)——\
         `RustEngineAdapter` 這個名字也在禁列內，它字面上就含有 RustEngine",
    ),
    (
        "import colorlull_engine",
        "uniffi 生成的型別由 EngineBridge 轉出（specs/ios-scaffold.md §2）。\
         Shell 只寫 `import EngineBridge`",
    ),
];

pub fn run(root: &Path) -> Result<()> {
    let dir = root.join(SHELL_DIR);
    if !dir.is_dir() {
        bail!("找不到 {SHELL_DIR}——App Shell 的目錄名變了？請一併更新 xtask/src/lint_ios.rs");
    }

    let mut sources = Vec::new();
    collect_swift(&dir, &mut sources)?;
    if sources.is_empty() {
        bail!("{SHELL_DIR} 底下一個 .swift 都沒有；lint-ios 掃了個空目錄，等於沒檢查");
    }
    sources.sort();

    let mut violations = Vec::new();
    for path in &sources {
        let source =
            fs::read_to_string(path).with_context(|| format!("讀取 {} 失敗", path.display()))?;
        let relative = path.strip_prefix(root).unwrap_or(path);

        for (line_no, line) in source.lines().enumerate() {
            // 註解要能自由討論這件事——本檔與 EngineFactory 的說明都提到 `RustEngine`。
            if is_comment(line) {
                continue;
            }
            for (needle, why) in FORBIDDEN {
                if line.contains(needle) {
                    violations.push(format!(
                        "{}:{}：出現 `{needle}`\n      {why}",
                        relative.display(),
                        line_no + 1
                    ));
                }
            }
        }
    }

    if violations.is_empty() {
        println!(
            "lint-ios：{SHELL_DIR} 的 {} 個 Swift 檔全數只依賴 EngineProtocol",
            sources.len()
        );
        return Ok(());
    }

    for violation in &violations {
        eprintln!("  ✗ {violation}");
    }
    bail!(
        "lint-ios：{} 項違規（驗收見 roadmap/S0.md「App Shell 端沒有任何一行直接引用 RustEngine」）",
        violations.len()
    )
}

/// 只認整行註解。`let x = 1 // RustEngine` 這種尾註解**故意**不放行——
/// 放行就得寫一個字串／註解的狀態機，而那正是「最笨的檢查」要避開的東西。
fn is_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*")
}

fn collect_swift(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("讀取目錄 {} 失敗", dir.display()))?
    {
        let path = entry
            .with_context(|| format!("列舉 {} 失敗", dir.display()))?
            .path();
        if path.is_dir() {
            collect_swift(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "swift") {
            out.push(path);
        }
    }
    Ok(())
}
