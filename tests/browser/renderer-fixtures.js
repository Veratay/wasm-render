import {
    applyCanvasFilterState,
    slugify,
    wireCanvasFilterControls,
    wireResultFilterControls,
} from "./ui/filter-controls.js";

let wasmModulePromise;
let canvasGalleryRef = null;

export function configureRendererFixtures({ canvasGallery }) {
    canvasGalleryRef = canvasGallery;
}

export function registerResultListItem(element, filterKey) {
    wireResultFilterControls(element, filterKey);
}

export async function loadRendererModule() {
    if (!wasmModulePromise) {
        wasmModulePromise = import("../../pkg/render.js");
    }
    return wasmModulePromise;
}

export function createCanvasSlot(label) {
    if (!canvasGalleryRef) {
        throw new Error("canvas gallery not configured");
    }
    const wrapper = document.createElement("div");
    wrapper.className = "canvas-wrapper";
    const title = document.createElement("h3");
    title.textContent = label;
    const canvas = document.createElement("canvas");
    canvas.width = 200;
    canvas.height = 200;
    const filterKey = slugify(label);
    canvas.id = `test-canvas-${filterKey}-${Math.floor(performance.now())}`;
    wrapper.dataset.filterKey = filterKey;
    wrapper.appendChild(title);
    wrapper.appendChild(canvas);
    canvasGalleryRef.appendChild(wrapper);
    wireCanvasFilterControls(wrapper, title, filterKey);
    applyCanvasFilterState();
    return { wrapper, canvas };
}

export async function withBatchedRenderer(label, fn) {
    const { CanvasComposer } = await loadRendererModule();
    const slot = createCanvasSlot(label);
    const composer = new CanvasComposer(slot.canvas.id);
    let renderer;
    let keepAlive = false;
    try {
        renderer = composer.add_batched_pass();
        const result = await fn({
            renderer,
            composer,
            canvas: slot.canvas,
            wrapper: slot.wrapper,
        });
        keepAlive = Boolean(result?.keepAlive);
        slot.wrapper.classList.add("pass");
    } catch (err) {
        slot.wrapper.classList.add("fail");
        throw err;
    } finally {
        if (!keepAlive) {
            safeFree(renderer);
            safeFree(composer);
        }
    }
}

export async function withTimeseriesRenderer(label, fn) {
    const { CanvasComposer } = await loadRendererModule();
    const slot = createCanvasSlot(label);
    const composer = new CanvasComposer(slot.canvas.id);
    let renderer;
    let keepAlive = false;
    let observing = false;
    try {
        renderer = composer.add_timeseries_pass();
        attachCanvasResizeObserver(slot.canvas, renderer);
        observing = true;
        const result = await fn({
            renderer,
            composer,
            canvas: slot.canvas,
            wrapper: slot.wrapper,
        });
        keepAlive = Boolean(result?.keepAlive);
        slot.wrapper.classList.add("pass");
    } catch (err) {
        slot.wrapper.classList.add("fail");
        safeFree(renderer);
        safeFree(composer);
        throw err;
    } finally {
        if (observing) {
            detachCanvasResizeObserver(slot.canvas);
        }
        if (!keepAlive) {
            safeFree(renderer);
            safeFree(composer);
        }
    }
}

function safeFree(instance) {
    try {
        instance?.free?.();
    } catch (err) {
        console.warn("renderer cleanup failed", err);
    }
}

export function buildSingleTriangle() {
    return new Float32Array([
        // x,   y,   z,   r, g, b, a
        0.0, 0.0, 0.0, 1, 0, 0, 1, //
        0.5, 0.0, 0.0, 0, 1, 0, 1, //
        0.0, 0.5, 0.0, 0, 0, 1, 1, //
    ]);
}

export function identityMatrix() {
    return new Float32Array([
        1, 0, 0, 0, //
        0, 1, 0, 0, //
        0, 0, 1, 0, //
        0, 0, 0, 1, //
    ]);
}

export function rotationTranslationMatrix(offset, angle) {
    const [x, y, z] = offset;
    const c = Math.cos(angle);
    const s = Math.sin(angle);
    return new Float32Array([
        c,
        0,
        s,
        0,
        0,
        1,
        0,
        0,
        -s,
        0,
        c,
        0,
        x,
        y,
        z,
        1,
    ]);
}

export function buildCubeMesh() {
    const positions = [
        // Front
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
        // Back
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
    ];
    const faces = [
        [0, 1, 2, 0, 2, 3], // front
        [5, 4, 7, 5, 7, 6], // back
        [4, 0, 3, 4, 3, 7], // left
        [1, 5, 6, 1, 6, 2], // right
        [3, 2, 6, 3, 6, 7], // top
        [4, 5, 1, 4, 1, 0], // bottom
    ];
    const colors = [
        [1, 0, 0, 1],
        [0, 1, 0, 1],
        [0, 0, 1, 1],
        [1, 1, 0, 1],
        [1, 0, 1, 1],
        [0, 1, 1, 1],
    ];
    const data = [];
    faces.forEach((face, faceIndex) => {
        const color = colors[faceIndex % colors.length];
        face.forEach((vertexIndex) => {
            const position = positions[vertexIndex];
            data.push(position[0], position[1], position[2], ...color);
        });
    });
    return new Float32Array(data);
}

export function buildTimeAxis(sampleCount, step) {
    const axis = new Float32Array(sampleCount);
    for (let i = 0; i < sampleCount; i += 1) {
        axis[i] = i * step;
    }
    return axis;
}

export function mapSeries(timestamps, sampler) {
    const values = new Float32Array(timestamps.length);
    for (let i = 0; i < timestamps.length; i += 1) {
        values[i] = sampler(timestamps[i], i);
    }
    return values;
}

export function resizeRendererForCanvas(target, canvas, cssWidth, cssHeight) {
    const dpr = window.devicePixelRatio ?? 1;
    const width = Math.round(cssWidth * dpr);
    const height = Math.round(cssHeight * dpr);
    canvas.width = width;
    canvas.height = height;
    target.resize(width, height);
}

export function attachCanvasResizeObserver(canvas, renderer) {
    const observer = new ResizeObserver(() => {
        const rect = canvas.getBoundingClientRect();
        resizeRendererForCanvas(renderer, canvas, rect.width, rect.height);
    });
    observer.observe(canvas);
    canvas.__resizeObserver = observer;
}

export function detachCanvasResizeObserver(canvas) {
    const observer = canvas.__resizeObserver;
    if (observer) {
        observer.disconnect();
        delete canvas.__resizeObserver;
    }
}
