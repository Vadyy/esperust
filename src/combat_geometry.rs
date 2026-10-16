use crate::melee::{HitShape, HitSpec};
use bevy::prelude::*;

pub(crate) fn in_hit_volume(spec: &HitSpec, origin: Vec3, forward: Vec3, target: Vec3) -> bool {
    let forward = forward.normalize_or(Vec3::Z);
    let yaw = forward.x.atan2(forward.z);
    let pitch = forward.y.atan2(Vec2::new(forward.x, forward.z).length());
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let local = rotation.inverse() * (target - origin);
    match spec.shape {
        HitShape::Ray {
            origin,
            direction,
            length,
        } => {
            let axes = Vec3::new(0.7, 1.6, 0.7);
            let from = (Vec3::from(origin) - local) / axes;
            let dir = Vec3::from(direction).normalize_or_zero() / axes;
            let a = dir.length_squared();
            let b = from.dot(dir);
            let c = from.length_squared() - 1.;
            let discriminant = b * b - a * c;
            if a <= 0. || discriminant < 0. {
                false
            } else {
                let first = (-b - discriminant.sqrt()) / a;
                let last = (-b + discriminant.sqrt()) / a;
                last >= 0. && first <= length
            }
        }
        HitShape::Cylinder {
            origin,
            radius,
            height,
        } => {
            let delta = local - Vec3::from(origin);
            delta.y.abs() <= height * 0.5 + 1.6
                && Vec2::new(delta.x, delta.z).length() <= radius + 0.7
        }
        HitShape::Box {
            origin,
            width,
            height,
            length,
        } => {
            let delta = local - Vec3::from(origin);
            delta.x.abs() <= width * 0.5 + 0.7
                && delta.y.abs() <= height * 0.5 + 1.6
                && delta.z >= -0.7
                && delta.z <= length + 0.7
        }
        HitShape::Fan {
            origin,
            height,
            inner_radius,
            outer_radius,
            angle,
        } => {
            let delta = local - Vec3::from(origin);
            let radius = Vec2::new(delta.x, delta.z).length();
            delta.y.abs() <= height * 0.5 + 1.6
                && radius <= outer_radius + 0.7
                && radius + 0.7 >= inner_radius
                && (radius <= 0.7 || delta.z / radius >= (angle * 0.5).cos())
        }
    }
}

pub(crate) fn melee_surface_hit(
    world: &crate::controller::CollisionWorld,
    hit: &crate::melee::HitSpec,
    feet: Vec3,
    facing: Vec3,
) -> Option<Transform> {
    use crate::melee::HitShape;
    let facing = facing.normalize_or(Vec3::NEG_Z);
    let right = facing.cross(Vec3::Y);
    if matches!(hit.shape, HitShape::Ray { .. }) {
        return None;
    }
    let (offset, height, width, inner, length, angle) = match hit.shape {
        HitShape::Ray { .. } => unreachable!(),
        HitShape::Fan {
            origin,
            height,
            inner_radius,
            outer_radius,
            angle,
        } => (
            Vec3::from(origin),
            height,
            0.,
            inner_radius,
            outer_radius,
            angle,
        ),
        HitShape::Box {
            origin,
            width,
            height,
            length,
        } => (Vec3::from(origin), height, width, 0., length, 0.),
        HitShape::Cylinder {
            origin,
            radius,
            height,
        } => (
            Vec3::from(origin),
            height,
            0.,
            0.,
            radius,
            std::f32::consts::TAU,
        ),
    };
    let centre = feet + right * offset.x + Vec3::Y * offset.y + facing * offset.z;
    let mut nearest: Option<crate::controller::SurfaceHit> = None;
    for y in [-0.49, 0., 0.49] {
        for sample in 0..=12 {
            let fraction = sample as f32 / 12. - 0.5;
            let direction = Quat::from_rotation_y(angle * fraction) * facing;
            let origin = centre + Vec3::Y * (height * y) + right * (width * fraction);
            if let Some(surface) = world.raycast_surface(origin, direction, length) {
                if surface.distance >= inner
                    && nearest.is_none_or(|old| surface.distance < old.distance)
                {
                    nearest = Some(surface);
                }
            }
        }
    }
    nearest.map(|surface| {
        Transform::from_translation(surface.position + surface.normal * 0.015)
            .with_rotation(Quat::from_rotation_arc(Vec3::Z, surface.normal))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(shape: HitShape) -> HitSpec {
        HitSpec {
            hit_index: 0,
            range: 3.5,
            damage: 17.,
            push: 0.,
            lift: 0.,
            stun: 0.,
            shape,
            attribute: crate::combat_types::AttackAttributeId::PlasmaStandWeak,
            critical: false,
        }
    }

    fn plasma_weak() -> HitSpec {
        spec(HitShape::Fan {
            origin: [0., 1.4, 0.5],
            height: 2.,
            inner_radius: 0.5,
            outer_radius: 3.5,
            angle: 120_f32.to_radians(),
        })
    }

    #[test]
    fn default_pitched_aim_hits_the_practice_target_without_inverting_height() {
        let pitch = -0.12_f32;
        let aim = Vec3::new(0., pitch.sin(), -pitch.cos());
        let target = Vec3::new(0., 1.6, -3.5);
        let weak = plasma_weak();
        let heavy = spec(HitShape::Box {
            origin: [0., 2.5, 0.],
            width: 2.,
            height: 2.,
            length: 5.,
        });
        for attack in [weak, heavy] {
            assert!(in_hit_volume(&attack, Vec3::ZERO, Vec3::NEG_Z, target));
            assert!(in_hit_volume(&attack, Vec3::ZERO, aim, target));
            assert!(!in_hit_volume(
                &attack,
                Vec3::ZERO,
                aim,
                target - Vec3::Z * 10.
            ));
            assert!(!in_hit_volume(
                &attack,
                Vec3::ZERO,
                aim,
                target + Vec3::Y * 10.
            ));
        }
    }

    #[test]
    fn attack_acceptance_is_invariant_under_world_yaw() {
        let attacks = [
            plasma_weak(),
            spec(HitShape::Box {
                origin: [0., 2.5, 1.],
                width: 3.,
                height: 3.5,
                length: 3.5,
            }),
            spec(HitShape::Cylinder {
                origin: [0., 0., 0.],
                radius: 5.,
                height: 6.,
            }),
        ];
        let samples = [
            Vec3::new(0., 1.6, 3.5),
            Vec3::new(1.5, 2.5, 2.),
            Vec3::new(-3., 1., 1.),
            Vec3::new(0., 9., 3.),
            Vec3::new(8., 0., 1.),
            Vec3::new(0., 0., -8.),
        ];
        for pitch in [-0.5_f32, -0.12, 0., 0.4] {
            let aim = Vec3::new(0., pitch.sin(), pitch.cos());
            for attack in &attacks {
                for target in samples {
                    let expected = in_hit_volume(attack, Vec3::ZERO, aim, target);
                    for turn in 0..24 {
                        let yaw = Quat::from_rotation_y(turn as f32 * std::f32::consts::TAU / 24.);
                        assert_eq!(
                            in_hit_volume(attack, Vec3::ZERO, yaw * aim, yaw * target),
                            expected,
                            "pitch={pitch}, turn={turn}, target={target:?}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn critical_ray_preserves_aim_pitch_origin_and_finite_length_at_every_yaw() {
        let mut ray = spec(HitShape::Ray {
            origin: [0., 2.5, 1.],
            direction: [0., 0., 1.],
            length: 4.,
        });
        ray.critical = true;
        for pitch in [-1_f32, -0.4, 0., 0.4, 1.] {
            for turn in 0..24 {
                let yaw = turn as f32 * std::f32::consts::TAU / 24.;
                let forward = Vec3::new(
                    pitch.cos() * yaw.sin(),
                    pitch.sin(),
                    pitch.cos() * yaw.cos(),
                );
                let up = Vec3::new(
                    -pitch.sin() * yaw.sin(),
                    pitch.cos(),
                    -pitch.sin() * yaw.cos(),
                );
                let side = Vec3::new(yaw.cos(), 0., -yaw.sin());
                let target = up * 2.5 + forward * 4.8;
                assert!(
                    in_hit_volume(&ray, Vec3::ZERO, forward, target),
                    "pitch={pitch}, turn={turn}"
                );
                assert!(!in_hit_volume(
                    &ray,
                    Vec3::ZERO,
                    forward,
                    target + side * 2.
                ));
                assert!(!in_hit_volume(
                    &ray,
                    Vec3::ZERO,
                    forward,
                    up * 2.5 + forward * 10.
                ));
                if pitch.abs() == 1. {
                    let flat = Vec3::new(yaw.sin(), 0., yaw.cos());
                    assert!(!in_hit_volume(&ray, Vec3::ZERO, flat, target));
                }
            }
        }
    }
}
