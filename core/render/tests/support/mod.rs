//! 測試共用：合成一份最小 `ColorPack`，以及 GPU readback。
//!
//! 每個 test binary 各自編譯本模組，用不到的那幾個一定會被判 dead code。
#![allow(dead_code)]

use colorpack::ColorPack;
use colorpack::manifest::{Aspect, Category, Difficulty, Manifest};
use colorpack::region::RegionEntry;

/// `region_ids` 的長度決定畫布尺寸，呼叫端給 `w × h` 個 ID。
pub fn pack(w: u32, h: u32, region_ids: Vec<u16>, with_shade: bool) -> ColorPack {
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
