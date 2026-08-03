//! `contracts/colorpack.schema.json` 是 SSOT（`specs/baker-core-design.md §3.8`）。
//! 這裡驗的是 Rust 型別的序列化結果確實符合它——兩邊漂移時當場失敗。

use colorpack::{Aspect, Category, Difficulty, Manifest, RegionEntry, manifest};
use serde_json::{Value, json};

fn schema_document() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/colorpack.schema.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("讀取 schema 失敗")).unwrap()
}

/// 把根的 `$ref` 換成指定的 `$defs`，其餘（含 `$defs` 本身）保留——
/// 讓同一份文件同時當 manifest 與 regions 的 schema 用。
fn schema_for(def: &str) -> Value {
    let mut doc = schema_document();
    doc["$ref"] = json!(format!("#/$defs/{def}"));
    doc
}

fn sample_manifest() -> Manifest {
    Manifest {
        schema_version: manifest::SCHEMA_VERSION.to_owned(),
        id: "kirby-demo-1".to_owned(),
        content_hash: format!("sha256:{}", "0".repeat(64)),
        canvas_size: [2048, 2048],
        aspect: Aspect::Square,
        region_count: 187,
        difficulty: Difficulty::Medium,
        category: Category::Cartoon,
        has_shade: true,
        palette: vec!["#FFC0CB".to_owned(), "#0A0A0A".to_owned()],
    }
}

#[test]
fn manifest_sample_passes_schema() {
    let validator = jsonschema::validator_for(&schema_for("manifest")).unwrap();
    let instance = serde_json::to_value(sample_manifest()).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "manifest 不符 schema：{errors:?}");
}

#[test]
fn regions_sample_passes_schema() {
    let validator = jsonschema::validator_for(&schema_for("regions")).unwrap();
    let instance = serde_json::to_value(vec![RegionEntry {
        id: 0,
        centroid: [10, 20],
        area: 4096,
        bbox: [0, 0, 64, 64],
        suggested_color: "#12AB34".to_owned(),
    }])
    .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "regions 不符 schema：{errors:?}");
}

/// schema 的六個 category 與 Rust 的 enum 必須逐一對得上。
#[test]
fn category_enum_matches_schema() {
    let doc = schema_document();
    let from_schema: Vec<&str> = doc["$defs"]["manifest"]["properties"]["category"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let from_rust: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(from_schema, from_rust);
}

/// schema 收緊過（例如漏了一個必填欄位）時，這條會抓到「什麼都能通過」的假綠燈。
#[test]
fn schema_actually_rejects_bad_manifest() {
    let validator = jsonschema::validator_for(&schema_for("manifest")).unwrap();
    let mut instance = serde_json::to_value(sample_manifest()).unwrap();
    instance["difficulty"] = json!("impossible");
    assert!(!validator.is_valid(&instance));

    let mut instance = serde_json::to_value(sample_manifest()).unwrap();
    instance["content_hash"] = json!("deadbeef");
    assert!(!validator.is_valid(&instance));

    let mut instance = serde_json::to_value(sample_manifest()).unwrap();
    instance.as_object_mut().unwrap().remove("palette");
    assert!(!validator.is_valid(&instance));
}
