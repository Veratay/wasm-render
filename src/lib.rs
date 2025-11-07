use js_sys::Float32Array;
use std::convert::TryInto;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    HtmlCanvasElement, WebGlBuffer, WebGlProgram, WebGlRenderingContext as Gl, WebGlShader,
    WebGlUniformLocation,
};

mod batcher;

use batcher::{
    BATCHED_VERTEX_STRIDE, COLOR_COMPONENTS, GeometryBatch, MATRIX_FLOATS, Mesh,
    POSITION_COMPONENTS,
};

const INSTANCE_COMPONENTS: i32 = 1;
const MAX_INSTANCE_CAP: usize = 256;
const MIN_CAMERA_DISTANCE: f32 = 0.01;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_str(s: &str);
}

#[wasm_bindgen]
pub struct BatchedRenderer {
    gl: Gl,
    program: WebGlProgram,
    vertex_buffer: WebGlBuffer,
    position_location: u32,
    color_location: u32,
    instance_location: u32,
    view_location: WebGlUniformLocation,
    projection_location: WebGlUniformLocation,
    models_location: WebGlUniformLocation,
    max_instances: usize,
    batch: GeometryBatch,
    meshes: Vec<Mesh>,
    canvas: HtmlCanvasElement,
    view_matrix: [f32; MATRIX_FLOATS],
    projection_matrix: [f32; MATRIX_FLOATS],
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
            .get_context("webgl")?
            .ok_or_else(|| error("webgl context unavailable"))?
            .dyn_into()?;

        gl.enable(Gl::DEPTH_TEST);
        gl.depth_func(Gl::LEQUAL);
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        let uniform_vectors = get_i32_parameter(&gl, Gl::MAX_VERTEX_UNIFORM_VECTORS)?;
        let max_instances = compute_instance_budget(uniform_vectors)?; // in mat4s

        let vertex_source = vertex_shader_source(max_instances);
        let vert_shader = compile_shader(&gl, Gl::VERTEX_SHADER, &vertex_source)?;
        let frag_shader = compile_shader(&gl, Gl::FRAGMENT_SHADER, FRAGMENT_SHADER_SOURCE)?;
        let program = link_program(&gl, &vert_shader, &frag_shader)?;

        let position_location = gl
            .get_attrib_location(&program, "a_position")
            .try_into()
            .map_err(|_| error("a_position attribute missing"))?;
        let color_location = gl
            .get_attrib_location(&program, "a_color")
            .try_into()
            .map_err(|_| error("a_color attribute missing"))?;
        let instance_location = gl
            .get_attrib_location(&program, "a_instance")
            .try_into()
            .map_err(|_| error("a_instance attribute missing"))?;

        let view_location = gl
            .get_uniform_location(&program, "u_view")
            .ok_or_else(|| error("u_view uniform missing"))?;
        let projection_location = gl
            .get_uniform_location(&program, "u_projection")
            .ok_or_else(|| error("u_projection uniform missing"))?;
        let models_location = gl
            .get_uniform_location(&program, "u_models")
            .ok_or_else(|| error("u_models uniform missing"))?;
        let vertex_buffer = gl
            .create_buffer()
            .ok_or_else(|| error("failed to create vertex buffer"))?;

        let view_matrix = identity_matrix();
        let projection_matrix = identity_matrix();

        let mut renderer = BatchedRenderer {
            gl,
            program,
            vertex_buffer,
            position_location,
            color_location,
            instance_location,
            view_location,
            projection_location,
            models_location,
            max_instances,
            batch: GeometryBatch::new(max_instances),
            meshes: Vec::new(),
            canvas,
            view_matrix,
            projection_matrix,
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
        self.meshes.push(mesh);
        Ok((self.meshes.len() - 1) as u32)
    }

    pub fn queue_instance(
        &mut self,
        mesh_handle: u32,
        transform: &Float32Array,
    ) -> Result<(), JsValue> {
        let mesh = self
            .meshes
            .get(mesh_handle as usize)
            .ok_or_else(|| error("invalid mesh handle"))?;
        let matrix_vec = transform.to_vec();
        if matrix_vec.len() != MATRIX_FLOATS {
            return Err(error("transform matrix must contain 16 floats"));
        }
        let matrix: [f32; MATRIX_FLOATS] = matrix_vec
            .try_into()
            .map_err(|_| error("transform matrix must contain 16 floats"))?;
        self.batch.push_instance(mesh, &matrix).map_err(error)?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), JsValue> {
        let vertex_count = self.batch.vertex_count();
        let instance_count = self.batch.instance_count();
        if vertex_count == 0 || instance_count == 0 {
            return Ok(());
        }

        self.bind_pipeline();

        let vertex_slice = self.batch.vertices();
        {
            let vertex_view = unsafe { Float32Array::view(vertex_slice) };
            self.gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &vertex_view,
                Gl::DYNAMIC_DRAW,
            );
        }

        let models = &self.batch.models()[..instance_count * MATRIX_FLOATS];
        self.gl
            .uniform_matrix4fv_with_f32_array(Some(&self.models_location), false, models);

        self.gl.draw_arrays(Gl::TRIANGLES, 0, vertex_count as i32);
        self.batch.clear();

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

    pub fn clear(&self, r: f32, g: f32, b: f32, a: f32) {
        self.gl
            .clear_color(clamp_unit(r), clamp_unit(g), clamp_unit(b), clamp_unit(a));
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
        self.max_instances as u32
    }

    pub fn queued_instances(&self) -> u32 {
        self.batch.instance_count() as u32
    }
}

#[wasm_bindgen]
pub fn test_wasm() -> JsValue {
    log_str("3D batched renderer ready");
    JsValue::TRUE
}

#[wasm_bindgen]
pub fn build_perspective(
    fov_y_radians: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> Result<Float32Array, JsValue> {
    if !fov_y_radians.is_finite() || fov_y_radians <= 0.0 {
        return Err(error("fov_y_radians must be positive"));
    }
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(error("aspect ratio must be positive"));
    }
    if !near.is_finite() || !far.is_finite() || near <= 0.0 || far <= near {
        return Err(error("near/far planes must satisfy 0 < near < far"));
    }

    let f = 1.0 / (fov_y_radians * 0.5).tan();
    let nf = 1.0 / (near - far);
    let mut out = [0.0; MATRIX_FLOATS];
    out[0] = f / aspect;
    out[5] = f;
    out[10] = (far + near) * nf;
    out[11] = -1.0;
    out[14] = 2.0 * far * near * nf;
    Ok(Float32Array::from(out.as_slice()))
}

#[wasm_bindgen]
pub fn build_orbit_view(
    target: &Float32Array,
    yaw: f32,
    pitch: f32,
    distance: f32,
) -> Result<Float32Array, JsValue> {
    let target_vec = vec3_from_array(target)?;
    let distance = distance.max(MIN_CAMERA_DISTANCE);
    let clamped_pitch = pitch.clamp(-1.553343, 1.553343); // ~ +/-89 degrees
    let cos_pitch = clamped_pitch.cos();
    let eye = [
        target_vec[0] + distance * cos_pitch * yaw.cos(),
        target_vec[1] + distance * clamped_pitch.sin(),
        target_vec[2] + distance * cos_pitch * yaw.sin(),
    ];
    let up = [0.0, 1.0, 0.0];
    let view = look_at_matrix(eye, [target_vec[0], target_vec[1], target_vec[2]], up)?;
    Ok(Float32Array::from(view.as_slice()))
}

fn compile_shader(gl: &Gl, shader_type: u32, source: &str) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| error("failed to create shader"))?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    let success = gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false);

    if success {
        Ok(shader)
    } else {
        let message = gl
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "unknown shader error".into());
        Err(error(&message))
    }
}

fn link_program(gl: &Gl, vert: &WebGlShader, frag: &WebGlShader) -> Result<WebGlProgram, JsValue> {
    let program = gl
        .create_program()
        .ok_or_else(|| error("failed to create program"))?;
    gl.attach_shader(&program, vert);
    gl.attach_shader(&program, frag);
    gl.link_program(&program);

    let success = gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);

    if success {
        Ok(program)
    } else {
        let message = gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "unknown program error".into());
        Err(error(&message))
    }
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
    Ok(max_instances.clamp(1, MAX_INSTANCE_CAP))
}

fn vertex_shader_source(max_instances: usize) -> String {
    VERTEX_SHADER_TEMPLATE.replace("{MAX_INSTANCE_COUNT}", &max_instances.to_string())
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

fn error(message: &str) -> JsValue {
    JsValue::from_str(message)
}

fn clamp_unit(value: f32) -> f32 {
    value.max(0.0).min(1.0)
}

fn vec3_from_array(array: &Float32Array) -> Result<[f32; 3], JsValue> {
    if array.length() != 3 {
        return Err(error("vec3 data must contain exactly 3 floats"));
    }
    let mut out = [0.0; 3];
    array.copy_to(&mut out);
    Ok(out)
}

fn look_at_matrix(
    eye: [f32; 3],
    target: [f32; 3],
    up: [f32; 3],
) -> Result<[f32; MATRIX_FLOATS], JsValue> {
    let forward = normalize(sub(target, eye))?;
    let right = normalize(cross(forward, up))?;
    let true_up = cross(right, forward);

    let mut out = [0.0; MATRIX_FLOATS];
    out[0] = right[0];
    out[1] = true_up[0];
    out[2] = -forward[0];
    out[4] = right[1];
    out[5] = true_up[1];
    out[6] = -forward[1];
    out[8] = right[2];
    out[9] = true_up[2];
    out[10] = -forward[2];
    out[15] = 1.0;

    out[12] = -dot(right, eye);
    out[13] = -dot(true_up, eye);
    out[14] = dot(forward, eye);
    Ok(out)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> Result<[f32; 3], JsValue> {
    let len_sq = dot(v, v);
    if len_sq <= f32::EPSILON {
        return Err(error("vector length must be > 0"));
    }
    let inv_len = len_sq.sqrt().recip();
    Ok([v[0] * inv_len, v[1] * inv_len, v[2] * inv_len])
}

impl BatchedRenderer {
    fn bind_pipeline(&self) {
        self.gl.use_program(Some(&self.program));
        self.gl
            .bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vertex_buffer));
        self.configure_attributes();
    }

    fn configure_attributes(&self) {
        let stride = (BATCHED_VERTEX_STRIDE * std::mem::size_of::<f32>()) as i32;
        let color_offset = (POSITION_COMPONENTS * std::mem::size_of::<f32>()) as i32;
        let instance_offset =
            ((POSITION_COMPONENTS + COLOR_COMPONENTS) * std::mem::size_of::<f32>()) as i32;

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

        self.gl.enable_vertex_attrib_array(self.instance_location);
        self.gl.vertex_attrib_pointer_with_i32(
            self.instance_location,
            INSTANCE_COMPONENTS,
            Gl::FLOAT,
            false,
            stride,
            instance_offset,
        );
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

const VERTEX_SHADER_TEMPLATE: &str = r#"
precision mediump float;
attribute vec3 a_position;
attribute vec4 a_color;
attribute float a_instance;
uniform mat4 u_view;
uniform mat4 u_projection;
const int MAX_INSTANCE_COUNT = {MAX_INSTANCE_COUNT};
uniform mat4 u_models[MAX_INSTANCE_COUNT];
varying vec4 v_color;

void main() {
    int idx = int(a_instance);
    mat4 model = u_models[idx];
    gl_Position = u_projection * u_view * model * vec4(a_position, 1.0);
    v_color = a_color;
}
"#;

const FRAGMENT_SHADER_SOURCE: &str = r#"
precision mediump float;
varying vec4 v_color;

void main() {
    gl_FragColor = v_color;
}
"#;
