//! `Fill` 清 `T_erase`（`docs/specs/E1-bucket.md §6`）。
//!
//! 資源矩陣裡 `Fill` 那一列的第二格：讀 `T_region`、clear `T_erase`。
//! 一個 scissor 至 region bbox 的小 render pass，成本與畫布大小無關。

use crate::gpu::Gpu;
use crate::resources::DocumentResources;

/// WGSL 端的 `vec4<u32>`：`.x` 是 region ID，其餘三格只為了 uniform 的 16-byte 對齊。
type ActiveRegion = [u32; 4];

pub struct ErasePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    target: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
}

impl ErasePass {
    pub fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("erase/document"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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

        let target = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_erase_target"),
            size: size_of::<ActiveRegion>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("erase_clear"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/erase_clear.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("erase_clear"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("erase_clear"),
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
                    format: wgpu::TextureFormat::R8Unorm,
                    // 寫的是絕對值 0.0，不是與現值混合。
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
            layout,
            target,
            bind_group: None,
        }
    }

    /// 換文件才呼叫。
    pub fn bind_document(&mut self, gpu: &Gpu, res: &DocumentResources) {
        let region = res
            .region()
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.bind_group = Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("erase/document"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&region),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.target.as_entire_binding(),
                },
            ],
        }));
    }

    /// 把 `region_id` 那一區的 `T_erase` 歸零。`bbox` 是 `[x, y, w, h]`。
    ///
    /// 自己 submit：油漆桶不在 frame 迴圈裡，等下一次 `render` 才生效的話，
    /// 擴散動畫的第一 frame 會看到還沒清乾淨的 `T_erase`。
    pub fn clear_region(&self, gpu: &Gpu, res: &DocumentResources, region_id: u32, bbox: [u32; 4]) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };

        let [cw, ch] = res.canvas_size();
        let [x, y, w, h] = bbox;
        // baker 的 bbox 應當落在畫布內，但夾一次比在驅動層炸掉便宜。
        let (w, h) = (w.min(cw.saturating_sub(x)), h.min(ch.saturating_sub(y)));
        if w == 0 || h == 0 {
            return;
        }

        gpu.queue()
            .write_buffer(&self.target, 0, bytemuck::bytes_of(&[region_id, 0, 0, 0]));

        let view = res
            .erase()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("erase_clear"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("erase_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // bbox 外與 bbox 內的其他區域都必須保留原值。
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
            pass.set_scissor_rect(x, y, w, h);
            pass.draw(0..3, 0..1);
        }

        gpu.queue().submit([encoder.finish()]);
    }
}
