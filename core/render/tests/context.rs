//! `RenderContext` 的狀態機（`docs/specs/E1-wgpu.md §2`）。
//!
//! surface 本身要真的 `CAMetalLayer` 才建得起來，只能在真機驗；
//! 這裡驗的是「device 與 DocumentResources 的生命週期不跟著 surface 走」（契約 C5）。

mod support;

use render::{FILL_DURATION, Frame, RenderContext, Transform};

/// 已知 pattern：把 `T_paint` 清成不透明紅。
const RED: [u8; 4] = [255, 0, 0, 255];

fn paint_red(ctx: &RenderContext) {
    let gpu = ctx.gpu().expect("gpu");
    let view = ctx
        .resources()
        .expect("resources")
        .paint()
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("paint red"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    gpu.queue().submit([encoder.finish()]);
}

#[test]
fn new_does_not_touch_the_gpu() {
    let ctx = RenderContext::new();

    assert!(ctx.gpu().is_none());
    assert!(ctx.resources().is_none());
}

#[test]
fn detach_keeps_device_and_document_resources() {
    let pack = support::pack(4, 2, vec![0; 8], false);
    let mut ctx = RenderContext::new();
    ctx.prepare_document(&pack).expect("prepare");
    paint_red(&ctx);

    ctx.detach_surface();

    let gpu = ctx.gpu().expect("detach 之後 device 仍在");
    let resources = ctx.resources().expect("detach 之後資源仍在");
    let painted = support::read_texture(gpu, resources.paint(), 4);
    assert_eq!(&painted[..4], RED);
}

/// `Effect::Filled` → GPU 的三件事一次做完（`E1-bucket §3`）：`Buf_palette`、
/// `T_erase`、擴散動畫。少做一件都會在畫面上看得出來。
#[test]
fn fill_writes_palette_clears_erase_and_starts_the_animation() {
    let pack = support::pack(4, 2, vec![0; 8], false);
    let mut ctx = RenderContext::new();
    ctx.prepare_document(&pack).expect("prepare");
    support::clear_texture(
        ctx.gpu().expect("gpu"),
        ctx.resources().expect("resources").erase(),
        wgpu::Color::WHITE,
    );

    ctx.fill(0, RED, [0; 4], [0, 0, 4, 2], [1.0, 1.0]);

    let gpu = ctx.gpu().expect("gpu");
    let res = ctx.resources().expect("resources");
    // palette 是編碼值的 f32，alpha 恆為 1.0（§5）。
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&support::read_buffer(gpu, res.palette())[..16]),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert!(
        support::read_texture(gpu, res.erase(), 1)
            .iter()
            .all(|&v| v == 0)
    );
    assert!(ctx.is_animating());
}

/// `render_with_dt` 是真正的實作，`render` 是拿內部 `Instant` 的 wrapper（§7.2）——
/// 測試走前者，不需要 mock 時鐘。沒有 surface 也要推進動畫。
#[test]
fn render_with_dt_advances_the_animation_without_a_surface() {
    let pack = support::pack(4, 2, vec![0; 8], false);
    let mut ctx = RenderContext::new();
    ctx.prepare_document(&pack).expect("prepare");
    ctx.fill(0, RED, [0; 4], [0, 0, 4, 2], [1.0, 1.0]);

    let frame = Frame {
        transform: Transform::fit([4, 2], [4, 2]),
        screen_size: [4, 2],
        background: [0.25, 0.25, 0.25, 1.0],
        brush_color: [0.0; 4],
    };
    ctx.render_with_dt(frame, FILL_DURATION / 2.0)
        .expect("frame");
    assert!(ctx.is_animating());

    ctx.render_with_dt(frame, FILL_DURATION / 2.0)
        .expect("frame");
    assert!(!ctx.is_animating());
}

#[test]
fn preparing_the_same_document_twice_reuses_the_resources() {
    let pack = support::pack(4, 2, vec![0; 8], false);
    let mut ctx = RenderContext::new();
    ctx.prepare_document(&pack).expect("first prepare");
    paint_red(&ctx);

    // 二次 attach 走的是同一條路：資源沿用，畫作不能消失。
    ctx.detach_surface();
    ctx.prepare_document(&pack).expect("second prepare");

    let gpu = ctx.gpu().expect("gpu");
    let resources = ctx.resources().expect("resources");
    let painted = support::read_texture(gpu, resources.paint(), 4);
    assert_eq!(&painted[..4], RED);
}
