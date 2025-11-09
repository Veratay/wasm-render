import("./pkg/render")
    .then(({ CanvasComposer }) => {
        const composer = new CanvasComposer("rootCanvas");
        const batched = composer.add_batched_pass();

        const triangle = new Float32Array([
            // x,   y,   z,   r, g, b, a
            -0.5, -0.5, 0.0, 0.95, 0.3, 0.2, 1.0,
            0.5, -0.5, 0.0, 0.2, 0.8, 0.4, 1.0,
            0.0, 0.5, 0.0, 0.15, 0.45, 0.95, 1.0,
        ]);

        const mesh = batched.register_mesh(triangle);
        batched.create_instance(mesh, identityMatrix());
        composer.set_clear_color(0.03, 0.03, 0.08, 1.0);
        composer.render();
    })
    .catch(console.error);

function identityMatrix() {
    return new Float32Array([
        1, 0, 0, 0, //
        0, 1, 0, 0, //
        0, 0, 1, 0, //
        0, 0, 0, 1, //
    ]);
}
