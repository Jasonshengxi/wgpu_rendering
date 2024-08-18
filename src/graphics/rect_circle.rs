use super::camera::CameraTransforms;
use super::color::{Color, RawColor};
use super::dynamic_storage::DynamicStorageBuffer;
use super::util;
use super::vectors::Vector2;
use bytemuck::{cast_slice, Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    Buffer, BufferUsages, Device, IndexFormat,
    PrimitiveTopology, RenderPass, RenderPipeline, ShaderModule, TextureFormat,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Zeroable, Pod)]
pub struct RectOrCircle {
    center: Vector2,
    size: Vector2,
    color: RawColor,
}

impl RectOrCircle {
    pub const fn circle(center: Vector2, radius: f32, color: Color) -> Self {
        Self {
            center,
            size: Vector2::new(radius, 0.0),
            color: color.raw(),
        }
    }

    pub const fn rectangle(center: Vector2, size: Vector2, color: Color) -> Self {
        Self {
            center,
            size,
            color: color.raw(),
        }
    }
}

pub struct RectCircleRenderPipeline {
    pub instance_data: DynamicStorageBuffer<RectOrCircle>,
    render_pipeline: RenderPipeline,

    empty_vertex_buffer: Buffer,
    index_buffer: Buffer,
}

impl RectCircleRenderPipeline {
    pub fn new(
        device: &Device,
        instance_data: DynamicStorageBuffer<RectOrCircle>,
        shader: ShaderModule,
        texture_format: TextureFormat,
    ) -> Self {
        let pipeline_layout = util::create_pipeline_layout(
            device,
            &[
                instance_data.bind_group_layout(),
                &CameraTransforms::create_bind_group_layout(device),
            ],
        );

        let render_pipeline = util::create_no_vertex_render_pipeline(
            device,
            &shader,
            &pipeline_layout,
            texture_format,
            PrimitiveTopology::TriangleList,
        );

        const INDEX_BUFFER_CONTENTS: &[u16] = &[0, 1, 2, 0, 2, 3];
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("rc index buffer"),
            contents: cast_slice(INDEX_BUFFER_CONTENTS),
            usage: BufferUsages::INDEX,
        });

        Self {
            instance_data,
            render_pipeline,
            empty_vertex_buffer: util::create_empty_vertex_buffer(device),
            index_buffer,
        }
    }

    pub fn render(&self, render_pass: &mut RenderPass, camera_transforms: &CameraTransforms) {
        render_pass.set_pipeline(&self.render_pipeline);
        self.instance_data.bind_to(render_pass, 0);
        camera_transforms.bind_group_to(render_pass, 1);
        render_pass.set_vertex_buffer(0, self.empty_vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..self.instance_data.len());
    }
}
