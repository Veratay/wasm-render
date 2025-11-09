use js_sys::{Array, Float32Array, Object, Reflect};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{WebGl2RenderingContext as Gl, WebGlProgram, WebGlUniformLocation};

use crate::context::{shared_context, SharedContext};
use crate::gpu::GlBuffer;
use crate::shader::{
    compile_shader, link_program, timeseries_fragment_shader_source,
    timeseries_vertex_shader_source,
};
use crate::utils::{array_to_vec, clamp_unit, error};

#[wasm_bindgen]
pub struct TimeSeriesRenderer {
    inner: Rc<RefCell<TimeSeriesRendererInner>>,
}

#[wasm_bindgen]
impl TimeSeriesRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<TimeSeriesRenderer, JsValue> {
        let context = shared_context(canvas_id)?;
        TimeSeriesRenderer::with_shared_context(context)
    }

    pub fn resize(&self, width: u32, height: u32) {
        let context = self.context_handle();
        context.resize(width, height);
    }

    pub fn clear(&self, r: f32, g: f32, b: f32, a: f32) {
        let color = [clamp_unit(r), clamp_unit(g), clamp_unit(b), clamp_unit(a)];
        let context = self.context_handle();
        context.clear(color, None);
    }

    pub fn set_series(&self, timestamps: &Float32Array, series: &Array) -> Result<(), JsValue> {
        self.inner.borrow_mut().set_series(timestamps, series)
    }

    pub fn draw(&self) -> Result<(), JsValue> {
        self.inner.borrow_mut().render_pass()
    }

    pub fn series_count(&self) -> u32 {
        self.inner.borrow().series_count()
    }

    pub fn sample_count(&self) -> u32 {
        self.inner.borrow().sample_count()
    }

    pub fn time_domain(&self) -> Float32Array {
        Float32Array::from(self.inner.borrow().time_range.as_slice())
    }

    pub fn value_domain(&self) -> Float32Array {
        Float32Array::from(self.inner.borrow().value_range.as_slice())
    }
}

impl TimeSeriesRenderer {
    pub(crate) fn with_shared_context(context: SharedContext) -> Result<Self, JsValue> {
        let inner = TimeSeriesRendererInner::new(context)?;
        Ok(TimeSeriesRenderer {
            inner: Rc::new(RefCell::new(inner)),
        })
    }

    pub(crate) fn inner(&self) -> Rc<RefCell<TimeSeriesRendererInner>> {
        self.inner.clone()
    }

    fn context_handle(&self) -> SharedContext {
        self.inner.borrow().context.clone()
    }
}

pub(crate) struct TimeSeriesRendererInner {
    pub(crate) context: SharedContext,
    gl: Gl,
    program: WebGlProgram,
    position_location: u32,
    color_location: WebGlUniformLocation,
    lines: Vec<LineSeries>,
    time_range: [f32; 2],
    value_range: [f32; 2],
    sample_count: u32,
    line_width_limits: [f32; 2],
}

impl TimeSeriesRendererInner {
    fn new(context: SharedContext) -> Result<Self, JsValue> {
        let gl = context.gl_clone();
        gl.disable(Gl::DEPTH_TEST);
        gl.disable(Gl::CULL_FACE);
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        let vert_shader =
            compile_shader(&gl, Gl::VERTEX_SHADER, timeseries_vertex_shader_source())?;
        let frag_shader =
            compile_shader(&gl, Gl::FRAGMENT_SHADER, timeseries_fragment_shader_source())?;
        let program = link_program(&gl, &vert_shader, &frag_shader)?;

        let position_location = gl
            .get_attrib_location(&program, "a_position")
            .try_into()
            .map_err(|_| error("a_position attribute missing"))?;
        let color_location = gl
            .get_uniform_location(&program, "u_color")
            .ok_or_else(|| error("u_color uniform missing"))?;
        let line_width_limits = query_line_width_limits(&gl);

        Ok(TimeSeriesRendererInner {
            context,
            gl,
            program,
            position_location,
            color_location,
            lines: Vec::new(),
            time_range: [0.0, 0.0],
            value_range: [0.0, 0.0],
            sample_count: 0,
            line_width_limits,
        })
    }

    pub(crate) fn render_pass(&mut self) -> Result<(), JsValue> {
        self.gl.use_program(Some(&self.program));
        self.gl.disable(Gl::DEPTH_TEST);
        self.gl.disable(Gl::CULL_FACE);
        self.gl.enable(Gl::BLEND);
        self.gl
            .blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        self.gl.enable_vertex_attrib_array(self.position_location);
        for line in &self.lines {
            line.draw(&self.gl, self.position_location, &self.color_location);
        }
        self.gl
            .disable_vertex_attrib_array(self.position_location);
        Ok(())
    }

    fn set_series(&mut self, timestamps: &Float32Array, series: &Array) -> Result<(), JsValue> {
        let samples = array_to_vec(timestamps);
        let sample_count = samples.len();
        if sample_count == 0 {
            if series.length() != 0 {
                return Err(error("series cannot be provided without timestamps"));
            }
            self.lines.clear();
            self.sample_count = 0;
            self.time_range = [0.0, 0.0];
            self.value_range = [0.0, 0.0];
            return Ok(());
        }

        let (time_min, time_max) = compute_range("timestamp", &samples)?;
        let (staged_lines, value_min, value_max) =
            stage_series(series, sample_count, self.line_width_limits)?;

        let mut active = 0usize;
        for staged in staged_lines {
            let positions = build_positions(
                &samples,
                &staged.values,
                time_min,
                time_max,
                value_min,
                value_max,
            );
            if let Some(existing) = self.lines.get_mut(active) {
                existing.update(&self.gl, &positions, staged.color, staged.line_width)?;
            } else {
                self.lines.push(LineSeries::from_positions(
                    &self.gl,
                    &positions,
                    staged.color,
                    staged.line_width,
                )?);
            }
            active += 1;
        }
        self.lines.truncate(active);

        self.sample_count = sample_count as u32;
        self.time_range = [time_min, time_max];
        self.value_range = [value_min, value_max];
        Ok(())
    }

    fn series_count(&self) -> u32 {
        self.lines.len() as u32
    }

    fn sample_count(&self) -> u32 {
        self.sample_count
    }
}

struct LineSeries {
    buffer: GlBuffer,
    point_count: i32,
    capacity: usize,
    color: [f32; 4],
    line_width: f32,
}

impl LineSeries {
    fn from_positions(
        gl: &Gl,
        positions: &[f32],
        color: [f32; 4],
        line_width: f32,
    ) -> Result<Self, JsValue> {
        let buffer = GlBuffer::new(gl)?;
        buffer.bind_array_buffer();
        let view = unsafe { Float32Array::view(positions) };
        gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &view, Gl::STATIC_DRAW);
        Ok(Self {
            buffer,
            point_count: (positions.len() / 2) as i32,
            capacity: positions.len(),
            color,
            line_width,
        })
    }

    fn update(
        &mut self,
        gl: &Gl,
        positions: &[f32],
        color: [f32; 4],
        line_width: f32,
    ) -> Result<(), JsValue> {
        self.point_count = (positions.len() / 2) as i32;
        self.buffer.bind_array_buffer();
        let view = unsafe { Float32Array::view(positions) };
        if positions.len() > self.capacity {
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &view, Gl::STATIC_DRAW);
            self.capacity = positions.len();
        } else {
            gl.buffer_sub_data_with_f64_and_array_buffer_view(Gl::ARRAY_BUFFER, 0.0, &view);
        }
        self.color = color;
        self.line_width = line_width;
        Ok(())
    }

    fn draw(&self, gl: &Gl, position_location: u32, color_location: &WebGlUniformLocation) {
        if self.point_count <= 0 {
            return;
        }
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(self.buffer.handle()));
        gl.vertex_attrib_pointer_with_i32(position_location, 2, Gl::FLOAT, false, 0, 0);
        gl.uniform4fv_with_f32_array(Some(color_location), &self.color);
        gl.line_width(self.line_width);
        gl.draw_arrays(Gl::LINE_STRIP, 0, self.point_count);
    }
}

struct SeriesStage {
    values: Vec<f32>,
    color: [f32; 4],
    line_width: f32,
}

fn stage_series(
    series: &Array,
    sample_count: usize,
    width_limits: [f32; 2],
) -> Result<(Vec<SeriesStage>, f32, f32), JsValue> {
    if series.length() == 0 {
        return Ok((Vec::new(), 0.0, 0.0));
    }

    let mut staged = Vec::with_capacity(series.length() as usize);
    let mut value_min = f32::INFINITY;
    let mut value_max = f32::NEG_INFINITY;

    for (index, entry) in series.iter().enumerate() {
        let object = entry
            .dyn_into::<Object>()
            .map_err(|_| error(&format!("series[{index}] must be an object")))?;

        let values_value = Reflect::get(&object, &JsValue::from_str("values"))
            .map_err(|_| error(&format!("series[{index}] missing values property")))?;
        let values_array = values_value
            .dyn_into::<Float32Array>()
            .map_err(|_| error(&format!("series[{index}].values must be Float32Array")))?;

        if values_array.length() as usize != sample_count {
            return Err(error(&format!(
                "series[{index}].values must match timestamp length"
            )));
        }
        let mut values = vec![0.0; sample_count];
        values_array.copy_to(&mut values);
        for value in &values {
            if !value.is_finite() {
                return Err(error("series values must be finite floats"));
            }
            value_min = value_min.min(*value);
            value_max = value_max.max(*value);
        }

        let color = extract_color(&object, index)?;
        let line_width = extract_line_width(&object, width_limits);

        staged.push(SeriesStage {
            values,
            color,
            line_width,
        });
    }

    if !value_min.is_finite() || !value_max.is_finite() {
        return Err(error(
            "series values must contain at least one finite sample",
        ));
    }

    if (value_max - value_min).abs() <= f32::EPSILON {
        let center = value_min;
        value_min = center - 0.5;
        value_max = center + 0.5;
    }

    Ok((staged, value_min, value_max))
}

fn extract_color(object: &Object, index: usize) -> Result<[f32; 4], JsValue> {
    let color_value = Reflect::get(object, &JsValue::from_str("color"))
        .map_err(|_| error(&format!("series[{index}] missing color property")))?;
    let color_array = color_value
        .dyn_into::<Float32Array>()
        .map_err(|_| error(&format!("series[{index}].color must be Float32Array")))?;
    if color_array.length() < 3 {
        return Err(error(&format!(
            "series[{index}].color requires at least three components"
        )));
    }
    let mut color = [0.0; 4];
    let mut buffer = vec![0.0; color_array.length() as usize];
    color_array.copy_to(&mut buffer);
    for i in 0..buffer.len().min(4) {
        color[i] = clamp_unit(buffer[i]);
    }
    if buffer.len() < 4 {
        color[3] = 1.0;
    }
    Ok(color)
}

fn extract_line_width(object: &Object, limits: [f32; 2]) -> f32 {
    let width_value =
        Reflect::get(object, &JsValue::from_str("lineWidth")).unwrap_or(JsValue::UNDEFINED);
    let requested = width_value
        .as_f64()
        .map(|v| v as f32)
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0);
    let min = limits[0];
    let max = limits[1].max(min);
    requested.clamp(min, max)
}

fn build_positions(
    timestamps: &[f32],
    values: &[f32],
    time_min: f32,
    time_max: f32,
    value_min: f32,
    value_max: f32,
) -> Vec<f32> {
    let mut out = Vec::with_capacity(values.len() * 2);
    let time_span = (time_max - time_min).abs().max(f32::EPSILON);
    let value_span = (value_max - value_min).abs().max(f32::EPSILON);
    for (index, value) in values.iter().enumerate() {
        let t = timestamps[index];
        let x = ((t - time_min) / time_span) * 2.0 - 1.0;
        let y = ((value - value_min) / value_span) * 2.0 - 1.0;
        out.push(x);
        out.push(y);
    }
    out
}

fn compute_range(label: &str, samples: &[f32]) -> Result<(f32, f32), JsValue> {
    let mut min_value = f32::INFINITY;
    let mut max_value = f32::NEG_INFINITY;
    for value in samples {
        if !value.is_finite() {
            return Err(error(&format!("{label}s must be finite floats")));
        }
        min_value = min_value.min(*value);
        max_value = max_value.max(*value);
    }

    if !min_value.is_finite() || !max_value.is_finite() {
        return Err(error(&format!(
            "{label}s must contain at least one finite value"
        )));
    }

    if (max_value - min_value).abs() <= f32::EPSILON {
        let center = min_value;
        min_value = center - 0.5;
        max_value = center + 0.5;
    }
    Ok((min_value, max_value))
}

fn query_line_width_limits(gl: &Gl) -> [f32; 2] {
    let raw = gl.get_parameter(Gl::ALIASED_LINE_WIDTH_RANGE);
    if let Ok(value) = raw {
        let array = Array::from(&value);
        let min = array
            .get(0)
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0);
        let max = array
            .get(1)
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| v.is_finite() && *v >= min)
            .unwrap_or(min);
        return [min, max.max(min)];
    }
    [1.0, 1.0]
}
