use std::array;
use std::collections::LinkedList;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, Mutex};

use nalgebra::{Point2, Point3, Vector2};
use rayon::prelude::*;

use super::blending::Blendable;
use super::framebuffer::{Framebuffer, MutableScanline};
use super::scissor::Scissor;
use super::shader::{FragmentContext, Shader, VertexContext, VertexOutput};

#[derive(Debug)]
pub enum RasterizerError {
    NoRenderTarget,
    RenderTargetUnfinished,
}

impl Display for RasterizerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::NoRenderTarget => "No render target pushed to the stack!",
                Self::RenderTargetUnfinished => "Render target still present on the stack!",
            }
        )
    }
}

impl Error for RasterizerError {}

#[derive(Debug, Clone, Copy)]
pub enum BlendFactor {
    Zero,
    One,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
}

#[derive(Debug, Clone, Copy)]
pub enum BlendOp {
    Add,
    SrcSubDst,
    DstSubSrc,
}

#[derive(Debug)]
pub struct ComponentBlendOp {
    pub op: BlendOp,
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
}

#[derive(Debug)]
pub struct BlendAttachment {
    pub color: Option<ComponentBlendOp>,
    pub alpha: Option<ComponentBlendOp>,
}

fn color_to_channels(color: u32) -> [f32; 4] {
    color.to_be_bytes().map(|c| (c as f32) / 256.0)
}

fn channels_to_color(channels: [f32; 4]) -> u32 {
    u32::from_be_bytes(channels.map(|c| (c * 256.0) as u8))
}

struct BlendContext {
    src_alpha: f32,
    dst_alpha: f32,
}

impl ComponentBlendOp {
    fn channel_term(value: f32, factor: &BlendFactor, context: &BlendContext) -> f32 {
        let coeff = match factor {
            BlendFactor::Zero => 0.0,
            BlendFactor::One => 1.0,
            BlendFactor::SrcAlpha => context.src_alpha,
            BlendFactor::OneMinusSrcAlpha => 1.0 - context.src_alpha,
            BlendFactor::DstAlpha => context.dst_alpha,
            BlendFactor::OneMinusDstAlpha => 1.0 - context.dst_alpha,
        };

        coeff * value
    }

    fn blend(&self, src: f32, dst: f32, context: &BlendContext) -> f32 {
        let src_term = Self::channel_term(src, &self.src_factor, context);
        let dst_term = Self::channel_term(dst, &self.dst_factor, context);

        match &self.op {
            BlendOp::Add => src_term + dst_term,
            BlendOp::SrcSubDst => src_term - dst_term,
            BlendOp::DstSubSrc => dst_term - src_term,
        }
    }
}

impl BlendAttachment {
    fn blend_colors(&self, src: u32, dst: u32) -> u32 {
        let src_channels = color_to_channels(src);
        let dst_channels = color_to_channels(dst);

        let context = BlendContext {
            src_alpha: src_channels[3],
            dst_alpha: dst_channels[3],
        };

        channels_to_color(array::from_fn(|i| {
            let component_op = match i {
                3 => &self.alpha,
                _ => &self.color,
            };

            match component_op {
                Some(op) => op.blend(src_channels[i], dst_channels[i], &context),
                None => src_channels[i],
            }
        }))
    }
}

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

    pub blending: Option<Vec<BlendAttachment>>,

    pub shader: T,
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
        let inverse_depth = flat_weights
            .iter()
            .zip(inverse_depths.iter())
            .map(|(w, d)| w * d)
            .sum::<f32>();

        Some(FragmentInfo {
            depth: 1.0 / inverse_depth,
            weights: array::from_fn(|i| flat_weights[i] * inverse_depths[i] / inverse_depth),
        })
    } else {
        None
    }
}

// returns false if fragment should be discarded
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

fn should_discard_fragment(
    x: usize,
    depth_mode: &DepthMode,
    current_depth: f32,
    scanline: &MutableScanline,
) -> bool {
    if current_depth < 0.0 {
        true
    } else {
        depth_mode.should_test() && !depth_test(x, current_depth, scanline)
    }
}

fn render_fragment<T: Shader>(
    x: usize,
    context: &FaceContext<T>,
    scanline: &mut MutableScanline,
    point: Point2<f32>,
    frag: FragmentInfo,
) {
    let color = context
        .call
        .pipeline
        .shader
        .fragment_stage(&FragmentContext {
            instance_id: context.instance_id,
            position: Point3::new(point.x, point.y, frag.depth),
            data: &context.call.data,
            working: T::Working::blend(
                &context.vertex_output.each_ref().map(|output| &output.data),
                &frag.weights,
            ),
        });

    for i in 0..scanline.color.len() {
        let row = &mut scanline.color[i];

        row[x] = match &context.call.pipeline.blending {
            Some(blending) => blending[i].blend_colors(color, row[x]),
            None => color,
        };
    }

    if context.call.pipeline.depth.should_write()
        && let Some(depth_row) = &mut scanline.depth
    {
        depth_row[x] = frag.depth;
    }
}

fn process_pixel<T: Shader>(x: usize, context: &FaceContext<T>, scanline: &mut MutableScanline) {
    let point = Point2::new(
        (((x as f32 + 0.5) / context.fb_width as f32) * 2.0) - 1.0,
        (((scanline.y as f32 + 0.5) / context.fb_height as f32) * 2.0) - 1.0,
    );

    let vertex_positions = context.vertex_output.each_ref().map(|data| data.position);
    if let Some(frag) = process_fragment_geometry(&vertex_positions, &point, &context.call.pipeline)
    {
        if should_discard_fragment(x, &context.call.pipeline.depth, frag.depth, scanline) {
            return;
        }

        render_fragment(x, context, scanline, point, frag);
    }
}

#[derive(Debug, Default)]
pub struct RenderStats {
    pub faces_processed: usize,
    pub faces_rendered: usize,
    pub instances: usize,
    pub calls: usize,
}

pub struct Rasterizer {
    stats: RenderStats,
    render_targets: LinkedList<Arc<Mutex<Framebuffer>>>,
}

impl Rasterizer {
    pub fn new() -> Rasterizer {
        Rasterizer {
            stats: RenderStats::default(),
            render_targets: LinkedList::new(),
        }
    }

    pub fn new_frame(&mut self) -> Result<(), RasterizerError> {
        if !self.render_targets.is_empty() {
            Err(RasterizerError::RenderTargetUnfinished)
        } else {
            self.stats = RenderStats::default();
            Ok(())
        }
    }

    pub fn stats<'a>(&'a self) -> &'a RenderStats {
        &self.stats
    }

    pub fn push_render_target(&mut self, target: Arc<Mutex<Framebuffer>>) {
        self.render_targets.push_back(target);
    }

    pub fn pop_render_target(&mut self) -> Result<(), RasterizerError> {
        match self.render_targets.pop_back() {
            Some(_) => Ok(()),
            None => Err(RasterizerError::NoRenderTarget),
        }
    }

    pub fn current_render_target(&mut self) -> Result<Arc<Mutex<Framebuffer>>, RasterizerError> {
        match self.render_targets.back() {
            Some(top) => Ok(top.clone()),
            None => Err(RasterizerError::NoRenderTarget),
        }
    }

    fn render_face<T: Shader + Sync>(
        &mut self,
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

        if let Some(scissor) = final_scissor {
            let fc = FaceContext {
                instance_id,
                call,
                vertex_output: &vertex_output,
                fb_width,
                fb_height,
            };

            framebuffer
                .scanlines(scissor.y, scissor.height)
                .par_iter_mut()
                .for_each(|scanline| {
                    for delta_x in 0..scissor.width {
                        process_pixel(scissor.x + delta_x, &fc, scanline);
                    }
                });

            self.stats.faces_rendered += 1;
        }
    }

    pub fn render_indexed<T: Shader + Sync>(
        &mut self,
        call: &IndexedRenderCall<T>,
    ) -> Result<(), RasterizerError> {
        let face_count = call.indices.len() / VERTICES_PER_FACE;

        // todo: do we care about unused indices?

        let top = self.current_render_target()?;
        let mut framebuffer = top.lock().unwrap();

        for i in 0..call.instance_count {
            for j in 0..face_count {
                self.render_face(call.first_instance + i, j, call, &mut framebuffer);
                self.stats.faces_processed += 1;
            }

            self.stats.instances += 1;
        }

        self.stats.calls += 1;
        Ok(())
    }
}
