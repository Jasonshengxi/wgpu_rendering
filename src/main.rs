use crate::vectors::Vector2;
use bytemuck::{cast_slice, Pod, Zeroable};
use pollster::block_on;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::collections::{HashSet, VecDeque};
use std::time::Instant;
use std::{iter, mem};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    include_wgsl, Backends, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BlendState, BufferAddress, BufferBindingType, BufferUsages,
    Color, ColorTargetState, ColorWrites, CommandEncoderDescriptor, CompositeAlphaMode,
    DeviceDescriptor, Features, FragmentState, FrontFace, IndexFormat, InstanceDescriptor, Limits,
    LoadOp, MemoryHints, MultisampleState, Operations, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PolygonMode, PowerPreference, PresentMode, PrimitiveState,
    PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    RequestAdapterOptions, ShaderStages, StoreOp, SurfaceConfiguration, TextureFormat,
    TextureUsages, TextureViewDescriptor, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

mod vectors;

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
struct Vertex {
    position: Vector2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
struct InstanceData {
    position: Vector2,
    size: Vector2,
    color: [f32; 3],
    _padding: u32,
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

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();

    let window = WindowBuilder::new()
        .with_resizable(false)
        .with_min_inner_size(PhysicalSize::new(1000, 1000))
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

    surface.configure(
        &device,
        &SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: texture_format,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode: CompositeAlphaMode::Auto,
            desired_maximum_frame_latency: 2,
            view_formats: Vec::new(),
        },
    );

    // const EDGE_COUNT: u16 = 32;
    //
    // let mut vertices = vec![Vertex::zeroed()];
    // for i in 0..EDGE_COUNT {
    //     let theta = (i as f32 / EDGE_COUNT as f32) * std::f32::consts::TAU;
    //     vertices.push(Vertex {
    //         position: [theta.cos(), theta.sin()],
    //     })
    // }
    //
    // let mut indices = Vec::new();
    // for i in 0..EDGE_COUNT {
    //     let next_i = (i + 1) % EDGE_COUNT;
    //     indices.push(0);
    //     indices.push(i + 1);
    //     indices.push(next_i + 1);
    // }

    let vertices = Box::new(
        [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)].map(|pos| Vertex {
            position: pos.into(),
        }),
    );

    let indices = Box::new([0u16, 1, 2, 0, 2, 3]);

    let vertices = Box::leak(vertices);
    let indices = Box::leak(indices);

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

        instances.push(InstanceData {
            size: Vector2::new(radius, if is_circle { 0.0 } else { radius }),
            position,
            color,
            ..Zeroable::zeroed()
        });
    }

    let instances = Box::leak(instances.into());

    let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("vertex buffer"),
        contents: cast_slice(vertices),
        usage: BufferUsages::VERTEX,
    });

    let vertex_buffer_layout = VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }],
    };

    let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("index buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: BufferUsages::INDEX,
    });
    let index_format = IndexFormat::Uint16;

    let instance_frag_data_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("instance frag data"),
        contents: cast_slice(instances),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    let instance_frag_data_bind_group_layout =
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("instance frag data bind group layout"),
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
        });

    let instance_frag_data_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("instance frag data bind group"),
        layout: &instance_frag_data_bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: instance_frag_data_buffer.as_entire_binding(),
        }],
    });

    let mut camera = Camera::default();

    let camera_uniform = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("camera uniform"),
        contents: cast_slice(&[camera]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let camera_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("camera bind group layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("camera bind group"),
        layout: &camera_bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: camera_uniform.as_entire_binding(),
        }],
    });

    let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));

    let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[
            &instance_frag_data_bind_group_layout,
            &camera_bind_group_layout,
        ],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(&render_pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: "vs_main",
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[vertex_buffer_layout],
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
            // cull_mode: Some(Face::Back),
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

                queue.write_buffer(&camera_uniform, 0, cast_slice(&[camera]));
                window.request_redraw();
            } else if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        const ZOOM_RATE: f32 = 1.1;
                        match delta {
                            MouseScrollDelta::LineDelta(_, y) => {
                                camera.zoom *= ZOOM_RATE.powf(y);
                            }
                            MouseScrollDelta::PixelDelta(position) => {
                                let y = position.y as f32;
                                camera.zoom *= ZOOM_RATE.powf(y / 14.0); // isn't 14 like the best font size or something
                            }
                        }
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

                            render_pass.set_pipeline(&render_pipeline);
                            render_pass.set_bind_group(0, &instance_frag_data_bind_group, &[]);
                            render_pass.set_bind_group(1, &camera_bind_group, &[]);
                            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            // render_pass.set_vertex_buffer(1, instance_buffer.slice(..));
                            render_pass.set_index_buffer(index_buffer.slice(..), index_format);
                            render_pass.draw_indexed(
                                0..(indices.len() as u32),
                                0,
                                0..(instances.len() as u32),
                            );
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
