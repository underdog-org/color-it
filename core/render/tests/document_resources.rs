//! 文件級 GPU 資源的配置契約（`docs/specs/E1-wgpu.md §4`／`§5`）。

mod support;

use render::{DocumentResources, Gpu};

#[test]
fn region_ids_survive_the_upload_bit_for_bit() {
    let ids: Vec<u16> = vec![0, 1, 2, 3, 65535, 300, 7, 0];
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, ids.clone(), false)).expect("resources");

    let bytes = support::read_texture(&gpu, resources.region(), 2);
    let read_back: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(read_back, ids);
}

#[test]
fn missing_shade_binds_a_one_by_one_white_dummy() {
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, vec![0; 8], false)).expect("resources");

    let shade = resources.shade();
    assert_eq!((shade.width(), shade.height()), (1, 1));
    // Multiply 的單位元。不是白的就會把整張畫布壓暗。
    assert_eq!(support::read_texture(&gpu, shade, 4), [255, 255, 255, 255]);
}

#[test]
fn present_shade_is_uploaded_at_canvas_size() {
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, vec![0; 8], true)).expect("resources");

    let shade = resources.shade();
    assert_eq!((shade.width(), shade.height()), (4, 2));
    assert_eq!(
        &support::read_texture(&gpu, shade, 4)[..4],
        [128, 128, 128, 255]
    );
}

#[test]
fn paint_starts_fully_transparent() {
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, vec![0; 8], false)).expect("resources");

    assert_eq!(support::read_texture(&gpu, resources.paint(), 4), [0; 32]);
}

#[test]
fn palette_has_one_rgba_f32_entry_per_region() {
    let ids: Vec<u16> = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, ids, false)).expect("resources");

    assert_eq!(resources.palette().size(), 8 * 16);
}

/// `E1-composite.md §5`：32 bytes／筆、長度 `region_count`（65535 區 = 2 MB）。
/// 零初始化＝全部未填色，那條由 `tests/composite.rs` 從畫面上驗。
#[test]
fn fill_animation_buffer_is_thirty_two_bytes_per_region() {
    let ids: Vec<u16> = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, ids, false)).expect("resources");

    assert_eq!(render::FILL_ANIM_SIZE, 32);
    assert_eq!(resources.fill().size(), 8 * render::FILL_ANIM_SIZE);
}

#[test]
fn undo_snapshot_targets_carry_copy_src_but_wet_does_not() {
    let gpu = Gpu::headless().expect("headless device");

    let resources =
        DocumentResources::new(&gpu, &support::pack(4, 2, vec![0; 8], false)).expect("resources");

    // E3 的 dirty tile 快照要對 T_paint／T_erase 做 COPY_SRC；T_wet 永遠不進 undo。
    assert!(
        resources
            .paint()
            .usage()
            .contains(wgpu::TextureUsages::COPY_SRC)
    );
    assert!(
        resources
            .erase()
            .usage()
            .contains(wgpu::TextureUsages::COPY_SRC)
    );
    assert!(
        !resources
            .wet()
            .usage()
            .contains(wgpu::TextureUsages::COPY_SRC)
    );
}
