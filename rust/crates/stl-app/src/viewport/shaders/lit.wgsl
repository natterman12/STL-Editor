// Bodies: three.js MeshStandardMaterial (r170) under a hemisphere light and two directional
// lights, written out sRGB encoded.

@group(1) @binding(0) var<uniform> b: Body;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) view_pos: vec3<f32>,   // three's vViewPosition (= -mvPosition)
    @location(1) normal: vec3<f32>,     // view space
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_body(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> VsOut {
    var o: VsOut;
    let wp = b.model * vec4<f32>(p, 1.0);
    o.pos = g.view_proj * wp;
    o.view_pos = -(g.view * wp).xyz;
    o.normal = (g.view * (b.normal_m * vec4<f32>(n, 0.0))).xyz;
    o.world = wp.xyz;
    return o;
}

const PI: f32 = 3.141592653589793;
const RECIPROCAL_PI: f32 = 0.3183098861837907;

fn pow2(x: f32) -> f32 { return x * x; }

fn f_schlick(f0: vec3<f32>, f90: f32, dot_vh: f32) -> vec3<f32> {
    let fresnel = exp2((-5.55473 * dot_vh - 6.98316) * dot_vh);
    return f0 * (1.0 - fresnel) + f90 * fresnel;
}

fn v_ggx_smith_correlated(alpha: f32, dot_nl: f32, dot_nv: f32) -> f32 {
    let a2 = pow2(alpha);
    let gv = dot_nl * sqrt(a2 + (1.0 - a2) * pow2(dot_nv));
    let gl = dot_nv * sqrt(a2 + (1.0 - a2) * pow2(dot_nl));
    return 0.5 / max(gv + gl, 1e-6);
}

fn d_ggx(alpha: f32, dot_nh: f32) -> f32 {
    let a2 = pow2(alpha);
    let denom = pow2(dot_nh) * (a2 - 1.0) + 1.0;
    return RECIPROCAL_PI * a2 / pow2(denom);
}

fn brdf_ggx(l: vec3<f32>, v: vec3<f32>, n: vec3<f32>, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let alpha = pow2(roughness);
    let h = normalize(l + v);
    let dot_nl = saturate(dot(n, l));
    let dot_nv = saturate(dot(n, v));
    let dot_nh = saturate(dot(n, h));
    let dot_vh = saturate(dot(v, h));
    let f = f_schlick(f0, 1.0, dot_vh);
    return f * (v_ggx_smith_correlated(alpha, dot_nl, dot_nv) * d_ggx(alpha, dot_nh));
}

// three's sRGBTransferOETF
fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(0.41666)) * 1.055 - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_body(in: VsOut) -> @location(0) vec4<f32> {
    // derivatives first, while every fragment of the quad is still running
    let fdx = dpdx(in.view_pos);
    let fdy = dpdy(in.view_pos);
    let smooth_n = normalize(in.normal);
    // three: normalize(cross(dFdx, dFdy)) with GL's y up; wgpu's y runs down
    let flat_n = -normalize(cross(fdx, fdy));
    let n = select(smooth_n, flat_n, b.params.y > 0.5);
    let dxy = max(abs(dpdx(n)), abs(dpdy(n)));
    let geometry_roughness = max(max(dxy.x, dxy.y), dxy.z);
    if clipped(in.world) {
        discard;
    }

    let metalness = b.params.x;
    let diffuse = b.color.rgb * (1.0 - metalness);
    let specular = mix(vec3<f32>(0.04), b.color.rgb, metalness);
    let roughness = min(max(b.emissive.w, 0.0525) + geometry_roughness, 1.0);
    let v = normalize(in.view_pos);

    // hemisphere light
    let w = 0.5 * dot(n, g.hemi_dir.xyz) + 0.5;
    let irradiance = mix(g.ground.rgb, g.sky.rgb, w) * g.hemi_dir.w;
    var out = irradiance * RECIPROCAL_PI * diffuse;

    // key and fill lights
    let lights = array<vec4<f32>, 2>(g.key_light, g.fill_light);
    for (var i = 0; i < 2; i++) {
        let l = lights[i].xyz;
        let irr = saturate(dot(n, l)) * lights[i].w;
        out += irr * RECIPROCAL_PI * diffuse;
        out += irr * brdf_ggx(l, v, n, specular, roughness);
    }
    out += b.emissive.rgb;
    return vec4<f32>(srgb_encode(out), b.color.a);
}

// Feature edges: a plain line colour.
@vertex
fn vs_edge(@location(0) p: vec3<f32>) -> VsOut {
    var o: VsOut;
    let wp = b.model * vec4<f32>(p, 1.0);
    o.pos = g.view_proj * wp;
    o.view_pos = vec3<f32>(0.0);
    o.normal = vec3<f32>(0.0, 0.0, 1.0);
    o.world = wp.xyz;
    return o;
}

@fragment
fn fs_edge(in: VsOut) -> @location(0) vec4<f32> {
    if clipped(in.world) {
        discard;
    }
    return b.edge;
}
