// Rocket render shader.
//
// Vertex shader: applies model → view → projection, passes color and
// world-space normal to the fragment shader.
// Fragment shader: simple Lambertian shading with a fixed light
// direction, plus an ambient term.

struct Uniforms {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let world = uniforms.model * vec4<f32>(in.position, 1.0);
    out.world_pos = world.xyz;
    out.clip_pos = uniforms.proj * uniforms.view * world;
    // Normal (assuming no non-uniform scaling, model matrix preserves normals).
    out.normal = (uniforms.model * vec4<f32>(in.normal, 0.0)).xyz;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Fixed light direction (from upper-front-right).
    let light_dir = normalize(vec3<f32>(0.5, 0.8, 0.6));
    let normal = normalize(in.normal);

    // Lambertian diffuse.
    let diff = max(dot(normal, light_dir), 0.0);

    // Ambient term.
    let ambient = 0.35;

    // Combine.
    let color = in.color * (ambient + 0.65 * diff);

    return vec4<f32>(color, 1.0);
}
