// Shared by every viewport shader. Lighting is in view space, as three.js does it.

struct Globals {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    // xyz: direction to the light in view space; w: intensity
    key_light: vec4<f32>,
    fill_light: vec4<f32>,
    hemi_dir: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
    // section plane (world): keep where dot(xyz, p) + w >= 0
    clip: vec4<f32>,
    // x: 1 when the section plane is on
    flags: vec4<f32>,
};

struct Body {
    // linear rgb, a: opacity
    color: vec4<f32>,
    // linear rgb, w: roughness
    emissive: vec4<f32>,
    // sRGB rgb and alpha of the feature edges
    edge: vec4<f32>,
    // x: metalness, y: 1 for flat shading
    params: vec4<f32>,
    // the body's on-screen transform (a gizmo drag; identity otherwise) and its normal matrix
    model: mat4x4<f32>,
    normal_m: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

fn clipped(world: vec3<f32>) -> bool {
    return g.flags.x > 0.5 && dot(g.clip.xyz, world) + g.clip.w < 0.0;
}
