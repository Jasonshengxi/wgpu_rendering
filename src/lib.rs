use camera::CameraTransforms;
use lines::LineRenderPipeline;
use pollster::block_on;
use rect_circle::RectCircleRenderPipeline;
use std::collections::{HashSet, VecDeque};
use std::iter;
use std::mem::replace;
use std::time::Instant;
use wgpu::{
    include_wgsl, Backends, CommandEncoderDescriptor, CompositeAlphaMode, DeviceDescriptor,
    Features, InstanceDescriptor, Limits, LoadOp, MemoryHints, Operations, PowerPreference,
    PresentMode, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, StoreOp,
    SurfaceConfiguration, TextureFormat, TextureUsages, TextureViewDescriptor,
};
use winit::dpi::PhysicalSize;
use winit::event::{Event, KeyEvent, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::WindowBuilder;

pub use camera::Camera;
pub use color::Color;
pub use dynamic_storage::DynamicStorageBuffer;
pub use lines::Line;
pub use rect_circle::RectOrCircle;
#[cfg(feature = "glam")]
pub use vectors::AsVector2;
pub use vectors::Vector2;
pub use winit::event::{ElementState, MouseButton};
pub use winit::keyboard::KeyCode;

mod camera;
mod color;
mod dynamic_storage;
mod lines;
mod rect_circle;
mod util;
mod vectors;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RenderStage {
    Line,
    RectsAndCircles,
}

#[derive(Default)]
pub struct RenderController {
    render_order: Vec<RenderStage>,
    lines: Vec<Line>,
    rects: Vec<RectOrCircle>,
}

impl RenderController {
    pub fn new() -> Self {
        Self::default()
    }

    fn clear(&mut self) {
        self.render_order.clear();
        self.lines.clear();
        self.rects.clear();
    }

    /// Panics if render stage has already been added.
    pub fn add_stage(&mut self, stage: RenderStage) {
        assert!(!self.render_order.contains(&stage));
        self.render_order.push(stage);
    }

    pub fn try_add_stage(&mut self, stage: RenderStage) -> bool {
        if self.render_order.contains(&stage) {
            false
        } else {
            self.render_order.push(stage);
            true
        }
    }

    pub fn add_line(&mut self, line: Line) {
        self.lines.push(line);
    }

    pub fn add_rect_or_circle(&mut self, shape: RectOrCircle) {
        self.rects.push(shape);
    }
}

#[allow(unused_variables)]
pub trait Renderable {
    const CAMERA_MOVE_SPEED: f32 = 0.01;
    const ZOOM_RATE: f32 = 1.1;
    const SHIFT_SPEED_MULT: f32 = 5.0;

    const USE_LINE_ALPHA: bool = false;

    fn initial_camera(&self) -> Camera {
        Camera::default()
    }

    fn tick(&mut self, access: &WindowAccess) {}
    fn render(&mut self, render: &mut RenderController);

    fn on_key_event(&mut self, key_code: KeyCode, state: ElementState, repeat: bool) {}
    fn on_mouse_event(&mut self, button: MouseButton, state: ElementState) {}
}

pub struct WindowAccess<'a> {
    keys_down: &'a HashSet<KeyCode>,
    keys_pressed: &'a HashSet<KeyCode>,
    keys_released: &'a HashSet<KeyCode>,

    buttons_down: &'a HashSet<MouseButton>,
    buttons_pressed: &'a HashSet<MouseButton>,
    buttons_released: &'a HashSet<MouseButton>,

    camera: &'a Camera,
    mouse_pos_screen: Vector2,
    mouse_pos_world: Vector2,
}

impl WindowAccess<'_> {
    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn is_key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    pub fn is_button_down(&self, button: MouseButton) -> bool {
        self.buttons_down.contains(&button)
    }

    pub fn is_button_pressed(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }

    pub fn is_button_released(&self, button: MouseButton) -> bool {
        self.buttons_released.contains(&button)
    }

    pub fn camera_target(&self) -> Vector2 {
        self.camera.target
    }

    pub fn camera_zoom(&self) -> f32 {
        self.camera.zoom
    }

    pub fn mouse_pos_screen(&self) -> Vector2 {
        self.mouse_pos_screen
    }

    pub fn mouse_pos_world(&self) -> Vector2 {
        self.mouse_pos_world
    }
}

pub fn run<A: Renderable>(mut application: A) {
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
        force_fallback_adapter: true,
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

    let mut camera_transforms = CameraTransforms::new(&device, size);
    camera_transforms.camera = application.initial_camera();

    let rect_circle_data = DynamicStorageBuffer::new(&device);
    let rect_circle_shader = device.create_shader_module(include_wgsl!("rect_circle.wgsl"));
    let mut rect_circle_render = RectCircleRenderPipeline::new(
        &device,
        rect_circle_data,
        rect_circle_shader,
        texture_format,
    );

    let line_data = DynamicStorageBuffer::new(&device);
    let line_shader = device.create_shader_module(include_wgsl!("lines.wgsl"));
    let mut line_render = LineRenderPipeline::new(
        &device,
        line_data,
        line_shader,
        texture_format,
        size,
        A::USE_LINE_ALPHA,
    );

    let mut frame_moments = VecDeque::new();
    let mut keys_down = HashSet::new();
    let mut keys_pressed = HashSet::new();
    let mut keys_released = HashSet::new();
    let mut buttons_down = HashSet::new();
    let mut buttons_pressed = HashSet::new();
    let mut buttons_released = HashSet::new();
    let mut command_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    let mut mouse_pos_screen = Vector2::default();
    let mut mouse_pos_world = Vector2::default();

    let mut render_controller = RenderController::new();

    let mut inner_size = window.inner_size();

    event_loop
        .run(|event, target| {
            if let Event::AboutToWait = event {
                const MOVE_DIRS: [(KeyCode, Vector2); 4] = [
                    (KeyCode::KeyW, Vector2::UP),
                    (KeyCode::KeyA, Vector2::LEFT),
                    (KeyCode::KeyS, Vector2::DOWN),
                    (KeyCode::KeyD, Vector2::RIGHT),
                ];

                {
                    let mut any = false;
                    let camera = &mut camera_transforms.camera;
                    for &(_, dir) in MOVE_DIRS
                        .iter()
                        .filter(|(code, _)| keys_down.contains(code))
                    {
                        let speed_mult = match keys_down.contains(&KeyCode::ShiftLeft) {
                            true => A::SHIFT_SPEED_MULT,
                            false => 1.0,
                        };

                        camera.target += dir * A::CAMERA_MOVE_SPEED / camera.zoom * speed_mult;
                        any = true;
                    }

                    if any {
                        mouse_pos_world =
                            camera_transforms.screen_to_world(mouse_pos_screen, inner_size);
                    }
                }

                camera_transforms.update_camera(&queue);

                let access = WindowAccess {
                    keys_down: &keys_down,
                    keys_pressed: &keys_pressed,
                    keys_released: &keys_released,
                    buttons_down: &buttons_down,
                    buttons_pressed: &buttons_pressed,
                    buttons_released: &buttons_released,
                    camera: &camera_transforms.camera,
                    mouse_pos_screen,
                    mouse_pos_world,
                };
                application.tick(&access);
                keys_pressed.clear();
                buttons_pressed.clear();
                keys_released.clear();
                buttons_released.clear();

                window.request_redraw();
            } else if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::Resized(new_size) => {
                        inner_size = new_size;
                        surface_config.width = new_size.width;
                        surface_config.height = new_size.height;
                        surface.configure(&device, &surface_config);

                        camera_transforms.update_aspect_ratio(&queue, new_size);

                        line_render.resize(&device, new_size);

                        mouse_pos_world =
                            camera_transforms.screen_to_world(mouse_pos_screen, inner_size);
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        mouse_pos_screen = Vector2::new(position.x as f32, position.y as f32);

                        mouse_pos_world =
                            camera_transforms.screen_to_world(mouse_pos_screen, inner_size);
                    }
                    WindowEvent::MouseInput { button, state, .. } => {
                        application.on_mouse_event(button, state);

                        match state {
                            ElementState::Pressed => {
                                buttons_down.insert(button);
                                buttons_pressed.insert(button);
                            }
                            ElementState::Released => {
                                buttons_down.remove(&button);
                                buttons_released.insert(button);
                            }
                        }
                    }
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        let zoom_ratio = match delta {
                            MouseScrollDelta::LineDelta(_, y) => A::ZOOM_RATE.powf(y),
                            MouseScrollDelta::PixelDelta(position) => {
                                let y = position.y as f32;
                                A::ZOOM_RATE.powf(y / 14.0) // isn't 14 like the best font size or something
                            }
                        };
                        camera_transforms.camera.zoom *= zoom_ratio;
                        camera_transforms.update_camera(&queue);

                        mouse_pos_world =
                            camera_transforms.screen_to_world(mouse_pos_screen, inner_size);
                    }
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                physical_key: PhysicalKey::Code(code),
                                state,
                                repeat,
                                ..
                            },
                        ..
                    } => {
                        application.on_key_event(code, state, repeat);

                        match state {
                            ElementState::Pressed => {
                                keys_down.insert(code);
                                keys_pressed.insert(code);
                            }
                            ElementState::Released => {
                                keys_down.remove(&code);
                                keys_released.insert(code);
                            }
                        };
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        frame_moments.push_back(now);

                        render_controller.clear();
                        application.render(&mut render_controller);

                        line_render.line_data.set_new_data(
                            &device,
                            &queue,
                            &render_controller.lines,
                        );
                        rect_circle_render.instance_data.set_new_data(
                            &device,
                            &queue,
                            &render_controller.rects,
                        );

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

                            for &stage in &render_controller.render_order {
                                match stage {
                                    RenderStage::RectsAndCircles => {
                                        rect_circle_render
                                            .render(&mut render_pass, &camera_transforms);
                                    }
                                    RenderStage::Line => {
                                        line_render.render(&mut render_pass, &camera_transforms);
                                    }
                                }
                            }
                        }

                        let new_ce = device.create_command_encoder(&CommandEncoderDescriptor::default());
                        let old_ce = replace(&mut command_encoder, new_ce);
                        queue.submit(iter::once(old_ce.finish()));

                        texture.present();
                    }
                    _ => {}
                }
            };
        })
        .unwrap();
}
