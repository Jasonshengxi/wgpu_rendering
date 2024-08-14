struct VertexInput {
    @location(0) position: vec2<f32>,
}

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

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
    vertex_data: VertexInput,
) -> VertexOutput {
    let inst_data = instance_data[instance_index];
    
    var position: vec2<f32>;
    if is_circle(inst_data) {
        position = vertex_data.position * inst_data.size.x + inst_data.offset;
    } else {
        position = vertex_data.position * inst_data.size + inst_data.offset;
    }
    
    return VertexOutput(
        vec4<f32>(position, 0.0, 1.0),
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

//    if is_circle(index) {
//        return vec4<f32>(1.0, 0.0, 0.0, 1.0);
//    }

    return vec4<f32>(inst_data.color, 1.0);
}
