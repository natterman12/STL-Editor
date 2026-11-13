// Unlit overlays, grid, axes and planes: three's MeshBasicMaterial / LineBasicMaterial. The
// colours come sRGB encoded and are written as they are.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) c0: vec4<f32>,
    @location(1) c1: vec4<f32>,
    @location(2) t: f32,
};

@vertex
fn vs_flat(@location(0) p: vec3<f32>, @location(1) c0: vec4<f32>, @location(2) c1: vec4<f32>, @location(3) t: f32) -> VsOut {
    var o: VsOut;
    o.pos = g.view_proj * vec4<f32>(p, 1.0);
    o.c0 = c0;
    o.c1 = c1;
    o.t = t;
    return o;
}

@fragment
fn fs_flat(in: VsOut) -> @location(0) vec4<f32> {
    return select(in.c0, in.c1, in.t >= 0.5);
}
