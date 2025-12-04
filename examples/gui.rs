use std::error::Error;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{array, iter};

use nalgebra::{Matrix4, Point3, Vector3};
use rand::prelude::*;

use rast::graphics::{
    Blendable, ClearValue, DepthMode, FragmentContext, Framebuffer, IndexedRenderCall, Pipeline,
    Rasterizer, Shader, VertexContext, VertexOutput, WindingOrder,
};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowAttributes, WindowId};

struct GraphicsContext {
    window: Rc<Window>,
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,

    rast: Rasterizer,
    framebuffer: Arc<Mutex<Framebuffer>>,
}

struct Vertex {
    position: Point3<f32>,
}

struct Instance {
    model: Matrix4<f32>,
    color: u32,
}

struct ShaderWorkingData {}

impl Blendable for ShaderWorkingData {
    fn blend(_data: &[&Self], _weights: &[f32]) -> Self {
        Self {}
    }
}

struct AppShader {}

struct AppUniforms {
    view_projection: Matrix4<f32>,
    vertices: Vec<Vertex>,
    instances: Vec<Instance>,
}

impl Shader for AppShader {
    type Uniform = AppUniforms;
    type Working = ShaderWorkingData;

    fn vertex_stage(&self, context: &VertexContext<Self::Uniform>) -> VertexOutput<Self::Working> {
        let vertex = &context.data.vertices[context.vertex_id];
        let instance = &context.data.instances[context.instance_id];

        let homogenous = vertex.position.to_homogeneous();
        let world = instance.model * homogenous;
        let screen = context.data.view_projection * world;

        VertexOutput {
            position: Point3::from_homogeneous(screen).unwrap(),
            data: ShaderWorkingData {},
        }
    }

    fn fragment_stage(&self, context: &FragmentContext<Self::Uniform, Self::Working>) -> u32 {
        context.data.instances[context.instance_id].color
    }
}

impl GraphicsContext {
    fn create_framebuffer(window: &Window) -> Framebuffer {
        let size = window.inner_size();
        Framebuffer::new(size.width as usize, size.height as usize, 1, true)
    }

    fn new(event_loop: &ActiveEventLoop) -> Result<GraphicsContext, Box<dyn Error>> {
        let context = softbuffer::Context::new(event_loop.owned_display_handle())?;

        let window = Rc::new(event_loop.create_window(WindowAttributes::default())?);
        let surface = softbuffer::Surface::new(&context, window.clone())?;

        Ok(GraphicsContext {
            window: window.clone(),
            surface,

            rast: Rasterizer::new(),
            framebuffer: Arc::new(Mutex::new(Self::create_framebuffer(&window))),
        })
    }

    fn current_fb_size(&self) -> (usize, usize) {
        let fb = self.framebuffer.lock().unwrap();
        fb.size()
    }

    fn update_context(&mut self) -> Result<(), Box<dyn Error>> {
        let size = self.window.inner_size();
        self.surface.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        )?;

        let (fb_width, fb_height) = self.current_fb_size();
        if size.width as usize != fb_width || size.height as usize != fb_height {
            let new_fb = Self::create_framebuffer(&self.window);
            self.framebuffer = Arc::new(Mutex::new(new_fb));
        }

        Ok(())
    }

    fn present(&mut self) -> Result<(), Box<dyn Error>> {
        let mut buffer = self.surface.buffer_mut()?;

        let fb = self.framebuffer.lock().unwrap();
        let attachments = fb.color_attachments();
        let output_attachment = &attachments[0];

        let (width, height) = output_attachment.size();
        let data = output_attachment.data();

        for i in 0..(width * height) {
            buffer[i] = data[i] >> 8;
        }

        buffer.present()?;
        Ok(())
    }
}

struct AppData {
    pipeline: Pipeline<AppShader>,
    uniforms: AppUniforms,
    indices: Vec<u16>,

    t0: Instant,
    theta: f32,
}

struct App {
    graphics: Option<GraphicsContext>,
    initialized: bool,

    last_update: Option<Instant>,
    data: AppData,
}

impl App {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        self.graphics = Some(GraphicsContext::new(event_loop)?);

        Ok(())
    }

    fn update(graphics: &GraphicsContext, data: &mut AppData) {
        let fb = graphics.framebuffer.lock().unwrap();
        let (width, height) = fb.size();

        let aspect = (width as f32) / (height as f32);

        let t1 = Instant::now();
        let delta = t1 - data.t0;
        data.t0 = t1;

        data.theta += delta.as_secs_f32() * std::f32::consts::PI / 4.0;
        let sin_theta = data.theta.sin();
        let cos_theta = data.theta.cos();

        let phi = sin_theta * std::f32::consts::PI / 4.0;
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        let radial = Point3::new(cos_theta * cos_phi, sin_phi, sin_theta * cos_phi);
        let camera_distance = 1.0;

        let view = Matrix4::look_at_rh(
            &(radial * camera_distance),
            &Point3::origin(),
            &Vector3::new(0.0, 1.0, 0.0),
        );

        let projection = Matrix4::new_perspective(aspect, std::f32::consts::PI / 4.0, 0.1, 100.0);
        data.uniforms.view_projection = projection * view;

        let instance_count = data.uniforms.instances.len();
        for i in 0..instance_count {
            let theta = std::f32::consts::PI * 2.0 * (i as f32) / (instance_count as f32);

            let scale = Matrix4::new_scaling(0.25);
            let rotation = Matrix4::new_rotation(Vector3::new(0.0, theta, 0.0));
            let translation = Matrix4::new_translation(&Vector3::new(0.0, 0.0, -0.5));

            let instance = &mut data.uniforms.instances[i];
            instance.model = scale * rotation * translation;
        }
    }

    fn clear_framebuffer(graphics: &mut GraphicsContext) {
        let mut fb = graphics.framebuffer.lock().unwrap();
        fb.clear(&ClearValue {
            color: 0x787878FF,
            depth: 1.0,
        });
    }

    fn render(graphics: &mut GraphicsContext, data: &AppData) -> Result<(), Box<dyn Error>> {
        Self::clear_framebuffer(graphics);

        graphics.rast.new_frame()?;
        graphics.rast.push_render_target(graphics.framebuffer.clone());

        graphics.rast.render_indexed(&IndexedRenderCall {
            pipeline: &data.pipeline,
            vertex_offset: 0,
            first_instance: 0,
            instance_count: data.uniforms.instances.len(),
            scissor: None,
            indices: &data.indices,
            data: &data.uniforms,
        })?;

        graphics.rast.pop_render_target()?;
        Ok(())
    }

    fn should_update(&self) -> bool {
        if let Some(timestamp) = &self.last_update {
            timestamp.elapsed().as_secs_f32() > 1.0 / 60.0
        } else {
            true
        }
    }

    fn redraw_requested(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(graphics) = &mut self.graphics {
            graphics.window.request_redraw();
        }

        if !self.should_update() {
            return Ok(());
        }

        if let Some(graphics) = &mut self.graphics {
            graphics.update_context()?;

            Self::update(graphics, &mut self.data);
            Self::render(graphics, &self.data)?;

            graphics.present()?;
        }

        self.last_update = Some(Instant::now());
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
        let should_ignore = match &self.graphics {
            Some(graphics) => window_id != graphics.window.id(),
            None => true,
        };

        if should_ignore {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw_requested().unwrap(),
            _ => (),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut rng = StdRng::from_os_rng();

    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut App {
        graphics: None,
        initialized: false,
        last_update: None,

        data: AppData {
            pipeline: Pipeline {
                depth: DepthMode::Write,
                cull_back: false,
                winding_order: WindingOrder::Clockwise,
                blending: None,
                shader: AppShader {},
            },
            uniforms: AppUniforms {
                view_projection: Matrix4::identity(),
                vertices: vec![
                    Vertex {
                        position: Point3::new(0.0, -0.5, 0.0),
                    },
                    Vertex {
                        position: Point3::new(-0.5, 0.5, 0.0),
                    },
                    Vertex {
                        position: Point3::new(0.5, 0.5, 0.0),
                    },
                ],
                instances: Vec::from_iter(
                    iter::repeat_with(|| Instance {
                        model: Matrix4::identity(),
                        color: u32::from_be_bytes(array::from_fn(|i| match i {
                            3 => 0xFF,
                            _ => (rng.next_u32() & 0xFF) as u8,
                        })),
                    })
                    .take(6),
                ),
            },
            indices: vec![0, 1, 2],

            t0: Instant::now(),
            theta: 0.0,
        },
    })?;

    Ok(())
}
