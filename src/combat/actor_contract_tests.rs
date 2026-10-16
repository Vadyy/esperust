use super::{PracticeTarget, target::Reaction};
use crate::{
    combat_types::{Attack, AttackAttributeId},
    content::simulation::{self, SimulationContent},
    controller::CollisionWorld,
    gameplay::{HitReaction, HitReactionKind, Motion, MovementInput, Player},
};
use bevy::prelude::*;

fn content() -> SimulationContent {
    let mut attributes = crate::content::tests::synthetic_attributes();
    let calls = attributes["effects"]["attribute_calls"]
        .as_array_mut()
        .unwrap();
    for id in [
        AttackAttributeId::CounterCritical,
        AttackAttributeId::CounterCounterCritical,
    ] {
        calls.push(serde_json::json!({"method":"SetDamageData", "target":format!("AttackAttribIndexer.GetAttackAttrib({})", id.source_name()), "arguments":[0]}));
    }
    simulation::from_fixture(
        crate::content::tests::synthetic_reactions(),
        crate::content::tests::synthetic_roots(),
        attributes,
    )
    .unwrap()
    .0
}

pub(super) fn actors(motion: &Motion, feet: Vec3) -> (Player, PracticeTarget, Transform) {
    let mut player = Player::new(feet, motion);
    player.actor_facing = Vec3::Z;
    let mut target = PracticeTarget::new(feet, 0);
    target.body.actor_facing = Vec3::Z;
    (player, target, Transform::from_translation(feet))
}
pub(super) fn hit_pair(
    content: &SimulationContent,
    player: &mut Player,
    target: &mut PracticeTarget,
    spec: &crate::melee::HitSpec,
    facing: Vec3,
) {
    player.receive_melee_hit(content, spec, facing);
    target.receive(content, spec, facing);
    assert_eq!(target.health(), player.health);
    assert_eq!(target.body.velocity(), player.velocity());
    assert_eq!(target.body.grounded, player.grounded);
    assert_eq!(target.body.actor_facing, player.actor_facing);
}
pub(super) fn tick_pair(
    content: &SimulationContent,
    world: &CollisionWorld,
    motion: &Motion,
    player: &mut Player,
    target: &mut PracticeTarget,
    transform: &mut Transform,
    dt: f32,
) {
    let yaw = player.actor_facing.x.atan2(-player.actor_facing.z);
    player.step(
        content,
        world,
        motion,
        MovementInput { yaw, ..default() },
        dt,
    );
    target.step(content, world, motion, transform, dt);
    assert!(
        (transform.translation - player.feet).length() < 0.0001,
        "player {:?}, target {:?}",
        player.feet,
        transform.translation
    );
    assert!((target.body.velocity() - player.velocity()).length() < 0.0001);
    assert_eq!(target.body.grounded, player.grounded);
    match (player.hit_reaction, target.body.hit_reaction) {
        (Some(a), Some(b)) => {
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.phase, b.phase);
            assert_eq!(a.age, b.age);
            assert_eq!(a.duration, b.duration);
        }
        (None, None) => {}
        other => panic!("reaction mismatch: {other:?}"),
    }
}
#[test]
fn critical_companions_preserve_launch_and_stun_for_both_receivers() {
    let content = &content();
    let motion = Motion::default();
    let (mut player, mut target, _) = actors(&motion, Vec3::ZERO);
    let mut launch = content.hit_spec(Attack::CounterRetaliation, 1).unwrap();
    launch.damage = 1.;
    hit_pair(content, &mut player, &mut target, &launch, Vec3::Z);
    let velocity = player.velocity();
    let mut critical = content.hit_spec(Attack::CounterRetaliation, 0).unwrap();
    critical.damage = 1.;
    assert!(critical.critical);
    let serial = target.reaction_serial;
    hit_pair(content, &mut player, &mut target, &critical, Vec3::Z);
    assert_eq!(target.body.velocity(), velocity);
    assert_eq!(target.reaction, Reaction::BlowStart);
    assert_eq!(target.reaction_serial, serial);
    player.receive_counter_stun(3.);
    target.counter_stun(3.);
    hit_pair(content, &mut player, &mut target, &critical, Vec3::Z);
    assert_eq!(target.stun, 3.);
    assert_eq!(target.reaction, Reaction::StunStart);
}

#[test]
fn down_damage_reduction_rehit_and_recovery_match_player() {
    let content = &content();
    let motion = Motion {
        gravity: 0.,
        movement_skin_fraction: 0.,
        ..default()
    };
    let (mut player, mut target, mut transform) = actors(&motion, Vec3::Y * 10.);
    let down = HitReaction {
        kind: HitReactionKind::Down,
        age: 0.,
        duration: 1.,
        moving: false,
        phase: 0,
    };
    player.hit_reaction = Some(down);
    target.set_reaction(Reaction::Down);
    let hit = content.hit_spec(Attack::CounterCombo(0), 1).unwrap();
    hit_pair(content, &mut player, &mut target, &hit, Vec3::Z);
    assert_eq!(target.health(), 100. - hit.damage * 0.5);
    assert_eq!(target.animation_action(), "down_hit");
    hit_pair(content, &mut player, &mut target, &hit, Vec3::Z);
    assert_eq!(target.health(), 100. - hit.damage * 1.5);
    tick_pair(
        content,
        &CollisionWorld::default(),
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        0.5,
    );
    assert_eq!(target.reaction, Reaction::DownDamage);
    tick_pair(
        content,
        &CollisionWorld::default(),
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        0.001,
    );
    assert_eq!(target.reaction, Reaction::Down);
    tick_pair(
        content,
        &CollisionWorld::default(),
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        1.,
    );
    assert_eq!(target.reaction, Reaction::Down);
    tick_pair(
        content,
        &CollisionWorld::default(),
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        0.001,
    );
    assert_eq!(target.reaction, Reaction::StandUp);
}

#[test]
fn selected_hit_profiles_share_receiver_damage_impulse_and_direction() {
    let content = &content();
    let motion = Motion::default();
    for attribute in AttackAttributeId::ALL {
        for facing in [Vec3::Z, Vec3::X, Vec3::NEG_Z, Vec3::NEG_X] {
            let (mut player, mut target, _) = actors(&motion, Vec3::ZERO);
            let mut hit = content.hit_spec(Attack::PlasmaWeak(0), 0).unwrap();
            hit.attribute = attribute;
            hit.damage = 1.;
            hit_pair(content, &mut player, &mut target, &hit, facing);
            assert_eq!(
                target.body.hit_reaction.map(|r| r.kind),
                player.hit_reaction.map(|r| r.kind)
            );
        }
    }
}

#[test]
fn lethal_hits_and_respawn_clear_transient_state() {
    let content = &content();
    let motion = Motion::default();
    let (mut player, mut target, mut transform) = actors(&motion, Vec3::Y * 10.);
    player.health = 1.;
    target.body.health = 1.;
    let hit = content.hit_spec(Attack::CounterRetaliation, 1).unwrap();
    hit_pair(content, &mut player, &mut target, &hit, Vec3::Z);
    assert_eq!(target.reaction, Reaction::Dead);
    assert!(!target.solid(0));
    target.last_hit = Some((10, 1));
    target.step(
        content,
        &CollisionWorld::default(),
        &motion,
        &mut transform,
        2.9,
    );
    assert!(!target.respawn_if_ready(&mut transform));
    target.step(
        content,
        &CollisionWorld::default(),
        &motion,
        &mut transform,
        0.11,
    );
    assert!(target.respawn_if_ready(&mut transform));
    assert_eq!(target.health(), 100.);
    assert_eq!(transform.translation, target.home);
    assert_eq!(target.body.velocity(), Vec3::ZERO);
    assert_eq!(target.last_hit, None);
    assert_eq!(target.reaction, Reaction::Idle);
}

#[test]
fn stun_boundaries_and_counter_damage_poses_match_player() {
    let content = &content();
    let motion = Motion {
        gravity: 0.,
        ..default()
    };
    let (mut player, mut target, mut transform) = actors(&motion, Vec3::Y * 10.);
    player.receive_counter_stun(2.);
    target.counter_stun(2.);
    let world = CollisionWorld::default();
    tick_pair(
        content,
        &world,
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        0.7,
    );
    assert_eq!(target.animation_action(), "stun_start");
    tick_pair(
        content,
        &world,
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        0.001,
    );
    assert_eq!(target.animation_action(), "stun_loop");
    tick_pair(
        content,
        &world,
        &motion,
        &mut player,
        &mut target,
        &mut transform,
        1.3,
    );
    assert_eq!(target.reaction, Reaction::Idle);
    target.fighter = Some(super::PracticeFighter::new(super::SparringMode::Attack));
    let hit = content.hit_spec(Attack::CounterCombo(0), 1).unwrap();
    for (facing, action) in [
        (Vec3::NEG_Z, "counter_damage_0"),
        (Vec3::Z, "counter_damage_1"),
        (Vec3::X, "counter_damage_2"),
        (Vec3::NEG_X, "counter_damage_3"),
    ] {
        target.receive(content, &hit, facing);
        assert_eq!(target.animation_action(), action);
    }
}
