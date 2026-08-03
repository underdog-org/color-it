//! Pass 1 Stroke 與 Pass 2 Commit（`docs/specs/E1-stroke.md §7`／`§8`／`§12`）。
//!
//! **驗證一律從 `T_paint` 讀回**：`T_wet` 沒有 `COPY_SRC`（`E1-wgpu §4`，它是單筆
//! 暫存、永遠不進 undo），所以「Pass 1 畫對了嗎」只能透過 commit 之後的結果看。
//! 這不是繞路——`T_wet` 的值本來就只有經過 Pass 2 才對使用者有意義。

mod support;

use colorpack::ColorPack;
use render::{
    Bounds, CommitPass, DocumentResources, Gpu, MaskBinding, MaskMode, MaskUniform, StrokePass,
};
use stroke::{Dab, MAX_DABS_PER_DRAW, TipId, Vec2};

const CANVAS: u32 = 64;
/// 左半 region 0、右半 region 1——mask mode A／B 的差別要看得出來。
const SPLIT: u32 = CANVAS / 2;

/// 讀回 `T_paint` 的比對容差。筆尖是 256×256 貼圖 ＋ linear filter，
/// 中心點取到的不會剛好是 1.0（四個中心 texel 的平均）。
const TOL: i32 = 3;

struct Harness {
    gpu: Gpu,
    res: DocumentResources,
    mask: MaskBinding,
    stroke: StrokePass,
    commit: CommitPass,
}

impl Harness {
    fn new() -> Self {
        let gpu = Gpu::headless().expect("headless device");
        let pack = pack();
        let res = DocumentResources::new(&gpu, &pack).expect("resources");

        let mask = MaskBinding::new(&gpu);
        let mut stroke = StrokePass::new(&gpu);
        stroke.bind_document(&gpu, &res);
        let mut commit = CommitPass::new(&gpu, &mask);
        commit.bind_document(&gpu, &res);

        let h = Self {
            gpu,
            res,
            mask,
            stroke,
            commit,
        };
        h.set_mask(MaskMode::Loose, 0);
        // wgpu 保證零初始化，但兩張 attachment 的起點是每條測試的前提，寫明比較好查。
        support::clear_texture(&h.gpu, h.res.wet(), wgpu::Color::TRANSPARENT);
        support::clear_texture(&h.gpu, h.res.paint(), wgpu::Color::TRANSPARENT);
        h
    }

    fn set_mask(&self, mode: MaskMode, active_region_id: u32) {
        self.mask.set(
            &self.gpu,
            MaskUniform {
                mode: mode as u32,
                active_region_id,
            },
        );
    }

    fn draw(&self, dabs: &[Dab], build_up: bool) {
        self.stroke.draw(&self.gpu, &self.res, dabs, build_up);
    }

    fn commit(&self, color: [f32; 4], opacity: f32, dabs: &[Dab]) {
        let bbox = Bounds::of_dabs(dabs)
            .and_then(|b| b.to_scissor(self.res.canvas_size()))
            .expect("非空筆畫必有 bbox");
        self.commit
            .commit(&self.gpu, &self.res, &self.mask, color, opacity, bbox);
    }

    fn clear_wet(&self, dabs: &[Dab]) {
        let bbox = Bounds::of_dabs(dabs)
            .and_then(|b| b.to_scissor(self.res.canvas_size()))
            .expect("非空筆畫必有 bbox");
        self.commit.clear_wet(&self.gpu, &self.res, bbox);
    }

    /// `T_paint` 的一個像素，RGBA premultiplied。
    fn paint_at(&self, x: u32, y: u32) -> [u8; 4] {
        let data = support::read_texture(&self.gpu, self.res.paint(), 4);
        let i = ((y * CANVAS + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }
}

fn pack() -> ColorPack {
    let ids = (0..CANVAS * CANVAS)
        .map(|i| u16::from(i % CANVAS >= SPLIT))
        .collect();
    support::pack(CANVAS, CANVAS, ids, false)
}

fn dab_at(x: f32, y: f32, size: f32, alpha: f32) -> Dab {
    Dab {
        pos: Vec2::new(x, y),
        size,
        angle: 0.0,
        alpha,
        tip: TipId::SoftRound,
    }
}

fn assert_near(actual: u8, expected: u8, what: &str) {
    let delta = i32::from(actual) - i32::from(expected);
    assert!(
        delta.abs() <= TOL,
        "{what}：實際 {actual}，期望 {expected}±{TOL}"
    );
}

/// 紅色不透明。commit 走 premultiplied，所以 `T_paint.r == T_paint.a`。
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

#[test]
fn a_committed_dab_lands_in_paint() {
    let h = Harness::new();
    let dabs = [dab_at(16.5, 32.5, 20.0, 1.0)];

    h.draw(&dabs, false);
    h.commit(RED, 1.0, &dabs);

    let center = h.paint_at(16, 32);
    assert_near(center[3], 255, "dab 中心的 alpha");
    // premultiplied：rgb 已經乘過 alpha，所以紅通道跟著 alpha 走。
    assert_near(center[0], center[3], "premultiplied 的紅通道");
    assert_eq!(center[2], 0, "藍通道不該有值");
}

#[test]
fn nothing_outside_the_dab_is_touched() {
    let h = Harness::new();
    let dabs = [dab_at(16.5, 32.5, 20.0, 1.0)];

    h.draw(&dabs, false);
    h.commit(RED, 1.0, &dabs);

    // 距離圓心 20 px、半徑只有 10——scissor 與筆尖形狀都該把它擋在外面。
    assert_eq!(h.paint_at(40, 32), [0, 0, 0, 0], "bbox 外的像素被寫到了");
}

/// `E1-stroke §12`：慢速來回塗抹同一處，濃度不隨次數變深。
#[test]
fn max_blend_does_not_darken_within_one_stroke() {
    let once = {
        let h = Harness::new();
        let dabs = [dab_at(32.5, 32.5, 20.0, 0.5)];
        h.draw(&dabs, false);
        h.commit(RED, 1.0, &dabs);
        h.paint_at(32, 32)
    };

    let h = Harness::new();
    let dabs = [dab_at(32.5, 32.5, 20.0, 0.5)];
    // 同一個位置畫五次——`One / One / Max` 下結果必須逐位元相同，不是「接近」。
    for _ in 0..5 {
        h.draw(&dabs, false);
    }
    h.commit(RED, 1.0, &dabs);

    assert_eq!(h.paint_at(32, 32), once, "重複塗抹讓濃度變深了");
}

/// `build_up` 走 `OneMinusDst / One / Add`，同一處會累積——E2 的噴槍靠它。
#[test]
fn build_up_accumulates_where_max_does_not() {
    let h = Harness::new();
    let dabs = [dab_at(32.5, 32.5, 20.0, 0.5)];
    for _ in 0..3 {
        h.draw(&dabs, true);
    }
    h.commit(RED, 1.0, &dabs);

    let a = h.paint_at(32, 32)[3];
    assert!(
        a > 200,
        "build_up 三次 0.5 應累積到 0.875 以上，實際 alpha {a}"
    );
}

/// `E1-stroke §12`：`opacity` 調整後，整筆濃度上限跟著變，而不是每個 dab 的濃度變。
#[test]
fn opacity_caps_the_whole_stroke() {
    let h = Harness::new();
    let dabs = [dab_at(32.5, 32.5, 20.0, 1.0)];

    h.draw(&dabs, false);
    h.commit(RED, 0.5, &dabs);

    assert_near(h.paint_at(32, 32)[3], 128, "opacity 0.5 的整筆上限");
}

/// D4 的實作面：Mode A 擋住 `active_region_id` 以外的區域，Mode B 無條件通過。
#[test]
fn mask_mode_a_blocks_the_neighbouring_region() {
    // 橫跨分界的一筆：左端在 region 0、右端在 region 1。
    let dabs = [dab_at(24.5, 32.5, 16.0, 1.0), dab_at(40.5, 32.5, 16.0, 1.0)];

    let strict = {
        let h = Harness::new();
        h.set_mask(MaskMode::Strict, 0);
        h.draw(&dabs, false);
        h.commit(RED, 1.0, &dabs);
        (h.paint_at(24, 32), h.paint_at(40, 32))
    };

    let h = Harness::new();
    h.set_mask(MaskMode::Loose, 0);
    h.draw(&dabs, false);
    h.commit(RED, 1.0, &dabs);
    let loose = (h.paint_at(24, 32), h.paint_at(40, 32));

    assert_near(strict.0[3], 255, "Mode A 的 active region 內");
    assert_eq!(strict.1, [0, 0, 0, 0], "Mode A 應該擋住隔壁區");
    assert_near(loose.0[3], 255, "Mode B 的 active region 內");
    assert_near(loose.1[3], 255, "Mode B 應該塗到隔壁區");
}

/// `§8.1`：commit 收尾清 `T_wet`。第二次 commit 因此什麼都不該加。
#[test]
fn commit_clears_wet_so_a_second_commit_is_a_no_op() {
    let h = Harness::new();
    let dabs = [dab_at(32.5, 32.5, 20.0, 1.0)];

    h.draw(&dabs, false);
    h.commit(RED, 0.5, &dabs);
    let after_first = h.paint_at(32, 32);

    h.commit(RED, 0.5, &dabs);
    assert_eq!(h.paint_at(32, 32), after_first, "`T_wet` 沒有被清乾淨");
}

/// `E1-stroke §12`：`cancel_stroke` 之後 `T_paint` 逐像素不變。
#[test]
fn discarding_wet_never_touches_paint() {
    let h = Harness::new();
    let dabs = [dab_at(32.5, 32.5, 20.0, 1.0)];

    h.draw(&dabs, false);
    h.clear_wet(&dabs);
    // 清完之後再 commit：`T_wet` 是空的，`T_paint` 就該原封不動。
    h.commit(RED, 1.0, &dabs);

    assert_eq!(h.paint_at(32, 32), [0, 0, 0, 0], "取消的筆畫污染了 T_paint");
}

/// `§9` 的重建：清掉 `T_wet` 再以全部真實樣本重跑 Pass 1，結果與從未畫過預測點相同。
#[test]
fn rebuilding_wet_drops_the_predicted_tail() {
    let real = [dab_at(20.5, 32.5, 16.0, 1.0)];
    let predicted = [dab_at(44.5, 32.5, 16.0, 1.0)];
    // 重建的 scissor 必須涵蓋預測點畫過的範圍，否則尾巴清不掉。
    let span = [real[0], predicted[0]];

    let h = Harness::new();
    h.draw(&real, false);
    h.draw(&predicted, false);
    h.clear_wet(&span);
    h.draw(&real, false);
    h.commit(RED, 1.0, &span);

    assert_near(h.paint_at(20, 32)[3], 255, "真實樣本的筆跡不見了");
    assert_eq!(
        h.paint_at(44, 32),
        [0, 0, 0, 0],
        "預測點的尾巴留在 T_paint 裡"
    );
}

/// `§7`：一 frame 內超過 `MAX_DABS_PER_DRAW` 要分批，**不是靜默截斷**——
/// 截斷會變成「畫太快就斷線」。
#[test]
fn more_dabs_than_one_draw_are_not_truncated() {
    let h = Harness::new();
    let mut dabs = vec![dab_at(16.5, 16.5, 12.0, 1.0); MAX_DABS_PER_DRAW];
    // 最後一個落在第二批，位置與前面全部不同。
    dabs.push(dab_at(48.5, 48.5, 12.0, 1.0));

    h.draw(&dabs, false);
    h.commit(RED, 1.0, &dabs);

    assert_near(h.paint_at(16, 16)[3], 255, "第一批的 dab");
    assert_near(h.paint_at(48, 48)[3], 255, "第二批的 dab 被截斷了");
}

#[test]
fn bounds_of_dabs_covers_the_diameter_not_the_radius() {
    let dabs = [dab_at(10.0, 10.0, 4.0, 1.0), dab_at(20.0, 12.0, 8.0, 1.0)];
    let b = Bounds::of_dabs(&dabs).expect("非空");

    assert_eq!(b.min, [8.0, 8.0]);
    assert_eq!(b.max, [24.0, 16.0]);
}

#[test]
fn an_empty_stroke_has_no_bounds() {
    assert!(Bounds::of_dabs(&[]).is_none());
}

/// 向外取整：少一個像素就是筆跡邊緣被 scissor 切掉一條。
#[test]
fn scissor_rounds_outwards_and_clamps_to_canvas() {
    let b = Bounds {
        min: [-3.5, 10.25],
        max: [12.5, 10.75],
    };
    assert_eq!(b.to_scissor([64, 64]), Some([0, 10, 13, 1]));
}

#[test]
fn a_bbox_entirely_off_canvas_has_no_scissor() {
    let b = Bounds {
        min: [100.0, 100.0],
        max: [120.0, 120.0],
    };
    assert_eq!(b.to_scissor([64, 64]), None);
}

/// NaN 夾不住：`as u32` 會給 0，看起來像個合法矩形。
#[test]
fn a_nan_bbox_has_no_scissor() {
    let b = Bounds {
        min: [f32::NAN, 0.0],
        max: [10.0, 10.0],
    };
    assert_eq!(b.to_scissor([64, 64]), None);
}
