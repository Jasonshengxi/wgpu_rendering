struct VertexOutput {
    @builtin(position) screen_position: vec4<f32>,
    @location(0) position: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
}

struct InstanceData {
    offset: vec2<f32>,
    size: vec2<f32>,
    color: vec3<f32>,
}

struct Camera {
    aim: vec2<f32>,
    zoom: f32,
}

@group(1) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(1)
var<uniform> aspect_transform: vec2<f32>;

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let in_position = get_vertex(in_vertex_index);
    let inst_data = instance_data[instance_index];

    let position = in_position
        * vec2<f32>(
            inst_data.size.x,
            select(inst_data.size.y, inst_data.size.x, is_circle(inst_data))
        )
        + inst_data.offset;
        
    let screen_position = (position - camera.aim) * camera.zoom * aspect_transform;

    return VertexOutput(
        vec4<f32>(screen_position, 0.0, 1.0),
        position,
        instance_index,
    );
}


fn is_circle(inst_data: InstanceData) -> bool {
    return inst_data.size.y == 0.0;
}

@group(0) @binding(0)
var<storage, read> instance_data: array<InstanceData>;

@fragment
fn fs_main(vertex_data: VertexOutput) -> @location(0) vec4<f32> {
    let index = vertex_data.instance_index;
    let inst_data = instance_data[index];

    if is_circle(inst_data) {
        let offset = vertex_data.position - inst_data.offset;
        let dist_sqr = dot(offset, offset);
        let radius = inst_data.size.x;
        if dist_sqr > radius * radius {
            discard;
        }
    }
    return vec4<f32>(inst_data.color, 1.0);
}

// this is a workaround to not being able to use const arrays
// lots of workarounds in this one as well since the WGSL thing is outdated
fn get_vertex(index: u32) -> vec2<f32> {
    switch (index) {
        case 0u: {
            return vec2<f32>(1.0, 1.0);
        }
        case 1u: {
            return vec2<f32>(-1.0, 1.0);
        }
        case 2u: {
            return vec2<f32>(-1.0, -1.0);
        }
        case 3u: {
            return vec2<f32>(1.0, -1.0);
        }
        default: {
            return vec2<f32>();
        }
    }
}

