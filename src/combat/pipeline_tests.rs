use super::*;
use super::{
    attacks::AttackBatch,
    cues::{CombatCue, CombatCues},
};
use crate::content::simulation::parity_content;
use crate::{
    controller::CollisionWorld,
    melee::{HitShape, Weapon},
};

fn fixture() -> (App, Entity) {
    fixture_with_content(
        crate::content::simulation::load_simulation(std::path::Path::new("assets"))
            .unwrap()
            .0,
    )
}

fn fixture_with_content(content: crate::content::simulation::SimulationContent) -> (App, Entity) {
    let mut app = App::new();
    app.insert_resource(content);
    let mut motion = Motion::default();
    motion.gravity = 0.;
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.actor_facing = Vec3::NEG_Z;
    let mut time = Time::<Fixed>::default();
    time.advance_by(std::time::Duration::from_secs_f32(0.02));
    app.insert_resource(player)
        .insert_resource(motion)
        .insert_resource(time)
        .insert_resource(Level {
            collision: CollisionWorld::default(),
            spawn: Vec3::ZERO,
            center: Vec3::ZERO,
            min_y: -100.,
            triangle_count: 0,
            death_zones: vec![],
            local_team: 0,
            sectors: vec![],
        })
        .init_resource::<ActorSweepStart>()
        .init_resource::<CombatStats>()
        .init_resource::<CombatCues>()
        .add_systems(Update, resolve_combat);
    let position = Vec3::NEG_Z * 3.;
    let target = app
        .world_mut()
        .spawn((
            PracticeTarget::new(position, 0),
            Transform::from_translation(position),
        ))
        .id();
    (app, target)
}

fn spec(critical: bool) -> HitSpec {
    HitSpec {
        hit_index: u8::from(critical),
        range: 10.,
        damage: 10.,
        push: 0.,
        lift: 0.,
        stun: 0.,
        shape: HitShape::Cylinder {
            origin: [0., 1.6, 0.],
            radius: 10.,
            height: 4.,
        },
        attribute: if critical {
            crate::combat_types::AttackAttributeId::CounterCritical
        } else {
            crate::combat_types::AttackAttributeId::CounterAttack1
        },
        critical,
    }
}

fn guard(target: &mut PracticeTarget) {
    let mut fighter = PracticeFighter::new(SparringMode::Guard);
    fighter.melee.attack = Some(Attack::CounterGuard);
    fighter.melee.elapsed = 0.6;
    fighter.melee.phase_elapsed = 0.6;
    fighter.melee.counter_stage = 1;
    target.fighter = Some(fighter);
}

#[test]
fn incoming_interruption_precedes_outgoing_collection_in_the_real_coordinator() {
    let (mut app, entity) = fixture();
    {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.melee.equipped = Weapon::PlasmaSword;
        player.melee.attack = Some(Attack::PlasmaWeak(0));
        player.melee.elapsed = 0.21;
        assert!(
            !attacks::collect(parity_content(), &player.melee)
                .unwrap()
                .specs
                .is_empty()
        );
    }
    {
        let mut target = app.world_mut().get_mut::<PracticeTarget>(entity).unwrap();
        let mut fighter = PracticeFighter::new(SparringMode::Attack);
        fighter.melee.attack = Some(Attack::CounterCombo(0));
        fighter.melee.elapsed = 0.34;
        fighter.melee.phase_elapsed = 0.34;
        target.fighter = Some(fighter);
    }
    app.update();
    let player = app.world().resource::<Player>();
    assert!(player.health < 100.);
    assert!(player.melee.attack.is_none());
    assert_eq!(
        app.world().get::<PracticeTarget>(entity).unwrap().health(),
        100.
    );
    let stats = app.world().resource::<CombatStats>();
    assert!(stats.incoming_hits > 0);
    assert_eq!(stats.hits, 0);
    assert!(
        app.world()
            .resource::<CombatCues>()
            .0
            .iter()
            .any(|cue| matches!(cue, CombatCue::Attack(attack) if attack.owner == entity))
    );
}

#[test]
fn counter_cancels_later_receivers_but_preserves_already_collected_specs() {
    let motion = Motion::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.actor_facing = Vec3::NEG_Z;
    player.melee.attack = Some(Attack::PlasmaWeak(0));
    let mut target = PracticeTarget::new(Vec3::NEG_Z * 3., 0);
    guard(&mut target);
    let mut transform = Transform::from_translation(target.home);
    let mut critical = spec(true);
    critical.attribute = crate::combat_types::AttackAttributeId::CounterCounterAttack;
    let batch = AttackBatch {
        attack: Attack::PlasmaWeak(0),
        serial: 8,
        specs: vec![spec(false), critical],
    };
    let mut stats = CombatStats::default();
    let mut cues = vec![];
    resolution::outgoing(
        parity_content(),
        &batch,
        Entity::PLACEHOLDER,
        &mut target,
        &mut transform,
        &mut player,
        &CollisionWorld::default(),
        &mut stats,
        &mut cues,
    );
    assert_eq!(stats.countered_hits, 1);
    assert_eq!(stats.guard_crushes, 1);
    assert_eq!(stats.hits, 1);
    assert_eq!(target.health(), 90.);
    assert_eq!(target.last_hit, Some((8, 1)));
    assert!(attacks::collect(parity_content(), &player.melee).is_none());
    assert!(matches!(&cues[0], CombatCue::Effect(effect) if effect.name == "stun"));
    assert!(matches!(&cues[1], CombatCue::Effect(effect) if effect.name == "guard_crush"));
    assert!(matches!(&cues[2], CombatCue::Hit(hit) if hit.critical));
}

#[test]
fn critical_companion_is_silent_until_lethal_and_snapshots_death_cues() {
    let mut player = Player::new(Vec3::ZERO, &Motion::default());
    let mut target = PracticeTarget::new(Vec3::NEG_Z * 3., 0);
    let mut transform = Transform::from_translation(target.home);
    let mut stats = CombatStats::default();
    let mut cues = vec![];
    let mut batch = AttackBatch {
        attack: Attack::CounterCombo(0),
        serial: 1,
        specs: vec![spec(true)],
    };
    resolution::outgoing(
        parity_content(),
        &batch,
        Entity::PLACEHOLDER,
        &mut target,
        &mut transform,
        &mut player,
        &CollisionWorld::default(),
        &mut stats,
        &mut cues,
    );
    assert_eq!(target.health(), 90.);
    assert_eq!(target.reaction, Reaction::Idle);
    assert_eq!(stats.hurt_sounds, 0);
    assert_eq!(cues.len(), 1);
    assert!(matches!(&cues[0], CombatCue::Hit(hit) if hit.critical));
    resolution::outgoing(
        parity_content(),
        &batch,
        Entity::PLACEHOLDER,
        &mut target,
        &mut transform,
        &mut player,
        &CollisionWorld::default(),
        &mut stats,
        &mut cues,
    );
    assert_eq!(target.health(), 90.);
    assert_eq!(cues.len(), 1);
    batch.serial += 1;
    target.body.health = 1.;
    resolution::outgoing(
        parity_content(),
        &batch,
        Entity::PLACEHOLDER,
        &mut target,
        &mut transform,
        &mut player,
        &CollisionWorld::default(),
        &mut stats,
        &mut cues,
    );
    assert_eq!(target.health(), 0.);
    assert_eq!(target.reaction, Reaction::Dead);
    assert_eq!(stats.death_sounds, 1);
    assert!(matches!(&cues[1], CombatCue::ClearVoices(_)));
    assert!(matches!(
        &cues[2],
        CombatCue::Voice {
            name: "_eu_voice_man_dead_normal",
            ..
        }
    ));
    assert!(
        matches!(&cues[3], CombatCue::Hit(hit) if hit.position == transform.translation + Vec3::Y * 1.6)
    );
    target.prepare_passive(Vec3::X * 100.);
    assert!(matches!(
        &cues[2],
        CombatCue::Voice {
            name: "_eu_voice_man_dead_normal",
            ..
        }
    ));
}

#[test]
fn occlusion_does_not_consume_a_hit_but_guard_does() {
    let mut player = Player::new(Vec3::ZERO, &Motion::default());
    let mut target = PracticeTarget::new(Vec3::NEG_Z * 3., 0);
    guard(&mut target);
    let mut transform = Transform::from_translation(target.home);
    let wall = CollisionWorld::new(vec![
        [
            Vec3::new(-20., -20., -1.5),
            Vec3::new(20., -20., -1.5),
            Vec3::new(20., 20., -1.5),
        ],
        [
            Vec3::new(-20., -20., -1.5),
            Vec3::new(20., 20., -1.5),
            Vec3::new(-20., 20., -1.5),
        ],
    ]);
    let mut stats = CombatStats::default();
    let mut cues = vec![];
    let batch = AttackBatch {
        attack: Attack::CounterCombo(0),
        serial: 42,
        specs: vec![spec(false)],
    };
    resolution::outgoing(
        parity_content(),
        &batch,
        Entity::PLACEHOLDER,
        &mut target,
        &mut transform,
        &mut player,
        &wall,
        &mut stats,
        &mut cues,
    );
    assert!(target.last_hit.is_none());
    assert!(cues.is_empty());
    for _ in 0..2 {
        resolution::outgoing(
            parity_content(),
            &batch,
            Entity::PLACEHOLDER,
            &mut target,
            &mut transform,
            &mut player,
            &CollisionWorld::default(),
            &mut stats,
            &mut cues,
        );
    }
    assert_eq!(target.last_hit, Some((42, 0)));
    assert_eq!(stats.countered_hits, 1);
    assert_eq!(target.health(), 100.);
    assert_eq!(cues.len(), 1);
}

#[test]
fn respawn_and_reset_cleanup_run_without_playback_assets() {
    let (mut app, entity) = fixture_with_content(crate::content::simulation::synthetic().0);
    {
        let mut target = app.world_mut().get_mut::<PracticeTarget>(entity).unwrap();
        target.body.health = 0.;
        target.set_reaction(Reaction::Dead);
        target.body.death_age = Some(2.99);
    }
    app.update();
    assert_eq!(
        app.world().get::<PracticeTarget>(entity).unwrap().health(),
        100.
    );
    assert!(matches!(app.world().resource::<CombatCues>().0.as_slice(),
        [CombatCue::ClearVoices(owner)] if *owner == entity));
    app.world_mut().resource_mut::<CombatCues>().0.clear();
    app.world_mut().resource_mut::<Player>().pose_reset_serial += 1;
    let label = app.world_mut().spawn(PracticeStatus).id();
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    assert!(app.world().get_entity(label).is_err());
    assert!(app.world().resource::<CombatCues>().0.is_empty());
}

#[test]
fn incoming_counter_waits_until_all_crossed_attack_cues_are_captured() {
    let motion = Motion::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.actor_facing = Vec3::NEG_Z;
    let mut defender = PracticeTarget::new(Vec3::ZERO, 0);
    guard(&mut defender);
    player.melee = defender.fighter.take().unwrap().melee;
    let mut target = PracticeTarget::new(Vec3::NEG_Z * 3., 0);
    target.fighter = Some(PracticeFighter::new(SparringMode::Attack));
    let mut transform = Transform::from_translation(target.home);
    let mut critical = spec(true);
    critical.attribute = crate::combat_types::AttackAttributeId::PlasmaJumpCritical;
    let batch = AttackBatch {
        attack: Attack::CounterCombo(0),
        serial: 1,
        specs: vec![spec(false), critical],
    };
    let mut stats = CombatStats::default();
    let mut cues = vec![];
    resolution::incoming(
        parity_content(),
        &batch,
        Entity::PLACEHOLDER,
        &mut target,
        &mut transform,
        &mut player,
        &CollisionWorld::default(),
        &mut stats,
        &mut cues,
    );
    assert_eq!(stats.countered_hits, 1);
    assert_eq!(stats.guarded_hits, 1);
    assert_eq!(player.health, 100.);
    assert_eq!(target.stun, 3.);
    assert_eq!(target.reaction, Reaction::StunStart);
    assert_eq!(cues.len(), 2);
    assert!(cues.iter().all(|cue| matches!(cue, CombatCue::Attack(_))));
}

#[test]
fn sparring_root_motion_is_swept_against_player_body() {
    let content = parity_content();
    let motion = Motion {
        gravity: 0.,
        movement_skin_fraction: 0.,
        ..default()
    };
    let world = CollisionWorld::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    let mut target = PracticeTarget::new(Vec3::NEG_Z * 2.5, 0);
    let mut fighter = PracticeFighter::new(SparringMode::Attack);
    fighter.melee.attack = Some(Attack::CounterCombo(0));
    fighter.melee.elapsed = 0.1;
    fighter.melee.phase_elapsed = 0.1;
    target.fighter = Some(fighter);
    let mut transform = Transform::from_translation(target.home);
    let start = ActorSweepStart {
        feet: player.feet,
        reset_serial: 0,
    };
    contact::step_target(
        content,
        &mut target,
        &mut transform,
        &mut player,
        &motion,
        &world,
        &start,
        0.2,
    );
    assert!(transform.translation.z > -2.5, "attack must try to advance");
    let axes = Vec3::from(motion.collision_axis_scales);
    assert!(((transform.translation - player.feet) / (axes * 2.)).length() >= 0.9999);
    assert_eq!(target.body.feet, transform.translation);
}

#[test]
fn real_coordinator_launches_dummy_and_spar_with_both_heavies() {
    for sparring in [false, true] {
        for (weapon, attack, at, lift) in [
            (Weapon::CounterSword, Attack::CounterRetaliation, 0.36, 30.),
            (Weapon::PlasmaSword, Attack::PlasmaStrong, 0.41, 20.),
        ] {
            let (mut app, entity) = fixture();
            if sparring {
                app.world_mut()
                    .get_mut::<PracticeTarget>(entity)
                    .unwrap()
                    .fighter = Some(PracticeFighter::new(SparringMode::Attack));
            }
            {
                let mut player = app.world_mut().resource_mut::<Player>();
                player.melee.equipped = weapon;
                player.melee.attack = Some(attack);
                player.melee.elapsed = at;
                player.melee.phase_elapsed = at;
            }
            app.update();
            let target = app.world().get::<PracticeTarget>(entity).unwrap();
            assert_eq!(target.reaction, Reaction::BlowStart);
            assert_eq!(target.body.velocity(), Vec3::new(0., lift, -30.));
            assert!(!target.body.grounded);
            assert!(target.health() < 100.);
            if let Some(fighter) = &target.fighter {
                assert!(fighter.melee.attack.is_none());
            }
            let initial = app.world().get::<Transform>(entity).unwrap().translation;
            app.world_mut().resource_mut::<Player>().melee.reset();
            app.update();
            let position = app.world().get::<Transform>(entity).unwrap().translation;
            assert!(position.y > initial.y && position.z < initial.z);
        }
    }
}

#[test]
fn practice_actor_uses_player_death_volumes_and_cancels_sparring() {
    let (mut app, entity) = fixture_with_content(crate::content::simulation::synthetic().0);
    let map: scene::Map = serde_json::from_value(serde_json::json!({
        "format_version":1, "units":"meters", "meshes":[],
        "bounds":{"min":[-100,-100,-100],"max":[100,100,100]},
        "boxes":[{"name":"alpha_limited_area", "size":[1,2,1],
            "matrix":[1,0,0,0,0,1,0,0,0,0,1,0,0,160,300,1]}]
    }))
    .unwrap();
    app.world_mut().resource_mut::<Level>().death_zones = scene::make_level(&map).death_zones;
    app.world_mut()
        .get_mut::<PracticeTarget>(entity)
        .unwrap()
        .fighter = Some(PracticeFighter::new(SparringMode::CounterHeavy));
    app.update();
    let target = app.world().get::<PracticeTarget>(entity).unwrap();
    assert_eq!(target.health(), 0.);
    assert_eq!(target.reaction, Reaction::Dead);
    assert!(!target.solid(0));
    assert!(target.fighter.as_ref().unwrap().melee.attack.is_none());
    assert!(app.world().resource::<CombatCues>().0.iter().any(|cue| matches!(cue, CombatCue::Voice { owner, name: "_eu_voice_man_dead_normal" } if *owner == entity)));
}
