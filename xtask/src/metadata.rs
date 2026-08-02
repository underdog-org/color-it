//! `cargo metadata` → [`CrateManifest`] 的轉換層。
//!
//! 與 [`crate::policy`] 分開，是為了讓 policy 引擎能用 fixture 測試。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::policy::CrateManifest;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_root: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    name: String,
}

pub struct Workspace {
    pub root: PathBuf,
    pub manifests: Vec<CrateManifest>,
}

/// package name → policy key：去掉 `colorit-` 前綴（`xtask` 本來就沒有）。
fn policy_key(package: &str) -> String {
    package
        .strip_prefix("colorit-")
        .unwrap_or(package)
        .to_string()
}

pub fn load() -> Result<Workspace> {
    // --no-deps：packages 只含 workspace member，且不必解析整棵依賴樹。
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("執行 cargo metadata 失敗")?;

    if !output.status.success() {
        bail!(
            "cargo metadata 回傳非零：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).context("解析 cargo metadata 輸出失敗")?;

    let members: BTreeSet<&str> = metadata.packages.iter().map(|p| p.name.as_str()).collect();

    let manifests = metadata
        .packages
        .iter()
        .map(|package| {
            let (internal, external) = package
                .dependencies
                .iter()
                .map(|dep| dep.name.as_str())
                .partition::<Vec<_>, _>(|name| members.contains(name));

            CrateManifest {
                key: policy_key(&package.name),
                package: package.name.clone(),
                internal: internal.into_iter().map(policy_key).collect(),
                external: external.into_iter().map(str::to_string).collect(),
            }
        })
        .collect();

    Ok(Workspace {
        root: metadata.workspace_root,
        manifests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colorit_prefix() {
        assert_eq!(policy_key("colorit-app-state"), "app-state");
        assert_eq!(policy_key("xtask"), "xtask");
    }
}
