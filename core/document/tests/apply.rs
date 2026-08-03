//! `document::apply` 的行為（`docs/specs/E1-bucket.md §2` §10）。
//!
//! **全部無 GPU**——`document` 不依賴 `render`，這份測試在 CI 的無顯卡機器上也要綠。

use document::{Document, Effect, Op};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn doc() -> Document {
    Document::new(vec![[0, 0, 10, 10], [10, 0, 6, 4], [0, 10, 10, 10]])
}

fn fill(doc: &mut Document, region_id: u32, color: [u8; 4]) -> Effect {
    doc.apply(Op::Fill { region_id, color })
}

#[test]
fn fill_reports_previous_color_and_bbox() {
    let mut doc = doc();

    assert_eq!(
        fill(&mut doc, 1, RED),
        Effect::Filled {
            region_id: 1,
            color: RED,
            // 未填色的區域 prev 全零，shader 端靠 `base.a == 0` 走白紙路徑。
            prev: [0, 0, 0, 0],
            bbox: [10, 0, 6, 4],
        }
    );
    assert_eq!(doc.palette()[1], RED);
}

#[test]
fn refilling_reports_the_colour_it_replaced() {
    let mut doc = doc();
    fill(&mut doc, 1, RED);

    // `prev` 是擴散動畫的起點（§7），不是「上一次的操作」。
    assert_eq!(
        fill(&mut doc, 1, BLUE),
        Effect::Filled {
            region_id: 1,
            color: BLUE,
            prev: RED,
            bbox: [10, 0, 6, 4],
        }
    );
}

#[test]
fn same_colour_refill_changes_nothing() {
    let mut doc = doc();
    fill(&mut doc, 1, RED);
    let before = doc.palette().to_vec();

    assert_eq!(fill(&mut doc, 1, RED), Effect::None);
    assert_eq!(doc.palette(), before.as_slice());
    assert_eq!(doc.colored_regions(), 1);
}

#[test]
fn unknown_region_id_changes_nothing() {
    let mut doc = doc();

    // 畫布外的 tap 走這條路徑——不 clamp，clamp 會讓誤觸填到邊緣區域（§4.3）。
    assert_eq!(fill(&mut doc, 3, RED), Effect::None);
    assert_eq!(fill(&mut doc, u32::MAX, RED), Effect::None);
    assert_eq!(doc.palette(), [[0; 4]; 3]);
    assert_eq!(doc.colored_regions(), 0);
}

#[test]
fn alpha_is_forced_opaque() {
    let mut doc = doc();

    // `Tool::Bucket { color }` 的 alpha 被忽略：填色即不透明，`a == 0` 是狀態旗標（§5）。
    let effect = fill(&mut doc, 0, [1, 2, 3, 0]);

    assert_eq!(doc.palette()[0], [1, 2, 3, 255]);
    assert!(matches!(effect, Effect::Filled { color, .. } if color == [1, 2, 3, 255]));
    assert_eq!(doc.colored_regions(), 1);
}

#[test]
fn alpha_stripped_refill_is_still_a_same_colour_noop() {
    let mut doc = doc();
    fill(&mut doc, 0, [1, 2, 3, 255]);

    // 正規化在比較之前——否則同一個顏色帶不同 alpha 進來會被當成換色，
    // 每次點擊都重播一次擴散動畫。
    assert_eq!(fill(&mut doc, 0, [1, 2, 3, 0]), Effect::None);
}

#[test]
fn colored_regions_counts_distinct_regions_not_taps() {
    let mut doc = doc();
    assert_eq!(doc.colored_regions(), 0);

    fill(&mut doc, 0, RED);
    fill(&mut doc, 2, RED);
    assert_eq!(doc.colored_regions(), 2);

    // 換色不是新填色。
    fill(&mut doc, 0, BLUE);
    assert_eq!(doc.colored_regions(), 2);
    assert_eq!(doc.total_regions(), 3);
}

#[test]
fn brush_stroke_is_shaped_but_inert() {
    let mut doc = doc();

    // E1 的筆刷不經過 document，`T_paint` 才是真相。只定型，不改狀態。
    let effect = doc.apply(Op::BrushStroke {
        color: RED,
        opacity: 0.5,
    });

    assert_eq!(effect, Effect::None);
    assert_eq!(doc.colored_regions(), 0);
}

#[test]
fn palette_length_follows_region_count() {
    let doc = Document::new(vec![[0, 0, 1, 1]; 65535]);

    // 65535 是 R16Uint 的上限，palette 直接以 ID 索引（§8）。
    assert_eq!(doc.palette().len(), 65535);
    assert_eq!(doc.total_regions(), 65535);
}
