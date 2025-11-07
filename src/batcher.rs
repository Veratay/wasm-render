use std::cmp;

pub const POSITION_COMPONENTS: usize = 3;
pub const COLOR_COMPONENTS: usize = 4;
pub const INSTANCE_ID_COMPONENTS: usize = 1;
pub const MESH_VERTEX_STRIDE: usize = POSITION_COMPONENTS + COLOR_COMPONENTS;
pub const BATCHED_VERTEX_STRIDE: usize = MESH_VERTEX_STRIDE + INSTANCE_ID_COMPONENTS;
pub const MATRIX_FLOATS: usize = 16;

#[derive(Clone)]
pub struct Mesh {
    data: Vec<f32>, // position (xyz) + color (rgba) per vertex
}

impl Mesh {
    pub fn new(data: Vec<f32>) -> Result<Self, &'static str> {
        if data.is_empty() {
            return Err("mesh requires at least one vertex");
        }
        if data.len() % MESH_VERTEX_STRIDE != 0 {
            return Err("mesh vertices must be (x, y, z, r, g, b, a)");
        }
        Ok(Self { data })
    }

    #[inline]
    pub fn raw(&self) -> &[f32] {
        &self.data
    }
}

pub struct GeometryBatch {
    vertices: Vec<f32>,
    model_matrices: Vec<f32>,
    max_instances: usize,
}

impl GeometryBatch {
    pub fn new(max_instances: usize) -> Self {
        let capacity = cmp::max(1, max_instances);
        Self {
            vertices: Vec::with_capacity(capacity * MESH_VERTEX_STRIDE * 3),
            model_matrices: Vec::with_capacity(capacity * MATRIX_FLOATS),
            max_instances: capacity,
        }
    }

    pub fn push_instance(
        &mut self,
        mesh: &Mesh,
        transform: &[f32; MATRIX_FLOATS],
    ) -> Result<usize, &'static str> {
        let instance_index = self.instance_count();
        if instance_index >= self.max_instances {
            return Err("instance limit exceeded for this batch");
        }

        self.model_matrices.extend_from_slice(transform);

        for chunk in mesh.raw().chunks(MESH_VERTEX_STRIDE) {
            self.vertices.extend_from_slice(chunk);
            self.vertices.push(instance_index as f32);
        }

        Ok(instance_index)
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.model_matrices.clear();
    }

    #[inline]
    pub fn instance_count(&self) -> usize {
        self.model_matrices.len() / MATRIX_FLOATS
    }

    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / BATCHED_VERTEX_STRIDE
    }

    #[inline]
    pub fn vertices(&self) -> &[f32] {
        &self.vertices
    }

    #[inline]
    pub fn models(&self) -> &[f32] {
        &self.model_matrices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vertex_data() -> Vec<f32> {
        vec![
            // tri 1
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, //
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, //
            0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, //
        ]
    }

    fn identity() -> [f32; MATRIX_FLOATS] {
        let mut m = [0.0; MATRIX_FLOATS];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        m
    }

    #[test]
    fn mesh_validation() {
        assert!(Mesh::new(vec![]).is_err());
        assert!(Mesh::new(vec![0.0; 5]).is_err()); // not stride-aligned
        assert!(Mesh::new(sample_vertex_data()).is_ok());
    }

    #[test]
    fn geometry_batch_pushes_vertices_and_matrices() {
        let mesh = Mesh::new(sample_vertex_data()).unwrap();
        let mut batch = GeometryBatch::new(4);
        let idx = batch.push_instance(&mesh, &identity()).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(batch.instance_count(), 1);

        // Each vertex should have the instance id appended.
        let verts = batch.vertices();
        assert_eq!(
            verts.len(),
            mesh.raw().len() + mesh.raw().len() / MESH_VERTEX_STRIDE
        );
        for chunk in verts.chunks(BATCHED_VERTEX_STRIDE) {
            assert_eq!(chunk.last().copied(), Some(0.0));
        }

        // Matrix data stored as-is.
        let matrix = identity();
        assert_eq!(batch.models(), &matrix[..]);
    }

    #[test]
    fn geometry_batch_enforces_instance_limit() {
        let mesh = Mesh::new(sample_vertex_data()).unwrap();
        let mut batch = GeometryBatch::new(1);
        assert!(batch.push_instance(&mesh, &identity()).is_ok());
        let err = batch.push_instance(&mesh, &identity()).unwrap_err();
        assert_eq!(err, "instance limit exceeded for this batch");
    }
}
