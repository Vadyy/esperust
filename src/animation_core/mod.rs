use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalPose {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
impl LocalPose {
    pub(crate) fn blend(self, mut target: Self, weight: f32) -> Self {
        if self.rotation.dot(target.rotation) < 0. {
            target.rotation = -target.rotation;
        }
        Self {
            translation: self.translation.lerp(target.translation, weight),
            rotation: self.rotation.slerp(target.rotation, weight).normalize(),
            scale: self.scale.lerp(target.scale, weight),
        }
    }
    pub(crate) fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

pub(crate) fn compose_world(
    local: &[LocalPose],
    parents: impl Iterator<Item = Option<usize>>,
    world: &mut Vec<Mat4>,
) {
    world.clear();
    for (pose, parent) in local.iter().zip(parents) {
        let matrix = pose.matrix();
        world.push(parent.map_or(matrix, |parent| world[parent] * matrix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_bounds_reject_overflow_and_end_frame_holds_its_endpoint() {
        assert!(valid_clip(0, 2, 4800., 10, 20));
        assert!(!valid_clip(0, 2, f64::INFINITY, 10, 20));
        assert!(!valid_clip(0, usize::MAX, 4800., 10, 20));
        assert!(!valid_clip(3, 2, 4800., 10, 20));
        assert!(!valid_clip(0, 2, 4800., 10, 19));
        let sample = frame_sample(40, 2, 10, 1.);
        assert_eq!(
            (sample.offset0, sample.offset1, sample.fraction),
            (20, 20, 0.)
        );
    }
    #[test]
    fn antipodal_rotations_keep_scale_and_hierarchy_order() {
        let parent = LocalPose {
            translation: Vec3::X,
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(2.),
        };
        let opposite = LocalPose {
            rotation: -parent.rotation,
            ..parent
        };
        let blended = parent.blend(opposite, 0.5);
        assert!(blended.rotation.dot(parent.rotation).abs() > 0.999999);
        assert_eq!(blended.scale, Vec3::splat(2.));
        let child = LocalPose {
            translation: Vec3::X,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let mut world = vec![Mat4::ZERO; 9];
        compose_world(&[blended, child], [None, Some(0)].into_iter(), &mut world);
        assert_eq!(world.len(), 2);
        assert!(
            world[1]
                .transform_point3(Vec3::ZERO)
                .distance(Vec3::new(1., 2., 0.))
                < 0.000001
        );
    }
}

pub(crate) mod skinning;

pub(crate) fn sample_pose_frames(
    first: &[f32],
    second: &[f32],
    fraction: f32,
    bones: usize,
) -> Vec<LocalPose> {
    let read = |data: &[f32], offset: usize| LocalPose {
        translation: Vec3::from_slice(&data[offset..offset + 3]),
        rotation: Quat::from_slice(&data[offset + 3..offset + 7]).normalize(),
        scale: Vec3::from_slice(&data[offset + 7..offset + 10]),
    };
    (0..bones)
        .map(|bone| read(first, bone * 10).blend(read(second, bone * 10), fraction))
        .collect()
}

#[derive(Clone, Copy)]
pub(crate) struct FrameSample {
    pub offset0: usize,
    pub offset1: usize,
    pub fraction: f32,
}
pub(crate) fn frame_sample(
    byte_offset: usize,
    frame_count: usize,
    stride: usize,
    frame: f64,
) -> FrameSample {
    let first = frame.floor() as usize;
    let second = (first + 1).min(frame_count - 1);
    FrameSample {
        offset0: byte_offset / 4 + first * stride,
        offset1: byte_offset / 4 + second * stride,
        fraction: frame.fract() as f32,
    }
}
pub(crate) fn valid_clip(
    byte_offset: usize,
    frame_count: usize,
    duration: f64,
    stride: usize,
    data_len: usize,
) -> bool {
    byte_offset % 4 == 0
        && frame_count >= 2
        && duration.is_finite()
        && duration > 0.
        && frame_count
            .checked_mul(stride)
            .and_then(|length| (byte_offset / 4).checked_add(length))
            .is_some_and(|end| end <= data_len)
}

pub(crate) mod storage;
