use super::{Controls, HitReactionKind, Motion, MovementAction, Player};
use crate::{Options, scene::Level};
use bevy::prelude::*;

#[derive(Component)]
pub struct Avatar;
#[derive(Component)]
pub struct View {
    pub(super) last_pose_reset_serial: u32,
}

#[derive(Resource, Default)]
pub(crate) struct CameraShake {
    active: Option<AttackShake>,
    reset_serial: u32,
}
struct AttackShake {
    source: Vec3,
    profile: crate::hit_rules::CameraShakeProfile,
    wave_age: f32,
    full_age: f32,
    fade_age: Option<f32>,
}
#[derive(Message)]
pub(crate) struct CameraShakeRequest {
    pub source: Vec3,
    pub attack: crate::melee::Attack,
}
impl CameraShake {
    fn request_attack(
        &mut self,
        content: &crate::content::simulation::ImpactPresentation,
        source: Vec3,
        attack: crate::melee::Attack,
    ) {
        let Some(profile) = content.attack_shake(attack) else {
            return;
        };
        if self.active.is_some() && !profile.replace {
            return;
        }
        let wave_age = self.active.as_ref().map_or(0., |active| active.wave_age);
        self.active = Some(AttackShake {
            source,
            profile,
            wave_age,
            full_age: 0.,
            fade_age: None,
        });
    }
}

pub(crate) fn capture_camera_shake(
    content: Res<crate::content::simulation::ImpactPresentation>,
    (mut hits, mut object_hits): (
        MessageReader<crate::combat::CombatHit>,
        MessageReader<CameraShakeRequest>,
    ),
    player: Res<Player>,
    motion: Res<Motion>,
    mut shake: ResMut<CameraShake>,
) {
    if shake.reset_serial != player.pose_reset_serial || player.health <= 0. {
        shake.reset_serial = player.pose_reset_serial;
        shake.active = None;
        hits.clear();
        object_hits.clear();
        return;
    }
    for hit in hits.read() {
        if !hit.local_attacker || hit.critical {
            continue;
        }
        shake.request_attack(
            &content,
            player.feet + Vec3::Y * (motion.collision_axis_scales[1] * 2.),
            hit.attack,
        );
    }
    for hit in object_hits.read() {
        shake.request_attack(&content, hit.source, hit.attack);
    }
}
pub fn present(
    player: Res<Player>,
    controls: Res<Controls>,
    motion: Res<Motion>,
    level: Res<Level>,
    options: Res<Options>,
    time: Res<Time<Fixed>>,
    frame_time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut avatar: Single<&mut Transform, (With<Avatar>, Without<View>)>,
    mut camera: Single<(&mut Transform, &mut Projection, &mut View), Without<Avatar>>,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("camera_and_avatar"));
    let feet = player.previous.lerp(player.feet, time.overstep_fraction());
    avatar.translation = feet;
    let facing_yaw = if let Some(emote) = player.emote { emote.yaw } else if player.action == MovementAction::WallJump
        || player.hit_reaction.is_some_and(|reaction| {
            matches!(
                reaction.kind,
                HitReactionKind::Blow
                    | HitReactionKind::Down
                    | HitReactionKind::DownDamage
                    | HitReactionKind::StandUp
            )
        })
        || matches!(
            player.melee.attack,
            Some(
                crate::melee::Attack::CounterCombo(_)
                    | crate::melee::Attack::CounterJump
                    | crate::melee::Attack::CounterRetaliation
            )
        ) {
        player.actor_facing.x.atan2(-player.actor_facing.z)
    } else {
        controls.yaw
    };
    avatar.rotation = Quat::from_rotation_y(std::f32::consts::PI - facing_yaw);
    let (camera, projection, view) = &mut *camera;
    if let Projection::Perspective(perspective) = &mut **projection {
        if view.last_pose_reset_serial != player.pose_reset_serial {
            perspective.fov = motion.camera_fov_degrees.to_radians();
            view.last_pose_reset_serial = player.pose_reset_serial;
        }
        let target = if !options.overview && player.action == MovementAction::Sprint {
            motion.camera_sprint_fov_degrees
        } else {
            motion.camera_fov_degrees
        };
        perspective.fov = camera_fov_step(
            perspective.fov.to_degrees(),
            target,
            frame_time.delta_secs(),
            motion.camera_fov_blend_factor,
        )
        .to_radians();
    }
    if options.overview {
        **camera = Transform::from_translation(level.center + Vec3::new(43., 43., 62.))
            .looking_at(level.center, Vec3::Y);
        return;
    }
    let forward = Vec3::new(controls.yaw.sin(), 0., -controls.yaw.cos());
    let right = Vec3::new(controls.yaw.cos(), 0., controls.yaw.sin());
    let look = forward * controls.pitch.cos() + Vec3::Y * controls.pitch.sin();
    let pivot = feet + Vec3::Y * motion.camera_height;
    let desired = pivot - look * motion.camera_distance + right * motion.camera_side;
    let delta = desired - pivot;
    let distance = delta.length();
    let allowed = level
        .collision
        .raycast(pivot, delta, distance)
        .map_or(distance, |hit| (hit - 0.2).max(0.3));
    camera.translation = pivot + delta / distance * allowed;
    camera.look_to(look, Vec3::Y);
    if let Some(active) = &mut shake.active {
        let dt = frame_time.delta_secs();
        active.wave_age += dt;
        if active.fade_age.is_none() {
            active.full_age += dt;
            if active.full_age > active.profile.full_seconds {
                active.fade_age = Some(0.);
            }
        }
        let envelope = if let Some(age) = &mut active.fade_age {
            *age += dt;
            (1. - *age / active.profile.fade_seconds.max(f32::EPSILON)).clamp(0., 1.)
        } else {
            1.
        };
        let distance = camera.translation.distance(active.source);
        let attenuation = if distance < active.profile.near {
            1.
        } else {
            ((active.profile.far - distance) / (active.profile.far - active.profile.near))
                .clamp(0., 1.)
        };
        let phase = active.wave_age * 1000. / 33.;
        let offset = Vec3::Y * (active.profile.up_factor * phase).sin()
            + Vec3::X * (active.profile.right_factor * phase).sin();
        let rotation = camera.rotation;
        camera.translation +=
            rotation * offset * (active.profile.amplitude * envelope * attenuation);
        if active
            .fade_age
            .is_some_and(|age| age > active.profile.fade_seconds)
        {
            shake.active = None;
        }
    }
}

fn camera_fov_step(current: f32, target: f32, dt: f32, blend_factor: f32) -> f32 {
    let frame_factor = (dt * 10.).clamp(0., 1.);
    current + (target - current) * (frame_factor * blend_factor).clamp(0., 1.)
}

#[cfg(test)]
mod camera_tests {
    use super::*;

    #[test]
    fn sprint_pov_uses_original_target_and_both_interpolation_clamps() {
        let motion = Motion::default();
        assert_eq!(motion.camera_fov_degrees, 60.);
        assert_eq!(motion.camera_sprint_fov_degrees, 80.);
        let step = |current, target, dt| {
            camera_fov_step(current, target, dt, motion.camera_fov_blend_factor)
        };
        assert!((step(60., 80., 0.016) - 61.6).abs() < 0.00001);
        assert!((step(80., 60., 0.016) - 78.4).abs() < 0.00001);
        assert_eq!(step(60., 80., 0.), 60.);
        assert_eq!(step(60., 80., 1.), 70.);
    }

    #[test]
    fn sprint_pov_reverses_smoothly_when_an_action_interrupts_it() {
        let mut fov = 60.;
        for _ in 0..10 {
            fov = camera_fov_step(fov, 80., 0.016, 0.5);
        }
        let widening = 80. - 20. * 0.92_f32.powi(10);
        assert!((fov - widening).abs() < 0.00001);
        let before = fov;
        fov = camera_fov_step(fov, 60., 0.016, 0.5);
        assert!(fov > 60. && fov < before);
        for _ in 0..120 {
            fov = camera_fov_step(fov, 60., 0.016, 0.5);
        }
        assert!((fov - 60.).abs() < 0.001);
    }
}
