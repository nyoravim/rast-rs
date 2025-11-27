pub struct VertexContext<'a, T> {
    pub vertex_id: usize,
    pub instance_id: usize,
    pub data: &'a T,
}

pub struct FragmentContext<'a, T> {
    pub instance_id: usize,
    pub data: &'a T,
}

pub struct VertexOutput<T> {
    pub position: [f32; 4],
    pub data: T,
}

pub trait ShaderWorkingData {
    fn blend<'a>(data: &[(&'a Self, f32)]) -> Self;
}

pub trait Shader {
    type Uniform;
    type Working: ShaderWorkingData;

    fn vertex_stage(&self, context: &VertexContext<Self::Uniform>) -> VertexOutput<Self::Working>;

    fn fragment_stage(
        &self,
        context: &FragmentContext<Self::Uniform>,
        inputs: &[VertexOutput<Self::Working>; 3],
    ) -> u32;
}
