struct ViewportUniform {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u_viewport: ViewportUniform;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = (in.pos.x / u_viewport.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.pos.y / u_viewport.viewport_size.y) * 2.0;
    
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.color.a <= 0.0) {
        discard;
    }
    // Premultiplied output
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
}
