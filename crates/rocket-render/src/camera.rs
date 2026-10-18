//! Orbit camera for the 3D viewport.
//!
//! Supports rotate (drag), zoom (scroll), and pan (shift-drag).
//! The camera orbits around a target point, defaulting to the rocket's
//! midpoint.

#![allow(
    clippy::similar_names,
    clippy::cast_lossless,
    clippy::many_single_char_names,
    clippy::unnecessary_cast,
    clippy::cast_precision_loss
)]

use eframe::egui;

/// An orbit camera.
#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    /// Distance from the target.
    pub distance: f32,
    /// Azimuth angle (radians, around Y axis).
    pub azimuth: f32,
    /// Elevation angle (radians, from XZ plane).
    pub elevation: f32,
    /// Target point the camera orbits around.
    pub target: [f32; 3],
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            distance: 0.65,
            azimuth: std::f32::consts::FRAC_PI_2,
            elevation: 0.22,
            target: [0.15, 0.0, 0.0], // Slightly aft of the nose tip.
        }
    }
}

impl OrbitCamera {
    /// Compute the camera's position in world space.
    #[must_use]
    pub fn position(&self) -> [f32; 3] {
        let ce = self.elevation.cos();
        let se = self.elevation.sin();
        let ca = self.azimuth.cos();
        let sa = self.azimuth.sin();
        [
            self.target[0] + self.distance * ce * ca,
            self.target[1] + self.distance * se,
            self.target[2] + self.distance * ce * sa,
        ]
    }

    /// Compute the view matrix (world → camera space) as a 4x4 matrix
    /// in column-major order (for wgpu/uniform upload).
    #[must_use]
    pub fn view_matrix(&self) -> [[f32; 4]; 4] {
        let eye = self.position();
        let center = self.target;
        let up = [0.0, 1.0, 0.0];

        look_at(eye, center, up)
    }

    /// Handle input (drag to rotate, scroll to zoom, shift-drag to pan).
    /// Returns `true` if the camera moved (requesting a repaint).
    pub fn handle_input(&mut self, response: &egui::Response, aspect: f32) -> bool {
        let mut changed = false;

        // Rotate (drag) or pan (shift-drag).
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            if response.ctx.input(|i| i.modifiers.shift) {
                // Pan: move the target.
                let pan_scale = self.distance * 0.002;
                self.target[0] -= delta.x as f32 * pan_scale;
                self.target[1] += delta.y as f32 * pan_scale;
            } else {
                // Rotate.
                self.azimuth -= delta.x as f32 * 0.01;
                self.elevation += delta.y as f32 * 0.01;
                // Clamp elevation to avoid flipping.
                self.elevation = self.elevation.clamp(-1.4, 1.4);
            }
            changed = true;
        }

        // Zoom (scroll).
        let scroll = if response.hovered() {
            response.ctx.input(|i| i.smooth_scroll_delta).y
        } else {
            0.0
        };
        if scroll.abs() > 0.0 {
            self.distance *= (-scroll * 0.002).exp();
            self.distance = self.distance.clamp(0.02, 1000.0);
            changed = true;
        }

        let _ = aspect; // Currently unused; reserved for future perspective adjustments.
        changed
    }
}

/// Compute a look-at view matrix (column-major, wgpu-compatible).
fn look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize([center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);

    // Column-major 4x4.
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
