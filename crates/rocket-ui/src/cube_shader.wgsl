//! WGSL shader for the spike cube.
//!
//! Vertex shader: rotates around Y, applies a simple perspective projection.
//! Fragment shader: outputs the per-vertex color.
//!
//! Note: no depth testing in this spike (painter's algorithm would sort
//! faces on the CPU; for the spike we accept minor visual artifacts).

struct Uniforms {
    angle: f32,
    aspect: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
) -> VsOut {
    var out: VsOut;

    // Rotate around Y axis
    let c = cos(uniforms.angle);
    let s = sin(uniforms.angle);
    let rot_x =  position.x * c + position.z * s;
    let rot_z = -position.x * s + position.z * c;

    // Camera at z = -4, looking toward +z
    let z = rot_z + 4.0;

    // Perspective projection (WebGPU clip space: z in [0, 1])
    let fov = 1.0; // tan(half_fov) ~ 45 degrees half-FOV
    let x_clip = rot_x * fov / (uniforms.aspect * z);
    let y_clip = position.y * fov / z;

    out.clip_pos = vec4<f32>(x_clip, y_clip, z / 100.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
