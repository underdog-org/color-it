//! `RenderContext` 的狀態機（`docs/specs/E1-wgpu.md §2`）。
//!
//! surface 本身要真的 `CAMetalLayer` 才建得起來，只能在真機驗；
//! 這裡驗的是「device 與 DocumentResources 的生命週期不跟著 surface 走」（契約 C5）。

mod support;

use render::RenderContext;

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
