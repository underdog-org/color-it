//! `assets/source/**` 的 `meta.json` 驗證。
//!
//! 這是 CI 唯一碰得到真實素材目錄的地方：`build-infra.md §5` 明訂 CI 以 `lfs: false`
//! checkout，PNG 只拿得到 pointer——但 `.gitattributes`

use std::path::PathBuf;

fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/source")
}

#[test]
fn every_source_directory_has_a_valid_meta_json() {
    let root = source_root();
    let mut checked = 0;
    for entry in std::fs::read_dir(&root).expect("讀不到 assets/source") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        // Source::load 不解碼 PNG，所以 LFS pointer 也跑得過——這裡驗的只有 meta.json。
        let (_, diagnostics) = baker::source::Source::load(&dir)
            .unwrap_or_else(|e| panic!("{} 的 meta.json 讀不起來：{e:#}", dir.display()));
        assert!(
            diagnostics.is_empty(),
            "{} 的 meta.json 不合規：{:?}",
            dir.display(),
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        checked += 1;
    }
    assert!(checked >= 3, "只檢查到 {checked} 個素材目錄，路徑可能錯了");
}
