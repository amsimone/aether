//! The rocket renderer: takes a [`Mesh`] and renders it inside an egui
//! panel using wgpu.
//!
//! This generalizes the Phase-0 spike's cube renderer to support:
//! - Arbitrary meshes (multiple parts combined).
//! - An orbit camera (rotate, zoom, pan).
//! - CG/CP markers (rendered as small colored spheres/crosses).
//! - A grid floor and axis gizmo.
//!
//! The integration pattern follows ADR-0010: raw wgpu via
//! `egui_wgpu::CallbackTrait`, sharing egui's wgpu instance.

#![allow(
    clippy::similar_names,
    clippy::cast_lossless,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::unnecessary_cast,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::min_ident_chars,
    clippy::map_unwrap_or,
    clippy::assigning_clones,
    clippy::derivable_impls
)]

pub mod camera;
pub mod geometry;

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

use crate::camera::OrbitCamera;
use crate::geometry::{build_rocket_mesh as build_mesh, Mesh, Vertex};

/// Uniforms uploaded to the GPU each frame.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    _pad: f32,
}

/// GPU-side resources, stored in egui's `callback_resources`.
pub struct RenderResources {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    line_pipeline: wgpu::RenderPipeline,
    line_vertex_buf: wgpu::Buffer,
    line_vertex_count: u32,
}

impl RenderResources {
    /// Initialize GPU resources from a rocket mesh.
    ///
    /// Call once when the mesh changes (dirty flag), not every frame.
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, mesh: &Mesh) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rocket_render_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rocket_render_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rocket_render_pl"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rocket_render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(target_format.into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Line pipeline (for grid + markers), reusing the same shader
        // with a different topology.
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rocket_render_line_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(target_format.into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rocket_render_vbo"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rocket_render_ibo"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rocket_render_ubo"),
            contents: bytemuck::cast_slice(&[Uniforms {
                view: identity(),
                proj: identity(),
                model: identity(),
                camera_pos: [0.0, 0.0, 0.0],
                _pad: 0.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rocket_render_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        // Build the grid + axis gizmo as a line mesh.
        let line_mesh = build_grid_and_axes();
        let line_vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rocket_render_line_vbo"),
            contents: bytemuck::cast_slice(&line_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            vertex_buf,
            index_buf,
            index_count: mesh.indices.len() as u32,
            uniform_buf,
            bind_group,
            line_pipeline,
            line_vertex_buf,
            line_vertex_count: line_mesh.vertices.len() as u32,
        }
    }

    fn update_uniforms(&self, queue: &wgpu::Queue, uniforms: &Uniforms) {
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&[*uniforms]));
    }
}

/// The egui wgpu paint callback.
struct RenderCallback {
    uniforms: Uniforms,
}

impl egui_wgpu::CallbackTrait for RenderCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get::<RenderResources>() {
            r.update_uniforms(queue, &self.uniforms);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<RenderResources>() else {
            return;
        };

        // Draw the grid (lines).
        if r.line_vertex_count > 0 {
            render_pass.set_pipeline(&r.line_pipeline);
            render_pass.set_bind_group(0, &r.bind_group, &[]);
            render_pass.set_vertex_buffer(0, r.line_vertex_buf.slice(..));
            render_pass.draw(0..r.line_vertex_count, 0..1);
        }

        // Draw the rocket (triangles).
        if r.index_count > 0 {
            render_pass.set_pipeline(&r.pipeline);
            render_pass.set_bind_group(0, &r.bind_group, &[]);
            render_pass.set_vertex_buffer(0, r.vertex_buf.slice(..));
            render_pass.set_index_buffer(r.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..r.index_count, 0, 0..1);
        }
    }
}

/// The state held by the eframe app for the 3D viewport.
pub struct RocketViewport {
    /// Orbit camera.
    pub camera: OrbitCamera,
    /// Last uploaded design, including every geometry parameter.
    last_rocket: Option<rocket_core::Rocket>,
    mesh: Mesh,
    selected: Option<rocket_core::ComponentId>,
    /// Show only the selected component, including internal parts.
    pub isolate_selected: bool,
    last_isolate: bool,
    /// Request camera framing on the next draw.
    pub fit_requested: bool,
    /// Stored render state (device + queue + format), captured on first init.
    render_state: Option<eframe::egui_wgpu::RenderState>,
}

impl Default for RocketViewport {
    fn default() -> Self {
        Self {
            camera: OrbitCamera::default(),
            last_rocket: None,
            mesh: Mesh::new(),
            selected: None,
            isolate_selected: false,
            last_isolate: false,
            fit_requested: true,
            render_state: None,
        }
    }
}

impl RocketViewport {
    /// Initialize or update GPU resources if the rocket has changed.
    ///
    /// Call this every frame; it only rebuilds the mesh + GPU buffers
    /// when the rocket's geometry has actually changed (dirty flag).
    /// On the first call, it captures the render state for later use.
    pub fn update_mesh(&mut self, rocket: &rocket_core::Rocket, cc: &eframe::CreationContext<'_>) {
        if self.render_state.is_none() {
            if let Some(rs) = &cc.wgpu_render_state {
                self.render_state = Some(rs.clone());
            }
        }
        self.rebuild_if_changed(rocket);
    }

    /// Rebuild the mesh if the rocket has changed, using the stored
    /// render state. Returns `true` if the mesh was rebuilt.
    ///
    /// Call this every frame after the initial `update_mesh` to keep
    /// the 3D view in sync with design edits.
    pub fn rebuild_if_changed(&mut self, rocket: &rocket_core::Rocket) -> bool {
        if self.last_rocket.as_ref() == Some(rocket) && self.last_isolate == self.isolate_selected {
            return false;
        }

        let Some(render_state) = &self.render_state else {
            return false;
        };

        self.mesh = build_mesh(rocket);
        let mut mesh = self.mesh.clone();
        for (id, range) in &self.mesh.parts {
            if Some(*id) == self.selected {
                for index in &self.mesh.indices[range.clone()] {
                    mesh.vertices[*index as usize].color = [0.25, 0.82, 0.98];
                }
            }
        }
        if self.isolate_selected {
            if let Some((_, range)) = self
                .mesh
                .parts
                .iter()
                .find(|(id, _)| Some(*id) == self.selected)
            {
                mesh.indices = self.mesh.indices[range.clone()].to_vec();
            }
        }
        let resources =
            RenderResources::new(&render_state.device, render_state.target_format, &mesh);

        render_state
            .renderer
            .write()
            .callback_resources
            .insert(resources);
        self.last_rocket = Some(rocket.clone());
        self.last_isolate = self.isolate_selected;
        true
    }

    /// Force a mesh rebuild on the next `rebuild_if_changed` call.
    pub fn mark_dirty(&mut self) {
        self.last_rocket = None;
    }

    /// Draw the 3D viewport into the given egui Ui.
    ///
    /// Handles camera input and renders the rocket + grid.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rocket: &rocket_core::Rocket,
        selected: &mut Option<rocket_core::ComponentId>,
    ) {
        if self.selected != *selected {
            self.selected = *selected;
            self.fit_requested |= self.isolate_selected;
            self.mark_dirty();
        }
        self.rebuild_if_changed(rocket);
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(egui::vec2(1.0, 1.0)),
            egui::Sense::click_and_drag(),
        );

        let aspect = if rect.height() > 0.0 {
            rect.width() / rect.height()
        } else {
            1.0
        };

        if self.fit_requested || response.double_clicked() {
            self.fit(aspect);
            self.fit_requested = false;
        }

        // Handle camera input.
        if self.camera.handle_input(&response, aspect) {
            ui.ctx().request_repaint();
        }

        // Build uniforms.
        let view = self.camera.view_matrix();
        let proj = perspective(aspect, std::f32::consts::FRAC_PI_4, 0.001, 10000.0);
        // Scale: rocket is in meters; we want to see ~0.5m objects.
        // The model matrix scales the world so the rocket fills the view.
        let scale = 1.0; // 1:1 for now; camera distance handles framing.
        let model = scale_matrix(scale);
        let camera_pos = self.camera.position();

        let uniforms = Uniforms {
            view,
            proj,
            model,
            camera_pos,
            _pad: 0.0,
        };

        if self.last_rocket.is_some() {
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                RenderCallback { uniforms },
            ));
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "wgpu backend not available\nor no rocket loaded",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(160),
            );
        }

        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                if let Some(id) = self.pick(pointer, rect, view, proj) {
                    *selected = Some(id);
                    ui.ctx().request_repaint();
                }
            }
        }
    }
}

impl RocketViewport {
    fn fit(&mut self, aspect: f32) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for (id, range) in &self.mesh.parts {
            if self.isolate_selected && self.selected.is_some() && self.selected != Some(*id) {
                continue;
            }
            for index in &self.mesh.indices[range.clone()] {
                let p = self.mesh.vertices[*index as usize].position;
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(p[axis]);
                    hi[axis] = hi[axis].max(p[axis]);
                }
            }
        }
        if !lo[0].is_finite() {
            self.camera = OrbitCamera::default();
            return;
        }
        for axis in 0..3 {
            self.camera.target[axis] = lo[axis].midpoint(hi[axis]);
        }
        let radius = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2))
            .sqrt()
            * 0.5;
        let half_fov = (std::f32::consts::FRAC_PI_8.tan() * aspect.min(1.0)).atan();
        self.camera.distance = (radius * 1.25 / half_fov.sin()).max(0.04);
    }

    fn pick(
        &self,
        pointer: egui::Pos2,
        rect: egui::Rect,
        view: [[f32; 4]; 4],
        proj: [[f32; 4]; 4],
    ) -> Option<rocket_core::ComponentId> {
        let mut hit = None;
        let mut nearest = f32::INFINITY;
        for (id, range) in &self.mesh.parts {
            if self.isolate_selected && self.selected.is_some() && Some(*id) != self.selected {
                continue;
            }
            for triangle in self.mesh.indices[range.clone()].as_chunks::<3>().0 {
                let projected: Vec<_> = triangle
                    .iter()
                    .map(|index| {
                        let p = self.mesh.vertices[*index as usize].position;
                        let clip = transform(proj, transform(view, [p[0], p[1], p[2], 1.0]));
                        [
                            rect.center().x + clip[0] / clip[3] * rect.width() * 0.5,
                            rect.center().y - clip[1] / clip[3] * rect.height() * 0.5,
                            clip[2] / clip[3],
                        ]
                    })
                    .collect();
                let (a, b, c) = (projected[0], projected[1], projected[2]);
                let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if denominator.abs() < 1e-6 {
                    continue;
                }
                let u = ((b[1] - c[1]) * (pointer.x - c[0]) + (c[0] - b[0]) * (pointer.y - c[1]))
                    / denominator;
                let v = ((c[1] - a[1]) * (pointer.x - c[0]) + (a[0] - c[0]) * (pointer.y - c[1]))
                    / denominator;
                let w = 1.0 - u - v;
                let depth = u * a[2] + v * b[2] + w * c[2];
                if u >= 0.0
                    && v >= 0.0
                    && w >= 0.0
                    && (0.0..=1.0).contains(&depth)
                    && depth < nearest
                {
                    hit = Some(*id);
                    nearest = depth;
                }
            }
        }
        hit
    }
}

fn transform(matrix: [[f32; 4]; 4], point: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|row| (0..4).map(|col| matrix[col][row] * point[col]).sum())
}

// ---- Matrix helpers ----

fn identity() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn scale_matrix(s: f32) -> [[f32; 4]; 4] {
    [
        [s, 0.0, 0.0, 0.0],
        [0.0, s, 0.0, 0.0],
        [0.0, 0.0, s, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Perspective projection matrix (column-major, wgpu-compatible with
/// depth range [0, 1]).
fn perspective(aspect: f32, fov_y: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (fov_y / 2.0).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, (far * near) / (near - far), 0.0],
    ]
}

/// Build a grid floor and axis gizmo as a line mesh.
fn build_grid_and_axes() -> Mesh {
    let mut mesh = Mesh::new();
    let grid_color = [0.10, 0.14, 0.19];
    let axis_x_color = [0.8, 0.2, 0.2]; // red = X (body axis)
    let axis_y_color = [0.2, 0.8, 0.2]; // green = Y
    let axis_z_color = [0.2, 0.2, 0.8]; // blue = Z

    // Grid: lines at y = -0.05 (just below the rocket).
    let grid_y = -0.05_f32;
    let grid_size = 0.4_f32;
    let grid_step = 0.025_f32;
    let mut i = -grid_size;
    while i <= grid_size {
        let v1 = Vertex {
            position: [i as f32, grid_y, -grid_size as f32],
            normal: [0.0, 1.0, 0.0],
            color: grid_color,
        };
        let v2 = Vertex {
            position: [i as f32, grid_y, grid_size as f32],
            normal: [0.0, 1.0, 0.0],
            color: grid_color,
        };
        mesh.vertices.push(v1);
        mesh.vertices.push(v2);
        let v3 = Vertex {
            position: [-grid_size as f32, grid_y, i as f32],
            normal: [0.0, 1.0, 0.0],
            color: grid_color,
        };
        let v4 = Vertex {
            position: [grid_size as f32, grid_y, i as f32],
            normal: [0.0, 1.0, 0.0],
            color: grid_color,
        };
        mesh.vertices.push(v3);
        mesh.vertices.push(v4);
        i += grid_step;
    }

    // Axes (slightly above the grid).
    let axis_y = grid_y;
    let axis_len = 0.03_f32;
    // X axis (red)
    mesh.vertices.push(Vertex {
        position: [0.0, axis_y, 0.0],
        normal: [0.0, 1.0, 0.0],
        color: axis_x_color,
    });
    mesh.vertices.push(Vertex {
        position: [axis_len, axis_y, 0.0],
        normal: [0.0, 1.0, 0.0],
        color: axis_x_color,
    });
    // Y axis (green)
    mesh.vertices.push(Vertex {
        position: [0.0, axis_y, 0.0],
        normal: [0.0, 1.0, 0.0],
        color: axis_y_color,
    });
    mesh.vertices.push(Vertex {
        position: [0.0, axis_y + axis_len, 0.0],
        normal: [0.0, 1.0, 0.0],
        color: axis_y_color,
    });
    // Z axis (blue)
    mesh.vertices.push(Vertex {
        position: [0.0, axis_y, 0.0],
        normal: [0.0, 1.0, 0.0],
        color: axis_z_color,
    });
    mesh.vertices.push(Vertex {
        position: [0.0, axis_y, axis_len],
        normal: [0.0, 1.0, 0.0],
        color: axis_z_color,
    });

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_core::{BodyTube, Component, ComponentId, Length, Rocket};

    fn body_viewport() -> RocketViewport {
        let mut rocket = Rocket::new("Viewport regression");
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(7),
            position: Length::millimeters(70.0),
            material: ComponentId::new(5),
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(24.0),
        }));
        RocketViewport {
            mesh: build_mesh(&rocket),
            ..RocketViewport::default()
        }
    }

    #[test]
    fn fitted_model_is_inside_clip_volume_at_wide_and_narrow_aspects() {
        for aspect in [0.5, 1.0, 2.0] {
            let mut viewport = body_viewport();
            viewport.fit(aspect);
            let view = viewport.camera.view_matrix();
            let proj = perspective(aspect, std::f32::consts::FRAC_PI_4, 0.001, 10000.0);
            let target = viewport.camera.target;
            let center = transform(view, [target[0], target[1], target[2], 1.0]);
            assert!(center[0].abs() < 1e-5 && center[1].abs() < 1e-5);
            for vertex in &viewport.mesh.vertices {
                let p = vertex.position;
                let clip = transform(proj, transform(view, [p[0], p[1], p[2], 1.0]));
                assert!(clip[3] > 0.0, "geometry must be in front of the camera");
                assert!(clip[0].abs() < clip[3] && clip[1].abs() < clip[3]);
                assert!((0.0..=1.0).contains(&(clip[2] / clip[3])));
            }
        }
    }

    #[test]
    fn clicking_visible_body_selects_it_and_background_misses() {
        let mut viewport = body_viewport();
        viewport.fit(1.0);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 600.0));
        let view = viewport.camera.view_matrix();
        let proj = perspective(1.0, std::f32::consts::FRAC_PI_4, 0.001, 10000.0);
        assert_eq!(
            viewport.pick(rect.center(), rect, view, proj),
            Some(ComponentId::new(7))
        );
        assert_eq!(viewport.pick(rect.min, rect, view, proj), None);
    }
}
