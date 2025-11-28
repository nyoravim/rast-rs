pub mod color;
pub mod graphics;

use std::sync::Mutex;

use nalgebra::Point3;

use color::RGBA8;
use graphics::*;

struct Vertex {
    position: Point3<f32>,
}

struct TestShader {
    // nothing
}

struct DummyWorking {
    // nothing
}

impl ShaderWorkingData for DummyWorking {
    fn blend(_data: &[ProcessedVertexOutput<Self>], _scale: f32) -> Self {
        DummyWorking {}
    }
}

struct TestUniformData {
    vertices: Box<[Vertex]>,
}

impl Shader for TestShader {
    type Uniform = TestUniformData;
    type Working = DummyWorking;

    fn vertex_stage(&self, context: &VertexContext<Self::Uniform>) -> VertexOutput<Self::Working> {
        VertexOutput {
            position: context.data.vertices[context.vertex_id].position,
            data: DummyWorking {},
        }
    }

    fn fragment_stage(&self, _context: &FragmentContext<Self::Uniform, Self::Working>) -> u32 {
        0xFF0000FF
    }
}

fn dump_image(data: &Image<u32>) {
    let (width, height) = data.size();
    let mut image = bmp::Image::new(width as u32, height as u32);

    for (x, y) in image.coordinates() {
        let color = match data.at(x as usize, y as usize) {
            Some(value) => RGBA8::from(value.clone()),
            None => RGBA8::default(),
        };

        image.set_pixel(
            x,
            y,
            bmp::Pixel {
                r: color.r,
                g: color.g,
                b: color.b,
            },
        );
    }

    image.save("dump.bmp").unwrap();
}

fn main() {
    let rast = Rasterizer {
        // uh
    };

    let mut fb = Framebuffer::new(1600, 900, 1, false);

    fb.clear(&ClearValue {
        color: 0x787878FF,
        depth: 1.0,
    });

    rast.render_indexed(&IndexedRenderCall {
        pipeline: &Pipeline {
            depth: DepthTesting {
                test: true,
                write: true,
            },
            cull_back: true,
            winding_order: WindingOrder::CounterClockwise,
            shader: TestShader {},
        },
        framebuffer: Mutex::new(&mut fb),
        vertex_offset: 0,
        first_instance: 0,
        instance_count: 1,
        scissor: None,
        indices: &[0, 2, 1],
        data: &TestUniformData {
            vertices: Box::new([
                Vertex {
                    position: Point3::new(0.0, -0.5, 0.0),
                },
                Vertex {
                    position: Point3::new(0.5, 0.5, 0.0),
                },
                Vertex {
                    position: Point3::new(-0.5, 0.5, 0.0),
                },
            ]),
        },
    });

    dump_image(&fb.color_attachments()[0]);
}
