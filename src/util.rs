#![allow(dead_code)]

use crate::color::Color;
use crate::vectors::Vector2;
use bytemuck::{cast_slice, NoUninit};
use rand::rngs::SmallRng;
use rand::Rng;
use wgpu::{
    BindGroupLayout, BlendState, Buffer, BufferDescriptor, BufferUsages, ColorTargetState,
    ColorWrites, Device, FragmentState, FrontFace, MultisampleState, PipelineCompilationOptions,
    PipelineLayout, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
    RenderPipeline, RenderPipelineDescriptor, ShaderModule, TextureFormat, VertexBufferLayout,
    VertexState, VertexStepMode,
};

pub trait RandExt {
    fn f32(&mut self) -> f32;
    fn f32_centered(&mut self) -> f32;
    fn u8(&mut self) -> u8;
    fn vec2_centered(&mut self) -> Vector2;
    fn color_srgb(&mut self) -> Color;
}

impl RandExt for SmallRng {
    fn f32(&mut self) -> f32 {
        self.random::<f32>()
    }

    fn f32_centered(&mut self) -> f32 {
        self.random::<f32>() * 2.0 - 1.0
    }

    fn u8(&mut self) -> u8 {
        self.random::<u8>()
    }

    fn vec2_centered(&mut self) -> Vector2 {
        Vector2::new(self.f32_centered(), self.f32_centered())
    }

    fn color_srgb(&mut self) -> Color {
        Color::srgb(self.u8(), self.u8(), self.u8())
    }
}

pub fn cast_thing<T: NoUninit>(thing: &T) -> &[u8] {
    use std::slice;
    cast_slice(slice::from_ref(thing))
}

pub fn create_empty_vertex_buffer(device: &Device) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: None,
        size: 0,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

pub fn create_pipeline_layout(
    device: &Device,
    bind_group_layouts: &[&BindGroupLayout],
) -> PipelineLayout {
    device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts,
        push_constant_ranges: &[],
    })
}

pub fn create_no_vertex_render_pipeline(
    device: &Device,
    shader: &ShaderModule,
    pipeline_layout: &PipelineLayout,
    texture_format: TextureFormat,
    topology: PrimitiveTopology,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(pipeline_layout),
        vertex: VertexState {
            module: shader,
            entry_point: "vs_main",
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[VertexBufferLayout {
                array_stride: 0,
                step_mode: VertexStepMode::Vertex,
                attributes: &[],
            }],
        },
        fragment: Some(FragmentState {
            module: shader,
            entry_point: "fs_main",
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: texture_format,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
        }),
        primitive: PrimitiveState {
            topology,
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
    })
}
