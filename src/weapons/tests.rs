use crate::content::simulation::parity_content;

#[test]
fn strike_snapshot_survives_combat_interruption_in_the_same_tick() {
    use crate::simulation::{SimulationPlugin, SimulationSet};
    let motion = crate::gameplay::Motion::default();
    let mut player = Player::new(Vec3::new(1., 2., 3.), &motion);
    player.melee.equipped = Weapon::PlasmaSword;
    player.melee.tick(
        parity_content(),
        crate::melee::MeleeInput {
            primary_pressed: true,
            primary_held: true,
            ..default()
        },
        crate::melee::MeleeContext {
            grounded: true,
            ..default()
        },
        0.01,
        100.,
    );
    player.melee.tick(
        parity_content(),
        crate::melee::MeleeInput {
            primary_released: true,
            ..default()
        },
        crate::melee::MeleeContext {
            grounded: true,
            ..default()
        },
        0.01,
        100.,
    );
    let mut app = App::new();
    app.insert_resource(
        crate::content::simulation::load_simulation(std::path::Path::new("assets"))
            .unwrap()
            .0,
    );
    app.add_plugins(SimulationPlugin)
        .insert_resource(player)
        .init_resource::<StrikeEvents>()
        .init_resource::<ActorAttackFrames>();
    app.add_systems(
        FixedUpdate,
        (|mut player: ResMut<Player>| {
            player.receive_counter_stun(3.);
            player.feet = Vec3::splat(50.);
        })
        .in_set(SimulationSet::ResolveCombat),
    );
    install_simulation(&mut app);
    app.add_systems(
        FixedUpdate,
        (|mut player: ResMut<Player>| {
            player.melee.tick(
                parity_content(),
                default(),
                crate::melee::MeleeContext {
                    grounded: true,
                    ..default()
                },
                0.25,
                100.,
            );
        })
        .in_set(SimulationSet::Move),
    );
    app.world_mut().run_schedule(FixedUpdate);
    let events = app.world().resource::<StrikeEvents>();
    assert_eq!(events.pending.len(), 1);
    assert_eq!(events.pending[0].attack, Attack::PlasmaWeak(0));
    assert_eq!(events.pending[0].origin.translation, Vec3::new(1., 2., 3.));
    let player = app.world().resource::<Player>();
    assert!(player.melee.attack.is_none());
    assert_eq!(player.feet, Vec3::splat(50.));
}
use super::*;

#[test]
fn counter_sword_keeps_separate_hand_carriers_and_only_its_registered_trail() {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let counter = &manifest.weapons["counter"];
    assert_eq!(counter.parts.len(), 2);
    let right = counter.trail_part(Some("right"));
    let left = counter.trail_part(Some("left"));
    assert_ne!(right.attach_bone, left.attach_bone);
    assert!(!right.meshes.is_empty());
    assert!(!left.meshes.is_empty());
    let motion = gameplay::Motion::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.melee.equipped = Weapon::CounterSword;
    player.grounded = false;
    assert!(equipment_holstered(&player, &motion, right));
    assert!(
        !equipment_holstered(&player, &motion, left),
        "Forearm flag1 must keep the left carrier equipped in the air"
    );
    player.melee.attack = Some(Attack::CounterJump);
    assert!(!equipment_holstered(&player, &motion, right));
    let left_animated = left
        .mesh_animations
        .values()
        .flat_map(|clip| &clip.meshes)
        .flat_map(|mesh| &mesh.frames)
        .filter(|frame| !frame.normals.is_empty())
        .count();
    assert!(
        left_animated > 0,
        "The locally skinned left-hand carrier must retain animated normals"
    );
    for attack in [
        Attack::CounterCombo(0),
        Attack::CounterCombo(1),
        Attack::CounterCombo(2),
        Attack::CounterCombo(3),
        Attack::CounterRetaliation,
    ] {
        let trails: Vec<_> = manifest
            .action_calls(attack)
            .filter(|call| call.method == "AddGhostTrail")
            .collect();
        assert_eq!(trails.len(), 1);
        assert_eq!(trails[0].arguments[0], "GhostTrail_counter_sword_01");
    }
    assert!(
        manifest
            .action_calls(Attack::CounterGuard)
            .all(|call| call.method != "AddGhostTrail")
    );
    let jump_cues: Vec<_> = manifest
        .action_calls(Attack::CounterJump)
        .filter(|call| call.method == "AddSequence")
        .collect();
    assert!(jump_cues.iter().any(|call| call.arguments[1]
        == "Resources/Effects/weapon_counter_upper01.seq"
        && call.arguments[3] == 500.));
}

#[test]
fn locally_skinned_weapon_normals_follow_the_mesh_frame_clock() {
    let mesh: AnimatedMesh = serde_json::from_value(serde_json::json!({
        "name": "weighted_hand", "texture": "", "indices": [], "uvs": [],
        "frames": [
            { "tick": 0., "alpha": 1., "positions": [[0., 0., 0.]], "normals": [[1., 0., 0.]] },
            { "tick": 100., "alpha": 1., "positions": [[1., 0., 0.]], "normals": [[0., 1., 0.]] }
        ]
    }))
    .unwrap();
    let normal = Vec3::from(sample_mesh_normals(&mesh, 50.).unwrap()[0]);
    assert!((normal.length() - 1.).abs() < 1e-6);
    assert!((normal.x - normal.y).abs() < 1e-6);
    assert_eq!(sample_mesh(&mesh, 50.).0[0], [0.5, 0., 0.]);
}

#[test]
fn source_stun_billboard_keeps_area_facing_camera_when_head_turns() {
    let reactions: serde_json::Value =
        serde_json::from_slice(&std::fs::read("assets/melee/reactions.json").unwrap()).unwrap();
    let stars: AnimatedMesh =
        serde_json::from_value(reactions["effects"]["models"]["stun3_b"]["meshes"][0].clone())
            .unwrap();
    assert_eq!(
        reactions["effects"]["sequences"]["stun"]["components"][2]["self_transform_sources"],
        serde_json::json!([0, 1, 0])
    );
    assert_eq!(
        reactions["effects"]["sequences"]["stun"]["components"][3]["self_transform_sources"],
        serde_json::json!([0, 0, 1])
    );
    assert_eq!(stars.shader_flags & (16 | 32 | 2), 16 | 2);
    let camera = Transform::from_rotation(Quat::from_rotation_y(0.7));
    for angle in [0., 1., 2., 3.] {
        let origin = Mat4::from_scale_rotation_translation(
            Vec3::splat(1.3),
            Quat::from_rotation_y(angle),
            Vec3::new(2., 3., 4.),
        );
        let points = effect_mesh_positions(&stars.frames[0].positions, origin, &camera, true);
        let a = Vec3::from(points[1]) - Vec3::from(points[0]);
        let b = Vec3::from(points[2]) - Vec3::from(points[0]);
        assert!(a.cross(b).normalize().dot(*camera.forward()).abs() > 0.999);
        assert!((a.length() - 0.180091 * 1.3).abs() < 0.0001);
    }
}

#[test]
fn stun_self_controls_keep_orbit_upright_and_stars_at_authored_scale() {
    let mut manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let reactions: serde_json::Value =
        serde_json::from_slice(&std::fs::read("assets/melee/reactions.json").unwrap()).unwrap();
    let effects: EffectManifest = serde_json::from_value(reactions["effects"].clone()).unwrap();
    manifest.effects.sequences.extend(effects.sequences);
    manifest.effects.models.extend(effects.models);
    let mut sequence = LiveSequence {
        id: 0,
        name: "stun".into(),
        age: 0.4,
        origin: Transform::from_scale(Vec3::splat(1.3)),
        serial: None,
        remove_on_exit: false,
        carry: vec![],
        owner: Some(Entity::PLACEHOLDER),
        bone_poses: HashMap::from([(
            "Bip01 Head".into(),
            Mat4::from_rotation_translation(Quat::from_rotation_x(1.2), Vec3::Y * 3.),
        )]),
    };
    let animation = Animation::load(vec![]).unwrap();
    let orbit = component_origin(&manifest, &sequence, 2, &Transform::default(), &animation);
    let (scale, rotation, position) = orbit.to_scale_rotation_translation();
    assert!(rotation.abs_diff_eq(Quat::IDENTITY, 0.0001));
    assert!(scale.abs_diff_eq(Vec3::splat(1.3), 0.0001));
    assert!((position.y - 3.9).abs() < 0.0001);
    for age in [0.1, 0.4, 0.8] {
        sequence.age = age;
        for index in [3, 5, 7] {
            let star = component_origin(
                &manifest,
                &sequence,
                index,
                &Transform::default(),
                &animation,
            );
            let (scale, _, position) = star.to_scale_rotation_translation();
            assert!(scale.abs_diff_eq(Vec3::ONE, 0.0001));
            assert!(position.y > 4.5);
        }
    }
}

#[test]
fn dagger_loader_fades_after_its_end_and_reactions_have_source_sequences() {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let loader = manifest.effects.sequences["shortknife01"]
        .components
        .iter()
        .find(|c| c.asset_model.is_some())
        .unwrap();
    assert_eq!((loader.fade_in_ms, loader.fade_out_ms), (100, 100));
    assert_eq!(loader.opacity(200.), 0.);
    assert_eq!(loader.opacity(300.), 1.);
    assert_eq!(loader.opacity(900.), 1.);
    assert_eq!(loader.opacity(950.), 0.5);
    assert_eq!(loader.opacity(1000.), 0.);
    let reactions: serde_json::Value =
        serde_json::from_slice(&std::fs::read("assets/melee/reactions.json").unwrap()).unwrap();
    assert!(
        reactions["effects"]["sequences"]["stun"]["components"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    assert_eq!(
        reactions["effects"]["sequences"]["dead_eff"]["duration_ms"],
        5000
    );
}

#[test]
#[ignore = "manual impact-path profiling over source map walls"]
fn counter_heavy_wall_timing() {
    use std::time::Instant;
    let mut melee = crate::melee::Melee::default();
    melee.equipped = Weapon::CounterSword;
    melee.attack = Some(Attack::CounterRetaliation);
    melee.tick(
        parity_content(),
        default(),
        crate::melee::MeleeContext {
            grounded: true,
            ..default()
        },
        0.36,
        100.,
    );
    let hit = melee
        .hit_specs(parity_content())
        .into_iter()
        .find(|hit| !hit.critical)
        .unwrap();
    let impact = crate::content::simulation::load_simulation(std::path::Path::new("assets"))
        .unwrap()
        .1;
    for id in scene::MapId::ALL {
        let map = scene::read_map_for(id).unwrap();
        let level = scene::make_level(&map);
        let samples: Vec<_> = map
            .meshes
            .iter()
            .filter(|mesh| mesh.collision)
            .flat_map(|mesh| {
                mesh.indices.chunks_exact(3).filter_map(|indices| {
                    let [a, b, c] = [
                        Vec3::from(mesh.positions[indices[0] as usize]),
                        Vec3::from(mesh.positions[indices[1] as usize]),
                        Vec3::from(mesh.positions[indices[2] as usize]),
                    ];
                    let normal = (b - a).cross(c - a).normalize_or_zero();
                    if normal.length_squared() < 0.9 || normal.y.abs() > 0.5 {
                        return None;
                    }
                    let facing = -normal.with_y(0.).normalize();
                    Some(((a + b + c) / 3. - facing * 3. - Vec3::Y * 2.5, facing))
                })
            })
            .take(128)
            .collect();
        let mut times = Vec::new();
        let mut hits = 0;
        let mut miss_max_us = 0;
        for (feet, facing) in samples {
            let start = Instant::now();
            if melee_surface_hit(&level.collision, &hit, feet, facing).is_some() {
                std::hint::black_box(impact.attack_shake(Attack::CounterRetaliation).unwrap());
                hits += 1;
            }
            times.push(start.elapsed().as_micros());
            let start = Instant::now();
            assert!(
                melee_surface_hit(&level.collision, &hit, Vec3::splat(100_000.), facing).is_none()
            );
            miss_max_us = miss_max_us.max(start.elapsed().as_micros());
        }
        assert!(
            hits > 0,
            "Source wall benchmark must exercise real contacts"
        );
        times.sort_unstable();
        println!(
            "heavy walls {}: samples={} contacts={} p50_us={} p95_us={} max_us={} miss_max_us={miss_max_us}",
            id.id(),
            times.len(),
            hits,
            times[times.len() / 2],
            times[times.len() * 95 / 100],
            times.last().unwrap()
        );
    }
}

#[test]
fn surface_hit_finds_wall_in_attack_shape_and_rejects_out_of_range() {
    let world = crate::controller::CollisionWorld::new(vec![
        [
            Vec3::new(-20., -20., 2.),
            Vec3::new(20., -20., 2.),
            Vec3::new(20., 20., 2.),
        ],
        [
            Vec3::new(-20., -20., 2.),
            Vec3::new(20., 20., 2.),
            Vec3::new(-20., 20., 2.),
        ],
    ]);
    let mut melee = crate::melee::Melee::default();
    melee.equipped = Weapon::PlasmaSword;
    melee.tick(
        parity_content(),
        crate::melee::MeleeInput {
            primary_pressed: true,
            primary_released: true,
            ..default()
        },
        crate::melee::MeleeContext {
            grounded: true,
            ..default()
        },
        0.016,
        100.,
    );
    melee.tick(
        parity_content(),
        default(),
        crate::melee::MeleeContext {
            grounded: true,
            ..default()
        },
        0.21,
        100.,
    );
    let hit = melee.hit_spec(parity_content()).unwrap();
    assert!(melee_surface_hit(&world, &hit, Vec3::ZERO, Vec3::Z).is_some());
    assert!(melee_surface_hit(&world, &hit, Vec3::ZERO, Vec3::NEG_Z).is_none());
    assert!(melee_surface_hit(&world, &hit, Vec3::NEG_Z * 40., Vec3::Z).is_none());
}

#[test]
fn weapon_switch_uses_source_fade_and_three_authored_slots() {
    assert_eq!(switch_opacity(0.), 0.);
    assert!((switch_opacity(0.1) - 0.5).abs() < 0.001);
    assert_eq!(switch_opacity(0.2), 1.);
    assert_eq!(switch_opacity(0.999), 1.);
    assert!((switch_opacity(1.1) - 0.5).abs() < 0.001);
    assert_eq!(switch_opacity(1.2), 0.);
    let layout: SwitchLayout =
        serde_json::from_slice(&std::fs::read("assets/ui/weapon-switch.json").unwrap()).unwrap();
    assert_eq!(layout.slots.len(), 3);
    assert_eq!(layout.slots[0].rect, [327., 467., 128., 128.]);
    assert_eq!(layout.slots[2].rect, [567., 467., 128., 128.]);
}

#[test]
fn selected_plasma_uses_source_movement_holster_and_draws_for_attacks() {
    let motion = gameplay::Motion::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.grounded = true;
    player.melee.equipped = Weapon::PlasmaSword;
    player.action = gameplay::MovementAction::Sprint;
    assert!(plasma_holstered(&player, &motion));
    player.action = gameplay::MovementAction::None;
    assert!(!plasma_holstered(&player, &motion));
    for action in [
        gameplay::MovementAction::DodgeLeft,
        gameplay::MovementAction::DodgeRight,
        gameplay::MovementAction::WallJump,
    ] {
        player.action = action;
        assert!(plasma_holstered(&player, &motion));
    }
    player.action = gameplay::MovementAction::None;
    player.grounded = false;
    assert!(plasma_holstered(&player, &motion));
    player.grounded = true;
    player.landing_age = 0.;
    assert!(plasma_holstered(&player, &motion));
    player.action = gameplay::MovementAction::Sprint;
    player.melee.attack = Some(Attack::PlasmaWeak(0));
    assert!(!plasma_holstered(&player, &motion));
    player.melee.equipped = Weapon::Dagger;
    player.melee.attack = None;
    assert!(!plasma_holstered(&player, &motion));
}

#[test]
fn trail_history_has_one_white_head_and_uses_global_emission_fade() {
    assert_eq!(trail_uv(0, 5, 0), [1., 1.]);
    assert_eq!(trail_uv(4, 5, 1), [0., 0.]);
    assert_eq!(trail_uv(2, 5, 0), [0.5, 1.]);
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let source = &manifest.ghosttrails["GhostTrail_two_hand_sword_01"];
    let trail = LiveTrail {
        name: "GhostTrail_two_hand_sword_01".into(),
        weapon: "plasma".into(),
        serial: 1,
        owner: None,
        start: 0.05,
        end: 0.35,
        emission_start: 5.05,
        emission_end: 5.35,
        samples: Vec::new(),
    };
    assert!(
        (trail_alpha(source, &trail, 5.2, 0.) - 1.).abs() < 1e-5,
        "A fresh point in an active trail must not fade in again"
    );
    assert!((trail_alpha(source, &trail, 5.2, 100.) - 0.5).abs() < 1e-5);
    assert!((trail_alpha(source, &trail, 5.4, 40.) - 0.4).abs() < 1e-5);
    assert_eq!(trail_alpha(source, &trail, 5.5, 150.), 0.);
    assert!(source.texture.ends_with("resources_image_gi.png"));
    assert!(
        manifest.ghosttrails["GhostTrail_two_hand_sword_02"]
            .texture
            .ends_with("resources_image_gi2.png")
    );
}

#[test]
fn ordinary_weak_plasma_variants_register_one_slash_and_no_voice() {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    for variant in 0..3 {
        assert!(
            !manifest
                .action_calls(Attack::PlasmaWeak(variant))
                .any(|call| call.method == "AddSoundBySex")
        );
        let target = format!("({})", attribute_key(Attack::PlasmaWeak(variant), 0));
        let cues: Vec<_> = manifest
            .effects
            .attribute_calls
            .iter()
            .filter(|call| call.target.ends_with(&target) && call.method == "SetAttackSequence")
            .collect();
        assert_eq!(cues.len(), 1);
        assert_eq!(
            asset_stem(cues[0].arguments[1].as_str().unwrap()),
            "sword_s"
        );
    }
}

#[test]
fn dagger_poke_silences_only_the_two_authored_attack_cues() {
    assert_eq!(
        (0..3)
            .map(|i| attack_sound_enabled(Attack::DaggerDash, i))
            .collect::<Vec<_>>(),
        [false, true, false]
    );
    assert!((0..3).all(|i| attack_sound_enabled(Attack::PlasmaDash, i)));
}

#[test]
fn rush_effect_keeps_bone_position_but_faces_the_actor_at_every_heading() {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let sources =
        manifest.effects.sequences["skill_swordman_battack"].components[0].actor_transform_sources;
    assert_eq!(sources, Some([0, 1, 0]));
    for yaw in [0., 0.7, 1.57, 3.14, 4.7] {
        let root = Mat4::from_rotation_y(yaw);
        let bone = root
            * Mat4::from_rotation_translation(
                Quat::from_rotation_y(2.1) * Quat::from_rotation_x(0.3),
                Vec3::new(0.2, 1.6, 0.1),
            );
        let result = apply_transform_sources(bone, root, sources);
        assert!(
            result
                .transform_point3(Vec3::ZERO)
                .distance(bone.transform_point3(Vec3::ZERO))
                < 1e-5
        );
        assert!(
            result
                .transform_vector3(Vec3::Z)
                .normalize()
                .distance(root.transform_vector3(Vec3::Z))
                < 1e-5
        );
    }
}

#[test]
fn jump_uses_weapon_state_impact_and_an_animated_sequence_helper() {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let impact = manifest
        .action_calls(Attack::PlasmaJump)
        .find(|call| call.method == "AddSequence")
        .unwrap();
    assert_eq!(
        impact.arguments[1],
        "Resources/Effects/skill_swordman_strike.seq"
    );
    assert_eq!(impact.arguments[2], "Resources/Sound/sword_power.ogg");
    assert_eq!(impact.arguments[3], 500.);
    let sequence = &manifest.effects.sequences["skill_swordman_strike"];
    assert_eq!(sequence.components[0].flags, [1, 1]);
    let helper = &manifest.effects.models["skill_swordman_gager_dummy01"].node_frames["dummy"];
    let helper_matrix = sample_node(helper, 2000.);
    assert!(
        helper_matrix
            .transform_point3(Vec3::ZERO)
            .distance(Vec3::new(0.0125088, 0., 1.3032251))
            < 1e-5
    );
    let animation = Animation::load(Vec::new()).unwrap();
    let live = LiveSequence {
        id: 0,
        owner: None,
        bone_poses: HashMap::new(),
        name: "skill_swordman_strike".into(),
        age: 0.4,
        origin: Transform::default(),
        serial: Some(1),
        remove_on_exit: false,
        carry: Vec::new(),
    };
    for yaw in [0., 1.2, 3.1, 4.9] {
        let actor = Transform::from_rotation(Quat::from_rotation_y(yaw));
        let expected = actor.to_matrix() * helper_matrix;
        for component in [0, 2, 3, 4] {
            assert!(
                component_origin(&manifest, &live, component, &actor, &animation)
                    .abs_diff_eq(expected, 1e-5)
            );
        }
    }
}

#[test]
fn counter_strike_queue_preserves_critical_and_both_volume_hits_crossed_in_one_physics_tick() {
    let motion = gameplay::Motion::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.melee.equipped = Weapon::CounterSword;
    player.melee.attack = Some(Attack::CounterCombo(0));
    let context = crate::melee::MeleeContext {
        grounded: true,
        ..default()
    };
    player
        .melee
        .tick(parity_content(), default(), context, 0.4, 100.);
    let mut app = App::new();
    app.insert_resource(
        crate::content::simulation::load_simulation(std::path::Path::new("assets"))
            .unwrap()
            .0,
    );
    app.insert_resource(player)
        .init_resource::<StrikeEvents>()
        .add_systems(FixedUpdate, capture_strikes);
    app.world_mut().run_schedule(FixedUpdate);
    let events = app.world().resource::<StrikeEvents>();
    assert_eq!(
        events
            .pending
            .iter()
            .map(|strike| strike.hit_index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    app.world_mut().resource_mut::<Player>().melee.tick(
        parity_content(),
        default(),
        context,
        0.016,
        100.,
    );
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(app.world().resource::<StrikeEvents>().pending.len(), 3);
}

#[test]
fn fixed_strikes_survive_a_render_frame_spanning_all_three_dagger_hits() {
    let motion = crate::gameplay::Motion::default();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.melee.equipped = Weapon::Dagger;
    let context = crate::melee::MeleeContext {
        grounded: true,
        ..default()
    };
    player.melee.tick(
        parity_content(),
        crate::melee::MeleeInput {
            secondary_pressed: true,
            ..default()
        },
        context,
        0.016,
        100.,
    );
    let mut app = App::new();
    app.insert_resource(
        crate::content::simulation::load_simulation(std::path::Path::new("assets"))
            .unwrap()
            .0,
    );
    app.insert_resource(player)
        .init_resource::<StrikeEvents>()
        .add_systems(FixedUpdate, capture_strikes);
    for _ in 0..80 {
        app.world_mut().resource_mut::<Player>().melee.tick(
            parity_content(),
            default(),
            context,
            0.016,
            100.,
        );
        app.world_mut().run_schedule(FixedUpdate);
    }
    let events = app.world().resource::<StrikeEvents>();
    assert_eq!(
        events
            .pending
            .iter()
            .map(|strike| strike.hit_index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    app.world_mut().resource_mut::<Player>().pose_reset_serial += 1;
    app.world_mut().run_schedule(FixedUpdate);
    assert!(app.world().resource::<StrikeEvents>().pending.is_empty());
}

#[test]
fn airborne_dagger_uses_the_registered_primary_cut_effect_and_sound_builder() {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
    let source = &manifest.builders[attack_builder(Attack::DaggerAir)];
    assert!(source.calls.iter().any(
        |call| call.method == "AddGhostTrail" && call.arguments[0] == "GhostTrail_SpyDagger_01"
    ));
    assert!(
        source
            .calls
            .iter()
            .any(|call| call.method == "AddSoundBySex"
                && call.arguments[0] == "Resources/Sound/voice_man_attack_2.ogg")
    );
    let clips: Vec<_> = source
        .calls
        .iter()
        .filter(|call| call.method == "SetAnim" && call.target.starts_with("state.GetAnimParam("))
        .map(|call| call.arguments[0].as_str().unwrap())
        .collect();
    assert_eq!(clips, ["SN013", "SN014"]);
}
