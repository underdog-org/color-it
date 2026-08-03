//! 測試共用：合成一份最小 `ColorPack`，以及 GPU readback。
//!
//! 每個 test binary 各自編譯本模組，用不到的那幾個一定會被判 dead code。
#![allow(dead_code)]

use colorpack::ColorPack;
use colorpack::manifest::{Aspect, Category, Difficulty, Manifest};
use colorpack::region::RegionEntry;

/// `region_ids` 的長度決定畫布尺寸，呼叫端給 `w × h` 個 ID。
/// 線稿全黑、shade 半灰——只驗資源配置的測試用。
pub fn pack(w: u32, h: u32, region_ids: Vec<u16>, with_shade: bool) -> ColorPack {
    pack_with(
        w,
        h,
        region_ids,
        [0, 0, 0, 255],
        with_shade.then_some([128, 128, 128, 255]),
    )
}

/// 線稿與 shade 的顏色可指定——composite 的合成測試要能算出預期值。
pub fn pack_with(
    w: u32,
    h: u32,
    region_ids: Vec<u16>,
    lineart: [u8; 4],
    shade: Option<[u8; 4]>,
) -> ColorPack {
    let mut pack = bare_pack(w, h, region_ids, shade.is_some());
    pack.lineart_png = solid_png(w, h, lineart);
    pack.shade_png = shade.map(|rgba| solid_png(w, h, rgba));
    pack
}

fn bare_pack(w: u32, h: u32, region_ids: Vec<u16>, with_shade: bool) -> ColorPack {
    assert_eq!(region_ids.len(), (w * h) as usize);

    let region_count = region_ids.iter().copied().max().unwrap_or(0) as u32 + 1;
    let regions = (0..region_count)
        .map(|id| RegionEntry {
            id,
            centroid: [0, 0],
            area: 1,
            bbox: [0, 0, w, h],
            suggested_color: "#FFFFFF".to_owned(),
        })
        .collect();

    ColorPack {
        manifest: Manifest {
            schema_version: "1.0".to_owned(),
            id: "test".to_owned(),
            content_hash: String::new(),
            canvas_size: [w, h],
            aspect: Aspect::Square,
            region_count,
            difficulty: Difficulty::Easy,
            category: Category::Animal,
            has_shade: with_shade,
            palette: vec![],
        },
        regions,
        region_ids,
        lineart_png: solid_png(w, h, [0, 0, 0, 255]),
        shade_png: with_shade.then(|| solid_png(w, h, [128, 128, 128, 255])),
        thumb_jpg: vec![],
    }
}

/// Composite 的 offscreen target。非 sRGB 變體，與 `SURFACE_FORMAT` 同一套語意。
pub fn offscreen(gpu: &render::Gpu, w: u32, h: u32) -> wgpu::Texture {
    gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// 把整張貼圖填成同一個值。
///
/// 走 render pass 的 `LoadOp::Clear` 而不是 `write_texture`——`T_wet` / `T_paint`
/// 沒有 `COPY_DST`（它們是 pass 的產物，不從 CPU 上傳），不該為了測試加 usage。
pub fn clear_texture(gpu: &render::Gpu, texture: &wgpu::Texture, color: wgpu::Color) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    gpu.queue().submit([encoder.finish()]);
}

/// 單色 RGBA8 PNG。`render` 端要真的解得開，所以不能用假 bytes。
pub fn solid_png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    let data: Vec<u8> = rgba
        .iter()
        .copied()
        .cycle()
        .take((w * h * 4) as usize)
        .collect();
    writer.write_image_data(&data).expect("png data");
    writer.finish().expect("png finish");
    out
}

/// 左右兩色的 RGBA8 PNG，`split_x` 之前是 `left`。
/// 用來證明 `T_line` / `T_shade` 是以**畫布 UV** 取樣而不是螢幕 UV——
/// 單色貼圖驗不出這件事。
pub fn split_png(w: u32, h: u32, split_x: u32, left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            data.extend_from_slice(if x < split_x { &left } else { &right });
        }
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&data).expect("png data");
    writer.finish().expect("png finish");
    out
}

/// 把 buffer 整條抓回 CPU。
pub fn read_buffer(gpu: &render::Gpu, buffer: &wgpu::Buffer) -> Vec<u8> {
    let staging = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: buffer.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, buffer.size());
    gpu.queue().submit([encoder.finish()]);

    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    gpu.device()
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");

    let slice = staging.slice(..);
    let out = slice.get_mapped_range().expect("mapped range").to_vec();
    staging.unmap();
    out
}

/// 把 texture 整張抓回 CPU。`bytes_per_row` 的 256 對齊由本函式處理。
pub fn read_texture(gpu: &render::Gpu, texture: &wgpu::Texture, bytes_per_pixel: u32) -> Vec<u8> {
    let (w, h) = (texture.width(), texture.height());
    let unpadded = w * bytes_per_pixel;
    let padded =
        unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

    let staging = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    gpu.queue().submit([encoder.finish()]);

    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    gpu.device()
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");

    let slice = staging.slice(..);
    let view = slice.get_mapped_range().expect("mapped range");
    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        out.extend_from_slice(&view[start..start + unpadded as usize]);
    }
    drop(view);
    staging.unmap();
    out
}
