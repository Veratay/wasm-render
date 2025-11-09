use js_sys::Float32Array;
use std::collections::BTreeMap;
use wasm_bindgen::JsValue;
use web_sys::WebGl2RenderingContext as Gl;

use crate::batcher::MATRIX_FLOATS;
use crate::gpu::GlBuffer;
use crate::utils::error;

pub(crate) struct MeshInstances {
    buffer: GlBuffer,
    transforms: Vec<[f32; MATRIX_FLOATS]>,
    handles: Vec<u32>,
    capacity: usize,
    pending: BTreeMap<usize, [f32; MATRIX_FLOATS]>,
    scratch: Vec<f32>,
}

impl MeshInstances {
    pub(crate) fn new(gl: &Gl, initial_capacity: usize) -> Result<Self, JsValue> {
        let buffer = GlBuffer::new(gl)?;
        buffer.bind_array_buffer();
        let capacity = initial_capacity.max(1);
        gl.buffer_data_with_i32(
            Gl::ARRAY_BUFFER,
            (capacity * MATRIX_FLOATS * std::mem::size_of::<f32>()) as i32,
            Gl::DYNAMIC_DRAW,
        );
        Ok(Self {
            buffer,
            transforms: Vec::new(),
            handles: Vec::new(),
            capacity,
            pending: BTreeMap::new(),
            scratch: Vec::new(),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.transforms.len()
    }

    pub(crate) fn buffer_handle(&self) -> &GlBuffer {
        &self.buffer
    }

    pub(crate) fn allocate(&mut self, gl: &Gl, matrix: &[f32; MATRIX_FLOATS]) -> Result<usize, JsValue> {
        let slot = self.transforms.len();
        self.transforms.push(*matrix);
        self.handles.push(0);
        self.ensure_capacity(gl, slot + 1)?;
        self.pending.insert(slot, *matrix);
        Ok(slot)
    }

    pub(crate) fn set_handle(&mut self, slot: usize, handle: u32) {
        if let Some(target) = self.handles.get_mut(slot) {
            *target = handle;
        }
    }

    pub(crate) fn update_slot(&mut self, slot: usize, matrix: &[f32; MATRIX_FLOATS]) -> Result<(), JsValue> {
        let target = self
            .transforms
            .get_mut(slot)
            .ok_or_else(|| error("invalid instance slot"))?;
        *target = *matrix;
        self.pending.insert(slot, *matrix);
        Ok(())
    }

    pub(crate) fn remove_slot(&mut self, slot: usize) -> Result<Option<u32>, JsValue> {
        if slot >= self.transforms.len() {
            return Err(error("invalid instance slot"));
        }
        let last_index = self.transforms.len() - 1;
        self.transforms.swap(slot, last_index);
        self.handles.swap(slot, last_index);
        self.transforms.pop();
        let _removed_handle = self.handles.pop();

        let moved_handle = if slot < self.transforms.len() {
            let handle = self.handles[slot];
            let matrix = self.transforms[slot];
            self.pending.insert(slot, matrix);
            Some(handle)
        } else {
            None
        };

        Ok(moved_handle)
    }

    pub(crate) fn ensure_capacity(&mut self, gl: &Gl, min_capacity: usize) -> Result<(), JsValue> {
        if self.capacity >= min_capacity.max(1) {
            return Ok(());
        }
        let mut new_capacity = self.capacity.max(1);
        while new_capacity < min_capacity {
            new_capacity *= 2;
        }
        self.capacity = new_capacity;
        self.buffer.bind_array_buffer();
        gl.buffer_data_with_i32(
            Gl::ARRAY_BUFFER,
            (self.capacity * MATRIX_FLOATS * std::mem::size_of::<f32>()) as i32,
            Gl::DYNAMIC_DRAW,
        );
        self.upload_all(gl);
        Ok(())
    }

    pub(crate) fn upload_all(&self, gl: &Gl) {
        if self.transforms.is_empty() {
            return;
        }
        let mut flat = Vec::with_capacity(self.transforms.len() * MATRIX_FLOATS);
        for matrix in &self.transforms {
            flat.extend_from_slice(matrix);
        }
        self.buffer.bind_array_buffer();
        let view = unsafe { Float32Array::view(&flat) };
        gl.buffer_sub_data_with_f64_and_array_buffer_view(Gl::ARRAY_BUFFER, 0.0, &view);
    }

    pub(crate) fn defragment(&mut self, gl: &Gl) {
        self.capacity = self.transforms.len().max(1);
        self.buffer.bind_array_buffer();
        gl.buffer_data_with_i32(
            Gl::ARRAY_BUFFER,
            (self.capacity * MATRIX_FLOATS * std::mem::size_of::<f32>()) as i32,
            Gl::DYNAMIC_DRAW,
        );
        self.upload_all(gl);
        self.pending.clear();
    }

    pub(crate) fn flush_pending(&mut self, gl: &Gl) {
        if self.pending.is_empty() {
            return;
        }
        self.scratch.clear();
        let mut current_start: Option<usize> = None;
        let mut last_slot = 0usize;
        for (slot, matrix) in self.pending.iter() {
            if let Some(start) = current_start {
                if *slot == last_slot + 1 {
                    self.scratch.extend_from_slice(matrix);
                } else {
                    self.write_chunk(gl, start, &self.scratch);
                    self.scratch.clear();
                    self.scratch.extend_from_slice(matrix);
                    current_start = Some(*slot);
                }
            } else {
                current_start = Some(*slot);
                self.scratch.extend_from_slice(matrix);
            }
            last_slot = *slot;
        }
        if let Some(start) = current_start {
            self.write_chunk(gl, start, &self.scratch);
        }
        self.pending.clear();
        self.scratch.clear();
    }

    pub(crate) fn write_chunk(&self, gl: &Gl, start_slot: usize, data: &[f32]) {
        if data.is_empty() {
            return;
        }
        self.buffer.bind_array_buffer();
        let offset = (start_slot * MATRIX_FLOATS * std::mem::size_of::<f32>()) as f64;
        let view = unsafe { Float32Array::view(data) };
        gl.buffer_sub_data_with_f64_and_array_buffer_view(Gl::ARRAY_BUFFER, offset, &view);
    }
}
