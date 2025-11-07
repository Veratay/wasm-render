const resultsList = document.getElementById("results");
const summaryLine = document.getElementById("summary");
const canvasGallery = document.getElementById("canvas-gallery");
let wasmModulePromise;

const tests = [
    {
        name: "registers a mesh, queues it, and flush resets the instance count",
        async run() {
            await withRenderer("Register > Queue > Flush", async ({ renderer }) => {
                const mesh = buildSingleTriangle();
                const handle = renderer.register_mesh(mesh);

                const transform = identityMatrix();
                renderer.queue_instance(handle, transform);
                const queued = renderer.queued_instances();
                if (queued !== 1) {
                    throw new Error(`expected 1 queued instance, saw ${queued}`);
                }

                renderer.flush();
                if (renderer.queued_instances() !== 0) {
                    throw new Error("flush should clear queued instances");
                }
            });
        },
    },
    {
        name: "rejects meshes that are not stride aligned",
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
        name: "enforces the per-batch instance limit",
        async run() {
            await withRenderer("Instance Limit Enforcement", async ({ renderer }) => {
                const mesh = buildSingleTriangle();
                const handle = renderer.register_mesh(mesh);
                const transform = identityMatrix();
                const limit = renderer.max_instances();

                for (let i = 0; i < limit; i += 1) {
                    renderer.queue_instance(handle, transform);
                }

                let overflowed = false;
                try {
                    renderer.queue_instance(handle, transform);
                } catch (_err) {
                    overflowed = true;
                }

                if (!overflowed) {
                    throw new Error("queuing past max_instances() should throw");
                }
            });
        },
    },
    {
        name: "interactive orbit camera uses Rust-built matrices",
        async run() {
            await withRenderer(
                "Orbit Camera Controls",
                async (ctx) => setupOrbitCameraDemo(ctx),
            );
        },
    },
];

runAllTests().catch((err) => {
    summaryLine.textContent = `Unhandled test harness error: ${err?.message ?? err}`;
    summaryLine.classList.add("fail");
    console.error(err);
});

async function runAllTests() {
    let passed = 0;
    for (const test of tests) {
        const li = document.createElement("li");
        li.textContent = `Running ${test.name}…`;
        resultsList.appendChild(li);
        try {
            await test.run();
            li.textContent = `✅ ${test.name}`;
            li.classList.add("pass");
            passed += 1;
        } catch (err) {
            li.textContent = `❌ ${test.name}: ${err?.message ?? err}`;
            li.classList.add("fail");
            console.error(`Test "${test.name}" failed`, err);
        }
    }

    if (passed === tests.length) {
        summaryLine.textContent = `${passed}/${tests.length} tests passing`;
        summaryLine.classList.add("pass");
    } else {
        summaryLine.textContent = `${passed}/${tests.length} tests passing`;
        summaryLine.classList.add("fail");
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
    canvas.id = `test-canvas-${slugify(label)}-${Math.floor(performance.now())}`;
    wrapper.appendChild(title);
    wrapper.appendChild(canvas);
    canvasGallery.appendChild(wrapper);
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

        sceneOffsets.forEach((offset, index) => {
            const angle = time * 0.001 + index * 0.5;
            const matrix = rotationTranslationMatrix(offset, angle);
            renderer.queue_instance(meshHandle, matrix);
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

function resizeRendererForCanvas(renderer, canvas, cssWidth, cssHeight) {
    const dpr = window.devicePixelRatio ?? 1;
    const width = Math.round(cssWidth * dpr);
    const height = Math.round(cssHeight * dpr);
    canvas.width = width;
    canvas.height = height;
    renderer.resize(width, height);
}

function slugify(label) {
    return label
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "");
}
