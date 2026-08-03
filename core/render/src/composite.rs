//! Pass 3 · Composite（`docs/specs/E1-composite.md`）。
//!
//! 每 frame 一個 full-screen triangle，把七個資源合成到 target。
//! **成本由螢幕解析度決定，不是畫布解析度**（§7）——rasterize 的是 surface 的像素。
//!
//! Bind group 分三組：
//!
//! - 0 = 文件資源（換文件才重建）
//! - 1 = mask（`MaskBinding`，與 Pass 2 共用，切 mode 只是一次 `write_buffer`）
//! - 2 = frame uniform（每 frame 更新）

use bytemuck::{Pod, Zeroable};

use crate::gpu::Gpu;
use crate::mask::MaskBinding;
use crate::resources::DocumentResources;

/// 畫布 transform（`core/engine/src/ffi.rs` 的同名型別，S0 已定）。
///
/// E1 的 `scale` 恆為 fit-to-screen，**縮放平移是 E2**——但反變換現在就寫對。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub scale: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Transform {
    /// 等比縮放至畫布完整可見並置中。E1 唯一會用到的 transform。
    pub fn fit(canvas: [u32; 2], screen: [u32; 2]) -> Self {
        let (cw, ch) = (canvas[0] as f32, canvas[1] as f32);
        let (sw, sh) = (screen[0] as f32, screen[1] as f32);
        let scale = (sw / cw).min(sh / ch);
        Self {
            scale,
            tx: (sw - cw * scale) / 2.0,
            ty: (sh - ch * scale) / 2.0,
        }
    }
}

/// 每 frame 更新的 composite 參數。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub transform: Transform,
    pub screen_size: [u32; 2],
    /// 畫布外的背景色，編碼值 RGBA。**不要用 `PAPER_WHITE`**——
    /// 否則使用者分不出畫布邊界在哪（§4）。由 `set_viewport` 一併帶入。
    pub background: [f32; 4],
    /// 進行中筆畫的顏色，編碼值 RGBA、straight alpha。
    ///
    /// 暫住 composite 的 uniform：`E1-wgpu §7.1` 只定了 mask，而第 ④ 層需要它。
    /// Pass 2 也要同一個值時再提升成共用 group。
    pub brush_color: [f32; 4],
}

/// WGSL 端 `struct Frame`。uniform address space 的對齊要求由 `_pad` 與欄位順序滿足。
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct FrameUniform {
    scale: f32,
    tx: f32,
    ty: f32,
    _pad: f32,
    screen_size: [f32; 2],
    canvas_size: [f32; 2],
    background: [f32; 4],
    brush_color: [f32; 4],
}

const _: () = assert!(size_of::<FrameUniform>() == 64);

pub struct CompositePass {
    pipeline: wgpu::RenderPipeline,
    doc_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    doc_bind_group: Option<wgpu::BindGroup>,
    canvas_size: [f32; 2],
}

impl CompositePass {
    /// `target_format` 讓 offscreen 測試能用 `Rgba8Unorm`，實機走 `SURFACE_FORMAT`。
    /// 兩者都是非 sRGB 變體——硬體不做任何 decode／encode（§2）。
    pub fn new(gpu: &Gpu, target_format: wgpu::TextureFormat, mask: &MaskBinding) -> Self {
        let device = gpu.device();

        let doc_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite/document"),
            entries: &[
                texture_entry(0, wgpu::TextureSampleType::Uint),
                texture_entry(1, filterable()),
                texture_entry(2, filterable()),
                texture_entry(3, filterable()),
                texture_entry(4, filterable()),
                texture_entry(5, filterable()),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                storage_entry(7),
                storage_entry(8),
            ],
        });

        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite/frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_frame"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite/frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

        // T_line／T_shade 的 linear filter。ClampToEdge 讓缺席時綁的 1×1 白色
        // dummy 在任何 UV 都取到白（Multiply 的單位元）。
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("composite/linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/composite.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite"),
            bind_group_layouts: &[Some(&doc_layout), Some(mask.layout()), Some(&frame_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                // Full-screen triangle：不綁 vertex buffer。
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    // composite 是最底層，輸出即最終值——不需要 blend。
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // 三角形頂點順序由 vertex_index 產生，不保證繞向。
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
            doc_layout,
            sampler,
            frame_buffer,
            frame_bind_group,
            doc_bind_group: None,
            canvas_size: [0.0, 0.0],
        }
    }

    /// 換文件才呼叫。Bind group 裡的貼圖與 buffer 在文件生命週期內都不換。
    pub fn bind_document(&mut self, gpu: &Gpu, res: &DocumentResources) {
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let (region, erase, paint, wet, shade, line) = (
            view(res.region()),
            view(res.erase()),
            view(res.paint()),
            view(res.wet()),
            view(res.shade()),
            view(res.line()),
        );

        self.doc_bind_group = Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite/document"),
            layout: &self.doc_layout,
            entries: &[
                bind(0, wgpu::BindingResource::TextureView(&region)),
                bind(1, wgpu::BindingResource::TextureView(&erase)),
                bind(2, wgpu::BindingResource::TextureView(&paint)),
                bind(3, wgpu::BindingResource::TextureView(&wet)),
                bind(4, wgpu::BindingResource::TextureView(&shade)),
                bind(5, wgpu::BindingResource::TextureView(&line)),
                bind(6, wgpu::BindingResource::Sampler(&self.sampler)),
                bind(7, res.palette().as_entire_binding()),
                bind(8, res.fill().as_entire_binding()),
            ],
        }));

        let [w, h] = res.canvas_size();
        self.canvas_size = [w as f32, h as f32];
    }

    /// `set_viewport`（§4）。畫布尺寸來自已綁定的文件，呼叫端不用重複提供。
    pub fn set_frame(&self, gpu: &Gpu, frame: Frame) {
        let uniform = FrameUniform {
            scale: frame.transform.scale,
            tx: frame.transform.tx,
            ty: frame.transform.ty,
            _pad: 0.0,
            screen_size: [frame.screen_size[0] as f32, frame.screen_size[1] as f32],
            canvas_size: self.canvas_size,
            background: frame.background,
            brush_color: frame.brush_color,
        };
        gpu.queue()
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// 沒有 `bind_document` 過就是 no-op——`attach_surface` 前的 frame 不該炸。
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        mask: &MaskBinding,
    ) {
        let Some(doc) = self.doc_bind_group.as_ref() else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // full-screen triangle 覆蓋每一個片段，clear 只是浪費頻寬。
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
        pass.set_bind_group(0, doc, &[]);
        pass.set_bind_group(1, mask.bind_group(), &[]);
        pass.set_bind_group(2, &self.frame_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn filterable() -> wgpu::TextureSampleType {
    wgpu::TextureSampleType::Float { filterable: true }
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

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bind(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}
