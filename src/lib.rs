use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_str(s: &str);
}

#[wasm_bindgen]
pub fn test_wasm() -> JsValue {
    log_str("Hello, world!");

    JsValue::TRUE
}
