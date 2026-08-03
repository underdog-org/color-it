//! Mask uniform（`docs/specs/E1-wgpu.md §7.1`）。Pass 2 與 Pass 3 共用。

mod support;

use render::{Gpu, MaskBinding, MaskMode, MaskUniform};

#[test]
fn uniform_is_two_little_endian_u32_in_declaration_order() {
    let uniform = MaskUniform {
        mode: MaskMode::Loose as u32,
        active_region_id: 300,
    };

    // WGSL 端是 struct { mode: u32, active_region_id: u32 }，順序錯了會靜默取錯欄位。
    assert_eq!(bytemuck::bytes_of(&uniform), &[1, 0, 0, 0, 44, 1, 0, 0]);
}

#[test]
fn strict_is_mode_zero_and_loose_is_mode_one() {
    assert_eq!(MaskMode::Strict as u32, 0);
    assert_eq!(MaskMode::Loose as u32, 1);
}

#[test]
fn switching_mode_only_rewrites_the_buffer() {
    let gpu = Gpu::headless().expect("headless device");
    let binding = MaskBinding::new(&gpu);
    binding.set(
        &gpu,
        MaskUniform {
            mode: MaskMode::Strict as u32,
            active_region_id: 7,
        },
    );

    // D4 要能在真機上即時切換比較，所以切 mode 只能是一次 write_buffer——
    // `set` 拿 `&self`，沒有重建 bind group 或 pipeline 的餘地。
    binding.set(
        &gpu,
        MaskUniform {
            mode: MaskMode::Loose as u32,
            active_region_id: 7,
        },
    );

    assert_eq!(
        support::read_buffer(&gpu, binding.buffer()),
        [1, 0, 0, 0, 7, 0, 0, 0]
    );
}
