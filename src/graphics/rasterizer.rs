use std::array;

use super::framebuffer::Framebuffer;
use super::shader::{Shader, VertexContext, VertexOutput};

pub struct DepthTesting {
    pub test: bool,
    pub write: bool,
}

pub enum WindingOrder {
    Clockwise,
    CounterClockwise,
}

pub struct Pipeline<S: Shader> {
    pub depth: DepthTesting,

    pub cull_back: bool,
    pub winding_order: WindingOrder,

    pub shader: S,
}

pub struct Rasterizer {
    // todo: multithreading worker
}

pub struct IndexedRenderCall<'a, T, S: Shader<Uniform = T>> {
    pub pipeline: &'a Pipeline<S>,
    pub framebuffer: &'a mut Framebuffer,

    pub vertex_offset: usize,
    pub first_instance: usize,
    pub instance_count: usize,

    pub indices: &'a [u16],
    pub data: &'a T,
}

pub const VERTICES_PER_FACE: usize = 3;

impl Rasterizer {
    fn render_face<T, S: Shader<Uniform = T>>(
        &self,
        instance_id: usize,
        face_index: usize,
        call: &IndexedRenderCall<T, S>,
    ) {
        let index_offset = face_index * VERTICES_PER_FACE;
        let _vertex_output: [VertexOutput<S::Working>; VERTICES_PER_FACE] = array::from_fn(|i| {
            let index = call.indices[index_offset + i];

            call.pipeline.shader.vertex_stage(&VertexContext {
                vertex_id: index as usize,
                instance_id: instance_id,
                data: call.data,
            })
        });

        // uh
    }

    pub fn render_indexed<T, S: Shader<Uniform = T>>(&self, call: &IndexedRenderCall<T, S>) {
        let face_count = call.indices.len() / VERTICES_PER_FACE;

        // todo: do we care about unused indices?

        for i in 0..call.instance_count {
            for j in 0..face_count {
                self.render_face(call.first_instance + i, j, call);
            }
        }
    }
}
