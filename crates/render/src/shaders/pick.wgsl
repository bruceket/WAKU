struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};

struct Instance {
    color: vec4<f32>,
    pick_id: vec4<u32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> inst: Instance;

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> @builtin(position) vec4<f32> {
    return globals.view_proj * vec4<f32>(position, 1.0);
}

@fragment
fn fs_main() -> @location(0) u32 {
    return inst.pick_id.x;
}
