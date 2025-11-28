use nalgebra::Point3;

pub struct VertexContext<'a, U> {
    pub vertex_id: usize,
    pub instance_id: usize,
    pub data: &'a U,
}

pub struct FragmentContext<'a, U, W> {
    pub instance_id: usize,
    pub position: Point3<f32>,

    pub data: &'a U,
    pub working: W,
}

pub struct VertexOutput<W> {
    pub position: Point3<f32>,
    pub data: W,
}

pub trait ShaderWorkingData {
    fn blend<'a>(data: &[(&'a Self, f32)]) -> Self;
}

pub trait Shader {
    type Uniform: Sync;
    type Working: ShaderWorkingData + Sync;

    fn vertex_stage(&self, context: &VertexContext<Self::Uniform>) -> VertexOutput<Self::Working>;
    fn fragment_stage(&self, context: &FragmentContext<Self::Uniform, Self::Working>) -> u32;
}
