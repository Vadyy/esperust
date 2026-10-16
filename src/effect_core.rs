use bevy::prelude::*;

pub(crate) fn source_vector(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], -value[2])
}
pub(crate) fn random(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / 16_777_216.
}
pub(crate) fn sample_key(keys: &[[f32; 8]], fraction: f32) -> [f32; 8] {
    if fraction <= keys[0][0] {
        return keys[0];
    }
    for pair in keys.windows(2) {
        if fraction <= pair[1][0] {
            let t = ((fraction - pair[0][0]) / (pair[1][0] - pair[0][0])).clamp(0., 1.);
            return std::array::from_fn(|i| pair[0][i] + (pair[1][i] - pair[0][i]) * t);
        }
    }
    *keys.last().unwrap()
}

pub(crate) fn frame_span<T>(
    frames: &[T],
    tick: f32,
    tick_of: impl Fn(&T) -> f32,
) -> (usize, usize, f32) {
    let second = frames
        .partition_point(|frame| tick_of(frame) < tick)
        .min(frames.len() - 1);
    let first = second.saturating_sub(1);
    let a = tick_of(&frames[first]);
    let b = tick_of(&frames[second]);
    let weight = if b > a {
        ((tick - a) / (b - a)).clamp(0., 1.)
    } else {
        0.
    };
    (first, second, weight)
}

pub(crate) fn billboard_axes(
    kind: u32,
    velocity: Vec3,
    position: Vec3,
    camera: &Transform,
) -> (Vec3, Vec3) {
    if kind == 1 {
        let up = velocity.normalize_or(Vec3::Y);
        (
            (camera.translation - position)
                .cross(up)
                .normalize_or(Vec3::X),
            up,
        )
    } else {
        (*camera.right(), *camera.up())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_keys_and_frame_endpoints_preserve_duplicate_boundaries() {
        let keys = [[0.; 8], [0.5; 8], [0.5; 8], [1.; 8]];
        assert_eq!(sample_key(&keys, -1.), keys[0]);
        assert_eq!(sample_key(&keys, 0.5), keys[1]);
        assert_eq!(sample_key(&keys, 2.), keys[3]);
        assert_eq!(frame_span(&[0., 2., 2., 4.], 2., |v| *v), (0, 1, 1.));
        assert_eq!(frame_span(&[0., 2., 2., 4.], 3., |v| *v), (2, 3, 0.5));
        assert_eq!(frame_span(&[0., 2.], -1., |v| *v), (0, 0, 0.));
        assert_eq!(frame_span(&[0., 2.], 3., |v| *v), (0, 1, 1.));
    }
    #[test]
    fn rng_stream_and_billboard_fallbacks_keep_owner_state() {
        let mut state = 1234;
        let mut other = state;
        assert_eq!(random(&mut state).to_bits(), random(&mut other).to_bits());
        assert_eq!(state, 332584831);
        let camera = Transform::from_xyz(0., 0., 1.);
        let (right, up) = billboard_axes(1, Vec3::ZERO, Vec3::ZERO, &camera);
        assert_eq!(up, Vec3::Y);
        assert_eq!(right, -Vec3::X);
        assert_eq!(
            billboard_axes(0, Vec3::X, Vec3::ZERO, &camera),
            (Vec3::X, Vec3::Y)
        );
        assert_eq!(source_vector([1., 2., 3.]), Vec3::new(1., 2., -3.));
    }
}
