//! Pass 1 · Stroke（`docs/specs/E1-stroke.md §7`）。
//!

use bytemuck::{Pod, Zeroable};
use stroke::{Dab, MAX_DABS_PER_DRAW, TipId};

use crate::bounds::Bounds;
use crate::gpu::Gpu;
use crate::resources::DocumentResources;

/// 筆尖貼圖的邊長，px（`E1-stroke.md §6.1`）。
const TIP_SIZE: u32 = 256;

/// `TipId` 的變體數 ＝ array 的 layer 數。三層都有內容。
const TIP_LAYERS: u32 = 3;

/// 軟圓筆尖的衰減指數：`coverage = (1 - d)^TIP_FALLOFF`，`d` 是到圓心的正規化距離。
///
/// 1.0 ＝ 線性衰減。**列入調校表**——它直接決定筆跡邊緣的軟硬，是盲測
pub const TIP_FALLOFF: f32 = 1.0;

/// 硬圓的邊緣過渡寬度，佔半徑的比例。
const HARD_EDGE: f32 = 0.10;
const GRAIN_FREQ: u32 = 14;
const GRAIN_CONTRAST: f32 = 2.2;
const GRAIN_SEED: u32 = 0x9e37;

/// WGSL 端的 `struct Stroke`。
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct StrokeUniform {
    canvas_size: [f32; 2],
    _pad: [f32; 2],
}

/// `Dab` 的 GPU 版面配置
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
    /// **沒有 fallback**：`tip_atlas` 填滿 `TIP_LAYERS` 層，缺層是建置期的錯，
    pub fn new(dab: &Dab) -> Self {
        Self {
            pos: [dab.pos.x, dab.pos.y],
            size: dab.size,
            angle: dab.angle,
            alpha: dab.alpha,
            layer: dab.tip.layer(),
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

/// 三張程序生成的筆尖，**不進 `.colorpack`、不進 `assets/`、不進 app bundle**
/// （`E1-stroke.md §6.1`）。它們是程式碼常數，不是文件資產。
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

    // 每個變體各寫一層。**這裡漏一層不會有 fallback 接住**（`DabInstance::new`），
    // 所以是 match 而不是清單——加了 `TipId` 卻忘了生成器，編譯就會擋下來。
    for tip in [TipId::SoftRound, TipId::HardRound, TipId::Grain] {
        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: tip.layer(),
                },
                aspect: wgpu::TextureAspect::All,
            },
            &tip_texels(tip),
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
    }
    texture
}

/// 一層的 R8 覆蓋率。純函式、零 GPU 依賴，所以三張 tip 的形狀在無 GPU 的 CI 上驗得到。
fn tip_texels(tip: TipId) -> Vec<u8> {
    let n = TIP_SIZE as f32;
    let mut data = Vec::with_capacity((TIP_SIZE * TIP_SIZE) as usize);
    for y in 0..TIP_SIZE {
        for x in 0..TIP_SIZE {
            // texel 中心，正規化到 ±1。
            let u = (x as f32 + 0.5) / n * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / n * 2.0 - 1.0;
            let d = (u * u + v * v).sqrt();

            let coverage = match tip {
                TipId::SoftRound => radial_falloff(d),
                TipId::HardRound => hard_edge(d),
                // 徑向遮罩不是裝飾：少了它，方形的 noise 會讓每個 dab 都是方塊。
                TipId::Grain => grain(x, y) * radial_falloff(d),
            };
            data.push((coverage.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    data
}

/// 解析式的徑向衰減：`coverage = (1 - d)^TIP_FALLOFF`，圓外為 0。
fn radial_falloff(d: f32) -> f32 {
    if d >= 1.0 {
        0.0
    } else {
        (1.0 - d).powf(TIP_FALLOFF)
    }
}

/// 同一條徑向路徑，邊緣換成窄過渡的 `smoothstep`：圓內滿覆蓋，最後 `HARD_EDGE`
/// 那一段落到 0。過渡不是 0 寬，因為那樣邊緣會鋸齒。
fn hard_edge(d: f32) -> f32 {
    1.0 - smoothstep(1.0 - HARD_EDGE, 1.0, d)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// value noise：格點取雜湊值、雙線性內插（權重過 `smoothstep`，否則格線會是硬折角）。
///
/// 一個 octave 就好。疊 octave 會多出「幾層、各佔多少」兩個常數，而顆粒要調的是
/// 粗細與對比——那正是 `GRAIN_FREQ` 與 `GRAIN_CONTRAST` 兩顆旋鈕。
fn grain(x: u32, y: u32) -> f32 {
    let scale = GRAIN_FREQ as f32 / TIP_SIZE as f32;
    let (fx, fy) = (x as f32 * scale, y as f32 * scale);
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (smoothstep(0.0, 1.0, fx - ix), smoothstep(0.0, 1.0, fy - iy));
    let (ix, iy) = (ix as i32, iy as i32);

    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let top = lerp(lattice(ix, iy), lattice(ix + 1, iy), tx);
    let bottom = lerp(lattice(ix, iy + 1), lattice(ix + 1, iy + 1), tx);
    let n = lerp(top, bottom, ty);

    // 繞 0.5 拉開對比。低對比是霧，高對比是砂。
    ((n - 0.5) * GRAIN_CONTRAST + 0.5).clamp(0.0, 1.0)
}

/// 格點雜湊 → `[0, 1)`。自己寫而不是拉 `rand`：需求只有「決定性、跨平台相同」，
/// 而 `rand` 的演算法會隨版本變——那會讓顆粒 tip 在升版時默默換一張。
fn lattice(x: i32, y: i32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1) ^ GRAIN_SEED;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x2974_5c85);
    h ^= h >> 16;
    // 取高 24 bit：f32 的尾數就這麼寬，除法是精確的 2 的冪。
    (h >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三層都有內容。**缺一層現在沒有 fallback**，所以這條守的是
    /// 「刻意缺層會產生明顯的空白筆跡」的另一面：正常路徑三層都不是空白。
    #[test]
    fn every_tip_layer_has_ink() {
        for tip in [TipId::SoftRound, TipId::HardRound, TipId::Grain] {
            let texels = tip_texels(tip);
            assert_eq!(texels.len(), (TIP_SIZE * TIP_SIZE) as usize);
            let inked = texels.iter().filter(|&&t| t > 0).count();
            assert!(
                inked > texels.len() / 4,
                "{tip:?} 只有 {inked} 個非零 texel——這層等於空白"
            );
        }
    }

    #[test]
    fn the_three_tips_are_actually_different() {
        let (soft, hard, grain) = (
            tip_texels(TipId::SoftRound),
            tip_texels(TipId::HardRound),
            tip_texels(TipId::Grain),
        );
        assert_ne!(soft, hard);
        assert_ne!(soft, grain);
        assert_ne!(hard, grain);
    }

    /// 「硬」的定義：從滿覆蓋掉到全透明只花很短的一段半徑。
    #[test]
    fn hard_round_is_flat_until_it_is_not() {
        assert!(hard_edge(0.5) > 0.99, "圓內該是滿覆蓋，硬邊才立得住");
        assert!(hard_edge(1.0) == 0.0);
        // 同一個半徑上，軟圓已經掉了一半，硬圓還沒開始掉。
        assert!(hard_edge(0.5) > radial_falloff(0.5) * 1.5);
    }

    /// 顆粒得真的不規則。一張常數貼圖也會通過「有內容」，但畫出來是軟圓。
    #[test]
    fn grain_is_irregular() {
        let samples: Vec<f32> = (0..TIP_SIZE)
            .step_by(3)
            .map(|i| grain(i, TIP_SIZE / 2))
            .collect();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        let var = samples.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / samples.len() as f32;
        assert!(var > 0.02, "顆粒的變異數只有 {var}——這張 tip 太平了");
    }

    /// 徑向遮罩把方形的 noise 收成圓形，否則每個 dab 都是方塊。
    #[test]
    fn grain_is_masked_into_a_circle() {
        let texels = tip_texels(TipId::Grain);
        let corner = texels[0];
        assert_eq!(corner, 0, "四角落在圓外，必須是 0");
    }
}
