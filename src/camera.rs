use crate::batcher::MATRIX_FLOATS;

pub const MIN_CAMERA_DISTANCE: f32 = 0.01;
const MAX_PITCH_ABS: f32 = 1.553343; // ~ +/-89 degrees

pub fn perspective_matrix(
    fov_y_radians: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> Result<[f32; MATRIX_FLOATS], &'static str> {
    if !fov_y_radians.is_finite() || fov_y_radians <= 0.0 {
        return Err("fov_y_radians must be positive");
    }
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err("aspect ratio must be positive");
    }
    if !near.is_finite() || !far.is_finite() || near <= 0.0 || far <= near {
        return Err("near/far planes must satisfy 0 < near < far");
    }

    let f = 1.0 / (fov_y_radians * 0.5).tan();
    let nf = 1.0 / (near - far);
    let mut out = [0.0; MATRIX_FLOATS];
    out[0] = f / aspect;
    out[5] = f;
    out[10] = (far + near) * nf;
    out[11] = -1.0;
    out[14] = 2.0 * far * near * nf;
    Ok(out)
}

pub fn orbit_view_matrix(
    target: [f32; 3],
    yaw: f32,
    pitch: f32,
    distance: f32,
) -> Result<[f32; MATRIX_FLOATS], &'static str> {
    let distance = distance.max(MIN_CAMERA_DISTANCE);
    let clamped_pitch = pitch.clamp(-MAX_PITCH_ABS, MAX_PITCH_ABS);
    let cos_pitch = clamped_pitch.cos();
    let eye = [
        target[0] + distance * cos_pitch * yaw.cos(),
        target[1] + distance * clamped_pitch.sin(),
        target[2] + distance * cos_pitch * yaw.sin(),
    ];
    let up = [0.0, 1.0, 0.0];
    look_at_matrix(eye, target, up)
}

fn look_at_matrix(
    eye: [f32; 3],
    target: [f32; 3],
    up: [f32; 3],
) -> Result<[f32; MATRIX_FLOATS], &'static str> {
    let forward = normalize(sub(target, eye))?;
    let right = normalize(cross(forward, up))?;
    let true_up = cross(right, forward);

    let mut out = [0.0; MATRIX_FLOATS];
    out[0] = right[0];
    out[1] = true_up[0];
    out[2] = -forward[0];
    out[4] = right[1];
    out[5] = true_up[1];
    out[6] = -forward[1];
    out[8] = right[2];
    out[9] = true_up[2];
    out[10] = -forward[2];
    out[15] = 1.0;

    out[12] = -dot(right, eye);
    out[13] = -dot(true_up, eye);
    out[14] = dot(forward, eye);
    Ok(out)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> Result<[f32; 3], &'static str> {
    let len_sq = dot(v, v);
    if len_sq <= f32::EPSILON {
        return Err("vector length must be > 0");
    }
    let inv_len = len_sq.sqrt().recip();
    Ok([v[0] * inv_len, v[1] * inv_len, v[2] * inv_len])
}
