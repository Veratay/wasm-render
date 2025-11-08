use wasm_bindgen::JsValue;
use web_sys::{WebGl2RenderingContext as Gl, WebGlProgram, WebGlShader};

pub fn compile_shader(gl: &Gl, shader_type: u32, source: &str) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| JsValue::from_str("failed to create shader"))?;
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
        Err(JsValue::from_str(&message))
    }
}

pub fn link_program(
    gl: &Gl,
    vert: &WebGlShader,
    frag: &WebGlShader,
) -> Result<WebGlProgram, JsValue> {
    let program = gl
        .create_program()
        .ok_or_else(|| JsValue::from_str("failed to create program"))?;
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
        Err(JsValue::from_str(&message))
    }
}

pub fn vertex_shader_source() -> &'static str {
    VERTEX_SHADER_SOURCE
}

pub fn fragment_shader_source() -> &'static str {
    FRAGMENT_SHADER_SOURCE
}

pub fn timeseries_vertex_shader_source() -> &'static str {
    TIMESERIES_VERTEX_SHADER_SOURCE
}

pub fn timeseries_fragment_shader_source() -> &'static str {
    TIMESERIES_FRAGMENT_SHADER_SOURCE
}

const VERTEX_SHADER_SOURCE: &str = r#"
precision mediump float;
attribute vec3 a_position;
attribute vec4 a_color;
attribute vec4 a_instance_col0;
attribute vec4 a_instance_col1;
attribute vec4 a_instance_col2;
attribute vec4 a_instance_col3;
uniform mat4 u_view;
uniform mat4 u_projection;
varying vec4 v_color;

void main() {
    mat4 model = mat4(
        a_instance_col0,
        a_instance_col1,
        a_instance_col2,
        a_instance_col3
    );
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

const TIMESERIES_VERTEX_SHADER_SOURCE: &str = r#"
precision mediump float;
attribute vec2 a_position;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

const TIMESERIES_FRAGMENT_SHADER_SOURCE: &str = r#"
precision mediump float;
uniform vec4 u_color;

void main() {
    gl_FragColor = u_color;
}
"#;
