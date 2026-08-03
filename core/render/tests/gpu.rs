//! `Gpu` 的建立契約（`docs/specs/E1-wgpu.md §2.1`）。

#[test]
fn headless_gpu_meets_required_limits() {
    let gpu = render::Gpu::headless().expect("headless device");

    assert!(gpu.device().limits().max_texture_dimension_2d >= 2048);
}
