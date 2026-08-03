// `Fill` 順帶清該區域的 `T_erase`（`docs/specs/E1-bucket.md §6`）。
//
// 填色要蓋掉先前的擦除痕跡，所以這是 `Fill` 的語意一部分，不是橡皮擦的附屬品。
// E1 的可觀測效果是零（`T_erase` 全程恆為 0），橡皮擦是 E2。

@group(0) @binding(0) var t_region: texture_2d<u32>;
/// `.x` 是這次填的 region ID。用 `vec4<u32>` 而不是單欄位 struct——uniform 的
/// 16-byte 對齊會把 `u32 + vec3<u32>` 撐成 32 bytes。
@group(0) @binding(1) var<uniform> filled: vec4<u32>;

/// Full-screen triangle，實際 rasterize 的範圍由 scissor 限在 region bbox。
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

/// bbox 是矩形但 region 不是——bbox 內不屬於本區的像素必須原封不動。
///
/// `discard` 而非寫回原值：`T_erase` 是 render attachment，讀寫同一張圖要 ping-pong，
/// 而 discard 零成本。
@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) f32 {
    if textureLoad(t_region, vec2<i32>(pos.xy), 0).r != filled.x {
        discard;
    }
    return 0.0;
}
