//! An egui_kittest renderer on the app's own wgpu and egui-wgpu (egui_kittest's `wgpu` feature
//! would pull in wgpu's GL backend). Paint callbacks run as they do in the app.
//!
//! Use from a test file with `#[path = "support/wgpu_renderer.rs"] mod wgpu_renderer;`.
#![allow(dead_code)]

// Keeps every test process out of the real 2.0 settings folder (runs before main).
#[path = "test_env.rs"]
mod test_env;

use std::iter;

use egui::TexturesDelta;
use egui_kittest::TestRenderer;

/// Renders the harness with `render_state`, the one the viewport was set up with.
pub struct WgpuRenderer {
    pub render_state: egui_wgpu::RenderState,
}

impl WgpuRenderer {
    pub fn new(render_state: egui_wgpu::RenderState) -> Self {
        WgpuRenderer { render_state }
    }
}

impl TestRenderer for WgpuRenderer {
    fn handle_delta(&mut self, delta: &mut TexturesDelta) {
        let rs = &self.render_state;
        let mut renderer = rs.renderer.write();
        for (id, images) in delta.set.drain() {
            for image in images {
                renderer.update_texture(&rs.device, &rs.queue, id, &image);
            }
        }
        for id in delta.free.drain() {
            renderer.free_texture(&id);
        }
    }

    fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<image::RgbaImage, String> {
        let rs = &self.render_state;
        let mut renderer = rs.renderer.write();
        let mut encoder = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("test egui") });
        let ppp = ctx.pixels_per_point();
        let size = ctx.content_rect().size() * ppp;
        let screen = egui_wgpu::ScreenDescriptor { pixels_per_point: ppp, size_in_pixels: [size.x.round() as u32, size.y.round() as u32] };
        let tris = ctx.tessellate(output.shapes.clone(), ppp);
        let user = renderer.update_buffers(&rs.device, &rs.queue, &mut encoder, &tris, &screen);
        let (w, h) = (screen.size_in_pixels[0].max(1), screen.size_in_pixels[1].max(1));
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test egui target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: rs.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("test egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &tris, &screen);
        }
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buf = rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let idx = rs.queue.submit(user.into_iter().chain(iter::once(encoder.finish())));
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
        rs.device
            .poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: Some(std::time::Duration::from_secs(10)) })
            .map_err(|e| format!("{e}"))?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = buf.slice(..).get_mapped_range().map_err(|e| format!("{e:?}"))?;
        let bgra = matches!(rs.target_format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for r in data.chunks_exact(row as usize) {
            for p in r[..(w * 4) as usize].chunks_exact(4) {
                if bgra {
                    px.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
                } else {
                    px.extend_from_slice(p);
                }
            }
        }
        image::RgbaImage::from_raw(w, h, px).ok_or_else(|| "bad image size".to_owned())
    }
}
