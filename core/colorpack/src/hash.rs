//! `content_hash` 的正規化定義（`specs/baker-core-design.md §3.3`）。
//!
//! SHA-256 是對**未壓縮內容**取的，不是 hash 整個 zip 檔——`architecture §8.4` 規定
//! 文件永遠指向它原本的 `asset_hash`，所以 hash 不能受 zip crate 版本或 deflate
//! 實作影響，否則升級一個依賴就會讓全世界的使用者作品失效。

use sha2::{Digest, Sha256};

pub const PREFIX: &str = "sha256:";

/// `for entry in ENTRY_ORDER: name_len u32 LE ‖ name ‖ data_len u64 LE ‖ data`。
///
/// 呼叫端負責照 `ENTRY_ORDER` 排序並排除 `manifest.json`（它自己要裝這個 hash）。
pub fn content_hash(entries: &[(&str, &[u8])]) -> String {
    let mut hasher = Sha256::new();
    for (name, data) in entries {
        hasher.update((name.len() as u32).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update((data.len() as u64).to_le_bytes());
        hasher.update(data);
    }
    format!("{PREFIX}{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_prefixed_lowercase_hex() {
        let h = content_hash(&[("a.bin", b"x")]);
        assert!(h.starts_with(PREFIX));
        let hex = &h[PREFIX.len()..];
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }

    /// 長度前綴的存在理由：不加長度的話，`("ab", "")` 與 `("a", "b")` 會同 hash。
    #[test]
    fn length_prefixes_prevent_boundary_collisions() {
        assert_ne!(
            content_hash(&[("ab", b""), ("c", b"")]),
            content_hash(&[("a", b""), ("bc", b"")])
        );
        assert_ne!(
            content_hash(&[("a", b"bc")]),
            content_hash(&[("a", b"b"), ("a", b"c")])
        );
    }
}
