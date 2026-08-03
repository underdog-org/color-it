//! Mask uniform（`docs/specs/E1-wgpu.md §7.1`）。Pass 2 與 Pass 3 共用。
//!
//! 放在每 frame 更新的 global uniform buffer，不是 per-pass bind group——
//! D4 要能在真機上即時切換兩種 mode 比較，切 mode 不該重建 pipeline。

use bytemuck::{Pod, Zeroable};

use crate::gpu::Gpu;

/// 條件式本身由 `E1-composite` 定義（§7.1 的警告：`architecture.md §4.4` 的
/// `id != REGION_LINEART` 不成立，baker 的 ID map 是滿的、沒有保留 ID）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MaskMode {
    /// A：`id == active_region_id`。
    Strict = 0,
    /// B：寬鬆。
    Loose = 1,
}

/// WGSL 端：`struct MaskUniform { mode: u32, active_region_id: u32 }`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct MaskUniform {
    pub mode: u32,
    /// mode A 才有意義。
    pub active_region_id: u32,
}

pub struct MaskBinding {
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl MaskBinding {
    pub fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buf_mask"),
            size: size_of::<MaskUniform>() as u64,
            // COPY_SRC 只為了 §8 的可驗證性——8 bytes，不影響記憶體帳。
            usage: wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mask"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mask"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            buffer,
            layout,
            bind_group,
        }
    }

    /// 拿 `&self`：切 mode 是一次 `write_buffer`，沒有重建 bind group 或 pipeline 的餘地。
    pub fn set(&self, gpu: &Gpu, uniform: MaskUniform) {
        gpu.queue()
            .write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
