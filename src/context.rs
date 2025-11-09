use std::rc::Rc;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext as Gl};

use crate::utils::error;

pub(crate) type SharedContext = Rc<CanvasContext>;

pub(crate) struct CanvasContext {
    canvas: HtmlCanvasElement,
    gl: Gl,
}

impl CanvasContext {
    pub(crate) fn new(canvas_id: &str) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or_else(|| error("missing window"))?;
        let document = window.document().ok_or_else(|| error("missing document"))?;
        let element = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| error("canvas not found"))?;
        let canvas = element
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| error("element is not a canvas"))?;

        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| error("webgl2 context unavailable"))?
            .dyn_into()
            .map_err(|_| error("failed to cast WebGL2 context"))?;

        let context = CanvasContext { canvas, gl };
        let width = context.canvas.width().max(1);
        let height = context.canvas.height().max(1);
        context
            .gl
            .viewport(0, 0, width as i32, height as i32);
        Ok(context)
    }

    pub(crate) fn gl_clone(&self) -> Gl {
        self.gl.clone()
    }

    pub(crate) fn resize(&self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.gl.viewport(0, 0, width as i32, height as i32);
    }

    pub(crate) fn clear(&self, color: [f32; 4], depth: Option<f32>) {
        self.gl.clear_color(color[0], color[1], color[2], color[3]);
        if let Some(depth) = depth {
            self.gl.clear_depth(depth);
            self.gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
        } else {
            self.gl.clear(Gl::COLOR_BUFFER_BIT);
        }
    }
}

pub(crate) fn shared_context(canvas_id: &str) -> Result<SharedContext, JsValue> {
    Ok(Rc::new(CanvasContext::new(canvas_id)?))
}
