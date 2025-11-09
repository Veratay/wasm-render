use js_sys::Float32Array;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::{WebGl2RenderingContext as Gl, WebGlProgram, WebGlUniformLocation};

use crate::batcher::{
    Mesh, COLOR_COMPONENTS, MATRIX_FLOATS, MESH_VERTEX_STRIDE, POSITION_COMPONENTS,
};
use crate::context::{shared_context, SharedContext};
use crate::gpu::{GlBuffer, VertexArray};
use crate::instances::InstanceStore;
use crate::mesh_instances::MeshInstances;
use crate::shader::{
    compile_shader, fragment_shader_source, link_program, vertex_shader_source,
};
use crate::utils::{
    array_to_vec, clamp_unit, copy_into_matrix, error, identity_matrix, matrix_from_array,
};

#[wasm_bindgen]
pub struct BatchedRenderer {
    inner: Rc<RefCell<BatchedRendererInner>>,
}

#[wasm_bindgen]
impl BatchedRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<BatchedRenderer, JsValue> {
        let context = shared_context(canvas_id)?;
        BatchedRenderer::with_shared_context(context)
    }

    pub fn register_mesh(&self, vertices: &Float32Array) -> Result<u32, JsValue> {
        self.inner.borrow_mut().register_mesh(vertices)
    }

    pub fn create_instance(
        &self,
        mesh_handle: u32,
        transform: &Float32Array,
    ) -> Result<u32, JsValue> {
        self.inner.borrow_mut().create_instance(mesh_handle, transform)
    }

    pub fn set_instance_transform(
        &self,
        instance_handle: u32,
        transform: &Float32Array,
    ) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .set_instance_transform(instance_handle, transform)
    }

    pub fn remove_instance(&self, instance_handle: u32) -> Result<(), JsValue> {
        self.inner.borrow_mut().remove_instance(instance_handle)
    }

    pub fn queue_instance(
        &self,
        mesh_handle: u32,
        transform: &Float32Array,
    ) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .queue_instance(mesh_handle, transform)
    }

    pub fn flush(&self) -> Result<(), JsValue> {
        self.inner.borrow_mut().render_pass()
    }

    pub fn set_view_matrix(&self, matrix: &Float32Array) -> Result<(), JsValue> {
        self.inner.borrow_mut().set_view_matrix(matrix)
    }

    pub fn set_projection_matrix(&self, matrix: &Float32Array) -> Result<(), JsValue> {
        self.inner.borrow_mut().set_projection_matrix(matrix)
    }

    pub fn clear(&self, r: f32, g: f32, b: f32, a: f32) {
        let color = [clamp_unit(r), clamp_unit(g), clamp_unit(b), clamp_unit(a)];
        let context = self.context_handle();
        context.clear(color, Some(1.0));
    }

    pub fn resize(&self, width: u32, height: u32) {
        let context = self.context_handle();
        context.resize(width, height);
    }

    pub fn max_instances(&self) -> u32 {
        self.inner.borrow().max_instances()
    }

    pub fn instance_count(&self) -> u32 {
        self.inner.borrow().instance_count()
    }

    pub fn queued_instances(&self) -> u32 {
        self.inner.borrow().queued_instances()
    }

    pub fn defragment_instances(&self) {
        self.inner.borrow_mut().defragment_instances();
    }
}

impl BatchedRenderer {
    pub(crate) fn with_shared_context(context: SharedContext) -> Result<Self, JsValue> {
        let inner = BatchedRendererInner::new(context)?;
        Ok(BatchedRenderer {
            inner: Rc::new(RefCell::new(inner)),
        })
    }

    pub(crate) fn inner(&self) -> Rc<RefCell<BatchedRendererInner>> {
        self.inner.clone()
    }

    fn context_handle(&self) -> SharedContext {
        self.inner.borrow().context.clone()
    }
}

pub(crate) struct BatchedRendererInner {
    pub(crate) context: SharedContext,
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
    view_matrix: [f32; MATRIX_FLOATS],
    projection_matrix: [f32; MATRIX_FLOATS],
    max_instances_per_draw: usize,
}

impl BatchedRendererInner {
    fn new(context: SharedContext) -> Result<Self, JsValue> {
        let gl = context.gl_clone();
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

        let renderer = BatchedRendererInner {
            context,
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
            view_matrix: identity_matrix(),
            projection_matrix: identity_matrix(),
            max_instances_per_draw,
        };

        renderer.gl.use_program(Some(&renderer.program));
        renderer.upload_view_matrix();
        renderer.upload_projection_matrix();

        Ok(renderer)
    }

    pub(crate) fn render_pass(&mut self) -> Result<(), JsValue> {
        if self.instance_store.is_empty() {
            self.transient_instances.clear();
            return Ok(());
        }

        self.prepare_pipeline();

        for mesh_index in 0..self.mesh_instances.len() {
            self.draw_mesh_instances(mesh_index)?;
        }

        self.remove_transient_instances();
        Ok(())
    }

    fn prepare_pipeline(&self) {
        self.gl.use_program(Some(&self.program));
        self.gl.enable(Gl::DEPTH_TEST);
        self.gl.depth_func(Gl::LEQUAL);
        self.gl.enable(Gl::CULL_FACE);
        self.gl.enable(Gl::BLEND);
        self.gl
            .blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);
        self.bind_globals();
    }

    pub(crate) fn register_mesh(&mut self, vertices: &Float32Array) -> Result<u32, JsValue> {
        let data = array_to_vec(vertices);
        let mesh = Mesh::new(data).map_err(error)?;
        let vertex_count = (mesh.raw().len() / MESH_VERTEX_STRIDE) as i32;
        if vertex_count <= 0 {
            return Err(error("mesh requires at least one triangle"));
        }

        let vao = VertexArray::new(&self.gl)?;
        let vertex_buffer = GlBuffer::new(&self.gl)?;
        let mesh_instances = MeshInstances::new(&self.gl, INITIAL_INSTANCE_HINT)?;

        self.gl.bind_vertex_array(Some(vao.handle()));
        vertex_buffer.bind_array_buffer();
        let vertex_view = unsafe { Float32Array::view(mesh.raw()) };
        self.gl
            .buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &vertex_view, Gl::STATIC_DRAW);
        self.configure_mesh_attributes();

        self.gl.bind_buffer(
            Gl::ARRAY_BUFFER,
            Some(mesh_instances.buffer_handle().handle()),
        );
        self.configure_instance_attributes();
        self.gl.bind_vertex_array(None);

        self.meshes
            .push(GpuMesh { vao, _vertex_buffer: vertex_buffer, vertex_count });
        self.mesh_instances.push(mesh_instances);
        Ok((self.meshes.len() - 1) as u32)
    }

    pub(crate) fn create_instance(
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

    pub(crate) fn set_instance_transform(
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

    pub(crate) fn remove_instance(&mut self, instance_handle: u32) -> Result<(), JsValue> {
        if self.remove_instance_internal(instance_handle)? {
            self.transient_instances
                .retain(|handle| *handle != instance_handle);
            Ok(())
        } else {
            Err(error("invalid instance handle"))
        }
    }

    pub(crate) fn queue_instance(
        &mut self,
        mesh_handle: u32,
        transform: &Float32Array,
    ) -> Result<(), JsValue> {
        let handle = self.create_instance(mesh_handle, transform)?;
        self.transient_instances.push(handle);
        Ok(())
    }

    pub(crate) fn set_view_matrix(&mut self, matrix: &Float32Array) -> Result<(), JsValue> {
        copy_into_matrix(&mut self.view_matrix, matrix)?;
        self.gl.use_program(Some(&self.program));
        self.upload_view_matrix();
        Ok(())
    }

    pub(crate) fn set_projection_matrix(&mut self, matrix: &Float32Array) -> Result<(), JsValue> {
        copy_into_matrix(&mut self.projection_matrix, matrix)?;
        self.gl.use_program(Some(&self.program));
        self.upload_projection_matrix();
        Ok(())
    }

    pub(crate) fn max_instances(&self) -> u32 {
        self.max_instances_per_draw as u32
    }

    pub(crate) fn instance_count(&self) -> u32 {
        self.instance_store.len() as u32
    }

    pub(crate) fn queued_instances(&self) -> u32 {
        self.transient_instances.len() as u32
    }

    pub(crate) fn defragment_instances(&mut self) {
        for instances in &mut self.mesh_instances {
            instances.flush_pending(&self.gl);
            instances.defragment(&self.gl);
        }
    }

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
        self.gl.bind_vertex_array(Some(mesh.vao.handle()));
        self.gl.draw_arrays_instanced(
            Gl::TRIANGLES,
            0,
            mesh.vertex_count,
            instances.len() as i32,
        );
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
    vao: VertexArray,
    _vertex_buffer: GlBuffer,
    vertex_count: i32,
}

const INITIAL_INSTANCE_HINT: usize = 256;

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
