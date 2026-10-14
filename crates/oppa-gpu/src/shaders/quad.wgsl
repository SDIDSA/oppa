struct ViewportUniform {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u_viewport: ViewportUniform;

struct VertexInput {
    @location(0) unit_pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) xf_abcd: vec4<f32>,
    @location(6) xf_t: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) rect_size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Baked quad corner, then the residual paint affine (decision
    // 517 — identity for ST-only chains, so unrotated instances land
    // on the exact old pixels; the SDF below stays in the unrotated
    // local frame, so rounded corners rotate rigidly).
    let baked = in.rect.xy + in.unit_pos * in.rect.zw;
    let pos_px = vec2<f32>(
        in.xf_abcd.x * baked.x + in.xf_abcd.y * baked.y + in.xf_t.x,
        in.xf_abcd.z * baked.x + in.xf_abcd.w * baked.y + in.xf_t.y
    );
    
    // Convert physical pixels to WebGPU clip space [-1, 1]
    let ndc_x = (pos_px.x / u_viewport.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos_px.y / u_viewport.viewport_size.y) * 2.0;
    
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_pos = in.unit_pos * in.rect.zw;
    out.rect_size = in.rect.zw;
    out.color = in.color;
    out.radii = in.radii;
    out.params = in.params;
    return out;
}

// Polynomial approximation of the Error Function (Abramowitz & Stegun 7.1.26)
fn erf_approx(x: f32) -> f32 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p  = 0.3275911;
    let sign = select(-1.0, 1.0, x >= 0.0);
    let ax = abs(x);
    let t = 1.0 / (1.0 + p * ax);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * exp(-ax * ax);
    return sign * y;
}

fn gaussian_1d(x: f32, x0: f32, x1: f32, sigma: f32) -> f32 {
    let inv_sig_sqrt2 = 1.0 / (sigma * 1.41421356);
    return 0.5 * (erf_approx((x - x0) * inv_sig_sqrt2) - erf_approx((x - x1) * inv_sig_sqrt2));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mode = in.params.x;
    var coverage: f32 = 0.0;
    
    if (mode == 1.0) {
        // Circle mode
        let c = 0.5 * in.rect_size;
        let radius = 0.5 * min(in.rect_size.x, in.rect_size.y);
        let dist = length(in.local_pos - c) - radius;
        coverage = clamp(0.5 - dist, 0.0, 1.0);
    } else if (mode == 2.0) {
        // Shadow mode
        let blur_radius = in.params.y;
        if (blur_radius <= 0.0) {
            // Crisp solid offset box
            let b = 0.5 * in.rect_size;
            let p = abs(in.local_pos - b) - b;
            let dist = max(p.x, p.y);
            coverage = clamp(0.5 - dist, 0.0, 1.0);
        } else {
            // Separable 2D Gaussian blur
            let margin = ceil(3.0 * blur_radius);
            let sigma = max(blur_radius * 0.577, 0.5); // sigma matching dual box-blur
            let x0 = margin;
            let x1 = in.rect_size.x - margin;
            let y0 = margin;
            let y1 = in.rect_size.y - margin;
            
            let gx = gaussian_1d(in.local_pos.x, x0, x1, sigma);
            let gy = gaussian_1d(in.local_pos.y, y0, y1, sigma);
            coverage = clamp(gx * gy, 0.0, 1.0);
        }
    } else {
        // Rect / RRect mode
        let b = 0.5 * in.rect_size;
        let p = in.local_pos - b;
        
        // Select corner radius
        var r: f32 = 0.0;
        if (in.local_pos.x < b.x) {
            if (in.local_pos.y < b.y) {
                r = in.radii.x; // top-left
            } else {
                r = in.radii.w; // bottom-left
            }
        } else {
            if (in.local_pos.y < b.y) {
                r = in.radii.y; // top-right
            } else {
                r = in.radii.z; // bottom-right
            }
        }
        r = min(r, min(b.x, b.y));
        
        let q = abs(p) - b + vec2<f32>(r, r);
        let dist = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
        coverage = clamp(0.5 - dist, 0.0, 1.0);
    }
    
    let final_alpha = in.color.a * coverage;
    if (final_alpha <= 0.0) {
        discard;
    }
    
    // Return premultiplied RGBA
    return vec4<f32>(in.color.rgb * final_alpha, final_alpha);
}
