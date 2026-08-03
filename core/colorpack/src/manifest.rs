//! `manifest.json` 的型別與 `schema_version` 檢查（`specs/baker-core-design.md §3.4`）。

use serde::{Deserialize, Serialize};

use crate::Error;

/// M1 起即為 `1.0`。`major.minor`——runtime 拒絕未知 major。
pub const SCHEMA_VERSION: &str = "1.0";

/// 難度門檻。SSOT 在 `docs/specs/assets-spec.md §8`，這裡是引用。
pub const DIFFICULTY_EASY_MAX: u32 = 60;
pub const DIFFICULTY_MEDIUM_MAX: u32 = 200;

/// `regions.bin` 是 R16，ID 必須放得進 u16。
pub const MAX_REGION_COUNT: u32 = 65535;

/// 欄位順序即 JSON 的序列化順序，與 `baker-core-design.md §3.4` 的樣本一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    pub id: String,
    /// `"sha256:" + lowercase hex`，由 `ColorPack::write_to` 填入（§3.3）。
    pub content_hash: String,
    /// 輸出解析度 `[w, h]`。
    pub canvas_size: [u32; 2],
    pub aspect: Aspect,
    pub region_count: u32,
    pub difficulty: Difficulty,
    pub category: Category,
    pub has_shade: bool,
    pub palette: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aspect {
    #[serde(rename = "1:1")]
    Square,
    /// 母帶 3072×4096 → 輸出 1536×2048（`baker-core-design.md §0`）。
    #[serde(rename = "3:4")]
    Portrait,
}

impl Aspect {
    /// 順序即 `contracts/colorpack.schema.json` 的 enum 順序（有跨檢測試）。
    pub const ALL: [Aspect; 2] = [Aspect::Square, Aspect::Portrait];

    /// 由母帶尺寸推導。不是允許的兩種尺寸就是 `None`。
    pub fn from_master_size(w: u32, h: u32) -> Option<Self> {
        match (w, h) {
            (4096, 4096) => Some(Self::Square),
            (3072, 4096) => Some(Self::Portrait),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Square => "1:1",
            Self::Portrait => "3:4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    Medium,
    Focused,
}

impl Difficulty {
    /// 順序即 `contracts/colorpack.schema.json` 的 enum 順序（有跨檢測試）。
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Focused];

    /// 門檻見 `assets-spec §8`：≤60 輕鬆｜61–200 適中｜>200 專注。
    pub fn from_region_count(n: u32) -> Self {
        if n <= DIFFICULTY_EASY_MAX {
            Self::Easy
        } else if n <= DIFFICULTY_MEDIUM_MAX {
            Self::Medium
        } else {
            Self::Focused
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Easy => "easy",
            Self::Medium => "medium",
            Self::Focused => "focused",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Anime,
    Mandala,
    Animal,
    Botanical,
    Scenery,
    Cartoon,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Anime,
        Category::Mandala,
        Category::Animal,
        Category::Botanical,
        Category::Scenery,
        Category::Cartoon,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anime => "anime",
            Self::Mandala => "mandala",
            Self::Animal => "animal",
            Self::Botanical => "botanical",
            Self::Scenery => "scenery",
            Self::Cartoon => "cartoon",
        }
    }
}

/// `major.minor` 的 major 必須與本 crate 相同；minor 不同視為相容。
pub fn check_schema_version(found: &str) -> Result<(), Error> {
    let reject = || {
        Err(Error::SchemaVersion {
            found: found.to_owned(),
            expected: SCHEMA_VERSION,
        })
    };
    // 形狀本身也要驗。契約 pattern 是 `^[0-9]+\.[0-9]+$`
    // （`contracts/colorpack.schema.json`）——只切第一個 `.` 的話 `"1"`、`"1.0.0"`
    // 都會通過，reader 就比契約寬，而寬的那一端遲早會收到寫端不打算支援的東西。
    let mut parts = found.split('.');
    let (Some(major), Some(minor), None) = (parts.next(), parts.next(), parts.next()) else {
        return reject();
    };
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !numeric(major) || !numeric(minor) {
        return reject();
    }
    if major == SCHEMA_VERSION.split('.').next().unwrap_or_default() {
        Ok(())
    } else {
        reject()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_thresholds_match_assets_spec() {
        assert_eq!(Difficulty::from_region_count(0), Difficulty::Easy);
        assert_eq!(Difficulty::from_region_count(60), Difficulty::Easy);
        assert_eq!(Difficulty::from_region_count(61), Difficulty::Medium);
        assert_eq!(Difficulty::from_region_count(200), Difficulty::Medium);
        assert_eq!(Difficulty::from_region_count(201), Difficulty::Focused);
    }

    #[test]
    fn aspect_only_accepts_the_two_master_sizes() {
        assert_eq!(Aspect::from_master_size(4096, 4096), Some(Aspect::Square));
        assert_eq!(Aspect::from_master_size(3072, 4096), Some(Aspect::Portrait));
        assert_eq!(Aspect::from_master_size(4096, 3072), None);
        assert_eq!(Aspect::from_master_size(2048, 2048), None);
    }

    #[test]
    fn unknown_major_is_rejected_minor_is_not() {
        assert!(check_schema_version("1.0").is_ok());
        assert!(check_schema_version("1.7").is_ok());
        assert!(check_schema_version("1.12").is_ok());
        assert!(check_schema_version("2.0").is_err());
    }

    /// reader 不能比契約寬：schema 的 pattern 是 `^[0-9]+\.[0-9]+$`。
    #[test]
    fn malformed_versions_are_rejected_even_with_the_right_major() {
        for bad in ["1", "1.", ".0", "1.0.0", "1.x", "", "v1.0", " 1.0"] {
            assert!(
                check_schema_version(bad).is_err(),
                "{bad:?} 不是合法的 schema_version"
            );
        }
    }
}
