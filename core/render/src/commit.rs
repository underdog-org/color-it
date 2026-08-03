//! Pass 2 · Commit，路徑 (a)（`docs/specs/E1-stroke.md §8`）。
//!
//! 抬筆時一次：`T_wet × opacity × mask` → `T_paint`（premultiplied over），
//! 然後清掉 `T_wet`。兩步都 scissor 至**整筆** bbox。
//!
//! E2 才補的 (b) ping-pong `T_bg`、(c) MRT 寫 `T_erase`、(d) `edge_boost`
//! 都不在這裡——E1 只實作 (a)（`E1-wgpu §7`）。

use bytemuck::{Pod, Zeroable};

use crate::gpu::Gpu;
use crate::mask::MaskBinding;
use crate::resources::DocumentResources;

/// WGSL 端的 `struct Commit`。
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct CommitUniform {
    color: [f32; 4],
    opacity: f32,
    _pad: [f32; 3],
}

pub struct CommitPass {
    pipeline: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
}

impl CommitPass {
    pub fn new(gpu: &Gpu, mask: &MaskBinding) -> Self {
        let device = gpu.device();

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("commit/document"),
            entries: &[
                texture_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry(1, wgpu::TextureSampleType::Uint),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_commit"),
            size: size_of::<CommitUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("commit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/commit.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("commit"),
            bind_group_layouts: &[Some(&layout), Some(mask.layout())],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("commit"),
            layout: Some(&pipeline_layout),
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    // premultiplied over：`T_paint` 是 read-modify-write，走硬體 blend，
                    // 不是 shader 讀寫同一張圖（`E1-wgpu §7` 第 2 條）。
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let clear_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wet_clear"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/wet_clear.wgsl").into()),
        });
        let clear_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wet_clear"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let clear = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wet_clear"),
            layout: Some(&clear_layout),
            vertex: wgpu::VertexState {
                module: &clear_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &clear_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::RED,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            clear,
            layout,
            uniform,
            bind_group: None,
        }
    }

    /// 換文件才呼叫。
    pub fn bind_document(&mut self, gpu: &Gpu, res: &DocumentResources) {
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let (wet, region) = (view(res.wet()), view(res.region()));

        self.bind_group = Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("commit/document"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&wet),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&region),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        }));
    }

    /// 抬筆：`T_wet` → `T_paint`，然後清 `T_wet`。`bbox` 是整筆的 `[x, y, w, h]`。
    ///
    /// `color` 是編碼值 straight alpha，`.a` 不參與——整筆濃度由 `opacity` 決定
    /// （`Tool::Brush.opacity` 覆寫值，`None` 時取 `preset.opacity`）。
    ///
    /// 自己 submit：抬筆不在 frame 迴圈裡，而 `clear_wet` 必須排在 commit 之後。
    pub fn commit(
        &self,
        gpu: &Gpu,
        res: &DocumentResources,
        mask: &MaskBinding,
        color: [f32; 4],
        opacity: f32,
        bbox: [u32; 4],
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let Some([x, y, w, h]) = clamp_bbox(res, bbox) else {
            return;
        };

        gpu.queue().write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&CommitUniform {
                color,
                opacity,
                _pad: [0.0; 3],
            }),
        );

        let view = res
            .paint()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("commit"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("commit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 之前的筆畫必須留著——這一筆是疊上去的。
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_bind_group(1, mask.bind_group(), &[]);
            pass.set_scissor_rect(x, y, w, h);
            pass.draw(0..3, 0..1);
        }
        gpu.queue().submit([encoder.finish()]);

        self.clear_wet(gpu, res, bbox);
    }

    /// 清 `T_wet`，scissor 至 `bbox`（`§8.1` 第 1 步）。
    ///
    /// `cancel_stroke` 也走這一支：palm rejection 事後判定失敗時直接清掉，
    /// **`T_paint` 從未被污染**——進行中的筆畫依定義不是持久狀態。
    pub fn clear_wet(&self, gpu: &Gpu, res: &DocumentResources, bbox: [u32; 4]) {
        let Some([x, y, w, h]) = clamp_bbox(res, bbox) else {
            return;
        };

        let view = res
            .wet()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wet_clear"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wet_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // bbox 外必須保留：同時有兩筆在跑不是 E1 的情境，但
                        // `LoadOp::Clear` 會讓 scissor 變成裝飾品。
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.clear);
            pass.set_scissor_rect(x, y, w, h);
            pass.draw(0..3, 0..1);
        }
        gpu.queue().submit([encoder.finish()]);
    }
}

/// 夾一次比在驅動層炸掉便宜——`bbox` 來自 dab 的包絡，畫布外的筆畫是常態。
fn clamp_bbox(res: &DocumentResources, bbox: [u32; 4]) -> Option<[u32; 4]> {
    let [cw, ch] = res.canvas_size();
    let [x, y, w, h] = bbox;
    if x >= cw || y >= ch {
        return None;
    }
    let (w, h) = (w.min(cw - x), h.min(ch - y));
    (w > 0 && h > 0).then_some([x, y, w, h])
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
