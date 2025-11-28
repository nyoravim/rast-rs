use std::array;
use std::sync::Mutex;

use nalgebra::{Matrix2, Point2, Point3};
use rayon::prelude::*;

use super::framebuffer::Framebuffer;
use super::scissor::Scissor;
use super::shader::{
    FragmentContext, ProcessedVertexOutput, Shader, ShaderWorkingData, VertexContext, VertexOutput,
};

#[derive(Debug)]
pub struct DepthTesting {
    pub test: bool,
    pub write: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum WindingOrder {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug)]
pub struct Pipeline<T: Shader> {
    pub depth: DepthTesting,

    pub cull_back: bool,
    pub winding_order: WindingOrder,

    pub shader: T,
}

pub struct Rasterizer {
    // todo: multithreading worker
}

pub struct IndexedRenderCall<'a, T: Shader> {
    pub pipeline: &'a Pipeline<T>,
    pub framebuffer: Mutex<&'a mut Framebuffer>,

    pub vertex_offset: usize,
    pub first_instance: usize,
    pub instance_count: usize,

    pub scissor: Option<Scissor>,

    pub indices: &'a [u16],
    pub data: &'a T::Uniform,
}

pub fn gen_scissor(uv: &[Point2<f32>], max_width: usize, max_height: usize) -> Scissor {
    let mut x0 = max_width;
    let mut y0 = max_height;

    let mut x1: usize = 0;
    let mut y1: usize = 0;

    for point in uv {
        let x = point.x.clamp(0.0, 1.0) * max_width as f32;
        let y = point.y.clamp(0.0, 1.0) * max_height as f32;

        x0 = (x.floor() as usize).min(x0);
        y0 = (y.floor() as usize).min(y0);

        x1 = (x.ceil() as usize).max(x1);
        y1 = (y.ceil() as usize).max(y1);
    }

    Scissor {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

fn signed_triangle_area(points: [&Point2<f32>; 3], winding: WindingOrder) -> f32 {
    let a = points[0];
    let b = points[1];
    let c = points[2];

    let mat = match winding {
        // rotate counterclockwise 90 deg
        WindingOrder::CounterClockwise => Matrix2::new(0.0, 1.0, -1.0, 0.0),

        // rotate clockwise 90 deg
        WindingOrder::Clockwise => Matrix2::new(0.0, -1.0, 1.0, 0.0),
    };

    let ab = b - a;
    let ac = c - a;

    let normal = mat * ab;
    ac.dot(&normal) / 2.0
}

pub const VERTICES_PER_FACE: usize = 3;

struct FragmentInfo {
    depth: f32,
    weights: [f32; VERTICES_PER_FACE],
}

fn process_fragment_geometry(
    triangle: &[Point3<f32>; VERTICES_PER_FACE],
    point: &Point2<f32>,
    winding: WindingOrder,
    cull_back: bool,
) -> Option<FragmentInfo> {
    let screen_points = triangle.each_ref().map(|p| p.xy());
    let areas = array::from_fn::<_, VERTICES_PER_FACE, _>(|i| {
        let a = &screen_points[(i + 1) % VERTICES_PER_FACE];
        let b = &screen_points[(i + 2) % VERTICES_PER_FACE];

        signed_triangle_area([a, b, point], winding)
    });

    // im not gonna bother trying to make this more idiomatic
    let areas_valid = areas.each_ref().map(|area| *area >= 0.0);
    let mut should_keep = areas_valid.iter().all(|valid| *valid);

    if !cull_back {
        // if we dont cull, also keep back
        should_keep |= areas_valid.iter().all(|valid| !*valid);
    }

    if should_keep {
        let area_sum = areas.iter().sum::<f32>();
        let flat_weights = areas.map(|area| area / area_sum);

        let inverse_depths = triangle.each_ref().map(|p| 1.0 / p.z);
        Some(FragmentInfo {
            depth: 1.0 / inverse_depths.iter().sum::<f32>(),
            weights: array::from_fn(|i| flat_weights[i] * inverse_depths[i]),
        })
    } else {
        None
    }
}

impl Rasterizer {
    fn render_pixel<T: Shader>(
        &self,
        x: usize,
        y: usize,
        instance_id: usize,
        call: &IndexedRenderCall<T>,
        vertex_output: &[VertexOutput<T::Working>; VERTICES_PER_FACE],
        fb_width: usize,
        fb_height: usize,
    ) {
        let point = Point2::new(
            (((x as f32 + 0.5) / fb_width as f32) * 2.0) - 1.0,
            (((y as f32 + 0.5) / fb_height as f32) * 2.0) - 1.0,
        );

        let vertex_positions = vertex_output.each_ref().map(|data| data.position);
        let frag_info = process_fragment_geometry(
            &vertex_positions,
            &point,
            call.pipeline.winding_order,
            call.pipeline.cull_back,
        );

        if let Some(frag) = frag_info {
            let color = call.pipeline.shader.fragment_stage(&FragmentContext {
                instance_id,
                position: Point3::new(point.x, point.y, frag.depth),
                data: &call.data,
                working: T::Working::blend(
                    &Vec::from_iter((0..VERTICES_PER_FACE).map(|i| ProcessedVertexOutput {
                        data: &vertex_output[i].data,
                        weight: frag.weights[i],
                    })),
                    frag.depth,
                ),
            });

            let mut fb = call.framebuffer.lock().unwrap();
            let num_attachments = fb.color_attachments().len();

            for i in 0..num_attachments {
                fb.set_color(i, x, y, color);
            }
        }
    }

    fn render_face<T: Shader + Sync>(
        &self,
        instance_id: usize,
        face_index: usize,
        call: &IndexedRenderCall<T>,
    ) {
        let index_offset = face_index * VERTICES_PER_FACE;
        let vertex_output = array::from_fn(|i| {
            let index = call.indices[index_offset + i];

            call.pipeline.shader.vertex_stage(&VertexContext {
                vertex_id: index as usize,
                instance_id: instance_id,
                data: call.data,
            })
        });

        let uv = vertex_output
            .each_ref()
            .map(|output| output.position.xy().map(|x| (x + 1.0) / 2.0));

        let (fb_width, fb_height) = call.framebuffer.lock().unwrap().size();
        let generated_scissor = gen_scissor(&uv, fb_width, fb_height);

        let final_scissor = match &call.scissor {
            Some(user_scissor) => generated_scissor.intersect_with(user_scissor),
            None => Some(generated_scissor), // move
        };

        if let Some(scissor) = final_scissor {
            scissor.coordinates().par_bridge().for_each(|(x, y)| {
                self.render_pixel(x, y, instance_id, call, &vertex_output, fb_width, fb_height);
            });
        }
    }

    pub fn render_indexed<T: Shader + Sync>(&self, call: &IndexedRenderCall<T>) {
        let face_count = call.indices.len() / VERTICES_PER_FACE;

        // todo: do we care about unused indices?

        for i in 0..call.instance_count {
            for j in 0..face_count {
                self.render_face(call.first_instance + i, j, call);
            }
        }
    }
}
