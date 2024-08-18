use super::camera::CameraTransforms;
use super::color::{Color, RawColor};
use super::dynamic_storage::DynamicStorageBuffer;
use super::util;
use super::vectors::Vector2;
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt, RenderEncoder};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Buffer,
    BufferBindingType, BufferUsages, CommandEncoder, Device, Extent3d,
    ImageSubresourceRange, PrimitiveTopology, RenderPass, RenderPipeline
    , ShaderModule, ShaderStages, StorageTextureAccess,
    Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDimension,
};
use winit::dpi::PhysicalSize;

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Zeroable, Pod)]
pub struct Line {
    from: Vector2,
    to: Vector2,
    color: RawColor,
}

impl Line {
    pub fn new(from: Vector2, to: Vector2, color: Color) -> Self {
        Self {
            from,
            to,
            color: color.raw_pre_mult(),
        }
    }
}

pub struct LineRenderPipeline {
    pub line_data: DynamicStorageBuffer<Line>,
    empty_vertex_buffer: Buffer,
    render_pipeline: RenderPipeline,

    use_alpha: u32,
    use_alpha_buffer: Buffer,
    accum_texture: Texture,
    accum_texture_view: TextureView,
    accum_bind_group_layout: BindGroupLayout,
    accum_bind_group: BindGroup,
}

impl LineRenderPipeline {
    fn create_accum_texture(device: &Device, window_size: PhysicalSize<u32>) -> Texture {
        device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: window_size.width,
                height: window_size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba32Float,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING, // | TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: TextureFormat::Rgba32Float,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    fn create_bind_group(
        device: &Device,
        layout: &BindGroupLayout,
        texture_view: &TextureView,
        use_alpha: &Buffer,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: use_alpha.as_entire_binding(),
                },
            ],
        })
    }

    pub fn new(
        device: &Device,
        line_data: DynamicStorageBuffer<Line>,
        shader: ShaderModule,
        texture_format: TextureFormat,
        window_size: PhysicalSize<u32>,
        use_alpha: bool,
    ) -> Self {
        let use_alpha = use_alpha as u32;

        let accum_texture = Self::create_accum_texture(device, window_size);
        let accum_texture_view = accum_texture.create_view(&Default::default());

        let use_alpha_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: util::cast_thing(&use_alpha),
            usage: BufferUsages::UNIFORM,
        });

        let accum_bind_group_layout = Self::create_bind_group_layout(device);
        let accum_bind_group = Self::create_bind_group(
            device,
            &accum_bind_group_layout,
            &accum_texture_view,
            &use_alpha_buffer,
        );

        let pipeline_layout = util::create_pipeline_layout(
            device,
            &[
                line_data.bind_group_layout(),
                &CameraTransforms::create_bind_group_layout(device),
                &accum_bind_group_layout,
            ],
        );

        let render_pipeline = util::create_no_vertex_render_pipeline(
            device,
            &shader,
            &pipeline_layout,
            texture_format,
            PrimitiveTopology::LineList,
        );
        Self {
            line_data,
            empty_vertex_buffer: util::create_empty_vertex_buffer(device),
            render_pipeline,
            accum_texture,
            accum_texture_view,
            accum_bind_group_layout,
            accum_bind_group,
            use_alpha,
            use_alpha_buffer,
        }
    }

    pub fn resize(&mut self, device: &Device, new_size: PhysicalSize<u32>) {
        let new_texture = Self::create_accum_texture(device, new_size);
        let new_texture_view = new_texture.create_view(&Default::default());
        let new_bind_group = Self::create_bind_group(
            device,
            &self.accum_bind_group_layout,
            &new_texture_view,
            &self.use_alpha_buffer,
        );

        self.accum_texture = new_texture;
        self.accum_texture_view = new_texture_view;
        self.accum_bind_group = new_bind_group;
    }

    pub fn pre_render(&self, command_encoder: &mut CommandEncoder) {
        if self.use_alpha > 0 {
            command_encoder.clear_texture(&self.accum_texture, &ImageSubresourceRange::default());
        }
    }

    pub fn render(&self, render_pass: &mut RenderPass, camera_transforms: &CameraTransforms) {
        render_pass.set_pipeline(&self.render_pipeline);
        self.line_data.bind_to(render_pass, 0);
        camera_transforms.bind_group_to(render_pass, 1);
        render_pass.set_bind_group(2, &self.accum_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.empty_vertex_buffer.slice(..));
        render_pass.draw(0..2, 0..self.line_data.len());
    }
}
