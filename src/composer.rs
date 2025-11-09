use std::cell::RefCell;
use std::rc::{Rc, Weak};

use wasm_bindgen::prelude::*;

use crate::batched::{BatchedRenderer, BatchedRendererInner};
use crate::context::{shared_context, SharedContext};
use crate::timeseries::{TimeSeriesRenderer, TimeSeriesRendererInner};
use crate::utils::{clamp_unit, error};

#[wasm_bindgen]
pub struct CanvasComposer {
    context: SharedContext,
    passes: Vec<RenderPass>,
    clear_color: [f32; 4],
    clear_depth: f32,
}

#[wasm_bindgen]
impl CanvasComposer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<CanvasComposer, JsValue> {
        let context = shared_context(canvas_id)?;
        Ok(CanvasComposer {
            context,
            passes: Vec::new(),
            clear_color: [0.02, 0.02, 0.05, 1.0],
            clear_depth: 1.0,
        })
    }

    pub fn add_batched_pass(&mut self) -> Result<BatchedRenderer, JsValue> {
        let renderer = BatchedRenderer::with_shared_context(self.context.clone())?;
        self.passes
            .push(RenderPass::Batched(PassHandle::new(&renderer.inner())));
        Ok(renderer)
    }

    pub fn add_timeseries_pass(&mut self) -> Result<TimeSeriesRenderer, JsValue> {
        let renderer = TimeSeriesRenderer::with_shared_context(self.context.clone())?;
        self.passes
            .push(RenderPass::TimeSeries(PassHandle::new(&renderer.inner())));
        Ok(renderer)
    }

    pub fn set_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [clamp_unit(r), clamp_unit(g), clamp_unit(b), clamp_unit(a)];
    }

    pub fn set_clear_depth(&mut self, depth: f32) -> Result<(), JsValue> {
        if !depth.is_finite() {
            return Err(error("clear depth must be finite"));
        }
        self.clear_depth = depth.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.context.resize(width, height);
    }

    pub fn render(&mut self) -> Result<(), JsValue> {
        self.context.clear(self.clear_color, Some(self.clear_depth));
        for pass in &self.passes {
            pass.render()?;
        }
        self.passes.retain(|pass| pass.is_alive());
        Ok(())
    }
}

enum RenderPass {
    Batched(PassHandle<BatchedRendererInner>),
    TimeSeries(PassHandle<TimeSeriesRendererInner>),
}

impl RenderPass {
    fn render(&self) -> Result<(), JsValue> {
        match self {
            RenderPass::Batched(handle) => handle.render(|inner| inner.render_pass()),
            RenderPass::TimeSeries(handle) => handle.render(|inner| inner.render_pass()),
        }
    }

    fn is_alive(&self) -> bool {
        match self {
            RenderPass::Batched(handle) => handle.is_alive(),
            RenderPass::TimeSeries(handle) => handle.is_alive(),
        }
    }
}

struct PassHandle<T> {
    inner: Weak<RefCell<T>>,
}

impl<T> PassHandle<T> {
    fn new(inner: &Rc<RefCell<T>>) -> Self {
        Self {
            inner: Rc::downgrade(inner),
        }
    }

    fn render<F>(&self, mut f: F) -> Result<(), JsValue>
    where
        F: FnMut(&mut T) -> Result<(), JsValue>,
    {
        if let Some(inner) = self.inner.upgrade() {
            f(&mut inner.borrow_mut())
        } else {
            Ok(())
        }
    }

    fn is_alive(&self) -> bool {
        self.inner.strong_count() > 0
    }
}
