use js_sys::Float32Array;
use wasm_bindgen::prelude::*;

mod batcher;
mod batched;
mod camera;
mod composer;
mod context;
mod gpu;
mod instances;
mod mesh_instances;
mod shader;
mod timeseries;
mod utils;

pub use batched::BatchedRenderer;
pub use composer::CanvasComposer;
pub use timeseries::TimeSeriesRenderer;

#[wasm_bindgen]
pub fn test_wasm() -> JsValue {
    utils::log("WebGL2 renderer ready");
    JsValue::TRUE
}

#[wasm_bindgen]
pub fn build_perspective(
    fov_y_radians: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> Result<Float32Array, JsValue> {
    let matrix = camera::perspective_matrix(fov_y_radians, aspect, near, far).map_err(utils::error)?;
    Ok(Float32Array::from(matrix.as_slice()))
}

#[wasm_bindgen]
pub fn build_orbit_view(
    target: &Float32Array,
    yaw: f32,
    pitch: f32,
    distance: f32,
) -> Result<Float32Array, JsValue> {
    let target_vec = utils::vec3_from_array(target)?;
    let view = camera::orbit_view_matrix(target_vec, yaw, pitch, distance).map_err(utils::error)?;
    Ok(Float32Array::from(view.as_slice()))
}
