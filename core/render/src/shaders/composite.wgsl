// Pass 3 · Composite（`docs/specs/E1-composite.md §3`）。
//
// 每 frame 一個 full-screen triangle，七個資源合成到 surface。
// **全程在 sRGB 編碼值上做，不 linearize**（§2）——畫布必須跟 baker 的 thumb.jpg 一樣。
//
// 這個 shader 只讀狀態、不改狀態，沒有任何「使用者在做什麼」的條件分支。

/// 未上色 = 白紙，不是透明（§3.1，`prd.md §4.1`）。編譯期常數，不做 uniform。
const PAPER_WHITE: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);

/// 擴散前緣的柔邊寬度，畫布像素（§5）。初值 24，實機調校列入 E1-perf。
const FILL_EDGE: f32 = 24.0;

struct FillAnim {
    origin: vec2<f32>,       // 點擊處，畫布像素座標
    max_radius: f32,         // region bbox 對角線
    progress: f32,           // 0..1，CPU 每 frame 只更新進行中的 entry
    prev_color: vec4<f32>,   // 這次填色之前該區域的顏色
}

struct MaskUniform {
    mode: u32,               // 0 = A 嚴格；1 = B 寬鬆
    active_region_id: u32,
}

struct Frame {
    scale: f32,
    tx: f32,
    ty: f32,
    _pad: f32,
    /// shader 不再用它（`canvas_pos` 改吃 `@builtin(position)`），但 `Frame`
    /// 的佈局與 Rust 端一一對應，拿掉會讓後面的欄位全部錯位。
    screen_size: vec2<f32>,
    canvas_size: vec2<f32>,
    /// 畫布外的背景色。**不是 PAPER_WHITE**，否則看不出畫布邊界（§4）。
    background: vec4<f32>,
    /// 進行中筆畫的顏色。straight alpha。
    brush_color: vec4<f32>,
}

@group(0) @binding(0) var t_region: texture_2d<u32>;
@group(0) @binding(1) var t_erase: texture_2d<f32>;
@group(0) @binding(2) var t_paint: texture_2d<f32>;
@group(0) @binding(3) var t_wet: texture_2d<f32>;
@group(0) @binding(4) var t_shade: texture_2d<f32>;
@group(0) @binding(5) var t_line: texture_2d<f32>;
@group(0) @binding(6) var samp: sampler;
@group(0) @binding(7) var<storage, read> palette: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read> fill: array<FillAnim>;

@group(1) @binding(0) var<uniform> m: MaskUniform;
@group(2) @binding(0) var<uniform> frame: Frame;

/// Full-screen triangle：不綁 vertex buffer，`draw(0..3)`。
/// 覆蓋整個 clip space 的單一三角形，比 quad 少一條對角線上的重複 rasterize。
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

/// 螢幕像素 → 畫布像素座標（浮點；整數化與邊界判斷由呼叫端做）。
///
/// **吃 `@builtin(position)` 而不是內插出來的 UV**：UV 要再乘一次 `screen_size`
/// 才回到像素，而 `x / w * w` 在 f32 下不保證等於 `x`——差的那個 ulp 會讓
/// 邊界像素 floor 到隔壁區，於是 shader 與 `Transform::canvas_pos` 對不上（`E1-bucket §4.2`）。
fn canvas_pos(screen: vec2<f32>) -> vec2<f32> {
    return (screen - vec2<f32>(frame.tx, frame.ty)) / frame.scale;
}

/// Mode B 恆回 1.0，也就是完全不遮罩（§6）。
fn mask(id: u32) -> f32 {
    return select(1.0, f32(id == m.active_region_id), m.mode == 0u);
}

/// premultiplied src over 不透明的 dst。
fn over(dst: vec3<f32>, src: vec4<f32>) -> vec3<f32> {
    return src.rgb + dst * (1.0 - src.a);
}

/// coverage × 筆刷顏色 → premultiplied，與第 ③ 層的 `T_paint` 同一套語意。
fn tint(coverage: f32, color: vec4<f32>) -> vec4<f32> {
    let a = coverage * color.a;
    return vec4<f32>(color.rgb * a, a);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = canvas_pos(pos.xy);

    // 畫布外：只寫背景色，不讀任何貼圖（§4／§7）。
    if p.x < 0.0 || p.y < 0.0 || p.x >= frame.canvas_size.x || p.y >= frame.canvas_size.y {
        return frame.background;
    }

    let cc = vec2<i32>(floor(p));
    // T_line／T_shade 走 textureSample（linear filter，畫布縮放時需要）。
    // 取樣座標是**畫布 UV** 不是螢幕 UV——letterbox 時兩者不同。
    let cuv = p / frame.canvas_size;

    let id = textureLoad(t_region, cc, 0).r;
    let erased = textureLoad(t_erase, cc, 0).r;

    // ① 油漆桶底色 ＋ 擴散動畫（§5）。
    // 未填色的區域 fill entry 全零：t 不影響結果，base.a 恆為 0。
    let f = fill[id];
    let d = distance(p, f.origin);
    let t = smoothstep(d - FILL_EDGE, d, f.progress * f.max_radius);
    let base = mix(f.prev_color, palette[id], t);

    // ② 未上色 = 白紙，且底色可被局部擦除。
    var color = mix(PAPER_WHITE, base.rgb, base.a);
    color = mix(color, PAPER_WHITE, erased);

    // ③ 已提交的筆刷（T_paint 是 premultiplied，見 E1-stroke §8）。
    let paint = textureLoad(t_paint, cc, 0);
    color = paint.rgb + color * (1.0 - paint.a);

    // ④ 進行中的筆畫。
    color = over(color, tint(textureLoad(t_wet, cc, 0).r, frame.brush_color) * mask(id));

    // ⑤⑥ Multiply，線稿蓋頂。
    color = color * textureSample(t_shade, samp, cuv).rgb;
    color = color * textureSample(t_line, samp, cuv).rgb;

    return vec4<f32>(color, 1.0);
}
