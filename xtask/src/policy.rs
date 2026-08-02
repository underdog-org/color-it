//! 依賴方向 policy 引擎。
//!
//! 刻意與 cargo 隔離：輸入是一組 [`CrateManifest`]，不碰檔案系統也不跑 cargo。
//! 這讓「在 stroke 加 wgpu 會失敗」這條 M0 驗收標準能寫成單元測試，
//! 而不必真的去改 `core/stroke/Cargo.toml`。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

/// `xtask/deps-policy.toml` 的內容。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub workspace: WorkspacePolicy,
    #[serde(default)]
    pub crates: BTreeMap<String, CratePolicy>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspacePolicy {
    /// 預設全禁的外部 crate，需在個別 crate 的 `external` 逐一解禁。
    #[serde(default, rename = "banned-external")]
    pub banned_external: BTreeSet<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CratePolicy {
    /// 允許依賴的 workspace 內部 crate（用 policy key，非 package name）。
    #[serde(default)]
    pub internal: BTreeSet<String>,
    /// 在此 crate 解禁的 `banned-external` 項目。
    #[serde(default)]
    pub external: BTreeSet<String>,
}

/// 一個 workspace member 的**直接**依賴。
///
/// 只看直接宣告——wgpu 經由 `render` 傳遞到 `engine` 是合法的。
/// 不區分 normal / dev / build：`stroke` 就算只在 dev-dependencies 列 wgpu
/// 也違反「純 CPU、零 GPU 依賴」。
#[derive(Debug, Clone)]
pub struct CrateManifest {
    /// policy key，即 package name 去掉 `colorit-` 前綴。
    pub key: String,
    pub package: String,
    pub internal: BTreeSet<String>,
    pub external: BTreeSet<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    /// workspace member 沒有在 policy 登記——新增 crate 忘了登記。
    Unregistered { key: String, package: String },
    /// policy 有一節，但 workspace 沒有這個 member——crate 改名或刪除後的殘留。
    StaleSection { key: String },
    /// 內部依賴不在 allowlist——違反 §5.1 的單向圖。
    DisallowedInternal { key: String, dep: String },
    /// 用了 banned-external 但該 crate 沒有解禁。
    BannedExternal { key: String, dep: String },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unregistered { key, package } => write!(
                f,
                "{package} 未在 deps-policy.toml 登記，請新增 [crates.{key}] 一節"
            ),
            Self::StaleSection { key } => write!(
                f,
                "deps-policy.toml 的 [crates.{key}] 沒有對應的 workspace member，請移除"
            ),
            Self::DisallowedInternal { key, dep } => write!(
                f,
                "{key} 不得依賴 {dep}（違反 architecture.md §5.1 的單向圖）；\
                 若這是有意的架構變更，請先改 [crates.{key}] 的 internal"
            ),
            Self::BannedExternal { key, dep } => write!(
                f,
                "{key} 不得依賴 {dep}（{dep} 屬 banned-external）；\
                 若這是有意的，請加進 [crates.{key}] 的 external"
            ),
        }
    }
}

/// 對照 policy 檢查所有 member，回傳所有違規（不在第一個就停）。
pub fn check(policy: &Policy, manifests: &[CrateManifest]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for manifest in manifests {
        let Some(rules) = policy.crates.get(&manifest.key) else {
            violations.push(Violation::Unregistered {
                key: manifest.key.clone(),
                package: manifest.package.clone(),
            });
            continue;
        };

        for dep in &manifest.internal {
            if !rules.internal.contains(dep) {
                violations.push(Violation::DisallowedInternal {
                    key: manifest.key.clone(),
                    dep: dep.clone(),
                });
            }
        }

        for dep in &manifest.external {
            if policy.workspace.banned_external.contains(dep) && !rules.external.contains(dep) {
                violations.push(Violation::BannedExternal {
                    key: manifest.key.clone(),
                    dep: dep.clone(),
                });
            }
        }
    }

    let present: BTreeSet<&str> = manifests.iter().map(|m| m.key.as_str()).collect();
    for key in policy.crates.keys() {
        if !present.contains(key.as_str()) {
            violations.push(Violation::StaleSection { key: key.clone() });
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(toml_src: &str) -> Policy {
        toml::from_str(toml_src).expect("fixture policy 應可解析")
    }

    fn manifest(key: &str, internal: &[&str], external: &[&str]) -> CrateManifest {
        CrateManifest {
            key: key.to_string(),
            package: format!("colorit-{key}"),
            internal: internal.iter().map(|s| s.to_string()).collect(),
            external: external.iter().map(|s| s.to_string()).collect(),
        }
    }

    const FIXTURE: &str = r#"
        [workspace]
        banned-external = ["wgpu"]

        [crates.colorpack]

        [crates.stroke]
        internal = ["colorpack"]

        [crates.render]
        internal = ["colorpack", "stroke"]
        external = ["wgpu"]
    "#;

    /// FIXTURE 的完整合法 member 集合。
    ///
    /// 每個測試都從完整集合出發再改動一項——只餵部分 member 會額外觸發
    /// `StaleSection`，測到的就不是想測的那條規則了。
    fn all_valid() -> Vec<CrateManifest> {
        vec![
            manifest("colorpack", &[], &[]),
            manifest("stroke", &["colorpack"], &["serde"]),
            manifest("render", &["colorpack", "stroke"], &["wgpu", "bytemuck"]),
        ]
    }

    /// 正例：合法設定無違規，且 render 用 wgpu 是被允許的。
    #[test]
    fn valid_config_has_no_violations() {
        assert_eq!(check(&policy(FIXTURE), &all_valid()), vec![]);
    }

    /// M0 驗收：刻意在 stroke 加入 wgpu 依賴時，lint 必須失敗。
    #[test]
    fn wgpu_in_stroke_is_rejected() {
        let mut manifests = all_valid();
        manifests[1] = manifest("stroke", &["colorpack"], &["wgpu"]);

        assert_eq!(
            check(&policy(FIXTURE), &manifests),
            vec![Violation::BannedExternal {
                key: "stroke".into(),
                dep: "wgpu".into(),
            }]
        );
    }

    #[test]
    fn unregistered_member_is_rejected() {
        let mut manifests = all_valid();
        manifests.push(manifest("document", &[], &[]));

        assert_eq!(
            check(&policy(FIXTURE), &manifests),
            vec![Violation::Unregistered {
                key: "document".into(),
                package: "colorit-document".into(),
            }]
        );
    }

    #[test]
    fn reverse_internal_dependency_is_rejected() {
        let mut manifests = all_valid();
        manifests[1] = manifest("stroke", &["colorpack", "render"], &[]);

        assert_eq!(
            check(&policy(FIXTURE), &manifests),
            vec![Violation::DisallowedInternal {
                key: "stroke".into(),
                dep: "render".into(),
            }]
        );
    }

    #[test]
    fn stale_policy_section_is_rejected() {
        let mut manifests = all_valid();
        manifests.pop(); // render 被移除，但 policy 還留著它那一節

        assert_eq!(
            check(&policy(FIXTURE), &manifests),
            vec![Violation::StaleSection {
                key: "render".into()
            }]
        );
    }

    #[test]
    fn all_violations_reported_at_once() {
        let violations = check(
            &policy(FIXTURE),
            &[
                manifest("colorpack", &[], &[]),
                manifest("stroke", &["colorpack", "render"], &["wgpu"]),
                manifest("render", &["colorpack", "stroke"], &["wgpu"]),
            ],
        );
        assert_eq!(violations.len(), 2);
    }
}
