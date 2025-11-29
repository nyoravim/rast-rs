pub mod color;
pub mod graphics;

use nalgebra::{Matrix4, Point3, Vector3};

use color::RGBA8;
use graphics::*;

struct Vertex {
    position: Point3<f32>,
}

struct Instance {
    model: Matrix4<f32>,
    color: u32,
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
    instances: Box<[Instance]>,
}

impl Shader for TestShader {
    type Uniform = TestUniformData;
    type Working = DummyWorking;

    fn vertex_stage(&self, context: &VertexContext<Self::Uniform>) -> VertexOutput<Self::Working> {
        let position = context.data.vertices[context.vertex_id].position;
        let instance = &context.data.instances[context.instance_id];

        let homogenous = position.to_homogeneous();
        let world_position = instance.model * homogenous;

        VertexOutput {
            position: Point3::from_homogeneous(world_position).unwrap(),
            data: DummyWorking {},
        }
    }

    fn fragment_stage(&self, context: &FragmentContext<Self::Uniform, Self::Working>) -> u32 {
        context.data.instances[context.instance_id].color
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

    let mut fb = Framebuffer::new(1600, 900, 1, true);

    fb.clear(&ClearValue {
        color: 0x787878FF,
        depth: 1.0,
    });

    println!("Cleared");

    rast.render_indexed(
        &IndexedRenderCall {
            pipeline: &Pipeline {
                depth: DepthMode::Write,
                cull_back: true,
                winding_order: WindingOrder::CounterClockwise,
                shader: TestShader {},
            },
            vertex_offset: 0,
            first_instance: 0,
            instance_count: 2,
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
                instances: Box::new([
                    Instance {
                        model: Matrix4::new_translation(&Vector3::new(0.25, 0.0, 0.25)),
                        color: 0x00FF00FF,
                    },
                    Instance {
                        model: Matrix4::new_translation(&Vector3::new(-0.25, 0.0, 0.5)),
                        color: 0xFF0000FF,
                    },
                ]),
            },
        },
        &mut fb,
    );

    println!("Rendered");

    dump_image(&fb.color_attachments()[0]);
    println!("Dumped image");
}
