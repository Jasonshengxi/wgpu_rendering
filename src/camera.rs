use super::util::cast_thing;
use super::vectors::Vector2;
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferBindingType, BufferUsages, Device, Queue,
    RenderPass, ShaderStages,
};
use winit::dpi::PhysicalSize;

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct Camera {
    pub target: Vector2,
    pub zoom: f32,
    _padding: u32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            ..Zeroable::zeroed()
        }
    }
}

impl Camera {
    pub const fn new(target: Vector2, zoom: f32) -> Self {
        Self {
            target,
            zoom,
            _padding: 0,
        }
    }
    
    pub fn zoomed_in_by(mut self, zoom: f32) -> Self {
        self.zoom *= zoom;
        self
    }

    pub fn covering(top_left: Vector2, bottom_right: Vector2) -> Self {
        let target = (top_left + bottom_right) / 2.0;
        let area = bottom_right - top_left;
        let max_dim = area.x.max(area.y);
        Self::new(target, max_dim.recip() * 2.0)
    }
}

pub struct CameraTransforms {
    pub camera: Camera,
    aspect_ratio: Vector2,
    camera_uniform: Buffer,
    aspect_transform_uniform: Buffer,
    bind_group: BindGroup,
}

impl CameraTransforms {
    pub fn screen_to_world(&self, screen_pos: Vector2, inner_size: PhysicalSize<u32>) -> Vector2 {
        self.normalized_to_world(Self::screen_to_normalize(screen_pos, inner_size))
    }
    
    pub fn screen_to_normalize(screen_pos: Vector2, inner_size: PhysicalSize<u32>) -> Vector2 {
        (screen_pos / Vector2::from(<[u32; 2]>::from(inner_size).map(|x| x as f32)))
            * Vector2::new(2.0, -2.0)
            - Vector2::new(1.0, -1.0)
    }

    pub fn normalized_to_world(&self, normalized_pos: Vector2) -> Vector2 {
        normalized_pos / self.aspect_ratio / self.camera.zoom + self.camera.target
    }
}

impl CameraTransforms {
    fn get_aspect_transform(size: PhysicalSize<u32>) -> Vector2 {
        let (width, height) = (size.width as f32, size.height as f32);
        let min_dim = f32::min(width, height);
        Vector2::new(min_dim / width, min_dim / height)
    }

    pub fn update_camera(&mut self, queue: &Queue) {
        queue.write_buffer(&self.camera_uniform, 0, cast_thing(&self.camera));
    }

    pub fn update_aspect_ratio(&mut self, queue: &Queue, size: PhysicalSize<u32>) {
        self.aspect_ratio = Self::get_aspect_transform(size);

        queue.write_buffer(
            &self.aspect_transform_uniform,
            0,
            cast_thing(&self.aspect_ratio),
        );
    }

    pub fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("camera bind group layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX,
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

    pub fn bind_group_to(&self, render_pass: &mut RenderPass, index: u32) {
        render_pass.set_bind_group(index, &self.bind_group, &[]);
    }

    pub fn new(device: &Device, inner_size: PhysicalSize<u32>) -> Self {
        let camera = Camera::default();

        let camera_uniform = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("camera uniform"),
            contents: cast_thing(&camera),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let aspect_ratio = Self::get_aspect_transform(inner_size);

        let aspect_transform_uniform = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("aspect transform"),
            contents: cast_thing(&aspect_ratio),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group_layout = Self::create_bind_group_layout(device);

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: camera_uniform.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: aspect_transform_uniform.as_entire_binding(),
                },
            ],
        });

        Self {
            camera,
            camera_uniform,
            aspect_transform_uniform,
            bind_group,
            aspect_ratio,
        }
    }
}
