//! 釘住 `architecture.md §4.1.2` 那句「Draw call 代價：零」。
//!
//! 橡皮擦的 commit 要在**同一個 render pass** 同時寫兩個 attachment：
//!   target 0 = `T_paint`  RGBA8Unorm，destination-out
//!   target 1 = `T_erase`  R8Unorm，   additive
//! 兩者格式不同、blend state 也不同。這在 wgpu／Metal 上能不能成立從未被驗證過。

mod support;

const W: u32 = 4;
const H: u32 = 4;

const SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array(vec2(-1.0, -3.0), vec2(-1.0, 1.0), vec2(3.0, 1.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

struct Out {
    @location(0) paint: vec4<f32>,
    @location(1) erase: vec4<f32>,
}

@fragment
fn fs_main() -> Out {
    var o: Out;
    // destination-out 只看 alpha：0.5 表示「擦掉一半」。
    o.paint = vec4<f32>(0.0, 0.0, 0.0, 0.5);
    // additive 只看 .r。
    o.erase = vec4<f32>(0.5, 0.0, 0.0, 1.0);
    return o;
}
"#;

#[test]
fn mrt_writes_two_formats_with_independent_blend() {
    let gpu = render::Gpu::headless().expect("headless device");
    let device = gpu.device();

    let paint = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("T_paint"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let erase = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("T_erase"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mrt-probe"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mrt-probe"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mrt-probe"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[
                // destination-out：dst × (1 - src.a)
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                // additive：dst + src
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::RED,
                }),
            ],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let paint_view = paint.create_view(&wgpu::TextureViewDescriptor::default());
    let erase_view = erase.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mrt-probe"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &paint_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 先鋪一層不透明紅，才看得出 destination-out 擦掉了多少。
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &erase_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    gpu.queue().submit([encoder.finish()]);

    let paint_px = support::read_texture(&gpu, &paint, 4);
    let erase_px = support::read_texture(&gpu, &erase, 1);

    // 紅 1.0 × (1 - 0.5) = 0.5 → 128（±1 容差留給不同 backend 的捨入）。
    assert!(
        (paint_px[0] as i32 - 128).abs() <= 1,
        "T_paint.r 應被 destination-out 擦成一半，實得 {}",
        paint_px[0]
    );
    assert!(
        (paint_px[3] as i32 - 128).abs() <= 1,
        "T_paint.a 同理，實得 {}",
        paint_px[3]
    );
    // 0 + 0.5 = 0.5 → 128
    assert!(
        (erase_px[0] as i32 - 128).abs() <= 1,
        "T_erase 應被 additive 加到一半，實得 {}",
        erase_px[0]
    );
}
