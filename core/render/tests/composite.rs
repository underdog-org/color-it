//! Pass 3 Composite 的 offscreen 比對（`docs/specs/E1-composite.md §9`）。
//!
//! 每一條對應驗收清單的一項。target 用 `Rgba8Unorm`——與 `SURFACE_FORMAT`
//! 同樣是非 sRGB 變體，硬體不做任何 decode／encode（§2），只差 channel 順序。

mod support;

use colorpack::ColorPack;
use render::{
    CompositePass, DocumentResources, FillAnim, Frame, Gpu, MaskBinding, MaskMode, MaskUniform,
    Transform,
};

/// 畫布外的背景色。刻意不是白的——§4 要求使用者看得出畫布邊界。
const BACKGROUND: [f32; 4] = [0.25, 0.25, 0.25, 1.0];

struct Canvas {
    gpu: Gpu,
    res: DocumentResources,
    mask: MaskBinding,
    pass: CompositePass,
    target: wgpu::Texture,
    screen: [u32; 2],
}

impl Canvas {
    /// 螢幕與畫布同尺寸、transform 為單位——大部分測試不需要 letterbox。
    fn new(pack: &ColorPack) -> Self {
        Self::with_screen(pack, pack.manifest.canvas_size)
    }

    fn with_screen(pack: &ColorPack, screen: [u32; 2]) -> Self {
        let gpu = Gpu::headless().expect("headless device");
        let res = DocumentResources::new(&gpu, pack).expect("resources");
        let mask = MaskBinding::new(&gpu);
        mask.set(
            &gpu,
            MaskUniform {
                mode: MaskMode::Loose as u32,
                active_region_id: 0,
            },
        );

        let mut pass = CompositePass::new(&gpu, wgpu::TextureFormat::Rgba8Unorm, &mask);
        pass.bind_document(&gpu, &res);

        let target = support::offscreen(&gpu, screen[0], screen[1]);
        Self {
            gpu,
            res,
            mask,
            pass,
            target,
            screen,
        }
    }

    fn frame(&self) -> Frame {
        Frame {
            transform: Transform {
                scale: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
            screen_size: self.screen,
            background: BACKGROUND,
            brush_color: [0.0, 0.0, 0.0, 0.0],
        }
    }

    fn render(&self, frame: Frame) -> Vec<u8> {
        self.pass.set_frame(&self.gpu, frame);
        let view = self
            .target
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.pass.draw(&mut encoder, &view, &self.mask);
        self.gpu.queue().submit([encoder.finish()]);
        support::read_texture(&self.gpu, &self.target, 4)
    }
}

fn px(pixels: &[u8], screen_w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * screen_w + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// f32 → u8 的 round-to-nearest 與 baker 的整數截除差一格，容差 2 涵蓋它。
#[track_caller]
fn close(actual: [u8; 4], expected: [u8; 4], tol: u8) {
    let ok = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tol);
    assert!(ok, "得到 {actual:?}，預期 {expected:?}（容差 {tol}）");
}

fn opaque(color: [f32; 3]) -> [f32; 4] {
    [color[0], color[1], color[2], 1.0]
}

/// 進度已走完的填色：`palette` 直接生效，不受動畫影響。
fn settled(res: &DocumentResources, gpu: &Gpu, region: u32, color: [f32; 4]) {
    res.write_palette(gpu, region, color);
    res.write_fill(
        gpu,
        region,
        FillAnim {
            origin: [0.0, 0.0],
            max_radius: 1.0e6,
            progress: 1.0,
            prev_color: [0.0; 4],
        },
    );
}

// ---------------------------------------------------------------------------

/// 驗收：未填色區域顯示 `PAPER_WHITE`。
///
/// 這條同時是 §3 差異 1 的迴歸測試：`Buf_palette` 全零，若沿用 `§4.2` 的
/// `mix(palette[id], PAPER_WHITE, erased)`（`erased == 0`）這裡會是**全黑**。
#[test]
fn never_filled_regions_are_paper_white_not_transparent() {
    let pack = support::pack_with(8, 8, vec![0; 64], [255, 255, 255, 255], None);
    let canvas = Canvas::new(&pack);

    let out = canvas.render(canvas.frame());

    close(px(&out, 8, 4, 4), [255, 255, 255, 255], 0);
}

/// 驗收：填色後擦除一塊，該塊回到 `PAPER_WHITE`。
#[test]
fn erasing_a_filled_region_returns_it_to_paper_white() {
    let pack = support::pack_with(8, 8, vec![0; 64], [255, 255, 255, 255], None);
    let canvas = Canvas::new(&pack);
    settled(&canvas.res, &canvas.gpu, 0, opaque([1.0, 0.0, 0.0]));

    let filled = canvas.render(canvas.frame());
    close(px(&filled, 8, 4, 4), [255, 0, 0, 255], 1);

    support::clear_texture(&canvas.gpu, canvas.res.erase(), wgpu::Color::WHITE);
    let erased = canvas.render(canvas.frame());

    close(px(&erased, 8, 4, 4), [255, 255, 255, 255], 0);
}

#[test]
fn composite_matches_baker_thumb_integer_math() {
    let base: [u8; 3] = [200, 96, 32];
    let shade: [u8; 4] = [128, 200, 255, 255];
    let lineart: [u8; 4] = [180, 255, 64, 255];

    let pack = support::pack_with(8, 8, vec![0; 64], lineart, Some(shade));
    let canvas = Canvas::new(&pack);
    settled(
        &canvas.res,
        &canvas.gpu,
        0,
        opaque([
            f32::from(base[0]) / 255.0,
            f32::from(base[1]) / 255.0,
            f32::from(base[2]) / 255.0,
        ]),
    );

    let out = canvas.render(canvas.frame());

    let expected = std::array::from_fn(|c| {
        if c == 3 {
            return 255;
        }
        let v = u32::from(base[c]) * u32::from(shade[c]) / 255;
        (v * u32::from(lineart[c]) / 255) as u8
    });
    close(px(&out, 8, 4, 4), expected, 2);
}

/// 驗收：無 `shade` 的文件與有 `shade` 的文件走**同一個 pipeline**，輸出正確。
///
/// 缺席時綁的是 1×1 全白 dummy，Multiply 的單位元——所以兩者輸出必須逐 byte 相同。
#[test]
fn absent_shade_is_indistinguishable_from_white_shade() {
    let ids = vec![0; 64];
    let lineart = [200, 200, 200, 255];
    let color = opaque([0.8, 0.4, 0.2]);

    let with = support::pack_with(8, 8, ids.clone(), lineart, Some([255, 255, 255, 255]));
    let without = support::pack_with(8, 8, ids, lineart, None);

    let mut outputs = Vec::new();
    for pack in [with, without] {
        let canvas = Canvas::new(&pack);
        settled(&canvas.res, &canvas.gpu, 0, color);
        outputs.push(canvas.render(canvas.frame()));
    }

    assert_eq!(outputs[0], outputs[1]);
}

/// 驗收：首次填色是白紙淡入，重新填色是交叉淡出，連點兩次不跳變（§5 的三情況表）。
#[test]
fn fill_animation_covers_the_three_cases() {
    // 取樣點是像素 (60, 4)，片段座標落在**中心** (60.5, 4.5)——origin 取同一條
    // 中心線，距離才剛好 60，`smoothstep(36, 60, progress * 128)`。
    let pack = support::pack_with(64, 8, vec![0; 512], [255, 255, 255, 255], None);
    let canvas = Canvas::new(&pack);
    let anim = |progress: f32, prev: [f32; 4]| FillAnim {
        origin: [0.5, 4.5],
        max_radius: 128.0,
        progress,
        prev_color: prev,
    };
    let at = |out: &[u8]| px(out, 64, 60, 4);

    // ① 首次填色：prev 全零（＝白紙）。前緣未到 → 仍是白紙；走完 → 純黑。
    canvas
        .res
        .write_palette(&canvas.gpu, 0, [0.0, 0.0, 0.0, 1.0]);
    canvas.res.write_fill(&canvas.gpu, 0, anim(0.0, [0.0; 4]));
    close(at(&canvas.render(canvas.frame())), [255, 255, 255, 255], 0);

    canvas.res.write_fill(&canvas.gpu, 0, anim(1.0, [0.0; 4]));
    close(at(&canvas.render(canvas.frame())), [0, 0, 0, 255], 1);

    // ② 重新填色：prev 是白色實色。progress 0 時**從舊色起算，不跳變**。
    let white = opaque([1.0, 1.0, 1.0]);
    canvas.res.write_fill(&canvas.gpu, 0, anim(0.0, white));
    close(at(&canvas.render(canvas.frame())), [255, 255, 255, 255], 0);

    // ③ progress 0.375 → x = 48 → smoothstep 正好 0.5 → 舊新各半。
    canvas.res.write_fill(&canvas.gpu, 0, anim(0.375, white));
    close(at(&canvas.render(canvas.frame())), [128, 128, 128, 255], 2);
}

/// 驗收：Mask A／B 即時切換，畫面立即反映，**無 pipeline 重建**。
///
/// 同一個 `CompositePass` 實例渲染兩次，中間只有一次 `MaskBinding::set`
/// （＝一次 `write_buffer`）——測試結構本身就是「不重建」的證明。
#[test]
fn mask_modes_switch_without_rebuilding_the_pipeline() {
    // 左半 region 0、右半 region 1。
    let ids: Vec<u16> = (0..64).map(|i| u16::from(i % 8 >= 4)).collect();
    let pack = support::pack_with(8, 8, ids, [255, 255, 255, 255], None);
    let canvas = Canvas::new(&pack);

    // 進行中的筆畫：整張 `T_wet` coverage = 1，筆刷純紅。Pass 1 歸 `E1-stroke`。
    support::clear_texture(&canvas.gpu, canvas.res.wet(), wgpu::Color::WHITE);
    let frame = Frame {
        brush_color: [1.0, 0.0, 0.0, 1.0],
        ..canvas.frame()
    };

    canvas.mask.set(
        &canvas.gpu,
        MaskUniform {
            mode: MaskMode::Strict as u32,
            active_region_id: 0,
        },
    );
    let strict = canvas.render(frame);
    close(px(&strict, 8, 1, 4), [255, 0, 0, 255], 1);
    // Mode A：非當前區域完全不受筆畫影響。
    close(px(&strict, 8, 6, 4), [255, 255, 255, 255], 0);

    canvas.mask.set(
        &canvas.gpu,
        MaskUniform {
            mode: MaskMode::Loose as u32,
            active_region_id: 0,
        },
    );
    let loose = canvas.render(frame);
    // Mode B 恆回 1.0，完全不遮罩（§6）——兩區都被塗到。
    close(px(&loose, 8, 1, 4), [255, 0, 0, 255], 1);
    close(px(&loose, 8, 6, 4), [255, 0, 0, 255], 1);
}

/// 驗收：畫布外區域顯示背景色，與畫布邊界清晰可辨。
///
/// 同時釘住 `T_line` 的取樣座標是**畫布 UV 而非螢幕 UV**（§3 對 spec 的偏離）：
/// 線稿的明暗交界在畫布 x = 16，letterbox 後應落在螢幕 x = 48，
/// 若誤用螢幕 UV 會跑到螢幕 x = 32。
#[test]
fn outside_the_canvas_shows_the_background_color() {
    let mut pack = support::pack_with(64, 64, vec![0; 4096], [255, 255, 255, 255], None);
    pack.lineart_png = support::split_png(64, 64, 16, [255, 255, 255, 255], [128, 128, 128, 255]);

    let screen = [128, 64];
    let canvas = Canvas::with_screen(&pack, screen);
    let frame = Frame {
        transform: Transform::fit([64, 64], screen),
        ..canvas.frame()
    };

    let out = canvas.render(frame);
    let bg = [64, 64, 64, 255]; // 0.25 → 63.75

    // 畫布佔螢幕 x ∈ [32, 96)，兩側是背景。
    close(px(&out, 128, 4, 32), bg, 1);
    close(px(&out, 128, 120, 32), bg, 1);

    // 螢幕 x = 40 → 畫布 x = 8 → 線稿白側。誤用螢幕 UV 的話這裡會是灰的。
    close(px(&out, 128, 40, 32), [255, 255, 255, 255], 0);
    // 螢幕 x = 88 → 畫布 x = 56 → 線稿灰側。
    close(px(&out, 128, 88, 32), [128, 128, 128, 255], 1);
}

/// `Transform::fit` 是 E1 唯一會用到的 transform，反變換的正確性靠它。
#[test]
fn fit_centres_the_canvas_on_the_long_axis() {
    let t = Transform::fit([64, 64], [128, 64]);

    assert_eq!(t.scale, 1.0);
    assert_eq!((t.tx, t.ty), (32.0, 0.0));
}
