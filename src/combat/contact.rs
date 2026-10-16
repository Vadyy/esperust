use super::{ActorSweepStart, PracticeTarget};
use crate::{
    controller::CollisionWorld,
    gameplay::{Motion, Player},
};
use bevy::prelude::*;

pub(super) fn step_target(
    content: &crate::content::simulation::SimulationContent,
    target: &mut PracticeTarget,
    transform: &mut Transform,
    player: &mut Player,
    motion: &Motion,
    world: &CollisionWorld,
    start: &ActorSweepStart,
    dt: f32,
) -> bool {
    let mut contacted_player = false;
    let target_start = transform.translation;
    target.step(content, world, motion, transform, dt);
    if target.solid(player.pose_reset_serial) {
        let axes = Vec3::from(motion.collision_axis_scales);
        if let Some((position, normal)) = actor_contact(
            world,
            target_start,
            transform.translation,
            start.feet,
            axes,
            motion.movement_skin_fraction,
        ) {
            transform.translation = position;
            target.body.constrain_actor_contact(position, normal);
        }
        if start.reset_serial == player.pose_reset_serial {
            let desired = player.feet;
            if let Some((position, normal)) = actor_contact(
                world,
                start.feet,
                desired,
                transform.translation,
                axes,
                motion.movement_skin_fraction,
            ) {
                player.constrain_actor_contact(position, normal);
                contacted_player = true;
            }
        }
    }
    contacted_player
}

pub(super) fn actor_contact(
    world: &crate::controller::CollisionWorld,
    start: Vec3,
    desired: Vec3,
    other: Vec3,
    axes: Vec3,
    skin: f32,
) -> Option<(Vec3, Vec3)> {
    if !start.is_finite()
        || !desired.is_finite()
        || !other.is_finite()
        || !axes.is_finite()
        || axes.min_element() <= 0.0
    {
        return None;
    }
    let expanded = axes * 2.0;
    let radius = 1.0001_f32;
    let relative = (start - other) / expanded;
    let delta = desired - start;
    let velocity = delta / expanded;
    let c = relative.length_squared() - radius * radius;
    if c < -0.000001 {
        let end = (desired - other) / expanded;
        if end.length_squared() >= radius * radius
            && end.length_squared() > relative.length_squared()
        {
            return None;
        }
        let direction = if end.length_squared() > 0.000001 {
            end.normalize()
        } else {
            Vec3::X
        };
        let normal = (direction / expanded).normalize();
        let separated = other + direction * expanded * radius;
        let position = world
            .move_ellipsoid(start, separated - start, axes, 0.0)
            .position;
        return Some((position, normal));
    }
    let a = velocity.length_squared();
    let b = relative.dot(velocity);
    if a < 0.00000001 || b >= 0.0 {
        return None;
    }
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = ((-b - discriminant.sqrt()) / a).max(0.0);
    if t > 1.0 {
        return None;
    }
    let contact = start + delta * t;
    let normal = (((contact - other) / expanded) / expanded).normalize_or_zero();
    let remaining = delta * (1.0 - t);
    let slide = remaining - normal * remaining.dot(normal).min(0.0);
    let position = world.move_ellipsoid(contact, slide, axes, skin).position;
    Some((position, normal))
}
