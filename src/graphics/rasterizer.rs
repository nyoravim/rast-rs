use std::array;

use nalgebra::{Point2, Point3, Vector2};
use rayon::prelude::*;

use super::framebuffer::{Framebuffer, MutableScanline};
use super::scissor::Scissor;
use super::shader::{
    FragmentContext, ProcessedVertexOutput, Shader, ShaderWorkingData, VertexContext, VertexOutput,
};

#[derive(Debug, Clone, Copy)]
pub enum DepthMode {
    DontCare,
    Test,
    Write,
}

impl DepthMode {
    fn should_test(&self) -> bool {
        match self {
            DepthMode::DontCare => false,
            _ => true,
        }
    }

    fn should_write(&self) -> bool {
        match self {
            DepthMode::Write => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WindingOrder {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug)]
pub struct Pipeline<T: Shader> {
    pub depth: DepthMode,

    pub cull_back: bool,
    pub winding_order: WindingOrder,

    pub shader: T,
}

pub struct Rasterizer {
    // todo: multithreading worker
}

pub struct IndexedRenderCall<'a, T: Shader> {
    pub pipeline: &'a Pipeline<T>,

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

fn rotate_cw(v: &Vector2<f32>) -> Vector2<f32> {
    Vector2::new(-v.y, v.x)
}

fn rotate_ccw(v: &Vector2<f32>) -> Vector2<f32> {
    Vector2::new(v.y, -v.x)
}

fn signed_triangle_area(points: [&Point2<f32>; 3], winding: WindingOrder) -> f32 {
    let a = points[0];
    let b = points[1];
    let c = points[2];

    let ab = b - a;
    let ac = c - a;

    let normal = match winding {
        // rotate counterclockwise 90 deg
        WindingOrder::CounterClockwise => rotate_ccw(&ab),

        // rotate clockwise 90 deg
        WindingOrder::Clockwise => rotate_cw(&ab),
    };

    ac.dot(&normal) / 2.0
}

pub const VERTICES_PER_FACE: usize = 3;

struct FragmentInfo {
    depth: f32,
    weights: [f32; VERTICES_PER_FACE],
}

fn process_fragment_geometry<T: Shader>(
    triangle: &[Point3<f32>; VERTICES_PER_FACE],
    point: &Point2<f32>,
    pipeline: &Pipeline<T>,
) -> Option<FragmentInfo> {
    let screen_points = triangle.each_ref().map(|p| p.xy());
    let areas: [_; VERTICES_PER_FACE] = array::from_fn(|i| {
        let a = &screen_points[(i + 1) % VERTICES_PER_FACE];
        let b = &screen_points[(i + 2) % VERTICES_PER_FACE];

        signed_triangle_area([a, b, point], pipeline.winding_order)
    });

    // im not gonna bother trying to make this more idiomatic
    let areas_valid = areas.each_ref().map(|area| *area >= 0.0);
    let mut should_keep = areas_valid.iter().all(|valid| *valid);

    if !pipeline.cull_back {
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

// returns false on failure
fn depth_test(x: usize, current_depth: f32, scanline: &MutableScanline) -> bool {
    if let Some(depth) = &scanline.depth {
        let closest_depth = depth[x];

        current_depth <= closest_depth
    } else {
        true
    }
}

struct FaceContext<'a, T: Shader> {
    instance_id: usize,
    call: &'a IndexedRenderCall<'a, T>,
    vertex_output: &'a [VertexOutput<T::Working>; VERTICES_PER_FACE],

    fb_width: usize,
    fb_height: usize,
}

impl Rasterizer {
    fn render_pixel<T: Shader>(
        &self,
        x: usize,
        context: &FaceContext<T>,
        scanline: &mut MutableScanline,
    ) {
        let point = Point2::new(
            (((x as f32 + 0.5) / context.fb_width as f32) * 2.0) - 1.0,
            (((scanline.y as f32 + 0.5) / context.fb_height as f32) * 2.0) - 1.0,
        );

        let vertex_positions = context.vertex_output.each_ref().map(|data| data.position);
        let frag_info =
            process_fragment_geometry(&vertex_positions, &point, &context.call.pipeline);

        if let Some(frag) = frag_info {
            if context.call.pipeline.depth.should_test() && !depth_test(x, frag.depth, scanline) {
                return;
            }

            let color = context
                .call
                .pipeline
                .shader
                .fragment_stage(&FragmentContext {
                    instance_id: context.instance_id,
                    position: Point3::new(point.x, point.y, frag.depth),
                    data: &context.call.data,
                    working: T::Working::blend(
                        &Vec::from_iter((0..VERTICES_PER_FACE).map(|i| ProcessedVertexOutput {
                            data: &context.vertex_output[i].data,
                            weight: frag.weights[i],
                        })),
                        frag.depth,
                    ),
                });

            for row in &mut scanline.color {
                row[x] = color;
            }

            if context.call.pipeline.depth.should_write()
                && let Some(depth_row) = &mut scanline.depth
            {
                depth_row[x] = frag.depth;
            }
        }
    }

    fn render_face<T: Shader + Sync>(
        &self,
        instance_id: usize,
        face_index: usize,
        call: &IndexedRenderCall<T>,
        framebuffer: &mut Framebuffer,
    ) {
        let index_offset = face_index * VERTICES_PER_FACE;
        let (fb_width, fb_height) = framebuffer.size();

        let vertex_output = array::from_fn(|i| {
            call.pipeline.shader.vertex_stage(&VertexContext {
                vertex_id: call.indices[index_offset + i] as usize,
                instance_id: instance_id,
                data: call.data,
            })
        });

        let uv = vertex_output
            .each_ref()
            .map(|output| output.position.xy().map(|x| (x + 1.0) / 2.0));

        let generated_scissor = gen_scissor(&uv, fb_width, fb_height);
        let final_scissor = match &call.scissor {
            Some(user_scissor) => generated_scissor.intersect_with(user_scissor),
            None => Some(generated_scissor), // move
        };

        let fc = FaceContext {
            instance_id,
            call,
            vertex_output: &vertex_output,
            fb_width,
            fb_height,
        };

        if let Some(scissor) = final_scissor {
            framebuffer
                .scanlines(scissor.y, scissor.height)
                .par_iter_mut()
                .for_each(|scanline| {
                    for delta_x in 0..scissor.width {
                        self.render_pixel(scissor.x + delta_x, &fc, scanline);
                    }
                });
        }
    }

    pub fn render_indexed<T: Shader + Sync>(
        &self,
        call: &IndexedRenderCall<T>,
        framebuffer: &mut Framebuffer,
    ) {
        let face_count = call.indices.len() / VERTICES_PER_FACE;

        // todo: do we care about unused indices?

        for i in 0..call.instance_count {
            for j in 0..face_count {
                self.render_face(call.first_instance + i, j, call, framebuffer);
            }
        }
    }
}
