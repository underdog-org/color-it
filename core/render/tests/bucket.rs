//! 油漆桶的 GPU 側驗收（`docs/specs/E1-bucket.md §10`）。
//!
//! `document` 那一半在 `core/document/tests/apply.rs`——那份不需要 GPU。

mod support;

use std::time::Instant;

use colorpack::ColorPack;
use render::{
    CompositePass, DocumentResources, ErasePass, FILL_ANIM_SIZE, FILL_DURATION, Fill, FillAnim,
    FillAnimator, Gpu, MaskBinding, MaskMode, MaskUniform, Transform, ease_out,
};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const NEVER_FILLED: [f32; 4] = [0.0; 4];

fn gpu_and_res(pack: &ColorPack) -> (Gpu, DocumentResources) {
    let gpu = Gpu::headless().expect("headless device");
    let res = DocumentResources::new(&gpu, pack).expect("resources");
    (gpu, res)
}

fn fill_entry(gpu: &Gpu, res: &DocumentResources, region_id: u32) -> FillAnim {
    let bytes = support::read_buffer(gpu, res.fill());
    let start = (u64::from(region_id) * FILL_ANIM_SIZE) as usize;
    *bytemuck::from_bytes(&bytes[start..start + FILL_ANIM_SIZE as usize])
}

#[track_caller]
fn close4(actual: [f32; 4], expected: [f32; 4]) {
    let ok = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| (a - e).abs() <= 1.0e-5);
    assert!(ok, "得到 {actual:?}，預期 {expected:?}");
}

// ---------------------------------------------------------------------------
// §4 · tap → region ID
// ---------------------------------------------------------------------------

/// §10：Rust 的 `canvas_pos` 與 shader 的同名函式逐點相等。
///
/// 兩者不可能共用程式碼（一個 Rust 一個 WGSL），所以用整張畫面釘住：每個 region
/// 給一個唯一的 palette 顏色，於是每個螢幕像素的輸出顏色就說出 shader 讀了哪一個
/// 畫布 texel。漂移的症狀是「點到隔壁區」——E2 上縮放平移之後才浮出來，
/// 而那時它看起來像手感問題。
#[test]
fn canvas_pos_matches_the_shader_per_pixel() {
    let (cw, ch) = (8_u32, 8_u32);
    // 每個畫布像素自成一區，於是「讀了哪個 texel」＝「輸出哪個顏色」。
    let ids: Vec<u16> = (0..(cw * ch) as u16).collect();
    // 線稿與 shade 全白：Multiply 的單位元，第 ⑤⑥ 層不改變顏色。
    let pack = support::pack_with(cw, ch, ids.clone(), [255; 4], Some([255; 4]));

    let gpu = Gpu::headless().expect("headless device");
    let res = DocumentResources::new(&gpu, &pack).expect("resources");
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

    for id in 0..(cw * ch) {
        // R 通道 = id * 4：64 區剛好落在 0..=252，u8 來回不損失。
        res.write_palette(&gpu, id, [(id * 4) as f32 / 255.0, 0.0, 0.0, 1.0]);
        res.write_fill(
            &gpu,
            id,
            FillAnim {
                origin: [0.0, 0.0],
                max_radius: 1.0e6,
                progress: 1.0,
                prev_color: NEVER_FILLED,
            },
        );
    }

    // 刻意 letterbox：scale 1.5、tx 4、ty 0。螢幕與畫布同尺寸的話這條測試等於沒測。
    let screen = [20_u32, 12_u32];
    let transform = Transform::fit([cw, ch], screen);
    let background = [1.0, 0.0, 1.0, 1.0];

    let target = support::offscreen(&gpu, screen[0], screen[1]);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    pass.set_frame(
        &gpu,
        render::Frame {
            transform,
            screen_size: screen,
            background,
            brush_color: [0.0; 4],
        },
    );
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    pass.draw(&mut encoder, &view, &mask);
    gpu.queue().submit([encoder.finish()]);
    let pixels = support::read_texture(&gpu, &target, 4);

    for sy in 0..screen[1] {
        for sx in 0..screen[0] {
            // fragment 的取樣點是像素中心。
            let center = [sx as f32 + 0.5, sy as f32 + 0.5];
            let expected = match res.region_at(transform.canvas_pos(center)) {
                Some(id) => [(id * 4) as u8, 0, 0, 255],
                None => [255, 0, 255, 255],
            };
            let i = ((sy * screen[0] + sx) * 4) as usize;
            let actual = [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]];
            assert_eq!(actual, expected, "螢幕像素 ({sx}, {sy})");
        }
    }
}

/// §10：畫布外的 tap 不改變任何狀態（`Buf_palette` 逐位元不變）。
#[test]
fn taps_outside_the_canvas_have_no_region() {
    let pack = support::pack(4, 4, vec![0; 16], false);
    let (gpu, res) = gpu_and_res(&pack);
    let before = support::read_buffer(&gpu, res.palette());

    // 不 clamp——clamp 會讓畫布外的誤觸填到邊緣區域（§4.3）。
    for outside in [
        [-0.5, 2.0],
        [2.0, -0.001],
        [4.0, 2.0],
        [2.0, 4.0],
        [f32::NAN, 2.0],
        [f32::INFINITY, 2.0],
    ] {
        assert_eq!(res.region_at(outside), None, "{outside:?}");
    }
    assert_eq!(res.region_at([0.0, 0.0]), Some(0));
    assert_eq!(res.region_at([3.999, 3.999]), Some(0));

    assert_eq!(support::read_buffer(&gpu, res.palette()), before);
}

/// 螢幕像素 → 畫布像素的整條路徑，含 letterbox 的兩側黑邊。
#[test]
fn screen_pixels_outside_the_letterbox_have_no_region() {
    let pack = support::pack(8, 8, (0..64).collect(), false);
    let (_gpu, res) = gpu_and_res(&pack);
    let transform = Transform::fit([8, 8], [20, 12]);

    // scale 1.5、tx 4：x < 4 與 x >= 16 的螢幕像素落在畫布外。
    assert_eq!(res.region_at(transform.canvas_pos([3.5, 6.0])), None);
    assert_eq!(res.region_at(transform.canvas_pos([16.5, 6.0])), None);
    assert_eq!(res.region_at(transform.canvas_pos([4.5, 0.5])), Some(0));
}

// ---------------------------------------------------------------------------
// §6 · Fill 清 T_erase
// ---------------------------------------------------------------------------

/// §10：`Fill` 之後該區域的 `T_erase` 為 0，測試預先注入非零 pattern。
///
/// bbox 刻意給成整張畫布——這樣通過就代表把關的是 `T_region` 的比對而不只是 scissor。
/// E1 的可觀測效果是零（`T_erase` 全程恆為 0），橡皮擦是 E2。
#[test]
fn fill_clears_erase_inside_the_region_only() {
    let (w, h) = (8_u32, 4_u32);
    let ids: Vec<u16> = (0..w * h).map(|i| u16::from(i % w >= w / 2)).collect();
    let pack = support::pack(w, h, ids, false);
    let (gpu, res) = gpu_and_res(&pack);

    let mut erase = ErasePass::new(&gpu);
    erase.bind_document(&gpu, &res);
    support::clear_texture(&gpu, res.erase(), wgpu::Color::WHITE);

    erase.clear_region(&gpu, &res, 0, [0, 0, w, h]);

    let pixels = support::read_texture(&gpu, res.erase(), 1);
    for y in 0..h {
        for x in 0..w {
            let expected = if x < w / 2 { 0 } else { 255 };
            assert_eq!(pixels[(y * w + x) as usize], expected, "({x}, {y})");
        }
    }
}

/// scissor 的那一半：bbox 外的同區像素不該被碰到。
///
/// 現實中不會發生（baker 的 bbox 涵蓋整個區域），但 scissor 沒設對的話這條會綠，
/// 而效能上的差別要到大畫布才看得出來。
#[test]
fn erase_clear_is_scissored_to_the_bbox() {
    let (w, h) = (8_u32, 4_u32);
    let pack = support::pack(w, h, vec![0; (w * h) as usize], false);
    let (gpu, res) = gpu_and_res(&pack);

    let mut erase = ErasePass::new(&gpu);
    erase.bind_document(&gpu, &res);
    support::clear_texture(&gpu, res.erase(), wgpu::Color::WHITE);

    erase.clear_region(&gpu, &res, 0, [2, 1, 3, 2]);

    let pixels = support::read_texture(&gpu, res.erase(), 1);
    for y in 0..h {
        for x in 0..w {
            let inside = (2..5).contains(&x) && (1..3).contains(&y);
            let expected = if inside { 0 } else { 255 };
            assert_eq!(pixels[(y * w + x) as usize], expected, "({x}, {y})");
        }
    }
}

// ---------------------------------------------------------------------------
// §7 · 擴散動畫的 CPU 推進
// ---------------------------------------------------------------------------

/// §10：`max_radius` 足以覆蓋整個 bbox——origin 取 bbox 四角逐一驗。
///
/// bbox 對角線在 origin 靠近角落時**不夠大**，擴散會在動畫結束時仍未覆蓋對角的
/// 另一端，視覺上是「填到一半就停了」（§7.4）。
#[test]
fn max_radius_reaches_every_bbox_corner() {
    let pack = support::pack(4, 4, vec![0; 16], false);
    let (gpu, res) = gpu_and_res(&pack);
    let mut anim = FillAnimator::new();

    let bbox = [10_u32, 20, 30, 40];
    let [bx, by, bw, bh] = bbox.map(|v| v as f32);
    let corners = [[bx, by], [bx + bw, by], [bx, by + bh], [bx + bw, by + bh]];
    let diagonal = bw.hypot(bh);

    for origin in corners {
        anim.begin(
            &gpu,
            &res,
            Fill {
                region_id: 0,
                origin,
                bbox,
                color: RED,
                prev: NEVER_FILLED,
            },
        );
        let max_radius = fill_entry(&gpu, &res, 0).max_radius;

        for corner in corners {
            let d = (corner[0] - origin[0]).hypot(corner[1] - origin[1]);
            assert!(
                max_radius >= d - 1.0e-3,
                "origin {origin:?} 的 max_radius {max_radius} 構不到角 {corner:?}（距離 {d}）"
            );
        }
        // 四角出發時最遠的角就是對角線，取對角線的一半（常見的寫法）會少一半。
        assert!((max_radius - diagonal).abs() < 1.0e-3);
    }
}

/// §7.3：ease-out cubic，180 ms。到 1 之後停止寫入。
#[test]
fn progress_follows_ease_out_cubic_and_settles_after_the_duration() {
    let pack = support::pack(4, 4, vec![0; 16], false);
    let (gpu, res) = gpu_and_res(&pack);
    let mut anim = FillAnimator::new();

    anim.begin(
        &gpu,
        &res,
        Fill {
            region_id: 0,
            origin: [1.0, 1.0],
            bbox: [0, 0, 4, 4],
            color: RED,
            prev: NEVER_FILLED,
        },
    );
    assert_eq!(fill_entry(&gpu, &res, 0).progress, 0.0);
    assert!(anim.is_animating());

    anim.advance(&gpu, &res, FILL_DURATION / 2.0);
    // p = 1 - (1 - 0.5)³ = 0.875，不是 0.5——線性曲線會在收尾處拖得太慢。
    assert!((fill_entry(&gpu, &res, 0).progress - 0.875).abs() < 1.0e-5);

    anim.advance(&gpu, &res, FILL_DURATION / 2.0);
    assert_eq!(fill_entry(&gpu, &res, 0).progress, 1.0);
    // 停止推進之後那一筆停在 progress == 1，shader 從此永遠算出目標色。
    assert!(!anim.is_animating());
}

/// §10：連點同一區兩次不同色，第二次動畫從**當前畫面顏色**起算。
#[test]
fn repeat_tap_resumes_from_the_on_screen_colour() {
    let pack = support::pack(4, 4, vec![0; 16], false);
    let (gpu, res) = gpu_and_res(&pack);
    let mut anim = FillAnimator::new();

    let fill = |color, prev| Fill {
        region_id: 0,
        origin: [1.0, 1.0],
        bbox: [0, 0, 4, 4],
        color,
        prev,
    };

    anim.begin(&gpu, &res, fill(RED, NEVER_FILLED));
    anim.advance(&gpu, &res, FILL_DURATION / 2.0);

    // 第二筆的起點不是舊的 palette 值，是此刻插值到一半的顏色——
    // 取 `palette[id]` 的話第二次動畫會從紅色跳變起算（§7.5）。
    anim.begin(&gpu, &res, fill(BLUE, RED));
    let t = ease_out(0.5);
    close4(
        fill_entry(&gpu, &res, 0).prev_color,
        [t, 0.0, 0.0, t], // mix(全零, RED, 0.875)
    );
}

/// 動畫已結束時 §7.5 的式子退化成舊 palette，不需要分支。
#[test]
fn repeat_tap_after_the_animation_starts_from_the_settled_colour() {
    let pack = support::pack(4, 4, vec![0; 16], false);
    let (gpu, res) = gpu_and_res(&pack);
    let mut anim = FillAnimator::new();

    let fill = |color, prev| Fill {
        region_id: 0,
        origin: [1.0, 1.0],
        bbox: [0, 0, 4, 4],
        color,
        prev,
    };

    anim.begin(&gpu, &res, fill(RED, NEVER_FILLED));
    anim.advance(&gpu, &res, FILL_DURATION);
    anim.begin(&gpu, &res, fill(BLUE, RED));

    close4(fill_entry(&gpu, &res, 0).prev_color, RED);
}

/// 多筆同時進行各自獨立推進——180 ms 內人手點得出來的筆數是個位數，不做批次上傳。
#[test]
fn concurrent_fills_advance_independently() {
    let pack = support::pack(
        4,
        4,
        vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        false,
    );
    let (gpu, res) = gpu_and_res(&pack);
    let mut anim = FillAnimator::new();

    anim.begin(
        &gpu,
        &res,
        Fill {
            region_id: 0,
            origin: [0.0, 0.0],
            bbox: [0, 0, 4, 4],
            color: RED,
            prev: NEVER_FILLED,
        },
    );
    anim.advance(&gpu, &res, FILL_DURATION / 2.0);
    anim.begin(
        &gpu,
        &res,
        Fill {
            region_id: 1,
            origin: [3.0, 3.0],
            bbox: [0, 0, 4, 4],
            color: BLUE,
            prev: NEVER_FILLED,
        },
    );
    anim.advance(&gpu, &res, FILL_DURATION / 2.0);

    assert_eq!(fill_entry(&gpu, &res, 0).progress, 1.0);
    assert!((fill_entry(&gpu, &res, 1).progress - 0.875).abs() < 1.0e-5);
    assert!(anim.is_animating());
}

// ---------------------------------------------------------------------------
// §10 · O(1)
// ---------------------------------------------------------------------------

/// §10：65535 區的合成文件，`tap` 的耗時與區域數無關。
///
/// 上界抓得很寬——這是「有沒有人偷偷加了泛洪填充」的煙霧警報，不是 benchmark。
/// 泛洪一次要掃過整個區域，差距是好幾個數量級。
#[test]
fn tap_cost_is_independent_of_region_count() {
    let (w, h) = (256_u32, 256_u32);
    // R16Uint 的上限：ID 0..=65534 加上最後一格重用 0，湊滿 65535 區。
    let ids: Vec<u16> = (0..w * h).map(|i| (i % 65535) as u16).collect();
    let pack = support::pack(w, h, ids, false);
    let (_gpu, res) = gpu_and_res(&pack);
    assert_eq!(pack.manifest.region_count, 65535);

    let transform = Transform::fit([w, h], [512, 512]);
    let mut hits = 0_u64;

    let start = Instant::now();
    for i in 0..100_000 {
        let p = transform.canvas_pos([(i % 512) as f32 + 0.5, (i % 511) as f32 + 0.5]);
        hits += u64::from(res.region_at(p).unwrap_or(0));
    }
    let elapsed = start.elapsed();

    assert!(hits > 0);
    assert!(elapsed.as_millis() < 500, "10 萬次 tap 花了 {elapsed:?}");
}
