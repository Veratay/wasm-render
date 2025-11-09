# Internal Architecture

## Canvas Surfaces & Composer
- `CanvasContext` owns the `<canvas>` element and the shared `WebGl2RenderingContext`. It centralises lookups, viewport clamping, and clear operations so every pass works against the exact same surface without re-querying the DOM.
- `CanvasComposer` is the orchestration layer that runs one render pass after another. Each pass is registered in creation order (e.g., batched geometry first, time-series overlay second). `render()` clears the surface exactly once using the configured color/depth values and then invokes every live pass. Dead passes (where the JS handle was freed) are automatically pruned.
- Passes expose their internals through `Rc<RefCell<…>>`, so the composer keeps only a weak handle. Dropping a renderer in JS is enough to make the pass disappear on the next frame.

## Render Pass Implementations

### Batched Renderer
- Lives in `batched.rs`. The wasm-facing `BatchedRenderer` is a thin wrapper around `Rc<RefCell<BatchedRendererInner>>`. This makes it easy to share the same inner state between the JS API and the composer.
- GPU resources use RAII wrappers (`gpu::GlBuffer`, `gpu::VertexArray`). When a mesh or instance buffer falls out of scope the WebGL buffer/VAO is deleted immediately, preventing leaks during long sessions.
- Instance data is split across two structures:
  * `InstanceStore` tracks logical handles, slot indices, and makes removals O(1) via a packed vector + free-list.
  * `MeshInstances` owns the per-mesh transform buffer. It lazily patches ranges via a `BTreeMap` of dirty slots and writes grouped slices with `buffer_sub_data`.
- Every frame `render_pass()` enforces the GL pipeline state (depth test, blending, divisors) so that composing multiple passes remains deterministic irrespective of who last touched the context.

### Time Series Renderer
- Implemented in `timeseries.rs` and also exposed as a pass. Just like the batched renderer it sits on top of the shared context and reconfigures GL state per draw (disables depth/cull, keeps blending on).
- `set_series` now stages CPU data and reuses existing `LineSeries` buffers when possible. Each `LineSeries` tracks its capacity; small updates call `buffer_sub_data`, while size increases trigger a full `buffer_data` reallocation. Colors/line widths are simply cached on the struct and applied every draw.
- Line width limits are queried once at construction and clamped whenever the JS caller provides a value. Colors are copied through `Float32Array::copy_to` to avoid repeated heap allocations.

## Data Handling & Utilities
- `utils.rs` centralises wasm boundary helpers such as `array_to_vec`, `matrix_from_array`, and safe fixed-length readers. All conversions now use `Float32Array::copy_to` to avoid intermediate `Vec` reallocations and to guarantee length validation.
- `identity_matrix`, `vec3_from_array`, and other small helpers live in the same module so that every pass consumes the same math utilities.

## GPU Resource Lifecycle
- `gpu.rs` contains small RAII guards for buffers and vertex arrays. They clone the `Gl` handle and call the matching delete function inside `Drop`, so forgetting to call `.free()` on the JS side won’t leak driver resources.
- Meshes store both an owned VAO and buffer, ensuring attribute wiring happens once per mesh. Instance buffers, line series buffers, and scratch allocations all respect the same pattern.

## Flow of a Frame
1. JS configures the composer (clear color/depth) and adds whichever passes it needs.
2. The application mutates per-pass state (e.g., updates transforms, switches time-series datasets).
3. `CanvasComposer::render` clears the canvas and depth buffer once, then sequentially asks each pass to draw.
4. Every pass re-binds its program, enforces its GL state, uploads pending data, and renders.
5. After rendering, the composer drops any pass whose JS handle has been freed, keeping the schedule tidy without extra bookkeeping from the caller.
