// Pass 1 · Stroke（`docs/specs/E1-stroke.md §7`）。
//
// instanced quad × dab_count → `T_wet`。一個 dab 一個 instance，
// vertex shader 依 `pos` / `size` / `angle` 展開四個頂點。
//
// **不讀 `T_region`**（`E1-wgpu §7` 資源矩陣第 1 條）：遮罩在 Pass 2 commit 時
// 算一次，不是每個 dab 算。

struct Stroke {
    canvas_size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: Stroke;
/// 從第一天就是 array 而非單張，為的是 E2 加 tip 不動 bind group layout（`§6.1`）。
@group(0) @binding(1) var t_tip: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // 同一個 instance 的四個頂點同值，內插只是浪費。
    @location(1) @interpolate(flat) alpha: f32,
    @location(2) @interpolate(flat) layer: u32,
}

/// `draw(0..4)` ＋ TriangleStrip：四個頂點就是一個 quad，不需要 index buffer。
@vertex
fn vs_main(
    @builtin(vertex_index) i: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: f32,
    @location(2) angle: f32,
    @location(3) alpha: f32,
    @location(4) layer: u32,
) -> VsOut {
    // (0,0) (1,0) (0,1) (1,1) → ±1 的筆尖局部座標。
    let local = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u)) * 2.0 - 1.0;

    // `size` 是**直徑**（`stroke::Dab`），所以半徑要除 2。
    let s = sin(angle);
    let c = cos(angle);
    let offset = vec2<f32>(local.x * c - local.y * s, local.x * s + local.y * c) * (size * 0.5);
    let canvas = pos + offset;

    // 畫布像素 → clip space。target 是 `T_wet`，尺寸即畫布尺寸——Pass 1 畫在畫布上、
    // 不畫在螢幕上，所以這裡完全不涉及 `Transform`。
    let ndc = canvas / u.canvas_size * 2.0 - 1.0;

    var out: VsOut;
    out.clip = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = local * 0.5 + 0.5;
    out.alpha = alpha;
    out.layer = layer;
    return out;
}

/// `coverage = tip[tip_id].sample(uv) × dab.alpha`（`§7`）。
///
/// 筆尖貼圖是 CPU 程序生成的（`§6.1`），所以這裡沒有任何 tip 的形狀知識——
/// E2 換成顆粒紋理時這個 shader 一個字都不用改。
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let coverage = textureSampleLevel(t_tip, samp, in.uv, in.layer, 0.0).r * in.alpha;
    return vec4<f32>(coverage, 0.0, 0.0, 1.0);
}
