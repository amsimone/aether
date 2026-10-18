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
            depth_stencil: None,
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
            depth_stencil: None,
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
    /// Whether GPU resources have been initialized for the current mesh.
    initialized: bool,
    /// Hash of the last mesh, for dirty-flag detection.
    mesh_hash: u64,
}

impl Default for RocketViewport {
    fn default() -> Self {
        Self {
            camera: OrbitCamera::default(),
            initialized: false,
            mesh_hash: 0,
        }
    }
}

impl RocketViewport {
    /// Initialize or update GPU resources if the rocket has changed.
    ///
    /// Call this every frame; it only rebuilds the mesh + GPU buffers
    /// when the rocket's geometry has actually changed (dirty flag).
    pub fn update_mesh(&mut self, rocket: &rocket_core::Rocket, cc: &eframe::CreationContext<'_>) {
        let new_hash = hash_rocket(rocket);
        if new_hash == self.mesh_hash && self.initialized {
            return; // No change.
        }

        let Some(render_state) = cc.wgpu_render_state.as_ref() else {
            return;
        };

        let mesh = build_mesh(rocket);
        let resources =
            RenderResources::new(&render_state.device, render_state.target_format, &mesh);

        // Replace any existing resources.
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(resources);
        self.initialized = true;
        self.mesh_hash = new_hash;
    }

    /// Draw the 3D viewport into the given egui Ui.
    ///
    /// Handles camera input and renders the rocket + grid.
    pub fn show(&mut self, ui: &mut egui::Ui, rocket: &rocket_core::Rocket) {
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::new(500.0, 400.0), egui::Sense::drag());

        let aspect = if rect.height() > 0.0 {
            rect.width() / rect.height()
        } else {
            1.0
        };

        // Handle camera input.
        if self.camera.handle_input(&response, aspect) {
            ui.ctx().request_repaint();
        }

        // Build uniforms.
        let view = self.camera.view_matrix();
        let proj = perspective(aspect, std::f32::consts::FRAC_PI_4, 0.01, 100.0);
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

        if self.initialized {
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

        // Show camera info on hover.
        let _ = rocket; // (Used in update_mesh, not here.)
    }
}

/// Hash a rocket's geometry for dirty-flag detection.
fn hash_rocket(rocket: &rocket_core::Rocket) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Hash the component count and positions (good enough for dirty detection;
    // a full hash would serialize, which is too slow for per-frame checks).
    rocket.root.components.len().hash(&mut hasher);
    for c in rocket.root.iter() {
        c.id().as_u64().hash(&mut hasher);
        c.position().as_meters().to_bits().hash(&mut hasher);
        c.length().as_meters().to_bits().hash(&mut hasher);
    }
    hasher.finish()
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
        [0.0, 0.0, far / (far - near), 1.0],
        [0.0, 0.0, -(far * near) / (far - near), 0.0],
    ]
}

/// Build a grid floor and axis gizmo as a line mesh.
fn build_grid_and_axes() -> Mesh {
    let mut mesh = Mesh::new();
    let grid_color = [0.3, 0.3, 0.3];
    let axis_x_color = [0.8, 0.2, 0.2]; // red = X (body axis)
    let axis_y_color = [0.2, 0.8, 0.2]; // green = Y
    let axis_z_color = [0.2, 0.2, 0.8]; // blue = Z

    // Grid: lines at y = -0.05 (just below the rocket).
    let grid_y = -0.05_f32;
    let grid_size = 1.0_f32;
    let grid_step = 0.1_f32;
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
    let axis_y = 0.001_f32;
    let axis_len = 0.5_f32;
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
