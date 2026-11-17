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

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = globals.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let lambert = max(dot(n, -normalize(globals.light_dir.xyz)), 0.0);
    let shade = 0.25 + 0.75 * lambert;
    return vec4<f32>(inst.color.rgb * shade, inst.color.a);
}
