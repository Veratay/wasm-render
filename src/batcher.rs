#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

pub const POSITION_COMPONENTS: usize = 3;
pub const COLOR_COMPONENTS: usize = 4;
pub const MESH_VERTEX_STRIDE: usize = POSITION_COMPONENTS + COLOR_COMPONENTS;
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

    #[test]
    fn mesh_validation() {
        assert!(Mesh::new(vec![]).is_err());
        assert!(Mesh::new(vec![0.0; 5]).is_err()); // not stride-aligned
        assert!(Mesh::new(sample_vertex_data()).is_ok());
    }
}
