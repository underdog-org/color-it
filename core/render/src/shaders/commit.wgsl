// Pass 2 · Commit，路徑 (a)（`docs/specs/E1-stroke.md §8`）。
//
// `tint × opacity × mask` → `T_paint`。抬筆時一次，scissor 至整筆 bbox。
//
// **`T_paint` 存 premultiplied alpha**，所以 `E1-composite §3` 第 ③ 層的 `over()`
// 是 `p.rgb + c × (1 - p.a)`，不是 straight-alpha 的版本。

struct MaskUniform {
    mode: u32,               // 0 = A 嚴格；1 = B 寬鬆
    active_region_id: u32,
}

struct Commit {
    /// 整筆的顏色，編碼值 straight alpha。`.a` 不參與——整筆濃度由 `opacity` 決定。
    color: vec4<f32>,
    /// `Tool::Brush.opacity` 覆寫值，`None` 時取 `preset.opacity`。
    ///
    /// 後面不補 `vec3<f32>` 的 pad——那會把 struct 撐到 48 bytes，
    /// 而 uniform 的 size 本來就會向上取到 16 的倍數（32）。Rust 端的 `_pad`
    /// 是同一個 32，兩邊對得上。
    opacity: f32,
}

@group(0) @binding(0) var t_wet: texture_2d<f32>;
@group(0) @binding(1) var t_region: texture_2d<u32>;
@group(0) @binding(2) var<uniform> u: Commit;
@group(1) @binding(0) var<uniform> m: MaskUniform;

/// Full-screen triangle；實際寫入範圍由 scissor 決定。
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

/// Mode B 恆回 1.0（`E1-composite §6`）——`REGION_LINEART` 不存在，
/// 線稿的無害性來自 composite 層序而不是 mask。
fn mask(id: u32) -> f32 {
    return select(1.0, f32(id == m.active_region_id), m.mode == 0u);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // target 是 `T_paint`，尺寸即畫布尺寸，所以 `@builtin(position)` 已經是畫布像素。
    let cc = vec2<i32>(floor(pos.xy));

    // **遮罩在這裡算一次，不是每個 dab 算**（`E1-wgpu §7` 資源矩陣第 1 條）。
    let id = textureLoad(t_region, cc, 0).r;
    let a = textureLoad(t_wet, cc, 0).r * u.opacity * mask(id);

    return vec4<f32>(u.color.rgb * a, a);
}
