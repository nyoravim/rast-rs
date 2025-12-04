use std::error::Error;
use std::sync::{Arc, Mutex};

use nalgebra::{Matrix4, Point3, Vector3};

use rast::graphics::*;

struct Vertex {
    position: Point3<f32>,
    color: u32,
}

struct TestShader {
    // nothing
}

struct TestWorking {
    color: u32,
}

impl Blendable for TestWorking {
    fn blend(data: &[&Self], weights: &[f32]) -> Self {
        TestWorking {
            color: u32::blend(
                &data.iter().map(|w| &w.color).collect::<Vec<&u32>>(),
                weights,
            ),
        }
    }
}

struct TestUniformData {
    model: Matrix4<f32>,
    vertices: Box<[Vertex]>,
}

impl Shader for TestShader {
    type Uniform = TestUniformData;
    type Working = TestWorking;

    fn vertex_stage(&self, context: &VertexContext<Self::Uniform>) -> VertexOutput<Self::Working> {
        let vertex = &context.data.vertices[context.vertex_id];

        let homogenous = vertex.position.to_homogeneous();
        let world_position = context.data.model * homogenous;

        VertexOutput {
            position: Point3::from_homogeneous(world_position).unwrap(),
            data: TestWorking {
                color: vertex.color,
            },
        }
    }

    fn fragment_stage(&self, context: &FragmentContext<Self::Uniform, Self::Working>) -> u32 {
        context.working.color
    }
}

fn dump_image(data: &Image<u32>) {
    let (width, height) = data.size();
    let mut image = bmp::Image::new(width as u32, height as u32);

    for (x, y) in image.coordinates() {
        let color = data.at(x as usize, y as usize).unwrap_or(&0).to_be_bytes();

        image.set_pixel(
            x,
            y,
            bmp::Pixel {
                r: color[0],
                g: color[1],
                b: color[2],
            },
        );
    }

    image.save("dump.bmp").unwrap();
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut rast = Rasterizer::new();
    let arc = Arc::new(Mutex::new(Framebuffer::new(1600, 900, 1, true)));

    {
        let mut fb = arc.lock().unwrap();
        fb.clear(&ClearValue {
            color: 0x787878FF,
            depth: 1.0,
        });
    }

    println!("Cleared");

    rast.push_render_target(arc.clone());
    rast.render_indexed(&IndexedRenderCall {
        pipeline: &Pipeline {
            depth: DepthMode::Write,
            cull_back: true,
            winding_order: WindingOrder::CounterClockwise,
            blending: None,
            shader: TestShader {},
        },
        vertex_offset: 0,
        first_instance: 0,
        instance_count: 1,
        scissor: None,
        indices: &[0, 2, 1],
        data: &TestUniformData {
            model: Matrix4::new_translation(&Vector3::new(0.0, 0.0, 0.5)),
            vertices: Box::new([
                Vertex {
                    position: Point3::new(0.0, -0.5, 0.0),
                    color: 0xFF0000FF,
                },
                Vertex {
                    position: Point3::new(0.5, 0.5, 0.0),
                    color: 0x00FF00FF,
                },
                Vertex {
                    position: Point3::new(-0.5, 0.5, 0.0),
                    color: 0x0000FFFF,
                },
            ]),
        },
    })?;

    rast.pop_render_target()?;
    println!("Rendered");

    let stats = rast.stats();
    println!("{} calls", stats.calls);
    println!("{} instances", stats.instances);
    println!("{} faces processed", stats.faces_processed);
    println!("{} faces rendered", stats.faces_rendered);

    {
        let fb = arc.lock().unwrap();
        dump_image(&fb.color_attachments()[0]);
    }

    println!("Dumped image");
    Ok(())
}
