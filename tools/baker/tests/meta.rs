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
        //
        // **刻意不看 `source-incomplete`**：那條驗的是繳交清單，而 `baker-seeds.md §2`
        // 把清單從 flats+reference 換成 seeds 之後，舊契約交付的手繪素材必然缺件。
        // 素材重交是 M0 的事，不該讓它把 meta.json 的迴歸網一起弄成紅的。
        let (_, diagnostics) = baker::source::Source::load(&dir)
            .unwrap_or_else(|e| panic!("{} 的 meta.json 讀不起來：{e:#}", dir.display()));
        let meta_problems: Vec<&String> = diagnostics
            .iter()
            .filter(|d| d.code != baker::report::code::SOURCE_INCOMPLETE)
            .map(|d| &d.message)
            .collect();
        assert!(
            meta_problems.is_empty(),
            "{} 的 meta.json 不合規：{meta_problems:?}",
            dir.display(),
        );
        checked += 1;
    }
    assert!(checked >= 3, "只檢查到 {checked} 個素材目錄，路徑可能錯了");
}
