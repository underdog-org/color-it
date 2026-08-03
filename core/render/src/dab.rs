//! Pass 1 · Stroke（`docs/specs/E1-stroke.md §7`）。
//!
//! 檔名是 `dab` 而不是 `stroke`：`render` 依賴 `stroke` crate，同名的模組會讓
//! `use stroke::Dab` 在 2018 起的 uniform path 下變成歧義（E0659）。
//!
//! ```text
//! instanced quad × dab_count → T_wet
//! scissor：本 frame 新增 dab 的 bbox（不是整筆的 bbox）
//! ```
//!
//! **自己 submit**，不併進 frame encoder：`MAX_DABS_PER_DRAW` 的分批要各自
//! `write_buffer` 同一條 instance buffer，而 `Queue::write_buffer` 是在 submit 時
//! 才依序落地的——同一次 submit 裡寫兩次，第二批會把第一批蓋掉，於是「畫太快」
//! 變成「只畫得出最後 4096 個 dab」。同一條 queue 上的順序仍然有保證。

use bytemuck::{Pod, Zeroable};
use stroke::{Dab, MAX_DABS_PER_DRAW, TipId};

use crate::bounds::Bounds;
use crate::gpu::Gpu;
use crate::resources::DocumentResources;

/// 筆尖貼圖的邊長，px（`E1-stroke.md §6.1`）。
const TIP_SIZE: u32 = 256;

/// `TipId` 的變體數 ＝ array 的 layer 數。**E1 只有 layer 0 有內容**——
/// 其餘兩層由 [`DabInstance::new`] 的 fallback 保證取不到（`§6`）。
const TIP_LAYERS: u32 = 3;

/// 軟圓筆尖的衰減指數：`coverage = (1 - d)^TIP_FALLOFF`，`d` 是到圓心的正規化距離。
///
/// 1.0 ＝ 線性衰減。E1 初值，**列入 `E1-perf §7` 的調校表**——它直接決定筆跡邊緣
/// 的軟硬，是 D3 盲測「看起來像不像筆」的第一嫌疑人。
pub const TIP_FALLOFF: f32 = 1.0;

/// WGSL 端的 `struct Stroke`。
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct StrokeUniform {
    canvas_size: [f32; 2],
    _pad: [f32; 2],
}

/// `Dab` 的 GPU 版面配置（`E1-stroke.md §14` 決議 G）。
///
/// `stroke::Dab` 刻意沒有 `repr(C)` 也沒有 `bytemuck`——版面配置是 `render` 的事，
/// Boundary 2 才守得住。
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct DabInstance {
    pos: [f32; 2],
    /// **直徑**，px。
    size: f32,
    angle: f32,
    alpha: f32,
    layer: u32,
}

const _: () = assert!(size_of::<DabInstance>() == 24);

impl DabInstance {
    /// 未實作的 tip 一律 fallback 到軟圓筆並記一次 log（`E1-stroke.md §6`）。
    ///
    /// fallback 做在這裡而不是 shader：layer 1／2 因此**在建構上**取不到，
    /// 空貼圖被取樣得到透明筆跡這種靜默失敗沒有發生的餘地。
    pub fn new(dab: &Dab) -> Self {
        let tip = if dab.tip.is_implemented() {
            dab.tip
        } else {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                eprintln!(
                    "[colorlull] tip {:?} 尚未實作（排程 E2），本次筆畫 fallback 到軟圓筆",
                    dab.tip
                );
            });
            TipId::SoftRound
        };

        Self {
            pos: [dab.pos.x, dab.pos.y],
            size: dab.size,
            angle: dab.angle,
            alpha: dab.alpha,
            layer: tip.layer(),
        }
    }
}

pub struct StrokePass {
    /// `build_up == false`：`One / One / Max`，同筆內不疊暗。
    normal: wgpu::RenderPipeline,
    /// `build_up == true`：`OneMinusDst / One / Add`，`over` 累積（噴槍／水彩，E2）。
    build_up: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    canvas_size: [u32; 2],
}

impl StrokePass {
    pub fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stroke"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_stroke"),
            size: size_of::<StrokeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 一批的上限就是這條 buffer 的長度——96 KB，配置一次不隨筆畫成長。
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_dabs"),
            size: (MAX_DABS_PER_DRAW * size_of::<DabInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tip = tip_atlas(gpu);
        let tip_view = tip.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // 貼圖邊緣值為 0，ClampToEdge 於是不會在筆尖外側漏出覆蓋。
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stroke/tip"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stroke"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tip_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stroke"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/dab.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stroke"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let make = |label: &str, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<DabInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32, 4 => Uint32
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::R8Unorm,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::RED,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        // 兩條 pipeline 在建立時就都備好：`build_up` 是 preset 的常數，
        // 換筆刷時重建 pipeline 會在第一筆卡一次 shader 編譯。
        let normal = make(
            "stroke(max)",
            blend(wgpu::BlendFactor::One, wgpu::BlendOperation::Max),
        );
        let build_up = make(
            "stroke(build_up)",
            blend(wgpu::BlendFactor::OneMinusDst, wgpu::BlendOperation::Add),
        );

        Self {
            normal,
            build_up,
            uniform,
            instances,
            bind_group,
            canvas_size: [0, 0],
        }
    }

    /// 換文件才呼叫。畫布尺寸是 vertex shader 的 clip space 換算基準。
    pub fn bind_document(&mut self, gpu: &Gpu, res: &DocumentResources) {
        let [w, h] = res.canvas_size();
        self.canvas_size = [w, h];
        gpu.queue().write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&StrokeUniform {
                canvas_size: [w as f32, h as f32],
                _pad: [0.0; 2],
            }),
        );
    }

    /// 把 `dabs` 畫進 `T_wet`，scissor 至**這一批**的 bbox。
    ///
    /// 呼叫端每 frame 只給新增的 dab（`StrokeBuilder::take_new`）——用整筆 bbox
    /// 會讓 scissor 隨筆畫變長而失去意義（`§7`）。
    ///
    /// 超過 `MAX_DABS_PER_DRAW` 就分批，**不是靜默截斷**：一 frame 內產生上千
    /// dab 的快速長筆畫是正常的，截斷會變成「畫太快就斷線」。
    pub fn draw(&self, gpu: &Gpu, res: &DocumentResources, dabs: &[Dab], build_up: bool) {
        let Some(scissor) = Bounds::of_dabs(dabs).and_then(|b| b.to_scissor(self.canvas_size))
        else {
            return;
        };

        let view = res
            .wet()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let pipeline = if build_up {
            &self.build_up
        } else {
            &self.normal
        };

        for batch in dabs.chunks(MAX_DABS_PER_DRAW) {
            let instances: Vec<DabInstance> = batch.iter().map(DabInstance::new).collect();
            gpu.queue()
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));

            let mut encoder =
                gpu.device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("stroke"),
                    });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("stroke"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // `T_wet` 是累積的：本 frame 之前的 dab 必須留著。
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    multiview_mask: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                let [x, y, w, h] = scissor;
                pass.set_scissor_rect(x, y, w, h);
                pass.draw(0..4, 0..batch.len() as u32);
            }
            gpu.queue().submit([encoder.finish()]);
        }
    }
}

/// 色彩與 alpha 用同一組——target 是 R8Unorm，只有 red 通道會被寫。
fn blend(src: wgpu::BlendFactor, operation: wgpu::BlendOperation) -> wgpu::BlendState {
    let component = wgpu::BlendComponent {
        src_factor: src,
        dst_factor: wgpu::BlendFactor::One,
        operation,
    };
    wgpu::BlendState {
        color: component,
        alpha: component,
    }
}

/// 程序生成的筆尖，**不進 `.colorpack` 也不進 app bundle**（`E1-stroke.md §6.1`）。
///
/// 只填 layer 0（軟圓）。E2 的顆粒／蠟筆紋才需要真的貼圖資產，屆時多填兩層即可，
/// bind group layout 不動。
fn tip_atlas(gpu: &Gpu) -> wgpu::Texture {
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("T_tip"),
        size: wgpu::Extent3d {
            width: TIP_SIZE,
            height: TIP_SIZE,
            depth_or_array_layers: TIP_LAYERS,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    gpu.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: TipId::SoftRound.layer(),
            },
            aspect: wgpu::TextureAspect::All,
        },
        &soft_round(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(TIP_SIZE),
            rows_per_image: Some(TIP_SIZE),
        },
        wgpu::Extent3d {
            width: TIP_SIZE,
            height: TIP_SIZE,
            depth_or_array_layers: 1,
        },
    );
    texture
}

/// 解析式的徑向衰減：`coverage = (1 - d)^TIP_FALLOFF`，圓外為 0。
fn soft_round() -> Vec<u8> {
    let n = TIP_SIZE as f32;
    let mut data = Vec::with_capacity((TIP_SIZE * TIP_SIZE) as usize);
    for y in 0..TIP_SIZE {
        for x in 0..TIP_SIZE {
            // texel 中心，正規化到 ±1。
            let u = (x as f32 + 0.5) / n * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / n * 2.0 - 1.0;
            let d = (u * u + v * v).sqrt();
            let coverage = if d >= 1.0 {
                0.0
            } else {
                (1.0 - d).powf(TIP_FALLOFF)
            };
            data.push((coverage * 255.0).round() as u8);
        }
    }
    data
}
