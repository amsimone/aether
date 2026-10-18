//! 3D cube renderer for the Phase-0 spike.
//!
//! Renders a rotating colored cube using wgpu, embedded inside an egui
//! panel via `egui_wgpu::Callback`. Proves the integration pattern from
//! ADR-0002: a 3D viewport sharing egui's wgpu instance.
//!
//! Follows the official egui `custom3d_wgpu.rs` example:
//! - GPU resources (pipeline, buffers) stored in `callback_resources`.
//! - The `prepare` callback (which has the `Queue`) updates uniforms.
//! - The `paint` callback issues draw calls on egui's render pass.

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

/// Host-side uniform data, uploaded to the GPU each frame.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    angle: f32,
    aspect: f32,
}

/// Vertex: position + per-vertex color.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
    ];

    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRIBS,
    };
}

// 6 faces × 4 corners, each face a distinct color.
const CUBE_VERTICES: &[Vertex] = &[
    // +X — red
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.85, 0.15, 0.15],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.85, 0.15, 0.15],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.85, 0.15, 0.15],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.85, 0.15, 0.15],
    },
    // -X — green
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.15, 0.75, 0.20],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.15, 0.75, 0.20],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.15, 0.75, 0.20],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.15, 0.75, 0.20],
    },
    // +Y — blue
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.20, 0.45, 0.90],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.20, 0.45, 0.90],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.20, 0.45, 0.90],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.20, 0.45, 0.90],
    },
    // -Y — yellow
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.90, 0.80, 0.15],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.90, 0.80, 0.15],
    },
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.90, 0.80, 0.15],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.90, 0.80, 0.15],
    },
    // +Z — magenta
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.80, 0.25, 0.75],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.80, 0.25, 0.75],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.80, 0.25, 0.75],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.80, 0.25, 0.75],
    },
    // -Z — cyan
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.15, 0.75, 0.85],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.15, 0.75, 0.85],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.15, 0.75, 0.85],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.15, 0.75, 0.85],
    },
];

const CUBE_INDICES: &[u16] = &[
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18,
    16, 18, 19, 20, 21, 22, 20, 22, 23,
];

/// GPU-side resources, stored in egui's `callback_resources` type-map.
struct CubeResources {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl CubeResources {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("spike_cube_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cube_shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("spike_cube_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("spike_cube_pl"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("spike_cube_pipeline"),
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

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("spike_cube_vbo"),
            contents: bytemuck::cast_slice(CUBE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("spike_cube_ibo"),
            contents: bytemuck::cast_slice(CUBE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("spike_cube_ubo"),
            contents: bytemuck::cast_slice(&[Uniforms {
                angle: 0.0,
                aspect: 1.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spike_cube_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            vertex_buf,
            index_buf,
            uniform_buf,
            bind_group,
        }
    }

    fn update_uniforms(&self, queue: &wgpu::Queue, angle: f32, aspect: f32) {
        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::cast_slice(&[Uniforms { angle, aspect }]),
        );
    }
}

/// Paint callback: carries per-frame data (angle, aspect) from the UI
/// to the GPU. `prepare` updates uniforms; `paint` issues draw calls.
struct CubeCallback {
    angle: f32,
    aspect: f32,
}

impl egui_wgpu::CallbackTrait for CubeCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get::<CubeResources>() {
            r.update_uniforms(queue, self.angle, self.aspect);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<CubeResources>() else {
            return;
        };
        render_pass.set_pipeline(&r.pipeline);
        render_pass.set_bind_group(0, &r.bind_group, &[]);
        render_pass.set_vertex_buffer(0, r.vertex_buf.slice(..));
        render_pass.set_index_buffer(r.index_buf.slice(..), wgpu::IndexFormat::Uint16);
        let index_count = u32::try_from(CUBE_INDICES.len()).expect("index count fits in u32");
        render_pass.draw_indexed(0..index_count, 0, 0..1);
    }
}

/// State for the 3D viewport, held by the eframe app.
pub struct CubeViewport {
    /// Current rotation angle (radians).
    pub angle: f32,
    /// Rotation speed (radians/second), controlled by the UI slider.
    pub speed: f32,
    /// Whether GPU resources have been initialized.
    pub initialized: bool,
}

impl Default for CubeViewport {
    fn default() -> Self {
        Self {
            angle: 0.0,
            speed: 0.5,
            initialized: false,
        }
    }
}

impl CubeViewport {
    /// Initialize GPU resources once, when the `CreationContext` is available.
    pub fn init(&mut self, cc: &eframe::CreationContext<'_>) {
        if self.initialized {
            return;
        }
        let Some(render_state) = cc.wgpu_render_state.as_ref() else {
            return; // No wgpu backend
        };
        let resources = CubeResources::new(&render_state.device, render_state.target_format);
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(resources);
        self.initialized = true;
    }

    /// Draw the 3D viewport into the given egui Ui.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(400.0, 400.0), egui::Sense::hover());

        let aspect = if rect.height() > 0.0 {
            rect.width() / rect.height()
        } else {
            1.0
        };

        if self.initialized {
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                CubeCallback {
                    angle: self.angle,
                    aspect,
                },
            ));
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "wgpu backend not available",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(160),
            );
        }
    }
}
