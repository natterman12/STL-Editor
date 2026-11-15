//! Section view, matching 1.x `viewer.js` (`setSection`, `setSectionEnabled`, `_applySection`).
//!
//! The cut is a three.js `Plane`: keep the half where `dot(n, p) + constant >= 0`. Turning the
//! view off keeps that plane, so the next on uses the same cut (the view bar's Section button).
//! The first time it comes on with no cut yet, it opens halfway up the parts on Z, the way
//! `startSection` does, and keeps the half away from the camera.
//!
//! The cut face is the body's own colour, 25 % darker (`BackSide`, same clip; milestone 7: 1.x
//! painted it red, `#d06060`). The renderer draws it with [`CAP_WGSL`] (cull front, after the
//! opaque bodies): every shown body gets it, a dimmed one too (its `back` mesh is opaque in 1.x);
//! preview meshes get none. With Hatch on (2.0's Section View, on by default there) the cap gets
//! darker diagonal lines in screen space; off, it is the plain darker colour. The grid is drawn
//! before the bodies and caps, so it never shows through either.

use super::camera::{self, V3};
use super::Viewport;

/// How much of the body's colour the cut face keeps (25 % darker). The shader has the same number.
pub const CAP_KEEP: f32 = 0.75;

/// New mesh bytes (vertices, indices, edges) copied to the GPU in one frame.
/// A larger upload continues on the next frames; the previous buffers stay on screen.
pub const UPLOAD_BUDGET_BYTES: u64 = 2 * 1024 * 1024;

/// Which way a cut faces. `startSection` begins on Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionAxis {
    X,
    Y,
    Z,
}

impl SectionAxis {
    fn normal(self) -> V3 {
        match self {
            SectionAxis::X => [1.0, 0.0, 0.0],
            SectionAxis::Y => [0.0, 1.0, 0.0],
            SectionAxis::Z => [0.0, 0.0, 1.0],
        }
    }

    fn index(self) -> usize {
        match self {
            SectionAxis::X => 0,
            SectionAxis::Y => 1,
            SectionAxis::Z => 2,
        }
    }
}

/// `setSection(axis, offset, flip)`: the plane kept from where the camera sits.
///
/// `offset` is the coordinate along `axis` (millimetres). `flip` swaps which half stays.
/// The camera side is `(camera position − target) · axis > 0`, as in `viewer.js`.
pub fn cut_plane(axis: SectionAxis, offset: f64, flip: bool, cam_pos: V3, cam_target: V3) -> [f64; 4] {
    let n = axis.normal();
    let cam_side = camera::dot(camera::sub(cam_pos, cam_target), n) > 0.0;
    let keep_positive = if cam_side { flip } else { !flip };
    if keep_positive {
        [n[0], n[1], n[2], -offset]
    } else {
        [-n[0], -n[1], -n[2], offset]
    }
}

/// The byte range of a mesh upload that fits in this frame. `offset` is how far the previous
/// frames got; the range is empty once `offset >= total`.
pub fn upload_span(offset: u64, total: u64) -> std::ops::Range<u64> {
    let start = offset.min(total);
    let end = start.saturating_add(UPLOAD_BUDGET_BYTES).min(total);
    start..end
}

/// Spacing of the hatch lines on the cut faces (points, along each screen axis).
pub const HATCH_SPACING: f64 = 6.0;

/// Back faces of a body, unlit, in the body's colour times [`CAP_KEEP`]; with Hatch (`flags.y`) darker diagonal
/// lines [`HATCH_SPACING`] points apart (`flags.z` is the frame's pixels per point). Append after `shaders/common.wgsl`
/// (it uses `g` and `clipped`).
///
/// Pipeline, matching three's `MeshBasicMaterial` + `BackSide` + the body's clipping plane:
/// cull front faces, depth compare `Greater`, depth write on, same sample count as the body.
/// Draw each visible body after the opaque pass. The colour is written sRGB, like the flat
/// shader, into the viewport's Unorm target.
pub const CAP_WGSL: &str = r#"
@group(1) @binding(0) var<uniform> cap_body: Body;

struct CapOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_cap(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> CapOut {
    var o: CapOut;
    // the normal is the body's; BackSide cull uses the triangle winding, not this value
    let _n = n.x + n.y + n.z;
    let wp = cap_body.model * vec4<f32>(p + vec3<f32>(_n * 0.0), 1.0);
    o.pos = g.view_proj * wp;
    o.world = wp.xyz;
    return o;
}

@fragment
fn fs_cap(in: CapOut) -> @location(0) vec4<f32> {
    if clipped(in.world) {
        discard;
    }
    // the body's own colour, 25 % darker (the uniform holds it linear; the flat shader's output
    // is sRGB encoded)
    let cap = cap_srgb(cap_body.color.rgb) * 0.75;
    if g.flags.y > 0.5 {
        // Hatch: diagonal lines at 45°, HATCH_SPACING points apart in screen space (so they
        // read the same at any zoom), about a point wide, softened at each edge.
        let s = max(g.flags.z, 1.0);
        let spacing = 6.0 * s;
        let t = fract((in.pos.x + in.pos.y) / spacing) * spacing;
        let dist = min(t, spacing - t);
        let half = 0.5 * s;
        let k = 1.0 - smoothstep(half, half + 0.8, dist);
        return vec4<f32>(mix(cap, cap * HATCH_DARKEN, k), 1.0);
    }
    return vec4<f32>(cap, 1.0);
}

// how dark the hatch lines are against the cap colour
const HATCH_DARKEN: f32 = 0.6;

fn cap_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(0.41666)) * 1.055 - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}
"#;

impl Viewport {
    /// The view bar's Section button and the X key: the one way to turn the view on or off.
    ///
    /// Off leaves the plane where it is. On, with no plane yet, cuts on Z through the middle
    /// of the parts (or through the origin when the scene is empty). Always asks for a repaint.
    pub fn set_section(&mut self, on: bool) {
        if on && self.section_plane.is_none() {
            self.section_plane = Some(self.opening_plane());
        }
        self.section_on = on;
        self.changed();
    }

    /// `setSection(axis, offset, flip)` from the current camera. Does not turn the view on;
    /// [`set_section`](Self::set_section) does that. `None` removes the cut, as `setSection(null)`.
    pub fn set_section_cut(&mut self, cut: Option<(SectionAxis, f64, bool)>) {
        self.section_plane = cut.map(|(axis, offset, flip)| cut_plane(axis, offset, flip, self.camera.position, self.camera.target));
        self.changed();
    }

    /// Halfway up the parts on Z. An empty scene uses the same stand-in box as `startSection`
    /// (`[-50, -50, -50]` .. `[50, 50, 50]`), not the camera's fit fallback.
    fn opening_plane(&self) -> [f64; 4] {
        let (lo, hi) = match self.scene_box() {
            Some(b) => (b.min, b.max),
            None => ([-50.0, -50.0, -50.0], [50.0, 50.0, 50.0]),
        };
        let axis = SectionAxis::Z;
        let offset = (lo[axis.index()] + hi[axis.index()]) * 0.5;
        cut_plane(axis, offset, false, self.camera.position, self.camera.target)
    }
}
