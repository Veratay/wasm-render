# JavaScript Usage Guide

## Creating a Composer
```js
import { CanvasComposer } from "./pkg/render.js";

const composer = new CanvasComposer("rootCanvas");
composer.set_clear_color(0.02, 0.02, 0.05, 1);
composer.set_clear_depth(1.0);
```
- The constructor looks up the canvas by id, creates/initialises a WebGL2 context, and configures shared state. All renderers created through this composer automatically target the same surface.

## Adding Passes
```js
const batched = composer.add_batched_pass();
const timeseries = composer.add_timeseries_pass(); // optional overlay
```
- Passes are executed in creation order every time `composer.render()` runs.
- Each pass exposes its domain-specific API (meshes/instances vs. chart series) directly on the returned object.

## Working with the Batched Renderer
```js
const meshHandle = batched.register_mesh(vertexFloat32Array);
const instance = batched.create_instance(meshHandle, modelMatrix);

batched.set_view_matrix(viewMatrix);
batched.set_projection_matrix(projectionMatrix);
```
- Mesh data is provided as a packed Float32Array `(x, y, z, r, g, b, a)` per vertex.
- Instances are addressed via the returned handle, letting you update or remove them later.
- You can still call `batched.flush()` for one-off draws, but prefer `composer.render()` when coordinating with other passes.

## Working with the Time Series Renderer
```js
timeseries.set_series(timestamps, [
  { values: seriesA, color: new Float32Array([1, 0, 0, 1]), lineWidth: 1.5 },
  { values: seriesB, color: new Float32Array([0, 0.6, 1, 0.9]) },
]);

console.log(timeseries.series_count(), timeseries.sample_count());
```
- Timestamps and every series use `Float32Array` to minimise copies across the wasm boundary.
- Repeated calls to `set_series` reuse GPU buffers whenever the series count stays constant, so incremental updates remain cheap.

## Rendering & Clearing
- Call `composer.set_clear_color` / `set_clear_depth` to define the next frame’s clear values.
- Invoke `composer.render()` once per frame; it clears the surface exactly once and then runs every registered pass.
- If you need to resize, call `composer.resize(width, height)` (values should already be multiplied by `devicePixelRatio` if you want a sharp canvas).

## Mixed Pipelines
- Need a batched background with an analytical overlay? Create both passes on the same composer:
  ```js
  const batched = composer.add_batched_pass();
  const lines = composer.add_timeseries_pass();
  // configure each independently, then:
  composer.render();
  ```
- Because every pass re-binds its GL state, order is deterministic and there is no shared-state leakage between them.

## Cleanup
- Drop a pass by calling `.free()` on the corresponding renderer. The composer holds only a weak reference, so the pass disappears automatically on the next `render()`.
- Calling `.free()` on the composer releases the shared context and deletes any remaining GPU resources.

## Legacy API Notes
- `BatchedRenderer::clear` / `TimeSeriesRenderer::clear` still work for standalone usage, but when you rely on multiple passes prefer the composer’s clear functions so the frame isn’t wiped mid-pipeline.
- The old `flush()` methods now simply call the pass’ render routine. They’re handy for unit tests, but the canonical flow is “mutate state → `composer.render()`”.
