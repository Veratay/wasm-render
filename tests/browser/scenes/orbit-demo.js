import {
    buildCubeMesh,
    resizeRendererForCanvas,
    rotationTranslationMatrix,
    loadRendererModule,
} from "../renderer-fixtures.js";

export async function setupOrbitCameraDemo({ renderer, composer, canvas, wrapper }) {
    const { build_orbit_view, build_perspective } = await loadRendererModule();
    canvas.style.width = "260px";
    canvas.style.height = "260px";
    resizeRendererForCanvas(composer, canvas, 260, 260);

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
        composer.set_clear_color(0.05, 0.05, 0.1, 1);
        const view = build_orbit_view(target, yaw, pitch, distance);
        renderer.set_view_matrix(view);

        instanceHandles.forEach((handle, index) => {
            const offset = sceneOffsets[index];
            const angle = time * 0.001 + index * 0.5;
            const matrix = rotationTranslationMatrix(offset, angle);
            renderer.set_instance_transform(handle, matrix);
        });

        composer.render();
        requestAnimationFrame(draw);
    };
    requestAnimationFrame(draw);

    return { keepAlive: true };
}
