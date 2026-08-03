//! 文件級 GPU 資源（`docs/specs/E1-wgpu.md §4`）。
//!
//! 一份文件一組，`attach_surface` 時配置、`detach_surface` 時**保留**（契約 C5）。
//! 尺寸一律 `manifest.canvas_size`，與螢幕無關。

use colorpack::{ColorPack, RegionEntry};
use wgpu::TextureUsages as U;

use crate::error::RenderError;
use crate::gpu::Gpu;

/// `Buf_palette` 的元素：linear-ish sRGB 編碼值 RGBA（§6 全程在編碼值上合成）。
/// `a == 0` 表示該區域尚未被油漆桶填過（§4.1）。
const PALETTE_ENTRY_SIZE: u64 = 16;

pub struct DocumentResources {
    canvas_size: [u32; 2],
    line: wgpu::Texture,
    shade: wgpu::Texture,
    region: wgpu::Texture,
    paint: wgpu::Texture,
    erase: wgpu::Texture,
    wet: wgpu::Texture,
    palette: wgpu::Buffer,
    region_ids: Vec<u16>,
    regions: Vec<RegionEntry>,
}

impl DocumentResources {
    pub fn new(gpu: &Gpu, pack: &ColorPack) -> Result<Self, RenderError> {
        let [w, h] = pack.manifest.canvas_size;
        let device = gpu.device();

        // T_line／T_shade：`textureSample` ＋ linear filter（畫布縮放時需要，§5.2）。
        let line = texture(
            device,
            "T_line",
            w,
            h,
            wgpu::TextureFormat::Rgba8Unorm,
            U::TEXTURE_BINDING | U::COPY_DST | U::COPY_SRC,
        );
        write_rgba8(gpu, &line, &decode_rgba8(&pack.lineart_png, w, h)?);

        // 缺席時綁 1×1 全白，不做 shader variant——composite 是 Multiply，白色即單位元。
        let shade = match &pack.shade_png {
            Some(png) => {
                let t = texture(
                    device,
                    "T_shade",
                    w,
                    h,
                    wgpu::TextureFormat::Rgba8Unorm,
                    U::TEXTURE_BINDING | U::COPY_DST | U::COPY_SRC,
                );
                write_rgba8(gpu, &t, &decode_rgba8(png, w, h)?);
                t
            }
            None => {
                let t = texture(
                    device,
                    "T_shade(dummy)",
                    1,
                    1,
                    wgpu::TextureFormat::Rgba8Unorm,
                    U::TEXTURE_BINDING | U::COPY_DST | U::COPY_SRC,
                );
                write_rgba8(gpu, &t, &[255, 255, 255, 255]);
                t
            }
        };

        let region = texture(
            device,
            "T_region",
            w,
            h,
            wgpu::TextureFormat::R16Uint,
            U::TEXTURE_BINDING | U::COPY_DST | U::COPY_SRC,
        );
        gpu.queue().write_texture(
            region.as_image_copy(),
            bytemuck::cast_slice(&pack.region_ids),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 2),
                rows_per_image: Some(h),
            },
            region.size(),
        );

        // T_paint／T_erase 現在就加 COPY_SRC：E3 的 undo 要對它們做 dirty tile 快照，
        // usage flag 不影響記憶體，事後追加卻要改資源建立的所有路徑。
        let paint = texture(
            device,
            "T_paint",
            w,
            h,
            wgpu::TextureFormat::Rgba8Unorm,
            U::TEXTURE_BINDING | U::RENDER_ATTACHMENT | U::COPY_SRC,
        );
        let erase = texture(
            device,
            "T_erase",
            w,
            h,
            wgpu::TextureFormat::R8Unorm,
            U::TEXTURE_BINDING | U::RENDER_ATTACHMENT | U::COPY_SRC,
        );
        // T_wet 是單筆暫存，永遠不進 undo（§4.3 #4），所以不給 COPY_SRC。
        let wet = texture(
            device,
            "T_wet",
            w,
            h,
            wgpu::TextureFormat::R8Unorm,
            U::TEXTURE_BINDING | U::RENDER_ATTACHMENT,
        );

        let palette = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_palette"),
            size: u64::from(pack.manifest.region_count) * PALETTE_ENTRY_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            canvas_size: [w, h],
            line,
            shade,
            region,
            paint,
            erase,
            wet,
            palette,
            region_ids: pack.region_ids.clone(),
            regions: pack.regions.clone(),
        })
    }

    pub fn canvas_size(&self) -> [u32; 2] {
        self.canvas_size
    }

    pub fn line(&self) -> &wgpu::Texture {
        &self.line
    }

    pub fn shade(&self) -> &wgpu::Texture {
        &self.shade
    }

    pub fn region(&self) -> &wgpu::Texture {
        &self.region
    }

    pub fn paint(&self) -> &wgpu::Texture {
        &self.paint
    }

    pub fn erase(&self) -> &wgpu::Texture {
        &self.erase
    }

    pub fn wet(&self) -> &wgpu::Texture {
        &self.wet
    }

    pub fn palette(&self) -> &wgpu::Buffer {
        &self.palette
    }

    /// 油漆桶的 `tap` 必須**同步**拿到 region ID（§5.1）——單像素 readback 至少一 frame
    /// stall，主互動吃不起。`E1-bucket` 只讀這一份，不得另開。
    pub fn region_ids(&self) -> &[u16] {
        &self.region_ids
    }

    /// `bbox` 給 `E1-bucket` 清 `T_erase` 用。
    pub fn regions(&self) -> &[RegionEntry] {
        &self.regions
    }
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn write_rgba8(gpu: &Gpu, texture: &wgpu::Texture, data: &[u8]) {
    gpu.queue().write_texture(
        texture.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 4),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}

/// `colorpack` 不解碼影像（那是它的邊界），所以 PNG 在這裡才變成像素。
fn decode_rgba8(png_bytes: &[u8], w: u32, h: u32) -> Result<Vec<u8>, RenderError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);

    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if info.width != w || info.height != h {
        return Err(RenderError::AssetMismatch(
            "PNG 尺寸與 manifest.canvas_size 不符",
        ));
    }

    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let frame = reader.next_frame(&mut buf)?;
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return Err(RenderError::AssetMismatch("PNG 無法正規化成 RGBA8"));
    }

    buf.truncate(frame.buffer_size());
    Ok(buf)
}
