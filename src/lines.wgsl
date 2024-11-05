struct VertexOutput {
    @builtin(position) screen_position: vec4<f32>,
    @location(0) position: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
}

struct InstanceData {
    start: vec2<f32>,
    end: vec2<f32>,
    color: vec4<f32>,
}

struct Camera {
    aim: vec2<f32>,
    zoom: f32,
}

@group(1) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(1)
var<uniform> aspect_transform: vec2<f32>;

@group(0) @binding(0)
var<storage, read> instance_data: array<InstanceData>;

@group(2) @binding(0)
var accum_texture: texture_storage_2d<rgba32float, read_write>;

@group(2) @binding(1)
var<uniform> use_alpha: u32;

@vertex
fn vs_main(
    @builtin(vertex_index) v_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let inst_data = instance_data[instance_index];
    let position = select(inst_data.end, inst_data.start, v_index == 0);

    let screen_position = (position - camera.aim) * camera.zoom * aspect_transform;
    return VertexOutput(
        vec4<f32>(screen_position, 0.0, 1.0),
        position,
        instance_index,
    );
}

@fragment
fn fs_main(vertex_data: VertexOutput) -> @location(0) vec4<f32> {
    let index = vertex_data.instance_index;
    let inst_data = instance_data[index];

    if use_alpha > 0 {
        let pixel = vec2<u32>(vertex_data.screen_position.xy);
        let accum_at: vec4<f32> = textureLoad(accum_texture, pixel);
        let new_accum = accum_at * (1 - inst_data.color.a) + inst_data.color;
        textureStore(accum_texture, pixel, new_accum);
        return vec4<f32>(new_accum.rgb * new_accum.a, 1.0);
    } else {
        return inst_data.color;
    }
}
