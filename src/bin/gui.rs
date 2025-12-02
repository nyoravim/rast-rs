use std::error::Error;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowAttributes, WindowId};

struct GraphicsContext {
    window: Rc<Window>,
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
    last_redraw: Instant,
}

impl GraphicsContext {
    fn new(event_loop: &ActiveEventLoop) -> Result<GraphicsContext, Box<dyn Error>> {
        let context = softbuffer::Context::new(event_loop.owned_display_handle())?;

        let window = Rc::new(event_loop.create_window(WindowAttributes::default())?);
        let surface = softbuffer::Surface::new(&context, window.clone())?;

        Ok(GraphicsContext {
            window,
            surface,
            last_redraw: Instant::now(),
        })
    }
}

#[derive(Default)]
struct App {
    graphics: Option<GraphicsContext>,
    initialized: bool,
}

impl App {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        self.graphics = Some(GraphicsContext::new(event_loop)?);

        Ok(())
    }

    fn render(graphics: &mut GraphicsContext) -> Result<(), Box<dyn Error>> {
        let mut buffer = graphics.surface.buffer_mut()?;
        buffer.iter_mut().for_each(|p| *p = 0x787878);

        println!("Test");
        Ok(())
    }

    fn redraw_requested(graphics: &mut GraphicsContext) -> Result<(), Box<dyn Error>> {
        graphics.window.request_redraw();

        if graphics.last_redraw.elapsed().as_secs_f32() > 1.0 / 60.0 {
            let size = graphics.window.inner_size();
            graphics.surface.resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )?;

            Self::render(graphics)?;

            graphics.surface.buffer_mut()?.present()?;
            graphics.last_redraw = Instant::now();
        }

        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.initialized {
            self.init(event_loop).unwrap();
            self.initialized = true;
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(graphics) = &mut self.graphics else {
            panic!("Graphics not initialized!");
        };

        if window_id != graphics.window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => Self::redraw_requested(graphics).unwrap(),
            _ => (),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;

    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut App {
        graphics: None,
        initialized: false,
    })?;

    Ok(())
}
