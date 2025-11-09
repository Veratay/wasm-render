import {
    withBatchedRenderer,
    withTimeseriesRenderer,
    buildSingleTriangle,
    identityMatrix,
    rotationTranslationMatrix,
    buildTimeAxis,
    mapSeries,
    resizeRendererForCanvas,
    configureRendererFixtures,
    createCanvasSlot,
    loadRendererModule,
    registerResultListItem,
} from "./renderer-fixtures.js";
import {
    initFilterControls,
    normalizeFilterKey,
    slugify,
    getActiveCanvasFilter,
} from "./ui/filter-controls.js";
import { setupOrbitCameraDemo } from "./scenes/orbit-demo.js";

const resultsList = document.getElementById("results");
const summaryLine = document.getElementById("summary");
const canvasGallery = document.getElementById("canvas-gallery");

initFilterControls({
    results: resultsList,
    gallery: canvasGallery,
    initialKey: getInitialFilterKey(),
});
configureRendererFixtures({ canvasGallery, resultsList });

const tests = [
    {
        label: "Persistent Instances",
        slug: slugify("Persistent Instances"),
        async run() {
            await withBatchedRenderer("Persistent Instances", async ({ renderer, composer }) => {
                const mesh = buildSingleTriangle();
                const meshHandle = renderer.register_mesh(mesh);
                renderer.create_instance(meshHandle, identityMatrix());

                composer.render();
                composer.render();

                const count = renderer.instance_count();
                if (count !== 1) {
                    throw new Error(
                        `expected persistent instance count to remain 1, saw ${count}`,
                    );
                }
            });
        },
    },
    {
        label: "Invalid Mesh Rejection",
        slug: slugify("Invalid Mesh Rejection"),
        async run() {
            await withBatchedRenderer("Invalid Mesh Rejection", async ({ renderer }) => {
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
            await withBatchedRenderer("Instance Handles", async ({ renderer, composer }) => {
                const mesh = buildSingleTriangle();
                const meshHandle = renderer.register_mesh(mesh);
                const handleA = renderer.create_instance(meshHandle, identityMatrix());
                const handleB = renderer.create_instance(
                    meshHandle,
                    rotationTranslationMatrix([0, 0, 0], 0),
                );

                const moved = rotationTranslationMatrix([1, 2, 3], Math.PI / 4);
                renderer.set_instance_transform(handleA, moved);
                composer.render();

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
            await withBatchedRenderer("Dynamic Batches", async ({ renderer, composer }) => {
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
                    throw new Error(`expected ${target} instances to be active`);
                }

                composer.render();
                composer.render();

                handles.forEach((handle) => renderer.remove_instance(handle));
                composer.render();

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
            await withBatchedRenderer("Orbit Camera Controls", async (ctx) =>
                setupOrbitCameraDemo(ctx),
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
            await withTimeseriesRenderer(
                "Timeseries Graph Mode",
                async ({ renderer, composer, canvas }) => {
                    canvas.style.width = "320px";
                    canvas.style.height = "200px";
                    resizeRendererForCanvas(renderer, canvas, 320, 200);

                    const timestamps = buildTimeAxis(240, 0.1);
                    const redSeries = mapSeries(
                        timestamps,
                        (t) => 55 + 18 * Math.sin(t * 0.4),
                    );
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
                    composer.set_clear_color(0.02, 0.02, 0.05, 1);
                    composer.render();

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
                },
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
    const filterKey = getActiveCanvasFilter();
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
        registerResultListItem(li, test.slug);
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
    const { CanvasComposer } = await loadRendererModule();
    const slot = createCanvasSlot("Shared Canvas Modes");
    const canvas = slot.canvas;
    const cssWidth = 360;
    const cssHeight = 240;
    canvas.style.width = `${cssWidth}px`;
    canvas.style.height = `${cssHeight}px`;

    let composer;
    let batched;
    let timeseries;
    let instHandle = null;
    try {
        composer = new CanvasComposer(canvas.id);
        batched = composer.add_batched_pass();
        timeseries = composer.add_timeseries_pass();

        resizeRendererForCanvas(composer, canvas, cssWidth, cssHeight);

        const meshHandle = batched.register_mesh(buildSingleTriangle());
        instHandle = batched.create_instance(
            meshHandle,
            rotationTranslationMatrix([0, 0, 0], 0),
        );

        composer.set_clear_color(0.1, 0.1, 0.12, 1);
        composer.render();
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
        composer.render();
        if (timeseries.series_count() !== series.length) {
            throw new Error("shared canvas timeseries renderer should track all series");
        }

        composer.render();
        slot.wrapper.classList.add("pass");
    } catch (err) {
        slot.wrapper.classList.add("fail");
        throw err;
    } finally {
        try {
            if (batched && instHandle !== null) {
                batched.remove_instance(instHandle);
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
        try {
            composer?.free?.();
        } catch (err) {
            console.warn("shared canvas composer free failed", err);
        }
    }
}

function getInitialFilterKey() {
    const params = new URLSearchParams(window.location.search);
    const raw = params.get("test");
    return normalizeFilterKey(raw);
}
