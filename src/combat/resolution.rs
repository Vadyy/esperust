use super::target::Reaction;
use super::{
    ActorEffect, CombatAttack, CombatHit, CombatStats, PracticeTarget, attacks::AttackBatch,
    cues::CombatCue,
};
use crate::{
    combat_geometry::in_hit_volume,
    controller::CollisionWorld,
    gameplay::{Motion, Player},
    melee::Attack,
    scene,
};
use bevy::prelude::*;

pub(super) fn incoming(
    content: &crate::content::simulation::SimulationContent,
    batch: &AttackBatch,
    entity: Entity,
    target: &mut PracticeTarget,
    transform: &Transform,
    player: &mut Player,
    world: &CollisionWorld,
    stats: &mut CombatStats,
    cues: &mut Vec<CombatCue>,
) {
    let attack = batch.attack;
    let facing = transform.rotation * Vec3::Z;
    let mut counter_stun = None;
    let fighter = target
        .fighter
        .as_mut()
        .expect("Sparring attack requires a fighter");
    for spec in batch.specs.iter().copied() {
        cues.push(CombatCue::Attack(CombatAttack {
            owner: entity,
            attack,
            spec,
            origin: *transform,
        }));
        if player.health <= 0.
            || !in_hit_volume(
                &spec,
                transform.translation,
                facing,
                player.feet + Vec3::Y * 1.6,
            )
        {
            continue;
        }
        let eye = transform.translation + Vec3::Y * 1.6;
        let delta = player.feet + Vec3::Y * 1.6 - eye;
        if world
            .raycast(eye, delta, (delta.length() - 0.7).max(0.))
            .is_some()
        {
            continue;
        }
        let toward = (transform.translation - player.feet)
            .with_y(0.)
            .normalize_or_zero();
        let angle = player
            .actor_facing
            .normalize_or_zero()
            .dot(toward)
            .clamp(-1., 1.)
            .acos()
            .to_degrees();
        let defense = crate::hit_rules::defend(content, &player.melee, spec.attribute, angle);
        let crush = matches!(
            defense,
            crate::hit_rules::Defense::Counter {
                crush_cue: true,
                ..
            } | crate::hit_rules::Defense::Damage { crush_cue: true }
        );
        if crush {
            stats.guard_crushes += 1;
            cues.push(CombatCue::Effect(ActorEffect {
                owner: None,
                name: "guard_crush",
                origin: Transform::from_translation(player.feet)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, player.actor_facing))
                    .with_scale(Vec3::splat(scene::PLAYER_MODEL_SCALE)),
            }));
        }
        match defense {
            crate::hit_rules::Defense::Counter { stun_seconds, .. } => {
                stats.countered_hits += 1;
                counter_stun = Some(stun_seconds);
            }
            crate::hit_rules::Defense::Block => {
                stats.guarded_hits += 1;
            }
            crate::hit_rules::Defense::Damage { .. } => {
                stats.incoming_hits += 1;
                fighter.melee.confirm_hit();
                player.receive_melee_hit(content, &spec, facing);
                cues.push(CombatCue::Hit(CombatHit {
                    position: player.feet + Vec3::Y * 1.6,
                    attack,
                    hit_index: spec.hit_index,
                    attribute: spec.attribute,
                    critical: spec.critical,
                    local_attacker: false,
                    lift: spec.lift,
                    stun: spec.stun,
                }));
            }
        }
    }
    if let Some(seconds) = counter_stun {
        target.counter_stun(seconds);
    }
}

pub(super) fn outgoing(
    content: &crate::content::simulation::SimulationContent,
    batch: &AttackBatch,
    entity: Entity,
    target: &mut PracticeTarget,
    transform: &mut Transform,
    player: &mut Player,
    world: &CollisionWorld,
    stats: &mut CombatStats,
    cues: &mut Vec<CombatCue>,
) {
    let attack = batch.attack;
    let attack_serial = batch.serial;
    for spec in batch.specs.iter().copied() {
        let key = (attack_serial, spec.hit_index);
        if target.health() <= 0.0 || target.last_hit == Some(key) {
            continue;
        }
        let origin = player.feet;
        let centre = transform.translation + Vec3::Y * 1.6;
        let aim = Vec3::from(player.melee.strike_direction()).normalize_or(player.actor_facing);
        if !in_hit_volume(&spec, origin, aim, centre) {
            continue;
        }
        let ray_origin = origin + Vec3::Y * 1.6;
        let offset = centre - ray_origin;
        if world
            .raycast(ray_origin, offset, (offset.length() - 0.7).max(0.0))
            .is_some()
        {
            continue;
        }
        target.last_hit = Some(key);
        if let Some(fighter) = &target.fighter {
            let toward = (player.feet - transform.translation)
                .with_y(0.)
                .normalize_or_zero();
            let angle = (transform.rotation * Vec3::Z)
                .dot(toward)
                .clamp(-1., 1.)
                .acos()
                .to_degrees();
            let defense = crate::hit_rules::defend(content, &fighter.melee, spec.attribute, angle);
            if matches!(
                defense,
                crate::hit_rules::Defense::Counter {
                    crush_cue: true,
                    ..
                } | crate::hit_rules::Defense::Damage { crush_cue: true }
            ) {
                stats.guard_crushes += 1;
                cues.push(CombatCue::Effect(ActorEffect {
                    owner: Some(entity),
                    name: "guard_crush",
                    origin: *transform,
                }));
            }
            match defense {
                crate::hit_rules::Defense::Counter { stun_seconds, .. } => {
                    stats.countered_hits += 1;
                    player.receive_counter_stun(stun_seconds);
                    cues.push(CombatCue::Effect(ActorEffect {
                        owner: None,
                        name: "stun",
                        origin: Transform::from_translation(player.feet)
                            .with_rotation(Quat::from_rotation_arc(Vec3::Z, player.actor_facing))
                            .with_scale(Vec3::splat(scene::PLAYER_MODEL_SCALE)),
                    }));
                    continue;
                }
                crate::hit_rules::Defense::Block => {
                    stats.guarded_hits += 1;
                    continue;
                }
                _ => {}
            }
        }
        if spec.critical {
            stats.critical_hits += 1;
        }
        target.body.actor_facing = transform.rotation * Vec3::Z;
        target.receive(content, &spec, aim);
        transform.rotation = Quat::from_rotation_arc(Vec3::Z, target.body.actor_facing);
        stats.hits += 1;
        stats.total_damage += spec.damage;
        if target.health() == 0.0 {
            stats.kills += 1;
        }
        stats.reactions |= 1
            << match target.reaction {
                Reaction::Flinch(_) => 0,
                Reaction::StunStart => 1,
                Reaction::BlowStart => 2,
                Reaction::Dead => 3,
                _ => 4,
            };
        if !spec.critical || target.health() == 0. || spec.stun > 0. {
            cues.push(CombatCue::ClearVoices(entity));
            let voice = if target.health() <= 0.0 {
                stats.death_sounds += 1;
                if spec.lift > 0.0 {
                    "_eu_voice_man_dead_blow"
                } else if spec.stun > 0.0 {
                    "_eu_voice_man_dead_shock"
                } else {
                    "_eu_voice_man_dead_normal"
                }
            } else {
                stats.hurt_sounds += 1;
                if spec.lift > 0.0 {
                    "voice_man_damage_1"
                } else if spec.stun > 0.0 {
                    "voice_man_damage_3"
                } else {
                    "voice_man_damage_2"
                }
            };
            cues.push(CombatCue::Voice {
                owner: entity,
                name: voice,
            });
        }
        cues.push(CombatCue::Hit(CombatHit {
            position: centre,
            attack,
            hit_index: spec.hit_index,
            attribute: spec.attribute,
            critical: spec.critical,
            local_attacker: true,
            lift: spec.lift,
            stun: spec.stun,
        }));
        player.melee.confirm_hit();
    }
}

pub(super) fn finish_player(
    content: &crate::content::simulation::SimulationContent,
    player: &mut Player,
    motion: &Motion,
    world: &CollisionWorld,
    cues: &mut Vec<CombatCue>,
) {
    if player
        .melee
        .attack
        .is_some_and(|attack| content.surface_success(attack))
        && player.melee.hit_specs(content).iter().any(|spec| {
            crate::combat_geometry::melee_surface_hit(world, spec, player.feet, player.actor_facing)
                .is_some()
        })
    {
        player.melee.confirm_hit();
        cues.push(CombatCue::CameraShake(
            crate::gameplay::CameraShakeRequest {
                source: player.feet + Vec3::Y * (motion.collision_axis_scales[1] * 2.),
                attack: player.melee.attack.unwrap(),
            },
        ));
    }
    player.melee.finish_hit_frame();
}

pub(super) fn finish_target(
    content: &crate::content::simulation::SimulationContent,
    target: &mut PracticeTarget,
    transform: &Transform,
    world: &CollisionWorld,
) {
    if let Some(fighter) = &mut target.fighter {
        if fighter.melee.attack == Some(Attack::CounterRetaliation)
            && fighter.melee.hit_specs(content).iter().any(|spec| {
                crate::combat_geometry::melee_surface_hit(
                    world,
                    spec,
                    transform.translation,
                    transform.rotation * Vec3::Z,
                )
                .is_some()
            })
        {
            fighter.melee.confirm_hit();
        }
        fighter.melee.finish_hit_frame();
    }
}

#[cfg(test)]
pub(super) fn hit_direction(forward: Vec3, toward_attacker: Vec3) -> u8 {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let toward = Vec3::new(toward_attacker.x, 0.0, toward_attacker.z);
    let front = toward.dot(forward);
    let side = toward.dot(Vec3::Y.cross(forward));
    if front.abs() >= side.abs() {
        if front >= 0.0 { 0 } else { 1 }
    } else if side >= 0.0 {
        3
    } else {
        2
    }
}
