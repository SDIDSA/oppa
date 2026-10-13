struct ViewportUniform {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u_viewport: ViewportUniform;

@group(1) @binding(0)
var u_atlas: texture_2d<f32>;

@group(1) @binding(1)
var u_sampler: sampler;

struct VertexInput {
    @location(0) unit_pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(5) xf_abcd: vec4<f32>,
    @location(6) xf_t: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Residual paint affine (decision 517 — same rule as the quad
    // shader; UVs stay unrotated so glyphs rotate as rigid quads).
    let baked = in.rect.xy + in.unit_pos * in.rect.zw;
    let pos_px = vec2<f32>(
        in.xf_abcd.x * baked.x + in.xf_abcd.y * baked.y + in.xf_t.x,
        in.xf_abcd.z * baked.x + in.xf_abcd.w * baked.y + in.xf_t.y
    );
    
    let ndc_x = (pos_px.x / u_viewport.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos_px.y / u_viewport.viewport_size.y) * 2.0;
    
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv.xy + in.unit_pos * in.uv.zw;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha_cov = textureSample(u_atlas, u_sampler, in.uv).r;
    let final_alpha = in.color.a * alpha_cov;
    if (final_alpha <= 0.0) {
        discard;
    }
    return vec4<f32>(in.color.rgb * final_alpha, final_alpha);
}
