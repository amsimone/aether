//! Procedural mesh generation for rocket components.
//!
//! Each function takes a component's parametric geometry and produces
//! a [`Mesh`] (vertices + indices) suitable for uploading to a wgpu
//! buffer. The meshes are unitless (meters, matching `rocket-core`).
//!
//! All meshes are generated with normals (for flat shading) and are
//! wound counter-clockwise (matching wgpu's default `FrontFace::Ccw`).

#![allow(
    clippy::similar_names,
    clippy::cast_lossless,
    clippy::too_many_lines,
    clippy::min_ident_chars,
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::unnecessary_cast,
    clippy::must_use_candidate
)]

use rocket_core::components::{
    BodyTube, Component, FinPlanform, FinSet, LaunchLug, NoseCone, Pod, Transition,
};

/// A vertex: position + normal + per-vertex color.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub const ATTRIBS: [eframe::egui_wgpu::wgpu::VertexAttribute; 3] = eframe::egui_wgpu::wgpu::vertex_attr_array![
        0 => Float32x3,  // position
        1 => Float32x3,  // normal
        2 => Float32x3,  // color
    ];

    pub const LAYOUT: eframe::egui_wgpu::wgpu::VertexBufferLayout<'static> =
        eframe::egui_wgpu::wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as eframe::egui_wgpu::wgpu::BufferAddress,
            step_mode: eframe::egui_wgpu::wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        };
}

/// A triangle mesh: vertices + indices.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// Create an empty mesh.
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Append another mesh to this one (offsetting indices).
    pub fn append(&mut self, other: &Mesh) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices
            .extend(other.indices.iter().map(|i| i + offset));
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Push a triangle (three vertices), computing the normal.
    fn push_triangle(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 3]) {
        // Normal via cross product (b-a) × (c-a), normalized.
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let nx = ab[1] * ac[2] - ab[2] * ac[1];
        let ny = ab[2] * ac[0] - ab[0] * ac[2];
        let nz = ab[0] * ac[1] - ab[1] * ac[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-9);
        let normal = [nx / len, ny / len, nz / len];

        let base = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            position: a,
            normal,
            color,
        });
        self.vertices.push(Vertex {
            position: b,
            normal,
            color,
        });
        self.vertices.push(Vertex {
            position: c,
            normal,
            color,
        });
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

// ---- Component mesh generators ----

/// Generate a mesh for a nose cone by lathing the profile function.
///
/// The nose is positioned with its tip at the origin and extends along
/// +X (the body axis). The caller translates it to the correct position.
pub fn nose_cone_mesh(nc: &NoseCone, color: [f32; 3]) -> Mesh {
    let length = nc.length.as_meters() as f32;
    let base_r = (nc.base_diameter.as_meters() / 2.0) as f32;
    let segments = 24; // Angular segments around the axis.
    let rings = 16; // Rings along the length.

    let mut mesh = Mesh::new();

    // Generate rings of vertices.
    // For each ring at axial position t ∈ [0, 1], the radius is
    // base_r * shape.profile(t).
    let mut ring_verts: Vec<Vec<usize>> = Vec::new();

    for ri in 0..=rings {
        let t = ri as f32 / rings as f32;
        let r = base_r * nc.shape.profile(t as f64) as f32;
        let x = t * length;

        let mut ring = Vec::new();
        for si in 0..segments {
            let angle = (si as f32 / segments as f32) * std::f32::consts::TAU;
            let y = r * angle.cos();
            let z = r * angle.sin();
            let v = Vertex {
                position: [x, y, z],
                normal: [0.0, 0.0, 0.0], // Computed below.
                color,
            };
            ring.push(mesh.vertices.len());
            mesh.vertices.push(v);
        }
        ring_verts.push(ring);
    }

    // Connect rings with quads (two triangles each).
    for ri in 0..rings {
        for si in 0..segments {
            let si_next = (si + 1) % segments;
            let a = mesh.vertices[ring_verts[ri][si]].position;
            let b = mesh.vertices[ring_verts[ri][si_next]].position;
            let c = mesh.vertices[ring_verts[ri + 1][si_next]].position;
            let d = mesh.vertices[ring_verts[ri + 1][si]].position;
            // Two triangles: (a, b, c) and (a, c, d)
            // Wound CCW when viewed from outside.
            mesh.push_triangle(a, d, c, color);
            mesh.push_triangle(a, c, b, color);
        }
    }

    // Cap the base with a disk (so the nose isn't open at the back).
    let base_center = mesh.vertices.len();
    mesh.vertices.push(Vertex {
        position: [length, 0.0, 0.0],
        normal: [1.0, 0.0, 0.0],
        color,
    });
    let base_ring = &ring_verts[rings];
    for si in 0..segments {
        let si_next = (si + 1) % segments;
        let a = mesh.vertices[base_center].position;
        let b = mesh.vertices[base_ring[si]].position;
        let c = mesh.vertices[base_ring[si_next]].position;
        mesh.push_triangle(a, c, b, color); // Wound for -X normal
    }

    mesh
}

/// Generate a mesh for a body tube (hollow cylinder, optionally capped).
pub fn body_tube_mesh(bt: &BodyTube, color: [f32; 3], capped: bool) -> Mesh {
    cylinder_mesh(
        bt.length.as_meters() as f32,
        (bt.outer_diameter.as_meters() / 2.0) as f32,
        color,
        capped,
    )
}

/// Generate a mesh for a transition (conical frustrum).
pub fn transition_mesh(t: &Transition, color: [f32; 3]) -> Mesh {
    frustrum_mesh(
        t.length.as_meters() as f32,
        (t.fore_diameter.as_meters() / 2.0) as f32,
        (t.aft_diameter.as_meters() / 2.0) as f32,
        color,
    )
}

/// Generate a mesh for a launch lug (small cylinder).
pub fn launch_lug_mesh(ll: &LaunchLug, color: [f32; 3]) -> Mesh {
    cylinder_mesh(
        ll.length.as_meters() as f32,
        (ll.outer_diameter.as_meters() / 2.0) as f32,
        color,
        false,
    )
}

/// Generate a mesh for a pod (side-mounted cylinder).
pub fn pod_mesh(p: &Pod, color: [f32; 3]) -> Mesh {
    cylinder_mesh(
        p.length.as_meters() as f32,
        (p.outer_diameter.as_meters() / 2.0) as f32,
        color,
        true,
    )
}

/// Generate a mesh for a fin set.
///
/// Each fin is a flat extrusion of its planform, rotated around the body
/// axis and offset to the body radius. We approximate the body radius
/// from the fin's mount position (the caller passes it).
pub fn fin_set_mesh(fs: &FinSet, body_radius: f32, color: [f32; 3]) -> Mesh {
    let mut mesh = Mesh::new();
    let thickness = fs.thickness.as_meters() as f32;
    let count = fs.count;

    for i in 0..count {
        let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // Generate the fin's planform vertices in 2D (x_axial, y_radial),
        // then extrude in thickness and rotate around X.
        let planform_2d = fin_planform_outline(&fs.planform);

        // For each planform vertex, create two vertices (front/back of fin).
        let half_t = thickness / 2.0;
        let mut front: Vec<[f32; 3]> = Vec::new();
        let mut back: Vec<[f32; 3]> = Vec::new();

        for (x, y) in &planform_2d {
            // Position before rotation: at (x, body_radius + y, ±half_t)
            let p_front = [*x, body_radius + *y, half_t];
            let p_back = [*x, body_radius + *y, -half_t];
            // Rotate around X axis: y' = y*cos - z*sin, z' = y*sin + z*cos
            front.push([
                p_front[0],
                p_front[1] * cos_a - p_front[2] * sin_a,
                p_front[1] * sin_a + p_front[2] * cos_a,
            ]);
            back.push([
                p_back[0],
                p_back[1] * cos_a - p_back[2] * sin_a,
                p_back[1] * sin_a + p_back[2] * cos_a,
            ]);
        }

        // Build triangles: front face, back face, and side strips.
        let n = planform_2d.len();
        if n < 3 {
            continue;
        }

        // Front face (triangulated as a fan).
        for k in 1..n - 1 {
            mesh.push_triangle(front[0], front[k + 1], front[k], color);
        }
        // Back face (reversed winding).
        for k in 1..n - 1 {
            mesh.push_triangle(back[0], back[k], back[k + 1], color);
        }
        // Side strips between front and back.
        for k in 0..n {
            let k_next = (k + 1) % n;
            mesh.push_triangle(front[k], front[k_next], back[k], color);
            mesh.push_triangle(front[k_next], back[k_next], back[k], color);
        }
    }

    mesh
}

/// Generate the 2D outline of a fin planform (axial x, radial y).
fn fin_planform_outline(planform: &FinPlanform) -> Vec<(f32, f32)> {
    match planform {
        FinPlanform::Trapezoidal {
            root_chord,
            tip_chord,
            span,
            sweep,
        } => {
            let cr = root_chord.as_meters() as f32;
            let ct = tip_chord.as_meters() as f32;
            let s = span.as_meters() as f32;
            let sw = sweep.as_meters() as f32;
            // Trapezoid: (0,0) → (cr,0) → (sw+ct, s) → (sw, s)
            vec![(0.0, 0.0), (cr, 0.0), (sw + ct, s), (sw, s)]
        }
        FinPlanform::Elliptical { root_chord, span } => {
            let cr = root_chord.as_meters() as f32;
            let s = span.as_meters() as f32;
            // Sample an ellipse: x from 0 to cr, y from 0 to s (half-ellipse).
            let n = 16;
            (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32;
                    let theta = std::f32::consts::PI * t; // 0 to π
                    let x = cr * 0.5 * (1.0 - theta.cos());
                    let y = s * theta.sin();
                    (x, y)
                })
                .collect()
        }
        FinPlanform::Custom { points } => {
            // Convert mm → m.
            points
                .iter()
                .map(|(x, y)| (*x as f32 * 1e-3, *y as f32 * 1e-3))
                .collect()
        }
    }
}

/// Generate a cylinder mesh (used by body tubes, launch lugs, pods).
fn cylinder_mesh(length: f32, radius: f32, color: [f32; 3], capped: bool) -> Mesh {
    let segments = 24;
    let mut mesh = Mesh::new();

    // Two rings of vertices: front (x=0) and back (x=length).
    let mut front_ring = Vec::new();
    let mut back_ring = Vec::new();

    for si in 0..segments {
        let angle = (si as f32 / segments as f32) * std::f32::consts::TAU;
        let y = radius * angle.cos();
        let z = radius * angle.sin();
        front_ring.push(mesh.vertices.len());
        mesh.vertices.push(Vertex {
            position: [0.0, y, z],
            normal: [0.0, y / radius, z / radius],
            color,
        });
        back_ring.push(mesh.vertices.len());
        mesh.vertices.push(Vertex {
            position: [length, y, z],
            normal: [0.0, y / radius, z / radius],
            color,
        });
    }

    // Side quads.
    for si in 0..segments {
        let si_next = (si + 1) % segments;
        let a = mesh.vertices[front_ring[si]].position;
        let b = mesh.vertices[front_ring[si_next]].position;
        let c = mesh.vertices[back_ring[si_next]].position;
        let d = mesh.vertices[back_ring[si]].position;
        mesh.push_triangle(a, d, c, color);
        mesh.push_triangle(a, c, b, color);
    }

    if capped {
        // Front cap (x=0, normal -X).
        let front_center = mesh.vertices.len();
        mesh.vertices.push(Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [-1.0, 0.0, 0.0],
            color,
        });
        for si in 0..segments {
            let si_next = (si + 1) % segments;
            mesh.push_triangle(
                mesh.vertices[front_center].position,
                mesh.vertices[front_ring[si_next]].position,
                mesh.vertices[front_ring[si]].position,
                color,
            );
        }
        // Back cap (x=length, normal +X).
        let back_center = mesh.vertices.len();
        mesh.vertices.push(Vertex {
            position: [length, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
            color,
        });
        for si in 0..segments {
            let si_next = (si + 1) % segments;
            mesh.push_triangle(
                mesh.vertices[back_center].position,
                mesh.vertices[back_ring[si]].position,
                mesh.vertices[back_ring[si_next]].position,
                color,
            );
        }
    }

    mesh
}

/// Generate a conical frustrum mesh (used by transitions).
fn frustrum_mesh(length: f32, fore_radius: f32, aft_radius: f32, color: [f32; 3]) -> Mesh {
    let segments = 24;
    let mut mesh = Mesh::new();

    let mut front_ring = Vec::new();
    let mut back_ring = Vec::new();

    for si in 0..segments {
        let angle = (si as f32 / segments as f32) * std::f32::consts::TAU;
        let (y_f, z_f) = (fore_radius * angle.cos(), fore_radius * angle.sin());
        let (y_b, z_b) = (aft_radius * angle.cos(), aft_radius * angle.sin());

        // Approximate normal: average of the front and back radial directions,
        // tilted to account for the slope.
        let n_len = ((y_b - y_f).powi(2) + (z_b - z_f).powi(2) + length.powi(2)).sqrt();
        let normal = [
            (aft_radius - fore_radius) / n_len.max(1e-9),
            (y_b + y_f) / (fore_radius + aft_radius).max(1e-9),
            (z_b + z_f) / (fore_radius + aft_radius).max(1e-9),
        ];

        front_ring.push(mesh.vertices.len());
        mesh.vertices.push(Vertex {
            position: [0.0, y_f, z_f],
            normal,
            color,
        });
        back_ring.push(mesh.vertices.len());
        mesh.vertices.push(Vertex {
            position: [length, y_b, z_b],
            normal,
            color,
        });
    }

    for si in 0..segments {
        let si_next = (si + 1) % segments;
        let a = mesh.vertices[front_ring[si]].position;
        let b = mesh.vertices[front_ring[si_next]].position;
        let c = mesh.vertices[back_ring[si_next]].position;
        let d = mesh.vertices[back_ring[si]].position;
        mesh.push_triangle(a, d, c, color);
        mesh.push_triangle(a, c, b, color);
    }

    mesh
}

/// Build a complete mesh for a rocket, by walking the component tree
/// and concatenating each component's mesh (with translation).
///
/// Each component's mesh is generated at the origin and then offset by
/// its axial position. The result is a single [`Mesh`] ready for upload.
pub fn build_rocket_mesh(rocket: &rocket_core::Rocket) -> Mesh {
    let mut combined = Mesh::new();

    // Default colors per component type (will be replaced by material-based
    // coloring in a future iteration).
    let nose_color = [0.85, 0.15, 0.15]; // red
    let body_color = [0.20, 0.45, 0.90]; // blue
    let fin_color = [0.15, 0.75, 0.20]; // green
    let transition_color = [0.80, 0.25, 0.75]; // magenta
    let lug_color = [0.50, 0.50, 0.50]; // gray
    let pod_color = [0.90, 0.80, 0.15]; // yellow
    let mass_color = [0.30, 0.30, 0.30]; // dark gray

    // Track body tube radii so fin sets can be positioned correctly.
    let mut body_radius_at: Vec<(f32, f32)> = Vec::new(); // (x_position, radius)

    for component in rocket.root.iter() {
        let pos = component.position().as_meters() as f32;

        let (submesh, _color) = match component {
            Component::NoseCone(nc) => (nose_cone_mesh(nc, nose_color), nose_color),
            Component::BodyTube(bt) => {
                body_radius_at.push((pos, (bt.outer_diameter.as_meters() / 2.0) as f32));
                body_radius_at.push((
                    pos + bt.length.as_meters() as f32,
                    (bt.outer_diameter.as_meters() / 2.0) as f32,
                ));
                (body_tube_mesh(bt, body_color, false), body_color)
            }
            Component::Transition(t) => (transition_mesh(t, transition_color), transition_color),
            Component::FinSet(fs) => {
                // Find the body radius at the fin's position.
                let r = body_radius_at
                    .iter()
                    .filter(|(x, _)| *x <= pos)
                    .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(_, r)| *r)
                    .unwrap_or(0.0125); // default 25mm diameter
                (fin_set_mesh(fs, r, fin_color), fin_color)
            }
            Component::LaunchLug(ll) => (launch_lug_mesh(ll, lug_color), lug_color),
            Component::Pod(p) => (pod_mesh(p, pod_color), pod_color),
            Component::MassObject(_) => {
                // Render as a small cube to make it visible.
                let mut m = Mesh::new();
                let s = 0.01_f32; // 1cm cube
                let cube_color = mass_color;
                // Simple cube (6 faces).
                let v = |x: f32, y: f32, z: f32| [pos + x * s, y * s, z * s];
                m.push_triangle(
                    v(-1.0, -1.0, -1.0),
                    v(1.0, -1.0, -1.0),
                    v(1.0, 1.0, -1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, -1.0),
                    v(1.0, 1.0, -1.0),
                    v(-1.0, 1.0, -1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, 1.0),
                    v(1.0, 1.0, 1.0),
                    v(1.0, -1.0, 1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, 1.0),
                    v(-1.0, 1.0, 1.0),
                    v(1.0, 1.0, 1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, 1.0, -1.0),
                    v(1.0, 1.0, 1.0),
                    v(1.0, 1.0, -1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, 1.0, -1.0),
                    v(-1.0, 1.0, 1.0),
                    v(1.0, 1.0, 1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, -1.0),
                    v(1.0, -1.0, 1.0),
                    v(1.0, -1.0, -1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, -1.0),
                    v(-1.0, -1.0, 1.0),
                    v(1.0, -1.0, 1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, -1.0),
                    v(-1.0, 1.0, 1.0),
                    v(-1.0, 1.0, -1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(-1.0, -1.0, -1.0),
                    v(-1.0, -1.0, 1.0),
                    v(-1.0, 1.0, 1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(1.0, -1.0, -1.0),
                    v(1.0, 1.0, 1.0),
                    v(1.0, -1.0, 1.0),
                    cube_color,
                );
                m.push_triangle(
                    v(1.0, -1.0, -1.0),
                    v(1.0, 1.0, -1.0),
                    v(1.0, 1.0, 1.0),
                    cube_color,
                );
                (m, cube_color)
            }
            Component::Coupler(c) => (
                cylinder_mesh(
                    c.length.as_meters() as f32,
                    (c.outer_diameter.as_meters() / 2.0) as f32,
                    [0.40, 0.40, 0.40],
                    false,
                ),
                [0.40, 0.40, 0.40],
            ),
        };

        // Translate the submesh by the component's axial position.
        // (Nose cones and other components generated at origin need this.)
        let mut translated = Mesh::new();
        for v in &submesh.vertices {
            translated.vertices.push(Vertex {
                position: [v.position[0] + pos, v.position[1], v.position[2]],
                normal: v.normal,
                color: v.color,
            });
        }
        translated.indices = submesh.indices.clone();
        combined.append(&translated);
    }

    combined
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::similar_names, clippy::cast_lossless)]
    use super::*;
    use rocket_core::components::{BodyTube, NoseCone, NoseShape};
    use rocket_core::{ComponentId, Length, Rocket};

    #[test]
    fn nose_cone_mesh_has_vertices() {
        let nc = NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: ComponentId::new(100),
            shape: NoseShape::Conical,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::ZERO,
        };
        let mesh = nose_cone_mesh(&nc, [1.0, 0.0, 0.0]);
        assert!(
            mesh.vertices.len() > 100,
            "nose cone should have many vertices"
        );
        assert!(
            mesh.indices.len() > 100,
            "nose cone should have many indices"
        );
        assert!(mesh.triangle_count() > 30);
    }

    #[test]
    fn body_tube_mesh_is_hollow() {
        let bt = BodyTube {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: ComponentId::new(100),
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(24.0),
        };
        let mesh = body_tube_mesh(&bt, [0.0, 0.0, 1.0], false);
        assert!(mesh.triangle_count() > 20);
    }

    #[test]
    fn build_rocket_mesh_combines_components() {
        let mut rocket = Rocket::new("Test");
        let balsa_id = rocket.materials.iter().next().unwrap().id;
        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa_id,
            shape: NoseShape::Conical,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::ZERO,
        }));
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(70.0),
            material: balsa_id,
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(24.0),
        }));

        let mesh = build_rocket_mesh(&rocket);
        assert!(mesh.vertices.len() > 200, "combined mesh should be large");
        assert!(mesh.triangle_count() > 50);
    }

    #[test]
    fn empty_rocket_produces_empty_mesh() {
        let rocket = Rocket::new("Empty");
        let mesh = build_rocket_mesh(&rocket);
        assert_eq!(mesh.vertices.len(), 0);
        assert_eq!(mesh.indices.len(), 0);
    }
}
