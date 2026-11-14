#![allow(unexpected_cfgs)]
//! The GPU side: one buffer set per body, uploaded only when the body's revision changes; a
//! large upload is copied a few megabytes at a time so one frame stays short. The scene is
//! rendered into the viewport's own 4x MSAA target (with depth), then copied into egui's pass.
//! The render is skipped when nothing changed since the last one.
//!
//! Errors: each render is encoded into the viewport's own command buffer inside error scopes.
//! A validation or out-of-memory error drops that buffer, the size-bound targets and any
//! half-copied upload, and keeps every finished body buffer (the device is still fine). A lost
//! device is never touched again; when egui hands the callback a new device (a fresh
//! `callback_resources` without a [`GpuState`]), a new state is made there and every body is
//! uploaded again from the prepared meshes the UI keeps.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::overlays::Depth;
use super::scene::{as_bytes, FlatVertex, LitVertex, PreparedMesh};

pub const MSAA: u32 = 4;
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const LIT_SHADER: &str = concat!(include_str!("shaders/common.wgsl"), include_str!("shaders/lit.wgsl"));
const FLAT_SHADER: &str = concat!(include_str!("shaders/common.wgsl"), include_str!("shaders/flat.wgsl"));
const BLIT_SHADER: &str = include_str!("shaders/blit.wgsl");

/// Matches `Globals` in `shaders/common.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub key_light: [f32; 4],
    pub fill_light: [f32; 4],
    pub hemi_dir: [f32; 4],
    pub sky: [f32; 4],
    pub ground: [f32; 4],
    pub clip: [f32; 4],
    pub flags: [f32; 4],
}

/// Matches `Body` in `shaders/common.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyUniform {
    pub color: [f32; 4],
    pub emissive: [f32; 4],
    pub edge: [f32; 4],
    pub params: [f32; 4],
    /// Where the body is drawn: identity, or a gizmo drag's transform (column-major). Moving a
    /// body this way needs no new mesh upload.
    pub model: [[f32; 4]; 4],
    /// The inverse transpose of `model`'s 3x3 part, for the normals.
    pub normal: [[f32; 4]; 4],
}

pub const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

impl Default for BodyUniform {
    fn default() -> Self {
        BodyUniform { color: [0.0; 4], emissive: [0.0; 4], edge: [0.0; 4], params: [0.0; 4], model: IDENTITY, normal: IDENTITY }
    }
}

/// A body (or a preview mesh) to draw this frame.
#[derive(Clone, Debug)]
pub struct BodyDraw {
    /// Cache key of its GPU buffers.
    pub key: String,
    /// The buffers are uploaded again only when this changes.
    pub revision: u64,
    pub mesh: Arc<PreparedMesh>,
    pub uniform: BodyUniform,
    pub visible: bool,
    /// Drawn after the opaque pass, without depth writes (dimmed bodies, ghost fins).
    pub transparent: bool,
    pub edges: bool,
    /// Gets the section cut face (a body, dimmed or not; never a preview mesh).
    pub cap: bool,
    /// Distance from the camera, for back-to-front order of transparent bodies.
    pub depth: f64,
}

/// A run of the flat vertex buffer drawn with one pipeline.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatBatch {
    /// three's `renderOrder` (the grid is -1, drawn before the transparent bodies).
    pub order: i32,
    pub lines: bool,
    pub depth: Depth,
    pub range: Range<u32>,
}

/// Everything one render needs, built by [`super::Viewport::show`] on the UI thread.
#[derive(Clone, Debug, Default)]
pub struct FrameData {
    /// Changes whenever anything in the picture changes.
    pub id: u64,
    pub size: [u32; 2],
    /// The viewport background, sRGB encoded (CSS `--bg`).
    pub clear: [f64; 4],
    pub globals: Globals,
    pub bodies: Vec<BodyDraw>,
    pub flat_version: u64,
    pub flat: Arc<Vec<FlatVertex>>,
    /// Sorted by `order`.
    pub batches: Vec<FlatBatch>,
    /// The transform handles (drawn last, over everything): their own small buffer, rebuilt
    /// when the gizmo or the camera moves without touching the overlays' buffer.
    pub gizmo_version: u64,
    pub gizmo: Arc<Vec<FlatVertex>>,
    pub gizmo_batches: Vec<FlatBatch>,
}

/// Counters the tests and `uibench` read.
#[derive(Debug, Default)]
pub struct Stats {
    /// Body meshes sent to the GPU. A mesh counts once, when its last chunk has been copied.
    pub uploads: AtomicU64,
    /// Scene renders (a frame that only copies the last render isn't counted).
    pub renders: AtomicU64,
    /// A changed body still has bytes to copy. [`super::Viewport::show`] asks for another frame.
    pub upload_pending: AtomicBool,
    /// GPU states made (one per device the viewport has drawn on).
    pub gpu_states: AtomicU64,
    /// Renders dropped because the GPU reported an error.
    pub gpu_errors: AtomicU64,
    /// The device the viewport draws on was lost (cleared when a new one takes over).
    pub device_lost: AtomicBool,
    /// The revision each body key has on the GPU: what is on screen. The viewport picks and
    /// highlights against that revision until a newer one lands.
    on_gpu: Mutex<HashMap<String, u64>>,
}

impl Stats {
    pub fn uploads(&self) -> u64 {
        self.uploads.load(Ordering::Relaxed)
    }
    pub fn renders(&self) -> u64 {
        self.renders.load(Ordering::Relaxed)
    }
    pub fn gpu_states(&self) -> u64 {
        self.gpu_states.load(Ordering::Relaxed)
    }
    pub fn gpu_errors(&self) -> u64 {
        self.gpu_errors.load(Ordering::Relaxed)
    }
    pub fn device_lost(&self) -> bool {
        self.device_lost.load(Ordering::Relaxed)
    }
    /// The revision of `key` on the GPU, if any.
    pub fn on_gpu(&self, key: &str) -> Option<u64> {
        self.on_gpu_map().get(key).copied()
    }
    pub(crate) fn on_gpu_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, u64>> {
        self.on_gpu.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What one [`render_frame`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    /// There is a picture to paint.
    pub shown: bool,
    /// A body upload still has bytes to copy: draw another frame.
    pub more: bool,
    /// A body upload finished: the UI should look again (it picks against the new revision).
    pub uploaded: bool,
    /// The GPU reported an error; the frame was dropped and will be drawn again.
    pub error: bool,
}

struct GpuBody {
    revision: u64,
    vertices: wgpu::Buffer,
    indices: Option<wgpu::Buffer>,
    count: u32,
    edges: Option<(wgpu::Buffer, u32)>,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    blit_bind: wgpu::BindGroup,
}

struct Pipelines {
    body_opaque: wgpu::RenderPipeline,
    /// Back faces, unlit `#d06060`, drawn only while the section cut is on.
    cap: wgpu::RenderPipeline,
    body_transparent: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    /// (lines, depth) -> pipeline
    flat: HashMap<(bool, Depth), wgpu::RenderPipeline>,
    blit: wgpu::RenderPipeline,
}

/// The viewport's GPU resources. Lives in egui-wgpu's `callback_resources`.
pub struct GpuState {
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    body_layout: wgpu::BindGroupLayout,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bodies: HashMap<String, GpuBody>,
    /// A revision still being copied. The previous [`GpuBody`] stays on screen until this finishes.
    pending: HashMap<String, PendingUpload>,
    flat: Option<(u64, wgpu::Buffer, u64)>,
    /// The gizmo's vertex buffer: (version, buffer, capacity).
    gizmo: Option<(u64, wgpu::Buffer, u64)>,
    targets: Option<Targets>,
    last: Option<(u64, [u32; 2])>,
    max_dim: u32,
    /// Set by the device-lost callback.
    lost: Arc<AtomicBool>,
    /// Set when an error scope reports an error after the frame (WebGPU resolves them later).
    late_error: Arc<AtomicBool>,
    pub stats: Arc<Stats>,
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn blend_alpha() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

struct PipeDesc<'a> {
    label: &'a str,
    module: &'a wgpu::ShaderModule,
    layout: &'a wgpu::PipelineLayout,
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    topology: wgpu::PrimitiveTopology,
    cull: Option<wgpu::Face>,
    depth_write: bool,
    compare: wgpu::CompareFunction,
    bias: wgpu::DepthBiasState,
    blend: Option<wgpu::BlendState>,
}

fn pipeline(device: &wgpu::Device, d: PipeDesc<'_>) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(d.label),
        layout: Some(d.layout),
        vertex: wgpu::VertexState {
            module: d.module,
            entry_point: Some(d.vs),
            buffers: d.buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: d.topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: d.cull,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(d.depth_write),
            depth_compare: Some(d.compare),
            stencil: wgpu::StencilState::default(),
            bias: d.bias,
        }),
        multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState {
            module: d.module,
            entry_point: Some(d.fs),
            targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: d.blend, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Reversed depth: nearer is larger. A body is pushed back a little (`polygonOffset 1, 1`) so
/// its edges win, and faces painted on it pulled forward (`polygonOffset -2, -2`).
const BODY_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState { constant: -1, slope_scale: -1.0, clamp: 0.0 };
const OVERLAY_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 };
const NO_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState { constant: 0, slope_scale: 0.0, clamp: 0.0 };

/// One body's new buffers, filled [`super::section::UPLOAD_BUDGET_BYTES`] at a time.
struct PendingUpload {
    revision: u64,
    vertices: wgpu::Buffer,
    vert_len: u64,
    vert_done: u64,
    indices: Option<wgpu::Buffer>,
    index_len: u64,
    index_done: u64,
    count: u32,
    edges: Option<wgpu::Buffer>,
    edge_count: u32,
    edge_len: u64,
    edge_done: u64,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

fn gpu_buffer(device: &wgpu::Device, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let align = wgpu::COPY_BUFFER_ALIGNMENT as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(align).next_multiple_of(align),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// How many bytes of `data` to copy this frame, starting at `done`. A multiple of 4, and at
/// most the bytes still allowed in `budget`.
fn chunk_len(done: u64, len: u64, budget: u64) -> u64 {
    if done >= len || budget < wgpu::COPY_BUFFER_ALIGNMENT as u64 {
        return 0;
    }
    let span = super::section::upload_span(done, len);
    let mut n = (span.end - span.start).min(budget);
    n -= n % wgpu::COPY_BUFFER_ALIGNMENT as u64;
    n
}

fn copy_chunk(queue: &wgpu::Queue, budget: &mut u64, buf: &wgpu::Buffer, data: &[u8], done: &mut u64) {
    if *done >= data.len() as u64 {
        return;
    }
    let n = chunk_len(*done, data.len() as u64, *budget);
    if n == 0 {
        // A tail shorter than the copy alignment: pad it out and finish.
        let rest = data.len() as u64 - *done;
        if rest > 0 && rest < wgpu::COPY_BUFFER_ALIGNMENT as u64 && *budget >= wgpu::COPY_BUFFER_ALIGNMENT as u64 {
            let mut padded = [0u8; 4];
            padded[..rest as usize].copy_from_slice(&data[*done as usize..]);
            queue.write_buffer(buf, *done, &padded);
            *done = data.len() as u64;
            *budget -= wgpu::COPY_BUFFER_ALIGNMENT as u64;
        }
        return;
    }
    let start = *done as usize;
    queue.write_buffer(buf, *done, &data[start..start + n as usize]);
    *done += n;
    *budget -= n;
}

impl PendingUpload {
    fn start(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, d: &BodyDraw) -> Self {
        let m = &d.mesh;
        let vert_bytes = as_bytes(&m.vertices);
        let index_bytes = m.indices.as_ref().map(|i| as_bytes(i));
        let edge_bytes = m.edges.as_ref().filter(|e| !e.is_empty()).map(|e| as_bytes(e));
        let vertices = gpu_buffer(device, "viewport body vertices", vert_bytes.len() as u64, wgpu::BufferUsages::VERTEX);
        let indices = index_bytes.map(|b| gpu_buffer(device, "viewport body indices", b.len() as u64, wgpu::BufferUsages::INDEX));
        let edges = edge_bytes.map(|b| gpu_buffer(device, "viewport body edges", b.len() as u64, wgpu::BufferUsages::VERTEX));
        let count = m.indices.as_ref().map_or(m.vertices.len(), |i| i.len()) as u32;
        let edge_count = m.edges.as_ref().map(|e| e.len()).unwrap_or(0) as u32;
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport body uniform"),
            size: std::mem::size_of::<BodyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport body"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        PendingUpload {
            revision: d.revision,
            vertices,
            vert_len: vert_bytes.len() as u64,
            vert_done: 0,
            indices,
            index_len: index_bytes.map(|b| b.len() as u64).unwrap_or(0),
            index_done: 0,
            count,
            edges,
            edge_count,
            edge_len: edge_bytes.map(|b| b.len() as u64).unwrap_or(0),
            edge_done: 0,
            uniform,
            bind,
        }
    }

    /// Copy the next slice. True when vertices, indices and edges have all landed.
    fn copy(&mut self, queue: &wgpu::Queue, d: &BodyDraw, budget: &mut u64) -> bool {
        let m = &d.mesh;
        copy_chunk(queue, budget, &self.vertices, as_bytes(&m.vertices), &mut self.vert_done);
        if let (Some(buf), Some(idx)) = (&self.indices, m.indices.as_ref()) {
            copy_chunk(queue, budget, buf, as_bytes(idx), &mut self.index_done);
        }
        if let (Some(buf), Some(edges)) = (&self.edges, m.edges.as_ref().filter(|e| !e.is_empty())) {
            copy_chunk(queue, budget, buf, as_bytes(edges), &mut self.edge_done);
        }
        self.vert_done >= self.vert_len && self.index_done >= self.index_len && self.edge_done >= self.edge_len
    }

    fn into_body(self) -> GpuBody {
        GpuBody {
            revision: self.revision,
            vertices: self.vertices,
            indices: self.indices,
            count: self.count,
            edges: self.edges.map(|b| (b, self.edge_count)),
            uniform: self.uniform,
            bind: self.bind,
        }
    }
}

impl GpuState {
    /// `target_format` is the format of egui's pass (`RenderState::target_format`).
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, stats: Arc<Stats>) -> Self {
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport globals"),
            entries: &[uniform_entry(0)],
        });
        let body_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport body"),
            entries: &[uniform_entry(0)],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let lit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport lit"),
            bind_group_layouts: &[Some(&globals_layout), Some(&body_layout)],
            immediate_size: 0,
        });
        let flat_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport flat"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport blit"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let lit = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport lit"),
            source: wgpu::ShaderSource::Wgsl(LIT_SHADER.into()),
        });
        let cap_wgsl = include_str!("shaders/common.wgsl").to_owned() + super::section::CAP_WGSL;
        let cap_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport cap"),
            source: wgpu::ShaderSource::Wgsl(cap_wgsl.into()),
        });
        let flat = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport flat"),
            source: wgpu::ShaderSource::Wgsl(FLAT_SHADER.into()),
        });
        let blit = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let lit_vb = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LitVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        })];
        let edge_vb = [Some(wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        })];
        let flat_vb = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<FlatVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Float32],
        })];
        use wgpu::CompareFunction::*;
        use wgpu::PrimitiveTopology::*;
        let body = |label, depth_write, blend| {
            pipeline(
                device,
                PipeDesc {
                    label,
                    module: &lit,
                    layout: &lit_layout,
                    vs: "vs_body",
                    fs: "fs_body",
                    buffers: &lit_vb,
                    topology: TriangleList,
                    cull: Some(wgpu::Face::Back),
                    depth_write,
                    compare: Greater,
                    bias: BODY_BIAS,
                    blend,
                },
            )
        };
        let body_opaque = body("viewport body", true, None);
        let cap = pipeline(
            device,
            PipeDesc {
                label: "viewport cap",
                module: &cap_mod,
                layout: &lit_layout,
                vs: "vs_cap",
                fs: "fs_cap",
                buffers: &lit_vb,
                topology: TriangleList,
                cull: Some(wgpu::Face::Front),
                depth_write: true,
                compare: Greater,
                // pushed back like the bodies (three's polygonOffset 1, 1), so grid lines on the
                // cap's plane (a part standing on the bed) win cleanly instead of z-fighting
                bias: BODY_BIAS,
                blend: None,
            },
        );
        let body_transparent = body("viewport body (see-through)", false, Some(blend_alpha()));
        let edge = pipeline(
            device,
            PipeDesc {
                label: "viewport edges",
                module: &lit,
                layout: &lit_layout,
                vs: "vs_edge",
                fs: "fs_edge",
                buffers: &edge_vb,
                topology: LineList,
                cull: None,
                depth_write: false,
                compare: GreaterEqual,
                bias: NO_BIAS,
                blend: Some(blend_alpha()),
            },
        );
        let mut flat_pipes = HashMap::new();
        for lines in [false, true] {
            for depth in [Depth::Test, Depth::TestOffset, Depth::Off] {
                let (compare, bias) = match depth {
                    Depth::Test => (GreaterEqual, NO_BIAS),
                    Depth::TestOffset if !lines => (GreaterEqual, OVERLAY_BIAS),
                    Depth::TestOffset => (GreaterEqual, NO_BIAS),
                    Depth::Off => (Always, NO_BIAS),
                };
                let p = pipeline(
                    device,
                    PipeDesc {
                        label: "viewport flat",
                        module: &flat,
                        layout: &flat_layout,
                        vs: "vs_flat",
                        fs: "fs_flat",
                        buffers: &flat_vb,
                        topology: if lines { LineList } else { TriangleList },
                        cull: None,
                        depth_write: false,
                        compare,
                        bias,
                        blend: Some(blend_alpha()),
                    },
                );
                flat_pipes.insert((lines, depth), p);
            }
        }
        let blit_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport blit"),
            layout: Some(&blit_pl),
            vertex: wgpu::VertexState {
                module: &blit,
                entry_point: Some("vs_blit"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blit,
                entry_point: Some(if target_format.is_srgb() { "fs_blit_srgb" } else { "fs_blit" }),
                targets: &[Some(wgpu::ColorTargetState { format: target_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("viewport blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let lost = Arc::new(AtomicBool::new(false));
        {
            let lost = lost.clone();
            device.set_device_lost_callback(move |_reason, _msg| {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("viewport: GPU device lost ({_reason:?}): {_msg}");
                lost.store(true, Ordering::Relaxed);
            });
        }
        stats.on_gpu_map().clear();
        stats.device_lost.store(false, Ordering::Relaxed);
        stats.gpu_states.fetch_add(1, Ordering::Relaxed);
        GpuState {
            pipelines: Pipelines { body_opaque, cap, body_transparent, edge, flat: flat_pipes, blit: blit_pipe },
            globals,
            globals_bind,
            body_layout,
            blit_layout,
            sampler,
            bodies: HashMap::new(),
            pending: HashMap::new(),
            flat: None,
            gizmo: None,
            targets: None,
            last: None,
            max_dim: device.limits().max_texture_dimension_2d,
            lost,
            late_error: Arc::new(AtomicBool::new(false)),
            stats,
        }
    }

    /// The device is gone: nothing may be made or drawn with it any more.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// After a GPU error on a device that is still alive: make the targets, the overlay buffer
    /// and any half-copied upload again; keep every finished body buffer.
    fn recover_from_error(&mut self) {
        self.targets = None;
        self.last = None;
        self.flat = None;
        self.gizmo = None;
        self.pending.clear();
        self.stats.gpu_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn publish(&self) {
        let mut m = self.stats.on_gpu_map();
        m.clear();
        m.extend(self.bodies.iter().map(|(k, g)| (k.clone(), g.revision)));
    }

    /// Encode this frame's uploads and render into the viewport's own command buffer, inside
    /// error scopes. No buffer when it is invalid (an error was caught).
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData) -> (Rendered, Option<wgpu::CommandBuffer>) {
        if self.late_error.swap(false, Ordering::Relaxed) {
            self.recover_from_error();
        }
        let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("viewport") });
        let uploads_before = self.stats.uploads();
        let shown = self.prepare(device, queue, &mut enc, frame);
        let cmd = enc.finish();
        let failed_v = scope_failed(validation.pop(), &self.late_error);
        let failed_o = scope_failed(oom.pop(), &self.late_error);
        let more = self.stats.upload_pending.load(Ordering::Relaxed);
        let uploaded = self.stats.uploads() != uploads_before;
        if failed_v || failed_o {
            self.recover_from_error();
            return (Rendered { shown: false, more, uploaded, error: true }, None);
        }
        (Rendered { shown, more, uploaded, error: false }, Some(cmd))
    }

    /// The window was minimised, resized or its surface lost: drop the size-bound targets
    /// (they are made again at the next render) and keep every uploaded body.
    pub fn surface_lost(&mut self) {
        self.targets = None;
        self.last = None;
    }

    /// Number of bodies with buffers on the GPU.
    pub fn bodies_on_gpu(&self) -> usize {
        self.bodies.len()
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let tex = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let msaa = tex("viewport msaa", COLOR_FORMAT, MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = tex("viewport depth", DEPTH_FORMAT, MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = tex(
            "viewport image",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let resolve_view = resolve.create_view(&wgpu::TextureViewDescriptor::default());
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport blit"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&resolve_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.targets = Some(Targets {
            size,
            msaa: msaa.create_view(&wgpu::TextureViewDescriptor::default()),
            depth: depth.create_view(&wgpu::TextureViewDescriptor::default()),
            resolve,
            resolve_view,
            blit_bind,
        });
        self.last = None;
    }

    /// Copy at most [`super::section::UPLOAD_BUDGET_BYTES`] of changed meshes. The previous
    /// buffers stay drawable until the new revision's last chunk lands; `uploads` ticks then.
    /// Returns whether a later frame must copy more.
    fn pump_uploads(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData) -> bool {
        let live = |k: &String| frame.bodies.iter().any(|b| &b.key == k);
        let before = self.bodies.len();
        self.bodies.retain(|k, _| live(k));
        let mut changed = self.bodies.len() != before;
        self.pending.retain(|k, _| live(k));
        let mut budget = super::section::UPLOAD_BUDGET_BYTES;
        let mut more = false;
        for d in &frame.bodies {
            if self.bodies.get(&d.key).is_some_and(|g| g.revision == d.revision) {
                self.pending.remove(&d.key);
                continue;
            }
            let stale = self.pending.get(&d.key).is_none_or(|p| p.revision != d.revision);
            if stale {
                // Allocating the buffers is cheap; the copy is what this budget limits. Don't
                // start another body once this frame's bytes are spent.
                if budget < wgpu::COPY_BUFFER_ALIGNMENT as u64 {
                    more = true;
                    continue;
                }
                let layout = self.body_layout.clone();
                self.pending.insert(d.key.clone(), PendingUpload::start(device, &layout, d));
            }
            let job = self.pending.get_mut(&d.key).expect("just inserted");
            if job.copy(queue, d, &mut budget) {
                let job = self.pending.remove(&d.key).expect("finished job");
                self.bodies.insert(d.key.clone(), job.into_body());
                self.stats.uploads.fetch_add(1, Ordering::Relaxed);
                self.last = None;
                changed = true;
            } else {
                more = true;
            }
        }
        if changed {
            self.publish();
        }
        more
    }

    /// Bring the GPU up to date with `frame` and render it, unless the last render already
    /// shows it. Returns false when there is nothing to show (zero size).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, frame: &FrameData) -> bool {
        let size = [frame.size[0].min(self.max_dim), frame.size[1].min(self.max_dim)];
        let more = self.pump_uploads(device, queue, frame);
        self.stats.upload_pending.store(more, Ordering::Relaxed);
        if size[0] == 0 || size[1] == 0 {
            return false;
        }
        self.ensure_targets(device, size);
        if self.last == Some((frame.id, size)) {
            return true;
        }
        // flat geometry
        if self.flat.as_ref().is_none_or(|f| f.0 != frame.flat_version) {
            let bytes = as_bytes(&frame.flat[..]);
            if let Some((v, buf, cap)) = self.flat.as_mut().filter(|f| f.2 >= bytes.len() as u64) {
                queue.write_buffer(buf, 0, &bytes[..bytes.len() / 4 * 4]);
                *v = frame.flat_version;
                let _ = cap;
            } else {
                let cap = (bytes.len() as u64).max(1024).next_power_of_two();
                let buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("viewport overlays"),
                    size: cap,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                queue.write_buffer(&buf, 0, bytes);
                self.flat = Some((frame.flat_version, buf, cap));
            }
        }
        if frame.gizmo.is_empty() {
            // nothing to keep
        } else if self.gizmo.as_ref().is_none_or(|f| f.0 != frame.gizmo_version) {
            let bytes = as_bytes(&frame.gizmo[..]);
            if let Some((v, buf, _)) = self.gizmo.as_mut().filter(|f| f.2 >= bytes.len() as u64) {
                queue.write_buffer(buf, 0, &bytes[..bytes.len() / 4 * 4]);
                *v = frame.gizmo_version;
            } else {
                let cap = (bytes.len() as u64).max(1024).next_power_of_two();
                let buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("viewport gizmo"),
                    size: cap,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                queue.write_buffer(&buf, 0, bytes);
                self.gizmo = Some((frame.gizmo_version, buf, cap));
            }
        }
        queue.write_buffer(&self.globals, 0, as_bytes(std::slice::from_ref(&frame.globals)));
        for d in &frame.bodies {
            if let Some(g) = self.bodies.get(&d.key) {
                queue.write_buffer(&g.uniform, 0, as_bytes(std::slice::from_ref(&d.uniform)));
            }
        }

        let t = self.targets.as_ref().expect("targets were just made");
        let [r, gc, b, a] = frame.clear;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewport"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.msaa,
                depth_slice: None,
                resolve_target: Some(&t.resolve_view),
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r, g: gc, b, a }), store: wgpu::StoreOp::Discard },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.globals_bind, &[]);

        let draw_body = |pass: &mut wgpu::RenderPass<'_>, g: &GpuBody| {
            pass.set_bind_group(1, &g.bind, &[]);
            pass.set_vertex_buffer(0, g.vertices.slice(..));
            if let Some(i) = &g.indices {
                pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..g.count, 0, 0..1);
            } else {
                pass.draw(0..g.count, 0..1);
            }
        };
        // 1. opaque bodies
        pass.set_pipeline(&self.pipelines.body_opaque);
        for d in frame.bodies.iter().filter(|d| d.visible && !d.transparent) {
            if let Some(g) = self.bodies.get(&d.key) {
                draw_body(&mut pass, g);
            }
        }
        // 1b. section cap: the body's back faces, unlit, clipped by the same plane. A dimmed
        // body keeps its opaque cap (viewer.js `o.back`); preview meshes have none.
        if frame.globals.flags[0] > 0.5 {
            pass.set_pipeline(&self.pipelines.cap);
            for d in frame.bodies.iter().filter(|d| d.visible && d.cap) {
                if let Some(g) = self.bodies.get(&d.key) {
                    draw_body(&mut pass, g);
                }
            }
        }
        let flat_buf = self.flat.as_ref().map(|f| &f.1);
        let draw_batches = |pass: &mut wgpu::RenderPass<'_>, keep: &dyn Fn(i32) -> bool| {
            let Some(buf) = flat_buf else { return };
            for bt in frame.batches.iter().filter(|b| keep(b.order) && !b.range.is_empty()) {
                pass.set_pipeline(&self.pipelines.flat[&(bt.lines, bt.depth)]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(bt.range.clone(), 0..1);
            }
        };
        // 2. the grid (renderOrder -1), depth-tested against the bodies and the cap as in 1.x:
        // it shows on the cut face where the cap's back faces lie on or under the grid's plane.
        // That is the "hatch" on 1.x's cut faces (viewer.js's cap itself is a flat #d06060).
        draw_batches(&mut pass, &|o| o < 0);
        // 3. see-through bodies and every body's edges, back to front
        let mut order: Vec<&BodyDraw> = frame.bodies.iter().filter(|d| d.visible).collect();
        order.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        for d in order {
            let Some(g) = self.bodies.get(&d.key) else { continue };
            if d.transparent {
                pass.set_pipeline(&self.pipelines.body_transparent);
                draw_body(&mut pass, g);
            }
            if let (true, Some((eb, n))) = (d.edges, &g.edges) {
                pass.set_pipeline(&self.pipelines.edge);
                pass.set_bind_group(1, &g.bind, &[]);
                pass.set_vertex_buffer(0, eb.slice(..));
                pass.draw(0..*n, 0..1);
            }
        }
        // 4. planes, overlays, axes, sketches, markers... by renderOrder
        draw_batches(&mut pass, &|o| o >= 0);
        // 5. the transform handles (three's renderOrder Infinity, no depth test)
        if let (false, Some((_, buf, _))) = (frame.gizmo.is_empty(), self.gizmo.as_ref()) {
            for bt in frame.gizmo_batches.iter().filter(|b| !b.range.is_empty()) {
                pass.set_pipeline(&self.pipelines.flat[&(bt.lines, bt.depth)]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(bt.range.clone(), 0..1);
            }
        }
        drop(pass);
        self.last = Some((frame.id, size));
        self.stats.renders.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Draw the last render into egui's pass (its viewport is the widget's rectangle).
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'static>) {
        let Some(t) = &self.targets else { return };
        if self.last.is_none() {
            return;
        }
        pass.set_pipeline(&self.pipelines.blit);
        pass.set_bind_group(0, &t.blit_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The last render, RGBA8 rows (sRGB encoded), for tests and screenshots.
    pub fn read_image(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<(u32, u32, Vec<u8>)> {
        let t = self.targets.as_ref()?;
        let [w, h] = t.size;
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("viewport readback") });
        enc.copy_texture_to_buffer(
            t.resolve.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let idx = queue.submit(std::iter::once(enc.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
        device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: None }).ok()?;
        rx.recv().ok()?.ok()?;
        let data = buf.slice(..).get_mapped_range().ok()?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for r in data.chunks_exact(row as usize) {
            out.extend_from_slice(&r[..(w * 4) as usize]);
        }
        Some((w, h, out))
    }
}

/// Whether an error scope caught an error. Native scopes resolve at once; WebGPU's resolve
/// later and set `late` for the next frame.
fn scope_failed(fut: impl std::future::Future<Output = Option<wgpu::Error>> + 'static, late: &Arc<AtomicBool>) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = late;
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        let mut fut = std::pin::pin!(fut);
        match fut.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(Some(e)) => {
                eprintln!("viewport: GPU error, frame dropped: {e}");
                true
            }
            _ => false,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let late = late.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if fut.await.is_some() {
                late.store(true, Ordering::Relaxed);
            }
        });
        false
    }
}

/// One frame on the GPU, from egui's paint callback or [`Headless`]: make the [`GpuState`] on
/// this device when `resources` has none (a new or recreated renderer), skip everything on a
/// lost device, otherwise upload and render. Submit the returned buffer before the pass that
/// paints the picture (egui submits a callback's buffers before its own).
pub fn render_frame(
    resources: &mut egui_wgpu::CallbackResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    stats: &Arc<Stats>,
    frame: &FrameData,
) -> (Rendered, Option<wgpu::CommandBuffer>) {
    if resources.get::<GpuState>().is_some_and(|g| g.is_lost()) {
        stats.device_lost.store(true, Ordering::Relaxed);
        stats.upload_pending.store(false, Ordering::Relaxed);
        return (Rendered::default(), None);
    }
    if resources.get::<GpuState>().is_none() {
        resources.insert(GpuState::new(device, format, stats.clone()));
    }
    let gpu = resources.get_mut::<GpuState>().expect("just inserted");
    gpu.render(device, queue, frame)
}

/// The egui paint callback: carries one frame's data to the GPU.
pub struct ViewportCallback {
    pub frame: Arc<FrameData>,
    pub ctx: egui::Context,
    /// egui's target format, for a [`GpuState`] made on a new device.
    pub format: wgpu::TextureFormat,
    pub stats: Arc<Stats>,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let (r, cmd) = render_frame(resources, device, queue, self.format, &self.stats, &self.frame);
        if r.more || r.uploaded || r.error {
            self.ctx.request_repaint();
        }
        cmd.into_iter().collect()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        if let Some(gpu) = resources.get::<GpuState>().filter(|g| !g.is_lost()) {
            gpu.paint(pass);
        }
    }
}

/// Runs a future that wgpu resolves without an event loop (native adapters and devices).
#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

/// A GPU device without a window, for tests, `uibench` and screenshots.
#[cfg(not(target_arch = "wasm32"))]
pub struct Headless {
    pub render_state: egui_wgpu::RenderState,
}

#[cfg(not(target_arch = "wasm32"))]
impl Headless {
    /// `None` when the machine has no usable GPU adapter.
    pub fn new() -> Option<Self> {
        let setup = egui_wgpu::WgpuSetup::without_display_handle();
        let instance = block_on(setup.new_instance());
        let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
        let render_state =
            block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::default())).ok()?;
        Some(Headless { render_state })
    }

    /// Render the viewport at `w` x `h` pixels (as if shown in a `w` x `h` point rectangle at
    /// one pixel per point) and read the picture back: RGBA8 rows. `None` for a zero size.
    pub fn render(&self, vp: &mut super::Viewport, w: u32, h: u32) -> Option<Vec<u8>> {
        let (r, _) = self.frame(vp, w, h);
        if !r.shown {
            return None;
        }
        let renderer = self.render_state.renderer.read();
        let gpu = renderer.callback_resources.get::<GpuState>().filter(|g| !g.is_lost())?;
        gpu.read_image(&self.render_state.device, &self.render_state.queue).map(|(_, _, px)| px)
    }

    /// Render without reading back, and wait for the GPU to finish: one frame's cost (for
    /// `uibench` and the timing test). False for a zero size.
    pub fn render_and_wait(&self, vp: &mut super::Viewport, w: u32, h: u32) -> bool {
        let (r, idx) = self.frame(vp, w, h);
        if let Some(idx) = idx {
            let _ = self.render_state.device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: None });
        }
        r.shown
    }

    /// One frame through [`render_frame`], the paint callback's path, without egui.
    pub fn frame(&self, vp: &mut super::Viewport, w: u32, h: u32) -> (Rendered, Option<wgpu::SubmissionIndex>) {
        let rs = &self.render_state;
        if !vp.has_gpu() {
            vp.init_gpu(rs);
        }
        let frame = vp.frame_for_size(w as f32, h as f32, 1.0);
        self.submit(vp, &frame)
    }

    /// [`render_frame`] with a given frame (tests hand it broken frames).
    pub fn submit(&self, vp: &super::Viewport, frame: &FrameData) -> (Rendered, Option<wgpu::SubmissionIndex>) {
        let rs = &self.render_state;
        let mut renderer = rs.renderer.write();
        let (r, cmd) = render_frame(&mut renderer.callback_resources, &rs.device, &rs.queue, rs.target_format, &vp.stats, frame);
        drop(renderer);
        (r, cmd.map(|c| rs.queue.submit(std::iter::once(c))))
    }

    /// As when the window's surface is lost or the window is minimised.
    pub fn surface_lost(&self) {
        if let Some(gpu) = self.render_state.renderer.write().callback_resources.get_mut::<GpuState>() {
            gpu.surface_lost();
        }
    }

    pub fn bodies_on_gpu(&self) -> usize {
        self.render_state.renderer.read().callback_resources.get::<GpuState>().map_or(0, |g| g.bodies_on_gpu())
    }
}

/// A device for `xtask web --measure-preview`. Compiled only with `--cfg stl_preview_measure`.
#[cfg(all(target_arch = "wasm32", stl_preview_measure))]
pub struct MeasureGpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub state: GpuState,
}

/// Open WebGPU and compile the viewport shaders. Call this outside the timed stretch.
#[cfg(all(target_arch = "wasm32", stl_preview_measure))]
pub async fn measure_open_gpu() -> Result<MeasureGpu, String> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::BROWSER_WEBGPU;
    let instance = wgpu::Instance::new(desc);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .map_err(|e| format!("no WebGPU adapter: {e:?}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("preview measure"),
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("no WebGPU device: {e:?}"))?;
    let state = GpuState::new(&device, COLOR_FORMAT, Arc::new(Stats::default()));
    Ok(MeasureGpu { device, queue, state })
}

/// Copy one upload chunk and encode the frame. `Ok(true)` when another chunk remains.
#[cfg(all(target_arch = "wasm32", stl_preview_measure))]
pub fn measure_upload_frame(gpu: &mut MeasureGpu, frame: &FrameData) -> Result<bool, String> {
    let (rendered, cmd) = gpu.state.render(&gpu.device, &gpu.queue, frame);
    if let Some(cmd) = cmd {
        gpu.queue.submit(std::iter::once(cmd));
    }
    if rendered.error {
        return Err("gpu error during preview upload".into());
    }
    Ok(rendered.more)
}
