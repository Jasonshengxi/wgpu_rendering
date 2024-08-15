use std::mem;
use bytemuck::{cast_slice, Pod, Zeroable};
use wgpu::{BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState, Buffer, BufferAddress, BufferBindingType, BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder, Device, FragmentState, FrontFace, IndexFormat, MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, Queue, RenderPass, RenderPipeline, RenderPipelineDescriptor, ShaderModule, ShaderStages, TextureFormat, VertexBufferLayout, VertexState, VertexStepMode};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use crate::camera::CameraTransforms;
use crate::vectors::Vector2;

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct RectOrCircle {
    position: Vector2,
    size: Vector2,
    color: [f32; 3],
    _padding: u32,
}

impl RectOrCircle {
    pub fn circle(center: Vector2, radius: f32, color: [f32; 3]) -> Self {
        Self {
            position: center,
            size: Vector2::new(radius, 0.0),
            color,
            ..Zeroable::zeroed()
        }
    }

    pub fn rectangle(center: Vector2, size: Vector2, color: [f32; 3]) -> Self {
        Self {
            position: center,
            size,
            color,
            ..Zeroable::zeroed()
        }
    }
}


pub struct RectCircleRenderPipeline<'d> {
    pub drawer: RectCircleDrawer<'d>,
    render_pipeline: RenderPipeline,
}

impl<'d> RectCircleRenderPipeline<'d> {
    pub fn new(
        device: &Device,
        drawer: RectCircleDrawer<'d>,
        shader: ShaderModule,
        texture_format: TextureFormat,
    ) -> Self {
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                drawer.bind_group_layout(),
                &CameraTransforms::create_bind_group_layout(device),
            ],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: 0,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[],
                }],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: texture_format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            drawer,
            render_pipeline,
        }
    }

    pub fn render(&self, render_pass: &mut RenderPass, camera_transforms: &CameraTransforms) {
        render_pass.set_pipeline(&self.render_pipeline);
        self.drawer.bind_group_to(render_pass, 0);
        camera_transforms.bind_group_to(render_pass, 1);
        self.drawer.finish_render_pass(render_pass);
    }
}

pub struct RectCircleDrawer<'d> {
    device: &'d Device,

    instance_length: u32,
    instance_capacity: BufferAddress,

    empty_vertex_buffer: Buffer,
    index_buffer: Buffer,

    instance_buffer: Buffer,
    instance_bind_group_layout: BindGroupLayout,
    instance_bind_group: BindGroup,
}

impl<'d> RectCircleDrawer<'d> {
    const INDEX_VALUES: [u16; 6] = [0, 1, 2, 0, 2, 3];

    pub fn bind_group_layout(&self) -> &BindGroupLayout {
        &self.instance_bind_group_layout
    }

    pub fn bind_group_to(&self, render_pass: &mut RenderPass, index: u32) {
        render_pass.set_bind_group(index, &self.instance_bind_group, &[]);
    }

    fn buffer_descriptor(
        size: BufferAddress,
        mapped_at_creation: bool,
    ) -> BufferDescriptor<'static> {
        BufferDescriptor {
            label: Some("instance buffer"),
            size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation,
        }
    }

    fn create_bind_group<'a>(
        device: &'a Device,
        layout: &'a BindGroupLayout,
        buffer: &'a Buffer,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("instance bind group"),
            layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    pub fn set_new_shapes(&mut self, queue: &Queue, new_instances: &[RectOrCircle]) {
        if new_instances.len() <= self.instance_capacity as usize {
            queue.write_buffer(&self.instance_buffer, 0, cast_slice(new_instances));
        } else {
            let new_shape_capacity = (new_instances.len() as BufferAddress).next_power_of_two();
            let new_data = cast_slice(new_instances);
            self.update_buffer_len(new_shape_capacity, true);

            self.instance_buffer.slice(..).get_mapped_range_mut()[..new_data.len()]
                .copy_from_slice(new_data);
            self.instance_buffer.unmap();
        }
        self.instance_length = new_instances.len() as u32;
    }

    const fn shape_to_byte_capacity(shape_capacity: BufferAddress) -> BufferAddress {
        shape_capacity * (mem::size_of::<RectOrCircle>() as BufferAddress)
    }

    pub fn shrink_to_fit(&mut self, command_encoder: &mut CommandEncoder) {
        let shape_capacity = self.instance_length as BufferAddress;
        let old_buffer = self.update_buffer_len(shape_capacity, false);

        command_encoder.copy_buffer_to_buffer(
            &old_buffer,
            0,
            &self.instance_buffer,
            0,
            Self::shape_to_byte_capacity(shape_capacity),
        );
    }

    pub fn update_buffer_len(
        &mut self,
        new_shape_capacity: BufferAddress,
        mapped_at_creation: bool,
    ) -> Buffer {
        let new_byte_capacity = Self::shape_to_byte_capacity(new_shape_capacity);

        let new_buffer = self.device.create_buffer(&Self::buffer_descriptor(
            new_byte_capacity,
            mapped_at_creation,
        ));

        let new_bind_group =
            Self::create_bind_group(self.device, &self.instance_bind_group_layout, &new_buffer);

        let old_instance_buffer = mem::replace(&mut self.instance_buffer, new_buffer);
        self.instance_bind_group = new_bind_group;
        self.instance_capacity = new_shape_capacity;

        old_instance_buffer
    }

    pub fn finish_render_pass(&self, render_pass: &mut RenderPass) {
        render_pass.set_vertex_buffer(0, self.empty_vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);
        render_pass.draw_indexed(
            0..(Self::INDEX_VALUES.len() as u32),
            0,
            0..self.instance_length,
        );
    }

    pub fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("instance bind group layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    pub fn new(
        device: &'d Device,
        instance_capacity: BufferAddress,
        initial_instances: Option<&[RectOrCircle]>,
    ) -> Self {
        let empty_vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 0,
            usage: BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("index buffer"),
            contents: cast_slice(Self::INDEX_VALUES.as_slice()),
            usage: BufferUsages::INDEX,
        });

        let instance_buffer_size = Self::shape_to_byte_capacity(instance_capacity);
        let instance_buffer = device.create_buffer(&Self::buffer_descriptor(
            instance_buffer_size,
            initial_instances.is_some(),
        ));

        if let Some(initial_instances) = initial_instances {
            assert!(initial_instances.len() as BufferAddress <= instance_capacity);
            // unashamedly stolen from `create_buffer_init`
            instance_buffer.slice(..).get_mapped_range_mut()[..instance_buffer_size as usize]
                .copy_from_slice(cast_slice(initial_instances));
            instance_buffer.unmap();
        }

        let instance_bind_group_layout = Self::create_bind_group_layout(device);

        let instance_bind_group =
            Self::create_bind_group(device, &instance_bind_group_layout, &instance_buffer);

        Self {
            device,

            instance_length: initial_instances.map_or(0, |x| x.len() as u32),
            instance_capacity,

            empty_vertex_buffer,
            index_buffer,

            instance_buffer,
            instance_bind_group_layout,
            instance_bind_group,
        }
    }
}