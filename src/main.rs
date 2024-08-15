use crate::vectors::Vector2;
use bytemuck::{cast_slice, NoUninit, Pod, Zeroable};
use pollster::block_on;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::collections::{HashSet, VecDeque};
use std::time::Instant;
use std::{iter, mem};
use wgpu::util::{BufferInitDescriptor, DeviceExt, RenderEncoder};
use wgpu::{
    include_wgsl, Backends, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState, Buffer,
    BufferAddress, BufferBindingType, BufferDescriptor, BufferUsages, Color, ColorTargetState,
    ColorWrites, CommandEncoderDescriptor, CompositeAlphaMode, Device, DeviceDescriptor, Features,
    FragmentState, FrontFace, IndexFormat, InstanceDescriptor, Limits, LoadOp, MemoryHints,
    MultisampleState, Operations, PipelineCompilationOptions, PipelineLayout,
    PipelineLayoutDescriptor, PolygonMode, PowerPreference, PresentMode, PrimitiveState,
    PrimitiveTopology, Queue, RenderPass, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, ShaderModule, ShaderStages,
    StoreOp, SurfaceConfiguration, TextureFormat, TextureUsages, TextureViewDescriptor,
    VertexBufferLayout, VertexState, VertexStepMode,
};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

mod vectors;

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
struct CircleOrRect {
    position: Vector2,
    size: Vector2,
    color: [f32; 3],
    _padding: u32,
}

impl CircleOrRect {
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

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
struct Camera {
    target: Vector2,
    zoom: f32,
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

struct RectCircleRenderPipeline {
    drawer: RectCircleDrawer,
    shader: ShaderModule,
    pipeline_layout: PipelineLayout,
    render_pipeline: RenderPipeline,
}

impl RectCircleRenderPipeline {
    pub fn new(
        device: &Device,
        drawer: RectCircleDrawer,
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
            shader,
            pipeline_layout,
            render_pipeline,
        }
    }

    pub fn render(&self, render_pass: &mut RenderPass, camera_transforms: &CameraTransforms) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, self.drawer.get_bind_group(), &[]);
        render_pass.set_bind_group(1, camera_transforms.get_bind_group(), &[]);
        self.drawer.finish_render_pass(render_pass);
    }
}

struct RectCircleDrawer {
    instance_length: u32,
    instance_capacity: BufferAddress,

    empty_vertex_buffer: Buffer,
    index_buffer: Buffer,

    instance_buffer: Buffer,
    instance_bind_group_layout: BindGroupLayout,
    instance_bind_group: BindGroup,
}

impl RectCircleDrawer {
    const INDEX_VALUES: [u16; 6] = [0, 1, 2, 0, 2, 3];

    pub fn bind_group_layout(&self) -> &BindGroupLayout {
        &self.instance_bind_group_layout
    }

    pub fn get_bind_group(&self) -> &BindGroup {
        &self.instance_bind_group
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

    pub fn new_with_instances(device: &Device, instances: &[CircleOrRect]) -> Self {
        Self::new(device, instances.len() as BufferAddress, Some(instances))
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
        device: &Device,
        instance_capacity: BufferAddress,
        initial_instances: Option<&[CircleOrRect]>,
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

        let instance_buffer_size =
            instance_capacity * (mem::size_of::<CircleOrRect>() as BufferAddress);
        let instance_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("instance buffer"),
            size: instance_buffer_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: initial_instances.is_some(),
        });

        if let Some(initial_instances) = initial_instances {
            assert!(initial_instances.len() as BufferAddress <= instance_capacity);
            // unashamedly stolen from `create_buffer_init`
            instance_buffer.slice(..).get_mapped_range_mut()[..instance_buffer_size as usize]
                .copy_from_slice(cast_slice(initial_instances));
            instance_buffer.unmap();
        }

        let instance_bind_group_layout = Self::create_bind_group_layout(device);

        let instance_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("instance bind group"),
            layout: &instance_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: instance_buffer.as_entire_binding(),
            }],
        });

        Self {
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

struct CameraTransforms {
    pub camera: Camera,
    camera_uniform: Buffer,
    aspect_transform_uniform: Buffer,
    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,
}

impl CameraTransforms {
    fn get_aspect_transform(size: PhysicalSize<u32>) -> [f32; 2] {
        let (width, height) = (size.width as f32, size.height as f32);
        let min_dim = f32::min(width, height);
        [min_dim / width, min_dim / height]
    }

    pub fn update_camera(&mut self, queue: &Queue) {
        queue.write_buffer(&self.camera_uniform, 0, cast_thing(&self.camera));
    }

    pub fn update_aspect_ratio(&mut self, queue: &Queue, size: PhysicalSize<u32>) {
        queue.write_buffer(
            &self.aspect_transform_uniform,
            0,
            cast_thing(&Self::get_aspect_transform(size)),
        );
    }

    pub fn get_bind_group_layout(&self) -> &BindGroupLayout {
        &self.bind_group_layout
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

    pub fn get_bind_group(&self) -> &BindGroup {
        &self.bind_group
    }

    pub fn new(device: &Device, inner_size: PhysicalSize<u32>) -> Self {
        let camera = Camera::default();

        let camera_uniform = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("camera uniform"),
            contents: cast_thing(&camera),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let aspect_transform_uniform = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("aspect transform"),
            contents: cast_thing(&Self::get_aspect_transform(inner_size)),
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
            bind_group_layout,
            bind_group,
        }
    }
}

fn cast_thing<T: NoUninit>(thing: &T) -> &[u8] {
    use std::slice;
    cast_slice(slice::from_ref(thing))
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();

    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(1600, 1000))
        .build(&event_loop)
        .unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    let instance = wgpu::Instance::new(InstanceDescriptor {
        backends: Backends::PRIMARY,
        ..Default::default()
    });

    let surface = instance.create_surface(&window).unwrap();
    let adapter = block_on(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .unwrap();

    let (device, queue) = block_on(adapter.request_device(
        &DeviceDescriptor {
            label: None,
            required_features: Features::empty(),
            required_limits: Limits::default(),
            memory_hints: MemoryHints::Performance,
        },
        None,
    ))
    .unwrap();

    let capability = surface.get_capabilities(&adapter);
    let texture_format = capability
        .formats
        .into_iter()
        .find(TextureFormat::is_srgb)
        .unwrap();

    let size = window.inner_size();
    let mut surface_config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: texture_format,
        width: size.width,
        height: size.height,
        present_mode: PresentMode::AutoVsync,
        alpha_mode: CompositeAlphaMode::Auto,
        desired_maximum_frame_latency: 2,
        view_formats: Vec::new(),
    };

    surface.configure(&device, &surface_config);

    let mut instances = Vec::new();
    let mut rng = SmallRng::seed_from_u64(1000);
    let mut rand = || rng.random::<f32>();
    for _ in 0..1_000_000 {
        let position = Vector2::new(rand() * 2.0 - 1.0, rand() * 2.0 - 1.0);
        let distance = position.length() * std::f32::consts::FRAC_1_SQRT_2;

        fn lerp(from: f32, to: f32, time: f32) -> f32 {
            (1.0 - time) * from + time * to
        }

        let color = [rand(), rand(), rand()];
        let radius = rand() * 0.01 * lerp(3.0, 0.6, distance);
        // let is_circle = rand() < 0.5;
        let is_circle = true;

        instances.push(CircleOrRect {
            size: Vector2::new(radius, if is_circle { 0.0 } else { radius }),
            position,
            color,
            ..Zeroable::zeroed()
        });
    }

    let instances = Box::leak(Box::<[CircleOrRect]>::from(instances));

    let rect_circle_data = RectCircleDrawer::new_with_instances(&device, instances);
    let mut camera_transforms = CameraTransforms::new(&device, size);

    let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));
    let rect_circle_render =
        RectCircleRenderPipeline::new(&device, rect_circle_data, shader, texture_format);

    let mut frame_moments = VecDeque::new();

    let mut keys_pressed = HashSet::new();

    event_loop
        .run(|event, target| {
            if let Event::AboutToWait = event {
                const MOVE_DIRS: [(KeyCode, Vector2); 4] = [
                    (KeyCode::KeyW, Vector2::UP),
                    (KeyCode::KeyA, Vector2::LEFT),
                    (KeyCode::KeyS, Vector2::DOWN),
                    (KeyCode::KeyD, Vector2::RIGHT),
                ];
                const MOVE_SPEED: f32 = 0.01;
                const SHIFT_SPEED_MULT: f32 = 5.0;

                let camera = &mut camera_transforms.camera;
                for &(_, dir) in MOVE_DIRS
                    .iter()
                    .filter(|(code, _)| keys_pressed.contains(code))
                {
                    let speed_mult = match keys_pressed.contains(&KeyCode::ShiftLeft) {
                        true => SHIFT_SPEED_MULT,
                        false => 1.0,
                    };

                    camera.target += dir * MOVE_SPEED / camera.zoom * speed_mult;
                }

                camera_transforms.update_camera(&queue);
                window.request_redraw();
            } else if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::Resized(new_size) => {
                        surface_config.width = new_size.width;
                        surface_config.height = new_size.height;
                        surface.configure(&device, &surface_config);

                        camera_transforms.update_aspect_ratio(&queue, new_size);
                    }
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        const ZOOM_RATE: f32 = 1.1;
                        let zoom_ratio = match delta {
                            MouseScrollDelta::LineDelta(_, y) => ZOOM_RATE.powf(y),
                            MouseScrollDelta::PixelDelta(position) => {
                                let y = position.y as f32;
                                ZOOM_RATE.powf(y / 14.0) // isn't 14 like the best font size or something
                            }
                        };
                        camera_transforms.camera.zoom *= zoom_ratio;
                        camera_transforms.update_camera(&queue);
                    }
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                physical_key: PhysicalKey::Code(code),
                                state,
                                repeat: false,
                                ..
                            },
                        ..
                    } => {
                        match state {
                            ElementState::Pressed => keys_pressed.insert(code),
                            ElementState::Released => keys_pressed.remove(&code),
                        };

                        if let ElementState::Pressed = state {
                            match code {
                                KeyCode::Enter => {
                                    println!("fps: {}", frame_moments.len());
                                }
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        frame_moments.push_back(now);

                        while frame_moments
                            .front()
                            .is_some_and(|inst| inst.elapsed().as_secs_f32() > 1.0)
                        {
                            frame_moments.pop_front();
                        }

                        let texture = surface.get_current_texture().unwrap();
                        let view = texture
                            .texture
                            .create_view(&TextureViewDescriptor::default());
                        let mut command_encoder =
                            device.create_command_encoder(&CommandEncoderDescriptor::default());

                        // begin drawing
                        {
                            let mut render_pass =
                                command_encoder.begin_render_pass(&RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: Operations {
                                            load: LoadOp::Clear(Color::BLACK),
                                            store: StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });
                            
                            rect_circle_render.render(&mut render_pass, &camera_transforms);
                        }

                        queue.submit(iter::once(command_encoder.finish()));
                        texture.present();
                    }
                    _ => {}
                }
            };
        })
        .unwrap();
}
