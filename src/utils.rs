use js_sys::Float32Array;
use wasm_bindgen::prelude::*;

use crate::batcher::MATRIX_FLOATS;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_str(s: &str);
}

pub(crate) fn log(message: &str) {
    log_str(message);
}

pub(crate) fn clamp_unit(value: f32) -> f32 {
    value.max(0.0).min(1.0)
}

pub(crate) fn error(message: &str) -> JsValue {
    JsValue::from_str(message)
}

pub(crate) fn identity_matrix() -> [f32; MATRIX_FLOATS] {
    let mut out = [0.0; MATRIX_FLOATS];
    out[0] = 1.0;
    out[5] = 1.0;
    out[10] = 1.0;
    out[15] = 1.0;
    out
}

pub(crate) fn matrix_from_array(source: &Float32Array) -> Result<[f32; MATRIX_FLOATS], JsValue> {
    read_fixed(source, "matrix")
}

pub(crate) fn copy_into_matrix(
    target: &mut [f32; MATRIX_FLOATS],
    source: &Float32Array,
) -> Result<(), JsValue> {
    if source.length() as usize != MATRIX_FLOATS {
        return Err(error("matrices must contain 16 floats"));
    }
    source.copy_to(target);
    Ok(())
}

pub(crate) fn vec3_from_array(array: &Float32Array) -> Result<[f32; 3], JsValue> {
    read_fixed(array, "vec3")
}

pub(crate) fn read_fixed<const N: usize>(
    source: &Float32Array,
    label: &str,
) -> Result<[f32; N], JsValue> {
    if source.length() as usize != N {
        return Err(error(&format!("{label} must contain exactly {N} floats")));
    }
    let mut out = [0.0f32; N];
    source.copy_to(&mut out);
    Ok(out)
}

pub(crate) fn array_to_vec(array: &Float32Array) -> Vec<f32> {
    let len = array.length() as usize;
    let mut out = vec![0.0; len];
    if len > 0 {
        array.copy_to(&mut out);
    }
    out
}
