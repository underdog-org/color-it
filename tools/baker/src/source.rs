//! 來源目錄解析與 `meta.json` 驗證（`assets-spec §4.5`）。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colorpack::Category;
use serde::Deserialize;

use crate::report::{Diagnostic, Stage, code};

pub const LINEART: &str = "lineart.png";
pub const SEEDS: &str = "seeds.png";
pub const SHADE: &str = "shade.png";
pub const META: &str = "meta.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub id: String,
    #[allow(dead_code, reason = "只給人看的欄位，baker 不使用但必須存在")]
    pub title: String,
    /// 讀成字串而非 enum：拼錯時要能把**實際寫了什麼**放進退件訊息。
    pub category: String,
    #[serde(default)]
    #[allow(dead_code, reason = "assets-spec §4.5：baker 完全忽略")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub dir: PathBuf,
    /// 資料夾名。`id` 的真相是它，`meta.json` 只是冗餘校驗（`architecture §9.1`）。
    pub folder_id: String,
    pub meta: Meta,
    pub lineart: PathBuf,
    pub seeds: PathBuf,
    pub shade: Option<PathBuf>,
}

impl Source {
    /// 讀取來源目錄。回傳的 diagnostics 是 `meta.json` 相關的檢查結果。
    pub fn load(dir: &Path) -> Result<(Self, Vec<Diagnostic>)> {
        let folder_id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| format!("{} 不是合法的素材目錄名", dir.display()))?
            .to_owned();

        let meta_path = dir.join(META);
        let meta_text = std::fs::read_to_string(&meta_path)
            .with_context(|| format!("讀取 {} 失敗", meta_path.display()))?;
        let meta: Meta = serde_json::from_str(&meta_text).with_context(|| {
            format!(
                "解析 {} 失敗（必須是 UTF-8 無 BOM 的合法 JSON）",
                meta_path.display()
            )
        })?;

        let mut diagnostics = Vec::new();
        if meta.id != folder_id {
            diagnostics.push(Diagnostic::error(
                code::META_ID_MISMATCH,
                Stage::Master,
                format!(
                    "meta.json 的 id 是 \"{}\"，資料夾名是 \"{folder_id}\"。\
                     id 是永久識別碼，想改名請改 title",
                    meta.id
                ),
            ));
        }
        if Category::parse(&meta.category).is_none() {
            let allowed: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
            diagnostics.push(Diagnostic::error(
                code::META_BAD_CATEGORY,
                Stage::Master,
                format!(
                    "meta.json 的 category 是 \"{}\"，必須是六選一：{}",
                    meta.category,
                    allowed.join(" / ")
                ),
            ));
        }

        let mut missing = Vec::new();
        let mut require = |name: &str| {
            let path = dir.join(name);
            if !path.is_file() {
                missing.push(name.to_owned());
            }
            path
        };
        let lineart = require(LINEART);
        let seeds = require(SEEDS);
        if !missing.is_empty() {
            diagnostics.push(Diagnostic::error(
                code::SOURCE_INCOMPLETE,
                Stage::Master,
                format!("缺少必交檔案：{}", missing.join("、")),
            ));
        }

        let shade_path = dir.join(SHADE);
        let shade = shade_path.is_file().then_some(shade_path);

        Ok((
            Self {
                dir: dir.to_owned(),
                folder_id,
                meta,
                lineart,
                seeds,
                shade,
            },
            diagnostics,
        ))
    }

    /// `meta.category` 已驗過才呼叫；驗不過時管線不會走到需要它的地方。
    pub fn category(&self) -> Option<Category> {
        Category::parse(&self.meta.category)
    }
}
