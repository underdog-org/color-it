// `T_wet` 的收尾清除（`docs/specs/E1-stroke.md §8.1`）。
//
// scissor 至整筆 bbox，**不是全畫布**——`LoadOp::Clear` 只吃得到整張 attachment，
// 而一張 1:1 畫布的全域 clear 是每抬一次筆就多一次滿頻寬寫入。

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

/// 寫的是絕對值 0.0，不是與現值混合（pipeline 的 blend 為 `None`）。
@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
