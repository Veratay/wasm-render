use js_sys::Float32Array;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

mod batcher;
mod camera;
mod instances;
mod shader;

use batcher::{COLOR_COMPONENTS, MATRIX_FLOATS, MESH_VERTEX_STRIDE, Mesh, POSITION_COMPONENTS};
use camera::{orbit_view_matrix, perspective_matrix};
use instances::InstanceStore;
use shader::{compile_shader, fragment_shader_source, link_program, vertex_shader_source};

const INITIAL_INSTANCE_HINT: usize = 256;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_str(s: &str);
}

#[wasm_bindgen]
pub struct BatchedRenderer {
    gl: Gl,
    program: WebGlProgram,
    position_location: u32,
    color_location: u32,
    instance_locations: [u32; 4],
    view_location: WebGlUniformLocation,
    projection_location: WebGlUniformLocation,
    meshes: Vec<GpuMesh>,
    mesh_instances: Vec<MeshInstances>,
    instance_store: InstanceStore,
    transient_instances: Vec<u32>,
    canvas: HtmlCanvasElement,
    view_matrix: [f32; MATRIX_FLOATS],
    projection_matrix: [f32; MATRIX_FLOATS],
    clear_color: [f32; 4],
    max_instances_per_draw: usize,
}

#[wasm_bindgen]
impl BatchedRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<BatchedRenderer, JsValue> {
        let window = web_sys::window().ok_or_else(|| error("missing window"))?;
        let document = window.document().ok_or_else(|| error("missing document"))?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| error("canvas not found"))?
            .dyn_into::<HtmlCanvasElement>()?;

        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| error("webgl2 context unavailable"))?
            .dyn_into()?;

        gl.enable(Gl::DEPTH_TEST);
        gl.depth_func(Gl::LEQUAL);
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        let uniform_vectors = get_i32_parameter(&gl, Gl::MAX_VERTEX_UNIFORM_VECTORS)?;
        let max_instances_per_draw = compute_instance_budget(uniform_vectors)?;

        let vert_shader = compile_shader(&gl, Gl::VERTEX_SHADER, vertex_shader_source())?;
        let frag_shader = compile_shader(&gl, Gl::FRAGMENT_SHADER, fragment_shader_source())?;
        let program = link_program(&gl, &vert_shader, &frag_shader)?;

        let position_location = gl
            .get_attrib_location(&program, "a_position")
            .try_into()
            .map_err(|_| error("a_position attribute missing"))?;
        let color_location = gl
            .get_attrib_location(&program, "a_color")
            .try_into()
            .map_err(|_| error("a_color attribute missing"))?;
        let instance_locations = [
            gl.get_attrib_location(&program, "a_instance_col0")
                .try_into()
                .map_err(|_| error("a_instance_col0 attribute missing"))?,
            gl.get_attrib_location(&program, "a_instance_col1")
                .try_into()
                .map_err(|_| error("a_instance_col1 attribute missing"))?,
            gl.get_attrib_location(&program, "a_instance_col2")
                .try_into()
                .map_err(|_| error("a_instance_col2 attribute missing"))?,
            gl.get_attrib_location(&program, "a_instance_col3")
                .try_into()
                .map_err(|_| error("a_instance_col3 attribute missing"))?,
        ];

        let view_location = gl
            .get_uniform_location(&program, "u_view")
            .ok_or_else(|| error("u_view uniform missing"))?;
        let projection_location = gl
            .get_uniform_location(&program, "u_projection")
            .ok_or_else(|| error("u_projection uniform missing"))?;

        let mut renderer = BatchedRenderer {
            gl,
            program,
            position_location,
            color_location,
            instance_locations,
            view_location,
            projection_location,
            meshes: Vec::new(),
            mesh_instances: Vec::new(),
            instance_store: InstanceStore::new(),
            transient_instances: Vec::new(),
            canvas,
            view_matrix: identity_matrix(),
            projection_matrix: identity_matrix(),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            max_instances_per_draw,
        };

        renderer.gl.use_program(Some(&renderer.program));
        renderer.upload_view_matrix();
        renderer.upload_projection_matrix();

        let width = renderer.canvas.width().max(1);
        let height = renderer.canvas.height().max(1);
        renderer.resize(width, height);

        Ok(renderer)
    }

    pub fn register_mesh(&mut self, vertices: &Float32Array) -> Result<u32, JsValue> {
        let data = vertices.to_vec();
        let mesh = Mesh::new(data).map_err(error)?;
        let vertex_count = (mesh.raw().len() / MESH_VERTEX_STRIDE) as i32;
        if vertex_count <= 0 {
            return Err(error("mesh requires at least one triangle"));
        }

        let vao = self
            .gl
            .create_vertex_array()
            .ok_or_else(|| error("failed to create vertex array"))?;
        let vertex_buffer = self
            .gl
            .create_buffer()
            .ok_or_else(|| error("failed to create vertex buffer"))?;

        let mesh_instances = MeshInstances::new(&self.gl, INITIAL_INSTANCE_HINT)?;

        self.gl.bind_vertex_array(Some(&vao));
        self.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&vertex_buffer));
        let vertex_view = unsafe { Float32Array::view(mesh.raw()) };
        self.gl
            .buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &vertex_view, Gl::STATIC_DRAW);
        self.configure_mesh_attributes();

        self.gl
            .bind_buffer(Gl::ARRAY_BUFFER, Some(mesh_instances.buffer()));
        self.configure_instance_attributes();
        self.gl.bind_vertex_array(None);

        self.meshes.push(GpuMesh {
            vao,
            _vertex_buffer: vertex_buffer,
            vertex_count,
        });
        self.mesh_instances.push(mesh_instances);
        Ok((self.meshes.len() - 1) as u32)
    }

    pub fn create_instance(
        &mut self,
        mesh_handle: u32,
        transform: &Float32Array,
    ) -> Result<u32, JsValue> {
        let mesh_index = mesh_handle as usize;
        let matrix = matrix_from_array(transform)?;
        let mesh_instances = self
            .mesh_instances
            .get_mut(mesh_index)
            .ok_or_else(|| error("invalid mesh handle"))?;
        let slot = mesh_instances.allocate(&self.gl, &matrix)?;
        let handle = self.instance_store.insert(mesh_index, slot, matrix);
        mesh_instances.set_handle(slot, handle);
        Ok(handle)
    }

    pub fn set_instance_transform(
        &mut self,
        instance_handle: u32,
        transform: &Float32Array,
    ) -> Result<(), JsValue> {
        let matrix = matrix_from_array(transform)?;
        let record = self
            .instance_store
            .get_mut(instance_handle)
            .ok_or_else(|| error("invalid instance handle"))?;
        record.transform = matrix;
        let instances = self
            .mesh_instances
            .get_mut(record.mesh_index)
            .ok_or_else(|| error("invalid mesh handle"))?;
        instances.update_slot(record.slot_index, &matrix)?;
        Ok(())
    }

    pub fn remove_instance(&mut self, instance_handle: u32) -> Result<(), JsValue> {
        if self.remove_instance_internal(instance_handle)? {
            self.transient_instances
                .retain(|handle| *handle != instance_handle);
            Ok(())
        } else {
            Err(error("invalid instance handle"))
        }
    }

    pub fn queue_instance(
        &mut self,
        mesh_handle: u32,
        transform: &Float32Array,
    ) -> Result<(), JsValue> {
        let handle = self.create_instance(mesh_handle, transform)?;
        self.transient_instances.push(handle);
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), JsValue> {
        if self.instance_store.is_empty() {
            self.transient_instances.clear();
            return Ok(());
        }

        self.gl.use_program(Some(&self.program));
        self.bind_globals();

        for mesh_index in 0..self.mesh_instances.len() {
            self.draw_mesh_instances(mesh_index)?;
        }

        self.remove_transient_instances();
        Ok(())
    }

    pub fn set_view_matrix(&mut self, matrix: &Float32Array) -> Result<(), JsValue> {
        copy_into_matrix(&mut self.view_matrix, matrix)?;
        self.gl.use_program(Some(&self.program));
        self.upload_view_matrix();
        Ok(())
    }

    pub fn set_projection_matrix(&mut self, matrix: &Float32Array) -> Result<(), JsValue> {
        copy_into_matrix(&mut self.projection_matrix, matrix)?;
        self.gl.use_program(Some(&self.program));
        self.upload_projection_matrix();
        Ok(())
    }

    pub fn clear(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [clamp_unit(r), clamp_unit(g), clamp_unit(b), clamp_unit(a)];
        self.gl.clear_color(
            self.clear_color[0],
            self.clear_color[1],
            self.clear_color[2],
            self.clear_color[3],
        );
        self.gl.clear_depth(1.0);
        self.gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.gl.viewport(0, 0, width as i32, height as i32);
    }

    pub fn max_instances(&self) -> u32 {
        self.max_instances_per_draw as u32
    }

    pub fn instance_count(&self) -> u32 {
        self.instance_store.len() as u32
    }

    pub fn queued_instances(&self) -> u32 {
        self.transient_instances.len() as u32
    }

    pub fn defragment_instances(&mut self) {
        for instances in &mut self.mesh_instances {
            instances.flush_pending(&self.gl);
            instances.defragment(&self.gl);
        }
    }
}

#[wasm_bindgen]
pub fn test_wasm() -> JsValue {
    log_str("WebGL2 renderer ready");
    JsValue::TRUE
}

#[wasm_bindgen]
pub fn build_perspective(
    fov_y_radians: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> Result<Float32Array, JsValue> {
    let matrix = perspective_matrix(fov_y_radians, aspect, near, far).map_err(error)?;
    Ok(Float32Array::from(matrix.as_slice()))
}

#[wasm_bindgen]
pub fn build_orbit_view(
    target: &Float32Array,
    yaw: f32,
    pitch: f32,
    distance: f32,
) -> Result<Float32Array, JsValue> {
    let target_vec = vec3_from_array(target)?;
    let view = orbit_view_matrix(target_vec, yaw, pitch, distance).map_err(error)?;
    Ok(Float32Array::from(view.as_slice()))
}

impl BatchedRenderer {
    fn bind_globals(&self) {
        self.upload_view_matrix();
        self.upload_projection_matrix();
    }

    fn configure_mesh_attributes(&self) {
        let stride = (MESH_VERTEX_STRIDE * std::mem::size_of::<f32>()) as i32;
        let color_offset = (POSITION_COMPONENTS * std::mem::size_of::<f32>()) as i32;
        self.gl.enable_vertex_attrib_array(self.position_location);
        self.gl.vertex_attrib_pointer_with_i32(
            self.position_location,
            POSITION_COMPONENTS as i32,
            Gl::FLOAT,
            false,
            stride,
            0,
        );

        self.gl.enable_vertex_attrib_array(self.color_location);
        self.gl.vertex_attrib_pointer_with_i32(
            self.color_location,
            COLOR_COMPONENTS as i32,
            Gl::FLOAT,
            false,
            stride,
            color_offset,
        );
    }

    fn configure_instance_attributes(&self) {
        let stride = (MATRIX_FLOATS * std::mem::size_of::<f32>()) as i32;
        for (index, &location) in self.instance_locations.iter().enumerate() {
            let offset = (index * 4 * std::mem::size_of::<f32>()) as i32;
            self.gl.enable_vertex_attrib_array(location);
            self.gl
                .vertex_attrib_pointer_with_i32(location, 4, Gl::FLOAT, false, stride, offset);
            self.gl.vertex_attrib_divisor(location, 1);
        }
    }

    fn draw_mesh_instances(&mut self, mesh_index: usize) -> Result<(), JsValue> {
        let mesh = self
            .meshes
            .get(mesh_index)
            .ok_or_else(|| error("mesh not found"))?;
        let instances = self
            .mesh_instances
            .get_mut(mesh_index)
            .ok_or_else(|| error("mesh not found"))?;
        instances.flush_pending(&self.gl);
        if instances.len() == 0 {
            return Ok(());
        }
        self.gl.bind_vertex_array(Some(&mesh.vao));
        self.gl
            .draw_arrays_instanced(Gl::TRIANGLES, 0, mesh.vertex_count, instances.len() as i32);
        Ok(())
    }

    fn remove_transient_instances(&mut self) {
        let handles: Vec<u32> = self.transient_instances.drain(..).collect();
        for handle in handles {
            let _ = self.remove_instance_internal(handle);
        }
    }

    fn remove_instance_internal(&mut self, handle: u32) -> Result<bool, JsValue> {
        let (mesh_index, slot_index) = match self.instance_store.get(handle) {
            Some(record) => (record.mesh_index, record.slot_index),
            None => return Ok(false),
        };
        let moved_handle = self.mesh_instances[mesh_index].remove_slot(slot_index)?;
        if let Some(moved) = moved_handle {
            if let Some(record) = self.instance_store.get_mut(moved) {
                record.slot_index = slot_index;
            }
            self.mesh_instances[mesh_index].set_handle(slot_index, moved);
        }
        self.instance_store.remove(handle);
        Ok(true)
    }

    fn upload_view_matrix(&self) {
        self.gl.uniform_matrix4fv_with_f32_array(
            Some(&self.view_location),
            false,
            &self.view_matrix,
        );
    }

    fn upload_projection_matrix(&self) {
        self.gl.uniform_matrix4fv_with_f32_array(
            Some(&self.projection_location),
            false,
            &self.projection_matrix,
        );
    }
}

struct GpuMesh {
    vao: WebGlVertexArrayObject,
    _vertex_buffer: WebGlBuffer,
    vertex_count: i32,
}

struct MeshInstances {
    buffer: WebGlBuffer,
    transforms: Vec<[f32; MATRIX_FLOATS]>,
    handles: Vec<u32>,
    capacity: usize,
    pending: BTreeMap<usize, [f32; MATRIX_FLOATS]>,
    scratch: Vec<f32>,
}

impl MeshInstances {
    fn new(gl: &Gl, initial_capacity: usize) -> Result<Self, JsValue> {
        let buffer = gl
            .create_buffer()
            .ok_or_else(|| error("failed to create instance buffer"))?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&buffer));
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

    fn len(&self) -> usize {
        self.transforms.len()
    }

    fn buffer(&self) -> &WebGlBuffer {
        &self.buffer
    }

    fn allocate(&mut self, gl: &Gl, matrix: &[f32; MATRIX_FLOATS]) -> Result<usize, JsValue> {
        let slot = self.transforms.len();
        self.transforms.push(*matrix);
        self.handles.push(0);
        self.ensure_capacity(gl, slot + 1)?;
        self.pending.insert(slot, *matrix);
        Ok(slot)
    }

    fn set_handle(&mut self, slot: usize, handle: u32) {
        if let Some(target) = self.handles.get_mut(slot) {
            *target = handle;
        }
    }

    fn update_slot(&mut self, slot: usize, matrix: &[f32; MATRIX_FLOATS]) -> Result<(), JsValue> {
        let target = self
            .transforms
            .get_mut(slot)
            .ok_or_else(|| error("invalid instance slot"))?;
        *target = *matrix;
        self.pending.insert(slot, *matrix);
        Ok(())
    }

    fn remove_slot(&mut self, slot: usize) -> Result<Option<u32>, JsValue> {
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

    fn ensure_capacity(&mut self, gl: &Gl, min_capacity: usize) -> Result<(), JsValue> {
        if self.capacity >= min_capacity.max(1) {
            return Ok(());
        }
        let mut new_capacity = self.capacity.max(1);
        while new_capacity < min_capacity {
            new_capacity *= 2;
        }
        self.capacity = new_capacity;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.buffer));
        gl.buffer_data_with_i32(
            Gl::ARRAY_BUFFER,
            (self.capacity * MATRIX_FLOATS * std::mem::size_of::<f32>()) as i32,
            Gl::DYNAMIC_DRAW,
        );
        self.upload_all(gl);
        Ok(())
    }

    fn upload_all(&self, gl: &Gl) {
        if self.transforms.is_empty() {
            return;
        }
        let mut flat = Vec::with_capacity(self.transforms.len() * MATRIX_FLOATS);
        for matrix in &self.transforms {
            flat.extend_from_slice(matrix);
        }
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.buffer));
        let view = unsafe { Float32Array::view(&flat) };
        gl.buffer_sub_data_with_f64_and_array_buffer_view(Gl::ARRAY_BUFFER, 0.0, &view);
    }

    fn defragment(&mut self, gl: &Gl) {
        self.capacity = self.transforms.len().max(1);
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.buffer));
        gl.buffer_data_with_i32(
            Gl::ARRAY_BUFFER,
            (self.capacity * MATRIX_FLOATS * std::mem::size_of::<f32>()) as i32,
            Gl::DYNAMIC_DRAW,
        );
        self.upload_all(gl);
        self.pending.clear();
    }

    fn flush_pending(&mut self, gl: &Gl) {
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

    fn write_chunk(&self, gl: &Gl, start_slot: usize, data: &[f32]) {
        if data.is_empty() {
            return;
        }
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.buffer));
        let offset = (start_slot * MATRIX_FLOATS * std::mem::size_of::<f32>()) as f64;
        let view = unsafe { Float32Array::view(data) };
        gl.buffer_sub_data_with_f64_and_array_buffer_view(Gl::ARRAY_BUFFER, offset, &view);
    }
}

fn identity_matrix() -> [f32; MATRIX_FLOATS] {
    let mut out = [0.0; MATRIX_FLOATS];
    out[0] = 1.0;
    out[5] = 1.0;
    out[10] = 1.0;
    out[15] = 1.0;
    out
}

fn copy_into_matrix(
    target: &mut [f32; MATRIX_FLOATS],
    source: &Float32Array,
) -> Result<(), JsValue> {
    let data = source.to_vec();
    if data.len() != MATRIX_FLOATS {
        return Err(error("matrices must contain 16 floats"));
    }
    target.copy_from_slice(&data);
    Ok(())
}

fn matrix_from_array(source: &Float32Array) -> Result<[f32; MATRIX_FLOATS], JsValue> {
    let mut matrix = [0.0; MATRIX_FLOATS];
    copy_into_matrix(&mut matrix, source)?;
    Ok(matrix)
}

fn vec3_from_array(array: &Float32Array) -> Result<[f32; 3], JsValue> {
    if array.length() != 3 {
        return Err(error("vec3 data must contain exactly 3 floats"));
    }
    let mut out = [0.0; 3];
    array.copy_to(&mut out);
    Ok(out)
}

fn clamp_unit(value: f32) -> f32 {
    value.max(0.0).min(1.0)
}

fn error(message: &str) -> JsValue {
    JsValue::from_str(message)
}

fn get_i32_parameter(gl: &Gl, param: u32) -> Result<i32, JsValue> {
    Ok(gl
        .get_parameter(param)?
        .as_f64()
        .ok_or_else(|| error("failed to query WebGL parameter"))? as i32)
}

fn compute_instance_budget(uniform_vectors: i32) -> Result<usize, JsValue> {
    let reserved_for_view_projection = 8; // two mat4 uniforms
    let available = uniform_vectors - reserved_for_view_projection;
    if available < 4 {
        return Err(error(
            "insufficient vertex uniform budget for per-instance transforms",
        ));
    }
    let max_instances = (available / 4) as usize;
    Ok(max_instances.max(1))
}
