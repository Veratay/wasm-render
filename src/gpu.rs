use wasm_bindgen::JsValue;
use web_sys::{WebGl2RenderingContext as Gl, WebGlBuffer, WebGlVertexArrayObject};

use crate::utils::error;

pub(crate) struct GlBuffer {
    gl: Gl,
    handle: WebGlBuffer,
}

impl GlBuffer {
    pub(crate) fn new(gl: &Gl) -> Result<Self, JsValue> {
        let handle = gl
            .create_buffer()
            .ok_or_else(|| error("failed to create buffer"))?;
        Ok(Self {
            gl: gl.clone(),
            handle,
        })
    }

    pub(crate) fn handle(&self) -> &WebGlBuffer {
        &self.handle
    }

    pub(crate) fn bind_array_buffer(&self) {
        self.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.handle));
    }
}

impl Drop for GlBuffer {
    fn drop(&mut self) {
        self.gl.delete_buffer(Some(&self.handle));
    }
}

pub(crate) struct VertexArray {
    gl: Gl,
    handle: WebGlVertexArrayObject,
}

impl VertexArray {
    pub(crate) fn new(gl: &Gl) -> Result<Self, JsValue> {
        let handle = gl
            .create_vertex_array()
            .ok_or_else(|| error("failed to create vertex array"))?;
        Ok(Self {
            gl: gl.clone(),
            handle,
        })
    }

    pub(crate) fn handle(&self) -> &WebGlVertexArrayObject {
        &self.handle
    }
}

impl Drop for VertexArray {
    fn drop(&mut self) {
        self.gl.delete_vertex_array(Some(&self.handle));
    }
}
