use camera::CameraTransforms;
use color::Color;
use dynamic_storage::DynamicStorageBuffer;
use lines::{Line, LineRenderPipeline};
use pollster::block_on;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rect_circle::{RectCircleRenderPipeline, RectOrCircle};
use rustc_hash::FxHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::iter;
use std::time::Instant;
use vectors::Vector2;
use wgpu::{
    include_wgsl, Backends, CommandEncoderDescriptor, CompositeAlphaMode, DeviceDescriptor,
    Features, InstanceDescriptor, Limits, LoadOp, MemoryHints, Operations, PowerPreference,
    PresentMode, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, StoreOp,
    SurfaceConfiguration, TextureFormat, TextureUsages, TextureViewDescriptor,
};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

mod camera;
mod color;
mod dynamic_storage;
mod lines;
mod rect_circle;
mod util;
mod vectors;

pub fn main() {
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

    let required_features =
        Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | Features::CLEAR_TEXTURE;
    let (device, queue) = block_on(adapter.request_device(
        &DeviceDescriptor {
            label: None,
            required_features,
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

    fn time_hash_u64() -> u64 {
        let now = Instant::now();
        let mut hasher = FxHasher::default();
        now.hash(&mut hasher);
        hasher.finish()
    }

    fn random_circles(amount: u32, rand_color: bool) -> Vec<RectOrCircle> {
        let mut instances = Vec::new();
        let mut rng = SmallRng::seed_from_u64(time_hash_u64());
        let mut rand = || rng.random::<f32>();
        for _ in 0..amount {
            let position = Vector2::new(rand() * 2.0 - 1.0, rand() * 2.0 - 1.0);
            let distance = position.length() * std::f32::consts::FRAC_1_SQRT_2;

            fn lerp(from: f32, to: f32, time: f32) -> f32 {
                (1.0 - time) * from + time * to
            }

            let color = if rand_color {
                Color::rgb(rand(), rand(), rand())
            } else {
                Color::ORANGE
            };
            let radius = rand() * 0.01 * lerp(3.0, 0.6, distance);
            instances.push(RectOrCircle::circle(position, radius, color));
        }

        instances
    }

    fn random_lines(amount: u32) -> Vec<Line> {
        let mut instances = Vec::new();
        let mut rng = SmallRng::seed_from_u64(time_hash_u64());
        let mut rand = || rng.random::<f32>();
        for _ in 0..amount {
            let from = Vector2::new(rand() * 2.0 - 1.0, rand() * 2.0 - 1.0);
            let to = Vector2::new(rand() * 2.0 - 1.0, rand() * 2.0 - 1.0);
            let color = Color::rgb(rand(), rand(), rand());
            instances.push(Line::new(from, to, color));
        }
        instances
    }

    fn the_sun(amount: u32) -> Vec<Line> {
        let mut instances = Vec::new();
        let mut rng = SmallRng::seed_from_u64(time_hash_u64());
        let mut rand = || rng.random::<f32>();
        for _ in 0..amount {
            let [from, to] = [(), ()].map(|_| {
                let theta = rand() * std::f32::consts::TAU;
                Vector2::new(theta.cos(), theta.sin())
            });
            instances.push(Line::new(from, to, Color::RED.with_alpha(0.01)));
        }
        instances
    }

    let mut camera_transforms = CameraTransforms::new(&device, size);

    let rect_circle_data = DynamicStorageBuffer::new(&device);
    let rect_circle_shader =
        device.create_shader_module(include_wgsl!("graphics/rect_circle.wgsl"));
    let mut rect_circle_render = RectCircleRenderPipeline::new(
        &device,
        rect_circle_data,
        rect_circle_shader,
        texture_format,
    );

    let line_data = DynamicStorageBuffer::new(&device);
    let line_shader = device.create_shader_module(include_wgsl!("graphics/lines.wgsl"));
    let mut line_render =
        LineRenderPipeline::new(&device, line_data, line_shader, texture_format, size, false);

    let mut frame_moments = VecDeque::new();
    let mut keys_pressed = HashSet::new();
    let mut command_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());

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

                        line_render.resize(&device, new_size);
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
                                KeyCode::Digit1 => {
                                    let amount = match keys_pressed.contains(&KeyCode::ShiftLeft) {
                                        true => 1_000_000,
                                        false => 1_000,
                                    };
                                    let circles = random_circles(
                                        amount,
                                        keys_pressed.contains(&KeyCode::ControlLeft),
                                    );

                                    rect_circle_render
                                        .instance_data
                                        .set_new_data(&device, &queue, &circles);
                                }
                                KeyCode::Digit2 => {
                                    let amount = match keys_pressed.contains(&KeyCode::ShiftLeft) {
                                        true => 100_000,
                                        false => 1_000,
                                    };
                                    let circles = match keys_pressed.contains(&KeyCode::ControlLeft)
                                    {
                                        true => the_sun(amount),
                                        false => random_lines(amount),
                                    };

                                    line_render.line_data.set_new_data(&device, &queue, &circles);
                                }
                                
                                KeyCode::KeyT => {
                                    let mut data = Vec::new();
                                    
                                    for i in 0..50 {
                                        let value = i as f32 / 50.0;
                                        let color = Color::rgb(1.0, value, 0.0);
                                        let center = Vector2::new(value, 0.0);
                                        let size = Vector2::new(0.5 / 50.0, 1.0);
                                        
                                        data.push(RectOrCircle::rectangle(center, size, color));
                                    }
                                    
                                    rect_circle_render.instance_data.set_new_data(&device, &queue, &data);
                                }

                                _ => {}
                            }
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        frame_moments.push_back(now);

                        line_render.pre_render(&mut command_encoder);

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

                        // begin drawing
                        {
                            let mut render_pass =
                                command_encoder.begin_render_pass(&RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: Operations {
                                            load: LoadOp::Clear(wgpu::Color::BLACK),
                                            store: StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });

                            rect_circle_render.render(&mut render_pass, &camera_transforms);
                            line_render.render(&mut render_pass, &camera_transforms);
                        }

                        take_mut::take(&mut command_encoder, |command_encoder| {
                            queue.submit(iter::once(command_encoder.finish()));
                            device.create_command_encoder(&CommandEncoderDescriptor::default())
                        });

                        texture.present();
                    }
                    _ => {}
                }
            };
        })
        .unwrap();
}
