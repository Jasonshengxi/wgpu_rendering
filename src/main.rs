use wgpu_rendering::{
    run, Color, ElementState, KeyCode, Line, RectOrCircle, RenderController, RenderStage,
    Renderable, Vector2, WindowAccess,
};

fn main() {
    run(TestApp::default());
}

#[derive(Default)]
struct TestApp {
    debug_queued: bool,
    mouse_pos: Option<Vector2>,
}

impl Renderable for TestApp {
    fn tick(&mut self, access: &WindowAccess) {
        self.mouse_pos = Some(access.mouse_pos_world());

        if self.debug_queued {
            println!("screen: {:?}", access.mouse_pos_screen());
            println!("world: {:?}", access.mouse_pos_world());
            self.debug_queued = false;
        }
    }

    fn render(&mut self, render: &mut RenderController) {
        if let Some(mouse_pos) = self.mouse_pos {
            render.add_stage(RenderStage::Line);
            render.add_stage(RenderStage::RectsAndCircles);

            render.add_line(Line::new(mouse_pos, Vector2::ZERO, Color::WHITE));
            render.add_rect_or_circle(RectOrCircle::circle(mouse_pos, 0.1, Color::RED))
        }

        render.try_add_stage(RenderStage::Line);
        for i in -10..=10 {
            for j in -10..=10 {
                let pos = Vector2::new(i as f32, j as f32) / 10.0;
                render.add_line(Line::new(pos, pos + Vector2::UP * 0.01, Color::GRAY));
            }
        }
    }

    fn on_key_event(&mut self, key_code: KeyCode, state: ElementState, _repeat: bool) {
        if let ElementState::Pressed = state {
            if let KeyCode::Enter = key_code {
                self.debug_queued = true;
            }
        }
    }
}
