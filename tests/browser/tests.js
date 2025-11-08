const resultsList = document.getElementById("results");
const summaryLine = document.getElementById("summary");
const canvasGallery = document.getElementById("canvas-gallery");
const initialFilterKey = getInitialFilterKey();
let activeCanvasFilter = initialFilterKey;
let wasmModulePromise;
applyCanvasFilterState();

const tests = [
    {
        label: "Persistent Instances",
        slug: slugify("Persistent Instances"),
        async run() {
            await withRenderer(
                "Persistent Instances",
                async ({ renderer }) => {
                    const mesh = buildSingleTriangle();
                    const meshHandle = renderer.register_mesh(mesh);
                    const instanceHandle = renderer.create_instance(
                        meshHandle,
                        identityMatrix(),
                    );

                    renderer.flush();
                    renderer.flush();

                    const count = renderer.instance_count();
                    if (count !== 1) {
                        throw new Error(
                            `expected persistent instance count to remain 1, saw ${count}`,
                        );
                    }
                    return { keepAlive: false };
                },
            );
        },
    },
    {
        label: "Invalid Mesh Rejection",
        slug: slugify("Invalid Mesh Rejection"),
        async run() {
            await withRenderer("Invalid Mesh Rejection", async ({ renderer }) => {
                let threw = false;
                try {
                    renderer.register_mesh(new Float32Array([0.0, 1.0, 2.0]));
                } catch (err) {
                    threw = true;
                }
                if (!threw) {
                    throw new Error("register_mesh should throw for malformed data");
                }
            });
        },
    },
    {
        label: "Instance Handles",
        slug: slugify("Instance Handles"),
        async run() {
            await withRenderer("Instance Handles", async ({ renderer }) => {
                const mesh = buildSingleTriangle();
                const meshHandle = renderer.register_mesh(mesh);
                const handleA = renderer.create_instance(
                    meshHandle,
                    identityMatrix(),
                );
                const handleB = renderer.create_instance(
                    meshHandle,
                    rotationTranslationMatrix([0, 0, 0], 0),
                );

                const moved = rotationTranslationMatrix([1, 2, 3], Math.PI / 4);
                renderer.set_instance_transform(handleA, moved);
                renderer.flush();

                renderer.remove_instance(handleB);
                const remaining = renderer.instance_count();
                if (remaining !== 1) {
                    throw new Error(
                        `expected a single instance to remain, saw ${remaining}`,
                    );
                }
            });
        },
    },
    {
        label: "Dynamic Batches",
        slug: slugify("Dynamic Batches"),
        async run() {
            await withRenderer("Dynamic Batches", async ({ renderer }) => {
                const mesh = buildSingleTriangle();
                const meshHandle = renderer.register_mesh(mesh);
                const maxPerBatch = renderer.max_instances();
                const target = maxPerBatch + 3;
                const handles = [];

                for (let i = 0; i < target; i += 1) {
                    const matrix = rotationTranslationMatrix([i * 0.1, 0, 0], 0);
                    handles.push(renderer.create_instance(meshHandle, matrix));
                }

                if (renderer.instance_count() !== target) {
                    throw new Error(
                        `expected ${target} instances to be active`,
                    );
                }

                renderer.flush();
                renderer.flush();

                handles.forEach((handle) => renderer.remove_instance(handle));
                renderer.flush();

                if (renderer.instance_count() !== 0) {
                    throw new Error("all instances should be removable");
                }
            });
        },
    },
    {
        label: "Orbit Camera Controls",
        slug: slugify("Orbit Camera Controls"),
        async run() {
            await withRenderer(
                "Orbit Camera Controls",
                async (ctx) => setupOrbitCameraDemo(ctx),
            );
        },
    },
    {
        label: "Shared Canvas Modes",
        slug: slugify("Shared Canvas Modes"),
        async run() {
            await runSharedCanvasTest();
        },
    },
    {
        label: "Timeseries Graph Mode",
        slug: slugify("Timeseries Graph Mode"),
        async run() {
            await withTimeseriesRenderer("Timeseries Graph Mode", async ({ renderer, canvas }) => {
                canvas.style.width = "320px";
                canvas.style.height = "200px";
                resizeRendererForCanvas(renderer, canvas, 320, 200);

                const timestamps = buildTimeAxis(240, 0.1);
                const redSeries = mapSeries(timestamps, (t) => 55 + 18 * Math.sin(t * 0.4));
                const blueSeries = mapSeries(
                    timestamps,
                    (t) => 48 + 14 * Math.cos(t * 0.27 + 0.6),
                );
                const greenSeries = mapSeries(
                    timestamps,
                    (_, index) => 30 + ((index % 60) / 60) * 20,
                );

                const series = [
                    {
                        values: redSeries,
                        color: new Float32Array([0.95, 0.34, 0.2, 1]),
                        lineWidth: 2,
                    },
                    {
                        values: blueSeries,
                        color: new Float32Array([0.2, 0.62, 0.94, 1]),
                        lineWidth: 1.5,
                    },
                    {
                        values: greenSeries,
                        color: new Float32Array([0.32, 0.84, 0.54, 0.9]),
                    },
                ];

                renderer.set_series(timestamps, series);
                renderer.clear(0.02, 0.02, 0.05, 1);
                renderer.draw();

                if (renderer.series_count() !== series.length) {
                    throw new Error("series_count should match provided series length");
                }
                if (renderer.sample_count() !== timestamps.length) {
                    throw new Error("sample_count should match timestamp length");
                }

                const timeDomain = renderer.time_domain();
                const valueDomain = renderer.value_domain();
                if (timeDomain.length < 2 || timeDomain[1] <= timeDomain[0]) {
                    throw new Error("time_domain must be ascending");
                }
                if (valueDomain.length < 2 || valueDomain[1] <= valueDomain[0]) {
                    throw new Error("value_domain must be ascending");
                }
            });
        },
    },
];

runAllTests().catch((err) => {
    summaryLine.textContent = `Unhandled test harness error: ${err?.message ?? err}`;
    summaryLine.classList.add("fail");
    console.error(err);
});

async function runAllTests() {
    const filterKey = activeCanvasFilter;
    const testsToRun = tests.filter(
        (test) => !filterKey || test.slug === filterKey,
    );

    if (testsToRun.length === 0) {
        summaryLine.textContent = filterKey
            ? `No tests matched the filter "${filterKey}".`
            : "No tests to run.";
        summaryLine.classList.remove("pass");
        summaryLine.classList.add("fail");
        return;
    }

    let passed = 0;
    for (const test of testsToRun) {
        const li = document.createElement("li");
        li.textContent = `Running ${test.label}…`;
        resultsList.appendChild(li);
        wireResultFilterControls(li, test.slug);
        applyCanvasFilterState();
        try {
            await test.run();
            li.textContent = `✅ ${test.label}`;
            li.classList.add("pass");
            passed += 1;
        } catch (err) {
            li.textContent = `❌ ${test.label}: ${err?.message ?? err}`;
            li.classList.add("fail");
            console.error(`Test "${test.label}" failed`, err);
        }
    }

    const total = testsToRun.length;
    const summary = `${passed}/${total} tests passing${
        filterKey ? " (filtered)" : ""
    }`;
    summaryLine.textContent = summary;
    summaryLine.classList.remove("pass", "fail");
    summaryLine.classList.add(passed === total ? "pass" : "fail");
}

async function runSharedCanvasTest() {
    const { BatchedRenderer, TimeSeriesRenderer } = await loadRendererModule();
    const slot = createCanvasSlot("Shared Canvas Modes");
    const canvas = slot.canvas;
    const cssWidth = 360;
    const cssHeight = 240;
    canvas.style.width = `${cssWidth}px`;
    canvas.style.height = `${cssHeight}px`;
    let batched;
    let timeseries;
    let instHandle = null;
    try {
        batched = new BatchedRenderer(canvas.id);
        timeseries = new TimeSeriesRenderer(canvas.id);

        resizeRendererForCanvas(batched, canvas, cssWidth, cssHeight);
        resizeRendererForCanvas(timeseries, canvas, cssWidth, cssHeight);

        const meshHandle = batched.register_mesh(buildSingleTriangle());
        instHandle = batched.create_instance(
            meshHandle,
            rotationTranslationMatrix([0, 0, 0], 0),
        );

        batched.clear(0.1, 0.1, 0.12, 1);
        batched.flush();
        if (batched.instance_count() !== 1) {
            throw new Error("shared canvas batched renderer should retain its instance");
        }

        const timestamps = buildTimeAxis(120, 0.15);
        const lowSeries = mapSeries(timestamps, (t) => 42 + 10 * Math.sin(t * 0.35));
        const highSeries = mapSeries(timestamps, (t) => 58 + 6 * Math.cos(t * 0.22));
        const series = [
            {
                values: lowSeries,
                color: new Float32Array([0.93, 0.56, 0.24, 0.95]),
                lineWidth: 1.5,
            },
            {
                values: highSeries,
                color: new Float32Array([0.24, 0.68, 0.98, 0.9]),
                lineWidth: 2.0,
            },
        ];

        timeseries.set_series(timestamps, series);
        timeseries.draw();
        if (timeseries.series_count() !== series.length) {
            throw new Error("shared canvas timeseries renderer should track all series");
        }

        batched.flush();
        timeseries.draw();
        slot.wrapper.classList.add("pass");
    } catch (err) {
        slot.wrapper.classList.add("fail");
        throw err;
    } finally {
        try {
            if (batched && instHandle !== null) {
                batched.remove_instance(instHandle);
                batched.flush();
            }
        } catch (err) {
            console.warn("shared canvas cleanup (batched) failed", err);
        }
        try {
            batched?.free?.();
        } catch (err) {
            console.warn("shared canvas batched free failed", err);
        }
        try {
            timeseries?.free?.();
        } catch (err) {
            console.warn("shared canvas timeseries free failed", err);
        }
    }
}

async function withRenderer(label, fn) {
    const { BatchedRenderer } = await loadRendererModule();
    const slot = createCanvasSlot(label);
    const renderer = new BatchedRenderer(slot.canvas.id);
    let keepAlive = false;
    try {
        const result = await fn({
            renderer,
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
            try {
                renderer.flush?.();
            } catch (err) {
                console.warn("flush during teardown failed", err);
            }
            renderer.free();
        }
    }
}

async function withTimeseriesRenderer(label, fn) {
    const { TimeSeriesRenderer } = await loadRendererModule();
    const slot = createCanvasSlot(label);
    const renderer = new TimeSeriesRenderer(slot.canvas.id);
    try {
        await fn({ renderer, canvas: slot.canvas, wrapper: slot.wrapper });
        slot.wrapper.classList.add("pass");
    } catch (err) {
        slot.wrapper.classList.add("fail");
        throw err;
    } finally {
        renderer.free();
    }
}

function buildSingleTriangle() {
    return new Float32Array([
        // x,   y,   z,   r, g, b, a
        0.0, 0.0, 0.0, 1, 0, 0, 1,
        0.5, 0.0, 0.0, 0, 1, 0, 1,
        0.0, 0.5, 0.0, 0, 0, 1, 1,
    ]);
}

function identityMatrix() {
    return new Float32Array([
        1, 0, 0, 0,
        0, 1, 0, 0,
        0, 0, 1, 0,
        0, 0, 0, 1,
    ]);
}

function buildCubeMesh() {
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

function loadRendererModule() {
    if (!wasmModulePromise) {
        wasmModulePromise = import("../../pkg/render.js");
    }
    return wasmModulePromise;
}

function createCanvasSlot(label) {
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
    canvasGallery.appendChild(wrapper);
    wireCanvasFilterControls(wrapper, title, filterKey);
    applyCanvasFilterState();
    return { wrapper, canvas };
}


async function setupOrbitCameraDemo({ renderer, canvas, wrapper }) {
    const { build_orbit_view, build_perspective } = await loadRendererModule();
    canvas.style.width = "260px";
    canvas.style.height = "260px";
    resizeRendererForCanvas(renderer, canvas, 260, 260);

    const instructions = document.createElement("p");
    instructions.textContent = "Drag to orbit, scroll to zoom. View matrix is built in Rust.";
    instructions.style.fontSize = "0.8rem";
    instructions.style.marginTop = "0.5rem";
    instructions.style.color = "#374151";
    wrapper.appendChild(instructions);

    const meshHandle = renderer.register_mesh(buildCubeMesh());
    const projection = build_perspective(
        Math.PI / 3,
        canvas.width / canvas.height,
        0.1,
        100.0,
    );
    renderer.set_projection_matrix(projection);

    const target = new Float32Array([0, 0, 0]);
    let yaw = 0.8;
    let pitch = 0.6;
    let distance = 6.0;

    const sceneOffsets = [
        [-2, 0, -2],
        [0, 0, 0],
        [2, 0, 2],
        [0, 1.5, -3],
    ];
    const instanceHandles = sceneOffsets.map((offset) =>
        renderer.create_instance(meshHandle, rotationTranslationMatrix(offset, 0)),
    );

    const pointerState = {
        active: false,
        pointerId: null,
        lastX: 0,
        lastY: 0,
    };

    canvas.addEventListener("pointerdown", (event) => {
        pointerState.active = true;
        pointerState.pointerId = event.pointerId;
        pointerState.lastX = event.clientX;
        pointerState.lastY = event.clientY;
        canvas.setPointerCapture(event.pointerId);
        event.preventDefault();
    });

    canvas.addEventListener("pointermove", (event) => {
        if (!pointerState.active || event.pointerId !== pointerState.pointerId) {
            return;
        }
        const dx = (event.clientX - pointerState.lastX) * 0.01;
        const dy = (event.clientY - pointerState.lastY) * 0.01;
        yaw += dx;
        pitch = Math.max(-1.4, Math.min(1.4, pitch - dy));
        pointerState.lastX = event.clientX;
        pointerState.lastY = event.clientY;
        event.preventDefault();
    });

    const releasePointer = (event) => {
        if (pointerState.pointerId === event.pointerId) {
            pointerState.active = false;
            pointerState.pointerId = null;
            if (canvas.hasPointerCapture(event.pointerId)) {
                canvas.releasePointerCapture(event.pointerId);
            }
        }
    };
    canvas.addEventListener("pointerup", releasePointer);
    canvas.addEventListener("pointercancel", releasePointer);

    canvas.addEventListener(
        "wheel",
        (event) => {
            const delta = Math.exp(event.deltaY * 0.001);
            distance = Math.max(1.5, Math.min(20, distance * delta));
            event.preventDefault();
        },
        { passive: false },
    );

    const draw = (time) => {
        renderer.clear(0.05, 0.05, 0.1, 1);
        const view = build_orbit_view(target, yaw, pitch, distance);
        renderer.set_view_matrix(view);

        instanceHandles.forEach((handle, index) => {
            const offset = sceneOffsets[index];
            const angle = time * 0.001 + index * 0.5;
            const matrix = rotationTranslationMatrix(offset, angle);
            renderer.set_instance_transform(handle, matrix);
        });

        renderer.flush();
        requestAnimationFrame(draw);
    };
    requestAnimationFrame(draw);

    return { keepAlive: true };
}

function rotationTranslationMatrix(offset, angle) {
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

function buildTimeAxis(sampleCount, step) {
    const axis = new Float32Array(sampleCount);
    for (let i = 0; i < sampleCount; i += 1) {
        axis[i] = i * step;
    }
    return axis;
}

function mapSeries(timestamps, sampler) {
    const values = new Float32Array(timestamps.length);
    for (let i = 0; i < timestamps.length; i += 1) {
        values[i] = sampler(timestamps[i], i);
    }
    return values;
}

function resizeRendererForCanvas(renderer, canvas, cssWidth, cssHeight) {
    const dpr = window.devicePixelRatio ?? 1;
    const width = Math.round(cssWidth * dpr);
    const height = Math.round(cssHeight * dpr);
    canvas.width = width;
    canvas.height = height;
    renderer.resize(width, height);
}

function getInitialFilterKey() {
    const params = new URLSearchParams(window.location.search);
    const raw = params.get("test");
    return normalizeFilterKey(raw);
}

function normalizeFilterKey(value) {
    return value ? slugify(value) : null;
}

function slugify(label) {
    return label
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "");
}

function wireCanvasFilterControls(wrapper, title, filterKey) {
    if (!filterKey) {
        return;
    }
    title.setAttribute("role", "button");
    title.setAttribute("aria-pressed", "false");
    title.tabIndex = 0;
    const toggle = () => {
        toggleCanvasFilter(filterKey);
    };
    title.addEventListener("click", toggle);
    title.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            toggle();
        }
    });
}

function toggleCanvasFilter(filterKey) {
    setActiveCanvasFilter(activeCanvasFilter === filterKey ? null : filterKey);
}

function setActiveCanvasFilter(newKey) {
    if (activeCanvasFilter === newKey) {
        return;
    }
    activeCanvasFilter = newKey;
    const url = updateFilterQueryParam();
    applyCanvasFilterState();
    window.location.assign(url);
}

function applyCanvasFilterState() {
    const wrappers = canvasGallery.querySelectorAll(".canvas-wrapper");
    wrappers.forEach((wrapper) => {
        const matches =
            !activeCanvasFilter || wrapper.dataset.filterKey === activeCanvasFilter;
        wrapper.classList.toggle("hidden", !matches);
        wrapper.classList.toggle("focused-filter", Boolean(activeCanvasFilter && matches));
        const title = wrapper.querySelector("h3");
        if (title) {
            title.setAttribute(
                "aria-pressed",
                activeCanvasFilter && matches ? "true" : "false",
            );
        }
    });
    canvasGallery.classList.toggle("has-filter", Boolean(activeCanvasFilter));

    const resultItems = resultsList.querySelectorAll("li[data-filter-key]");
    resultItems.forEach((item) => {
        const matches = activeCanvasFilter && item.dataset.filterKey === activeCanvasFilter;
        item.classList.toggle("focused-filter", Boolean(matches));
        item.setAttribute("aria-pressed", matches ? "true" : "false");
    });
}

function wireResultFilterControls(element, filterKey) {
    if (!filterKey) {
        return;
    }
    element.dataset.filterKey = filterKey;
    element.setAttribute("role", "button");
    element.setAttribute("aria-pressed", "false");
    element.tabIndex = 0;
    const toggle = () => toggleCanvasFilter(filterKey);
    element.addEventListener("click", toggle);
    element.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            toggle();
        }
    });
}

function updateFilterQueryParam() {
    const url = new URL(window.location.href);
    if (activeCanvasFilter) {
        url.searchParams.set("test", activeCanvasFilter);
    } else {
        url.searchParams.delete("test");
    }
    window.history.replaceState(null, "", url.toString());
    return url.toString();
}
