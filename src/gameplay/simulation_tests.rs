use super::*;
use crate::content::simulation::parity_content;
use crate::{gameplay::Controls, melee::Weapon, scene};

const DT: f32 = 1.0 / 240.0;

#[test]
fn god_mode_preserves_sp_until_disabled() {
    let motion = state_motion();
    let mut player = Player::new(Vec3::ZERO, &motion);
    player.god_mode = true;
    for cost in [
        motion.dodge_cost,
        motion.walljump_cost,
        motion.sprint_sp_per_second * DT,
        20.0,
    ] {
        player.spend_sp(cost);
        assert_eq!(player.sp, motion.max_sp);
    }
    player.god_mode = false;
    player.spend_sp(20.0);
    assert_eq!(player.sp, motion.max_sp - 20.0);
}

#[test]
fn empty_sp_wall_jump_enters_attempt_then_fails_once_without_launch() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    let mut player = Player::new(
        Vec3::new(-motion.collision_axis_scales[0] - 0.025, 2., 0.),
        &motion,
    );
    approach_test_wall(&mut player, &world, &motion);
    player.sp = 0.;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            jump: true,
            yaw: std::f32::consts::FRAC_PI_2,
            ..default()
        },
        DT,
    );
    assert_eq!(player.walljump_phase, WallJumpPhase::Windup);
    let serial = player.action_serial;
    finish_test_walljump_windup(&mut player, &world, &motion, std::f32::consts::FRAC_PI_2);
    assert_eq!(player.walljump_phase, WallJumpPhase::Failed);
    assert_eq!(player.wall_jumps, 0);
    assert_eq!(player.wall_jump_failures, 1);
    assert_eq!(player.action_serial, serial);
    for _ in 0..10 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    assert_eq!(player.wall_jump_failures, 1);
    assert!(player.sp < motion.walljump_cost);
}

fn state_motion() -> Motion {
    Motion {
        movement_skin_fraction: 0.0,
        ..default()
    }
}

#[test]
fn native_16ms_free_run_sprint_and_diagonals_match_original_steps() {
    let motion = Motion::default();
    let world = CollisionWorld::new(floor());
    for (movement, sprint, expected_step) in [
        (Vec2::Y, false, 0.161),
        (Vec2::Y, true, 0.249),
        (-Vec2::Y, false, 0.1434),
        (Vec2::new(1.0, 1.0), false, 0.1522),
        (Vec2::new(1.0, -1.0), false, 0.1522),
    ] {
        let mut player = standing(&world, &motion, 0.0);
        let before = player.feet;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement,
                sprint,
                ..default()
            },
            0.016,
        );
        let step = Vec2::new(player.feet.x - before.x, player.feet.z - before.z).length();
        assert!(
            (step - expected_step).abs() < 0.000_01,
            "{movement:?}/{sprint}: {step} vs {expected_step}"
        );
        if movement.x != 0.0 {
            assert!((player.feet.x.abs() - player.feet.z.abs()).abs() < 0.000_01);
        }
    }
}

#[test]
fn smooth_run_and_sprint_keep_original_60hz_speed_at_any_rate() {
    let motion = Motion {
        smooth_physics: true,
        ..default()
    };
    let world = CollisionWorld::new(floor());
    for hz in [1.0_f32, 30.0, 60.0, 62.5, 120.0, 179.82, 240.0, 1000.0] {
        for (movement, sprint, expected_speed) in [
            (Vec2::Y, false, 10.1),
            (Vec2::Y, true, 15.6),
            (-Vec2::Y, false, 9.0),
            (Vec2::new(1.0, 1.0), false, 9.55),
            (Vec2::new(1.0, -1.0), false, 9.55),
        ] {
            let mut player = standing(&world, &motion, 0.0);
            let start = player.feet;
            let mut elapsed = 0.0_f32;
            while elapsed < 1.0 - 1e-6 {
                let dt = (1.0 / hz).min(1.0 - elapsed);
                player.step(
                    parity_content(),
                    &world,
                    &motion,
                    MovementInput {
                        movement,
                        sprint,
                        ..default()
                    },
                    dt,
                );
                elapsed += dt;
            }
            let distance = (player.feet - start).with_y(0.0).length();
            assert!(
                (distance - expected_speed).abs() < 0.001,
                "{hz} Hz {movement:?}/{sprint}: {distance} vs {expected_speed}"
            );
            assert!(player.grounded);
        }
    }
}

#[test]
fn smooth_jump_preserves_60hz_apex_and_midflight_height() {
    let motion = Motion {
        smooth_physics: true,
        ..default()
    };
    let world = CollisionWorld::new(floor());
    for hz in [30.0_f32, 60.0, 62.5, 120.0, 179.82, 240.0, 1000.0] {
        for movement in [Vec2::ZERO, Vec2::Y] {
            let mut player = standing(&world, &motion, 0.0);
            let start = player.feet;
            let mut elapsed = 0.0_f32;
            let mut apex = 0.0_f32;
            for target_time in [0.25_f32, 1.0] {
                while elapsed < target_time - 1e-6 {
                    let dt = (1.0 / hz).min(target_time - elapsed);
                    player.step(
                        parity_content(),
                        &world,
                        &motion,
                        MovementInput {
                            movement,
                            jump: elapsed == 0.0,
                            ..default()
                        },
                        dt,
                    );
                    elapsed += dt;
                    apex = apex.max(player.feet.y - start.y);
                }
                if target_time == 0.25 {
                    assert!(
                        (player.feet.y - start.y - 3.89).abs() < 0.001,
                        "{hz} Hz height at 250ms: {}",
                        player.feet.y - start.y
                    );
                }
            }
            assert!((apex - 4.145).abs() < 0.003, "{hz} Hz apex: {apex}");
            assert!(player.grounded, "{hz} Hz jump must land");
            assert!((player.feet.y - start.y).abs() < 0.05);
            assert_eq!(player.jumps, 1);
        }
    }
}

#[test]
fn smooth_rate_changes_midjump_preserve_the_arc_without_resetting_motion() {
    let mut motion = Motion {
        smooth_physics: true,
        ..default()
    };
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    let start_y = player.feet.y;
    for (hz, count) in [(240.0, 60), (120.0, 30), (60.0, 15)] {
        if hz == 60.0 {
            motion.smooth_physics = false;
        }
        for tick in 0..count {
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    jump: player.jumps == 0 && tick == 0,
                    ..default()
                },
                1.0 / hz,
            );
        }
        if hz == 240.0 {
            assert!((player.feet.y - start_y - 3.89).abs() < 0.001);
        }
    }
    assert_eq!(player.jumps, 1);
    assert!(player.grounded);
}

#[test]
fn disabling_smooth_restores_native_per_call_padding_and_jump() {
    let mut motion = Motion {
        smooth_physics: true,
        ..default()
    };
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::Y,
            ..default()
        },
        DT,
    );
    let before = player.feet;
    motion.smooth_physics = false;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::Y,
            ..default()
        },
        DT,
    );
    assert!(((player.feet - before).length() - (11.0 * DT - 0.015)).abs() < 1e-5);
    let start_y = player.feet.y;
    let mut apex = 0.0_f32;
    for tick in 0..60 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                jump: tick == 0,
                ..default()
            },
            1.0 / 60.0,
        );
        apex = apex.max(player.feet.y - start_y);
    }
    assert!((apex - 4.145).abs() < 0.0001);
}

#[test]
fn default_and_native_16ms_jumps_reach_their_free_apices() {
    let motion = Motion::default();
    let world = CollisionWorld::new(floor());
    for (dt, expected_apex) in [
        (1.0 / crate::DEFAULT_PHYSICS_HZ as f32, 4.145),
        (0.016, 4.1328),
    ] {
        for movement in [Vec2::ZERO, Vec2::Y] {
            let mut player = standing(&world, &motion, 0.0);
            let initial_y = player.feet.y;
            let mut apex = 0.0_f32;
            for tick in 0..50 {
                player.step(
                    parity_content(),
                    &world,
                    &motion,
                    MovementInput {
                        movement,
                        jump: tick == 0,
                        ..default()
                    },
                    dt,
                );
                apex = apex.max(player.feet.y - initial_y);
            }
            assert!(
                (apex - expected_apex).abs() < 0.0001,
                "{dt}/{movement:?}: apex{apex}"
            );
            assert!(player.grounded);
        }
    }
}

fn floor() -> Vec<[Vec3; 3]> {
    let points = [
        Vec3::new(-100.0, 0.0, -100.0),
        Vec3::new(100.0, 0.0, -100.0),
        Vec3::new(100.0, 0.0, 100.0),
        Vec3::new(-100.0, 0.0, 100.0),
    ];
    vec![
        [points[0], points[1], points[2]],
        [points[0], points[2], points[3]],
    ]
}

fn wall(x: f32) -> Vec<[Vec3; 3]> {
    let points = [
        Vec3::new(x, 0.0, -100.0),
        Vec3::new(x, 20.0, -100.0),
        Vec3::new(x, 20.0, 100.0),
        Vec3::new(x, 0.0, 100.0),
    ];
    vec![
        [points[0], points[1], points[2]],
        [points[0], points[2], points[3]],
    ]
}

fn standing(world: &CollisionWorld, motion: &Motion, x: f32) -> Player {
    let mut player = Player::new(Vec3::new(x, 0.001, 0.0), motion);
    player.step(
        parity_content(),
        world,
        motion,
        MovementInput::default(),
        DT,
    );
    assert!(player.grounded);
    player.landing_age = 10.0;
    player
}

fn take_command(controls: &mut Controls) -> MovementInput {
    MovementInput {
        movement: controls.movement,
        jump: std::mem::take(&mut controls.jump),
        dodge: std::mem::take(&mut controls.dodge),
        dodge_side: std::mem::take(&mut controls.dodge_side),
        sprint: controls.sprint,
        crouch: controls.crouch,
        yaw: controls.yaw,
        pitch: controls.pitch,
        melee: controls.melee,
        emote: controls.emote.take(),
        cancel_emote: std::mem::take(&mut controls.cancel_emote),
    }
}

#[test]
fn counter_stun_preserves_ballistics_except_the_outgoing_walljump_exit() {
    let motion = state_motion();
    let mut player = Player::new(Vec3::Y * 5., &motion);
    let velocity = Vec3::new(3., -4., 5.);
    player.velocity = velocity;
    player.melee.attack = Some(crate::melee::Attack::CounterJump);
    player.receive_counter_stun(3.);
    assert_eq!(player.velocity, velocity);
    assert_eq!(player.hit_reaction.unwrap().duration, 3.);
    player.action = MovementAction::WallJump;
    player.velocity = velocity;
    player.receive_counter_stun(3.);
    assert_eq!(player.velocity, Vec3::ZERO);
}
#[test]
fn counter_guard_walk_uses_attack_speed_and_directional_rates_in_recovery() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for (direction, rate) in [
        (Vec2::Y, 1.),
        (Vec2::X, motion.side_multiplier),
        (-Vec2::Y, motion.back_multiplier),
    ] {
        let mut player = standing(&world, &motion, 0.);
        player.melee.equipped = Weapon::CounterSword;
        let start = player.feet;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: direction,
                melee: MeleeInput {
                    secondary_held: true,
                    ..default()
                },
                ..default()
            },
            DT,
        );
        assert!(
            ((player.feet - start).with_y(0.).length() - motion.base_speed * rate * DT).abs()
                < 0.0001
        );
        let start = player.feet;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: direction,
                melee: MeleeInput {
                    secondary_released: true,
                    ..default()
                },
                ..default()
            },
            DT,
        );
        assert_eq!(player.melee.phase, crate::melee::AttackPhase::Delay);
        assert!(
            ((player.feet - start).with_y(0.).length() - motion.base_speed * rate * DT).abs()
                < 0.0001
        );
    }
}
#[test]
fn counter_guard_lower_jump_keeps_guard_and_costs_no_sp_while_dodge_exits() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.);
    player.melee.equipped = Weapon::CounterSword;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                secondary_held: true,
                ..default()
            },
            ..default()
        },
        DT,
    );
    let sp = player.sp;
    player.velocity = Vec3::new(7., -8., 9.);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            jump: true,
            melee: MeleeInput {
                secondary_held: true,
                ..default()
            },
            ..default()
        },
        DT,
    );
    assert_eq!(
        player.melee.attack,
        Some(crate::melee::Attack::CounterGuard)
    );
    assert_eq!(player.guard_jump, Some(LowerJump::enter(1.)));
    assert!(!player.jump_started_moving);
    assert_eq!(player.sp, sp);
    assert!(!player.grounded);
    assert!((player.velocity.y - (motion.jump_speed + motion.gravity * DT)).abs() < 0.001);
    assert!((player.velocity.x - motion.base_speed).abs() < 0.001);
    assert!(player.velocity.z.abs() < 0.001);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            dodge: true,
            dodge_side: 1.,
            melee: MeleeInput {
                secondary_held: true,
                ..default()
            },
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::DodgeRight);
    assert!(player.melee.attack.is_none());
    assert_eq!(player.sp, sp - motion.dodge_cost);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            melee: MeleeInput {
                secondary_held: true,
                ..default()
            },
            ..default()
        },
        DT,
    );
    assert_eq!(
        player.melee.attack,
        Some(crate::melee::Attack::CounterGuard)
    );
    assert_eq!(player.action, MovementAction::None);
    assert!(
        player.velocity.y > 0.,
        "guard interruption preserves existing rise"
    );
}
#[test]
fn counter_jump_world_velocity_moves_forward_across_ticks_then_clears_at_recovery() {
    let dt = 0.016;
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::Y * 100., &motion);
    player.melee.equipped = Weapon::CounterSword;
    let command = MovementInput {
        yaw: std::f32::consts::FRAC_PI_2,
        ..default()
    };
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                primary_pressed: true,
                primary_held: true,
                ..default()
            },
            ..command
        },
        dt,
    );
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                primary_released: true,
                ..default()
            },
            ..command
        },
        dt,
    );
    assert_eq!(player.melee.attack, Some(crate::melee::Attack::CounterJump));
    let mut launched = false;
    for _ in 0..40 {
        let before = player.feet;
        player.step(parity_content(), &world, &motion, command, dt);
        if player.ballistic_horizontal.length() > 1. {
            assert!(
                (player.feet - before).x.abs() < 0.001,
                "drop is emitted after this frame's ballistic pass"
            );
            assert!((player.ballistic_horizontal.x - 60.).abs() < 0.001);
            launched = true;
            break;
        }
        assert!((player.feet - before).x.abs() < 0.001);
    }
    assert!(launched);
    let before = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            yaw: 0.,
            movement: Vec2::X,
            ..default()
        },
        dt,
    );
    assert!(
        ((player.feet - before).x - 60. * dt).abs() < 0.001,
        "later aim/input must not overwrite the launched XYZ"
    );
    assert!(
        player.actor_facing.distance(Vec3::NEG_Z) < 0.0001,
        "jump facing still follows the camera after launch"
    );
    for _ in 0..20 {
        player.step(parity_content(), &world, &motion, command, dt);
        if player.melee.phase == crate::melee::AttackPhase::Delay {
            break;
        }
    }
    assert_eq!(player.melee.phase, crate::melee::AttackPhase::Delay);
    assert_eq!(player.ballistic_horizontal, Vec3::ZERO);
    let before = player.feet;
    player.step(parity_content(), &world, &motion, command, dt);
    assert!((player.feet - before).x.abs() < 0.001);
}
#[test]
fn counter_combo_turns_root_movement_and_actor_with_camera_mid_swing() {
    let motion = Motion::default();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.);
    player.melee.equipped = Weapon::CounterSword;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                primary_pressed: true,
                primary_held: true,
                ..default()
            },
            ..default()
        },
        0.016,
    );
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                primary_released: true,
                ..default()
            },
            ..default()
        },
        0.016,
    );
    assert_eq!(
        player.melee.attack,
        Some(crate::melee::Attack::CounterCombo(0))
    );
    let serial = player.melee.serial;
    for _ in 0..8 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            0.016,
        );
    }
    let before = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.8,
            ..default()
        },
        0.016,
    );
    let delta = player.feet - before;
    assert!(
        delta.x > 0.01,
        "the ongoing swing must move toward the new camera yaw: {delta:?}"
    );
    assert!(delta.z.abs() < 0.0001);
    assert!(player.actor_facing.distance(Vec3::X) < 0.0001);
    assert_eq!(player.melee.serial, serial);
}
#[test]
fn counter_jump_free_flight_matches_original_control_positions() {
    let motion = Motion::default();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::new(-27.271838, 5.419134, 67.462524), &motion);
    player.velocity.y = 6.6;
    player.melee.equipped = Weapon::CounterSword;
    let command = MovementInput {
        yaw: -std::f32::consts::FRAC_PI_6,
        pitch: 0.6,
        ..default()
    };
    player.melee.aim_direction = [
        command.yaw.sin() * command.pitch.cos(),
        command.pitch.sin(),
        -command.yaw.cos() * command.pitch.cos(),
    ];
    let air = MeleeContext {
        grounded: false,
        plasma_jump_attack: true,
        guard_lower_busy: false,
    };
    player.melee.tick(
        parity_content(),
        MeleeInput {
            primary_pressed: true,
            primary_held: true,
            ..default()
        },
        air,
        0.001,
        100.,
    );
    player.melee.tick(
        parity_content(),
        MeleeInput {
            primary_released: true,
            ..default()
        },
        air,
        0.,
        100.,
    );
    player.step(parity_content(), &world, &motion, command, 0.016);
    assert!(
        (player.feet.y - 5.481534).abs() < 0.0001,
        "feet={:?}, velocity={:?}, attack={:?}",
        player.feet,
        player.velocity,
        player.melee.attack
    );
    let frozen = player.feet;
    for _ in 0..25 {
        player.step(parity_content(), &world, &motion, command, 0.016);
        assert!(player.feet.distance(frozen) < 0.0001);
    }
    for expected in [
        Vec3::new(-27.744338, 4.865533, 66.644136),
        Vec3::new(-28.216838, 4.249534, 65.825742),
        Vec3::new(-28.689338, 3.633534, 65.007349),
        Vec3::new(-29.161838, 3.017534, 64.188955),
    ] {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                yaw: 1.2,
                pitch: -0.5,
                ..default()
            },
            0.016,
        );
        assert!(
            player.feet.distance(expected) < 0.0001,
            "{:?} != {expected:?}",
            player.feet
        );
    }
}
#[test]
fn queued_weapon_switch_on_guard_dodge_carries_held_secondary() {
    let dt = 0.016;
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for side in [-1., 1.] {
        for (weapon, attack) in [
            (Weapon::PlasmaSword, crate::melee::Attack::PlasmaDash),
            (Weapon::Dagger, crate::melee::Attack::DaggerDash),
        ] {
            let mut player = standing(&world, &motion, 0.);
            player.melee.equipped = Weapon::CounterSword;
            let held = MeleeInput {
                secondary_held: true,
                ..default()
            };
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    melee: held,
                    ..default()
                },
                dt,
            );
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    melee: MeleeInput {
                        equip: Some(weapon),
                        ..held
                    },
                    ..default()
                },
                dt,
            );
            assert_eq!(player.melee.equipped, Weapon::CounterSword);
            assert_eq!(
                player.melee.attack,
                Some(crate::melee::Attack::CounterGuard)
            );
            let sp = player.sp;
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    movement: Vec2::X * side,
                    dodge: true,
                    dodge_side: side,
                    melee: held,
                    ..default()
                },
                dt,
            );
            assert!(player.action.is_dodge());
            assert!(!player.grounded);
            assert_eq!(player.melee.equipped, weapon);
            assert!(player.melee.attack.is_none());
            assert_eq!(player.sp, sp - motion.dodge_cost);
            let jumps = player.jumps;
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    movement: Vec2::X * side,
                    melee: held,
                    ..default()
                },
                dt,
            );
            assert_eq!(player.melee.attack, Some(attack));
            assert_eq!(player.action, MovementAction::None);
            assert!(player.velocity.y > 0.);
            assert_eq!(player.jumps, jumps);
        }
    }
}
#[test]
fn queued_guard_switch_does_not_dash_after_secondary_release_or_rejected_dodge() {
    let dt = 0.016;
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for accepted in [false, true] {
        let mut player = standing(&world, &motion, 0.);
        player.melee.equipped = Weapon::CounterSword;
        let held = MeleeInput {
            secondary_held: true,
            ..default()
        };
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                melee: held,
                ..default()
            },
            dt,
        );
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                melee: MeleeInput {
                    equip: Some(Weapon::PlasmaSword),
                    ..held
                },
                ..default()
            },
            dt,
        );
        if !accepted {
            player.sp = 0.;
        }
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::X,
                dodge: true,
                dodge_side: 1.,
                melee: held,
                ..default()
            },
            dt,
        );
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                melee: if accepted {
                    MeleeInput {
                        secondary_released: true,
                        ..default()
                    }
                } else {
                    held
                },
                ..default()
            },
            dt,
        );
        if accepted {
            assert_eq!(player.melee.equipped, Weapon::PlasmaSword);
            assert!(player.melee.attack.is_none());
        } else {
            assert_eq!(player.melee.equipped, Weapon::CounterSword);
            assert_eq!(
                player.melee.attack,
                Some(crate::melee::Attack::CounterGuard)
            );
        }
    }
}
#[test]
fn counter_guard_chord_launches_before_outer_dodge_and_returns_airborne_block() {
    let dt = 0.016;
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for side in [-1., 1.] {
        let mut player = standing(&world, &motion, 0.);
        player.melee.equipped = Weapon::CounterSword;
        let right = MeleeInput {
            secondary_held: true,
            ..default()
        };
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                melee: right,
                ..default()
            },
            dt,
        );
        let start = player.feet;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::X * side,
                dodge: true,
                dodge_side: side,
                melee: right,
                ..default()
            },
            dt,
        );
        assert!(player.action.is_dodge());
        assert!(player.melee.attack.is_none());
        assert!(player.velocity.y > 25.);
        assert!(!player.grounded);
        let launch_count = player.jumps;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::X * side,
                melee: right,
                ..default()
            },
            dt,
        );
        assert_eq!(
            player.melee.attack,
            Some(crate::melee::Attack::CounterGuard)
        );
        assert_eq!(player.action, MovementAction::None);
        assert_eq!(player.guard_jump.unwrap().phase, 1);
        let mut peak = player.feet.y;
        for _ in 0..50 {
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    movement: Vec2::X * side,
                    melee: right,
                    ..default()
                },
                dt,
            );
            peak = peak.max(player.feet.y);
        }
        assert!(
            peak - start.y > 4.,
            "original lower-jump launch must survive the outer transitions"
        );
        assert!(
            (player.feet - start).x.abs() > 8.,
            "held lateral input continues through lower jump"
        );
        assert!(player.grounded);
        assert_eq!(player.jumps, launch_count);
        assert_eq!(
            player.melee.attack,
            Some(crate::melee::Attack::CounterGuard)
        );
    }
    let mut player = standing(&world, &motion, 0.);
    player.melee.equipped = Weapon::CounterSword;
    let right = MeleeInput {
        secondary_held: true,
        ..default()
    };
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: right,
            ..default()
        },
        dt,
    );
    player.sp = 0.;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            dodge: true,
            dodge_side: 1.,
            melee: right,
            ..default()
        },
        dt,
    );
    assert_eq!(
        player.melee.attack,
        Some(crate::melee::Attack::CounterGuard)
    );
    assert!(!player.grounded);
    assert_eq!(
        player.jumps, 1,
        "lower chord handling precedes the independent outer Dodge SP gate"
    );
}
#[test]
fn counter_guard_air_entry_adopts_lower_jump_without_another_impulse() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for y in [10., -10.] {
        let mut player = Player::new(Vec3::Y * 10., &motion);
        player.melee.equipped = Weapon::CounterSword;
        player.velocity = Vec3::new(3., y, 4.);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                melee: MeleeInput {
                    secondary_held: true,
                    ..default()
                },
                ..default()
            },
            DT,
        );
        assert_eq!(
            player.melee.attack,
            Some(crate::melee::Attack::CounterGuard)
        );
        assert_eq!(player.guard_jump, Some(LowerJump::enter(y)));
        assert!((player.velocity.y - (y + motion.gravity * DT)).abs() < 0.001);
    }
}
#[test]
fn counter_guard_collision_field_pushes_back_without_rejecting_entry() {
    let motion = state_motion();
    let mut geometry = floor();
    geometry.extend(wall(1.5));
    let world = CollisionWorld::new(geometry);
    let mut player = standing(&world, &motion, 0.);
    player.melee.equipped = Weapon::CounterSword;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            yaw: std::f32::consts::FRAC_PI_2,
            melee: MeleeInput {
                secondary_held: true,
                ..default()
            },
            ..default()
        },
        DT,
    );
    assert_eq!(
        player.melee.attack,
        Some(crate::melee::Attack::CounterGuard)
    );
    assert!((player.feet.x + 0.5003).abs() < 0.001, "{}", player.feet.x);
    player.feet.x = 0.;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            yaw: std::f32::consts::FRAC_PI_2,
            melee: MeleeInput {
                secondary_released: true,
                ..default()
            },
            ..default()
        },
        DT,
    );
    assert_eq!(player.melee.phase, crate::melee::AttackPhase::Delay);
    assert!((player.feet.x + 0.5003).abs() < 0.001);
}
#[test]
fn equipped_melee_rates_reach_the_shared_movement_controller() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for (weapon, rate) in [
        (Weapon::Unarmed, 1.0),
        (Weapon::PlasmaSword, 1.1),
        (Weapon::Dagger, 1.2),
    ] {
        let mut player = standing(&world, &motion, 0.0);
        player.melee.equipped = weapon;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                ..default()
            },
            DT,
        );
        assert!((player.velocity.z + motion.base_speed * rate).abs() < 0.0001);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                dodge: true,
                dodge_side: 1.0,
                ..default()
            },
            DT,
        );
        assert!((player.velocity.x - motion.dodge_speed).abs() < 0.0001);
    }
}

#[test]
fn dagger_air_cut_cancels_dodge_and_respects_wall_jump_exit_momentum() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::Y * 30.0, &motion);
    player.melee.equipped = Weapon::Dagger;
    player.velocity.y = 15.0;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            dodge_side: -1.0,
            ..default()
        },
        DT,
    );
    let before = player.velocity.y;
    let cut = MovementInput {
        melee: MeleeInput {
            primary_held: true,
            ..default()
        },
        ..default()
    };
    player.step(parity_content(), &world, &motion, cut, DT);
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.melee.attack, Some(crate::melee::Attack::DaggerAir));
    assert!((player.velocity.y - (before + motion.gravity * DT)).abs() < 0.0001);
    player.melee.reset();
    player.action = MovementAction::WallJump;
    player.walljump_phase = WallJumpPhase::LaunchA;
    player.velocity = Vec3::new(10.0, 20.0, 8.0);
    player.walljump_velocity = player.velocity;
    player.step(parity_content(), &world, &motion, cut, DT);
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.velocity, Vec3::ZERO);
    assert_eq!(player.walljump_velocity, Vec3::ZERO);
}

#[test]
fn dagger_recovery_clears_velocity_after_the_outgoing_fall_step() {
    let motion = state_motion();
    let world = CollisionWorld::default();
    let mut player = Player::new(Vec3::Y * 100., &motion);
    player.melee.equipped = Weapon::Dagger;
    player.velocity.y = -20.;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                primary_held: true,
                ..default()
            },
            ..default()
        },
        0.016,
    );
    for _ in 0..34 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            0.016,
        );
    }
    assert_eq!(player.melee.phase, crate::melee::AttackPhase::Normal);
    let mut control = Player::new(player.feet, &motion);
    control.velocity = player.velocity;
    control.ballistic_horizontal = player.ballistic_horizontal;
    control.melee.equipped = Weapon::Dagger;
    control.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert!(
        player.feet.abs_diff_eq(control.feet, 0.00001),
        "expiry must retain this ballistic displacement"
    );
    assert_eq!(player.velocity, Vec3::ZERO);
    assert_eq!(player.melee.phase, crate::melee::AttackPhase::Delay);
    let feet = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert!(player.melee.attack.is_none());
    assert!(player.feet.y < feet.y);
}

#[test]
fn dagger_air_cut_applies_both_native_forward_movement_sweeps() {
    for (dt, smooth) in [
        (0.016, false),
        (1. / 60., false),
        (1. / 120., true),
        (1. / 240., true),
    ] {
        let motion = Motion {
            smooth_physics: smooth,
            ..default()
        };
        let world = CollisionWorld::default();
        let mut player = Player::new(Vec3::Y * 100., &motion);
        player.melee.equipped = Weapon::Dagger;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                melee: MeleeInput {
                    primary_held: true,
                    ..default()
                },
                ..default()
            },
            dt,
        );
        let before = player.feet;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                ..default()
            },
            dt,
        );
        let skin = motion.movement_skin_fraction * if smooth { dt * 60. } else { 1. };
        let expected = 2. * (motion.base_speed * 1.2 * 0.5 * dt - skin);
        assert!(
            (before.z - player.feet.z - expected).abs() < 0.00001,
            "held-W air cut must submit both half-rate moves, each with its own collision skin: dt={dt}, expected={expected}, actual={}",
            before.z - player.feet.z
        );
        assert!((player.velocity.z + 13.2).abs() < 0.00001);
        let before = player.feet;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            dt,
        );
        assert_eq!(
            player.feet.z, before.z,
            "the two moves still require held movement input"
        );
    }
}

#[test]
fn held_forward_jump_then_dagger_cut_extends_reach_without_a_new_impulse() {
    let motion = Motion::default();
    let world = CollisionWorld::new(floor());
    let jump = |click: Option<usize>| {
        let mut player = standing(&world, &motion, 0.);
        player.melee.equipped = Weapon::Dagger;
        let start = player.feet;
        let mut apex = player.feet.y;
        for tick in 0..120 {
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    movement: Vec2::Y,
                    jump: tick == 0,
                    melee: MeleeInput {
                        primary_pressed: click == Some(tick),
                        primary_held: click.is_some_and(|at| (at..at + 4).contains(&tick)),
                        ..default()
                    },
                    ..default()
                },
                1. / 60.,
            );
            apex = apex.max(player.feet.y);
            if tick > 1 && player.grounded {
                return ((player.feet - start).z.abs(), apex - start.y);
            }
        }
        panic!("jump never landed");
    };
    let plain = jump(None);
    let cuts: Vec<_> = [1, 2, 3, 4, 5, 6, 9, 12, 15, 18]
        .into_iter()
        .map(|tick| {
            let result = jump(Some(tick));
            println!(
                "click tick={tick}, reach={:.4}, apex={:.4}",
                result.0, result.1
            );
            result
        })
        .collect();
    let cut = cuts
        .iter()
        .copied()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap();
    println!(
        "Held W jump: plain reach={:.4}, dagger-cut reach={:.4}; apex plain={:.4}, cut={:.4}",
        plain.0, cut.0, plain.1, cut.1
    );
    assert!(
        cut.0 > plain.0 + 0.5,
        "the cut must not halve held-W speed throughout its normal phase"
    );
    assert!(
        (cut.1 - plain.1).abs() < 0.0001,
        "the reach comes from movement and timed fall cancellation, not a new upward impulse"
    );
}

#[test]
fn dagger_added_lower_movement_is_swept_against_walls() {
    let motion = Motion::default();
    let wall = [
        Vec3::new(-10., 0., -2.),
        Vec3::new(10., 0., -2.),
        Vec3::new(10., 110., -2.),
        Vec3::new(-10., 110., -2.),
    ];
    let world = CollisionWorld::new(vec![
        [wall[0], wall[1], wall[2]],
        [wall[0], wall[2], wall[3]],
    ]);
    let mut player = Player::new(Vec3::Y * 100., &motion);
    player.melee.equipped = Weapon::Dagger;
    for tick in 0..25 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                melee: MeleeInput {
                    primary_held: tick == 0,
                    ..default()
                },
                ..default()
            },
            0.016,
        );
    }
    assert!(
        player.feet.z >= -1.01,
        "both source movement calls must honor the one-metre actor extent"
    );
    assert!(
        player.wall_contact.is_some(),
        "the second sweep must retain its wall contact"
    );
}

#[test]
fn dagger_lower_jump_owns_touchdown_and_ground_entry_keeps_walk_off_ledge() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::Y * 0.3, &motion);
    player.melee.equipped = Weapon::Dagger;
    player.velocity.y = -1.;
    let held = MovementInput {
        melee: MeleeInput {
            primary_held: true,
            ..default()
        },
        ..default()
    };
    player.step(parity_content(), &world, &motion, held, 0.016);
    assert_eq!(player.dagger_jump.unwrap().phase, 2);
    for _ in 0..12 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            0.016,
        );
    }
    assert!(player.grounded);
    assert_eq!(player.dagger_jump.unwrap().phase, 3);
    assert!(player.melee.attack.is_some());
    for _ in 0..20 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            0.016,
        );
    }
    assert!(
        player.dagger_jump.is_none(),
        "lower landing returns to Walk independently of upper attack"
    );

    player.melee.reset();
    player.grounded = true;
    player.step(parity_content(), &world, &motion, held, 0.016);
    assert!(player.dagger_jump.is_none());
    let empty = CollisionWorld::default();
    player.step(
        parity_content(),
        &empty,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert!(!player.grounded);
    assert!(
        player.dagger_jump.is_none(),
        "selected Walk persists over support loss"
    );
}

#[test]
fn plasma_jump_freezes_ballistics_then_uses_its_source_drop_terminal_speed() {
    let motion = state_motion();
    let native_dt = 0.016;
    let world = CollisionWorld::default();
    let mut player = Player::new(Vec3::Y * 100.0, &motion);
    player.melee.equipped = Weapon::PlasmaSword;
    player.velocity.y = 15.0;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            melee: MeleeInput {
                primary_pressed: true,
                primary_released: true,
                ..default()
            },
            ..default()
        },
        native_dt,
    );
    for _ in 0..24 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            native_dt,
        );
    }
    assert_eq!(player.feet.y, 100.0);
    assert_eq!(player.velocity.y, 15.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        native_dt,
    );
    assert_eq!(player.velocity.y, -500.0);
    assert!((100.0 - player.feet.y - 8.0).abs() < 0.0001);
}

#[test]
fn native_side_then_new_space_chord_dodges_once_without_a_jump() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for side in [KeyCode::KeyA, KeyCode::KeyD] {
        let mut player = standing(&world, &motion, 0.0);
        let mut controls = Controls::default();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(side);
        controls.movement_keys(&keys, 1.0, &motion);
        player.step(
            parity_content(),
            &world,
            &motion,
            take_command(&mut controls),
            DT,
        );
        keys.clear();
        keys.press(KeyCode::Space);
        controls.movement_keys(&keys, 1.1, &motion);
        player.step(
            parity_content(),
            &world,
            &motion,
            take_command(&mut controls),
            DT,
        );
        assert_eq!(player.dodges, 1);
        assert_eq!(player.jumps, 0);
        assert_eq!(player.sp, 80.0);
        keys.clear();
        for _ in 0..200 {
            controls.movement_keys(&keys, 1.2, &motion);
            player.step(
                parity_content(),
                &world,
                &motion,
                take_command(&mut controls),
                DT,
            );
        }
        assert_eq!(
            player.dodges, 1,
            "Holding the chord cannot retrigger a dodge"
        );
        assert_eq!(player.jumps, 0);
    }
}

#[test]
fn native_space_then_side_is_a_jump_and_does_not_retroactively_dodge() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    let mut controls = Controls::default();
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::Space);
    controls.movement_keys(&keys, 1.0, &motion);
    player.step(
        parity_content(),
        &world,
        &motion,
        take_command(&mut controls),
        DT,
    );
    keys.clear();
    keys.press(KeyCode::KeyA);
    controls.movement_keys(&keys, 1.01, &motion);
    player.step(
        parity_content(),
        &world,
        &motion,
        take_command(&mut controls),
        DT,
    );
    assert_eq!(player.jumps, 1);
    assert_eq!(player.dodges, 0);
    assert!(player.vertical_speed > 26.0);
}

#[test]
fn shift_never_starts_a_movement_action() {
    let motion = state_motion();
    let mut controls = Controls::default();
    let mut keys = ButtonInput::<KeyCode>::default();
    for side in [KeyCode::KeyA, KeyCode::KeyD] {
        keys.reset_all();
        keys.press(side);
        keys.press(KeyCode::ShiftLeft);
        controls.movement_keys(&keys, 1.0, &motion);
        let command = take_command(&mut controls);
        assert!(!command.dodge && !command.jump && !command.sprint);
    }
}

#[test]
fn native_double_w_window_accepts_400ms_but_rejects_more_than_500ms() {
    let motion = state_motion();
    for (gap, expected) in [(0.4, true), (0.51, false)] {
        let mut controls = Controls::default();
        controls.forward_key(true, true, 1.0, motion.sprint_double_tap_window);
        controls.forward_key(false, false, 1.1, motion.sprint_double_tap_window);
        controls.forward_key(true, true, 1.0 + gap, motion.sprint_double_tap_window);
        assert_eq!(controls.sprint, expected);
    }
}

#[test]
fn native_double_w_pairs_are_anchored_to_the_first_press() {
    let motion = state_motion();
    let mut controls = Controls::default();
    controls.forward_key(true, true, 1.0, motion.sprint_double_tap_window);
    controls.forward_key(false, false, 1.1, motion.sprint_double_tap_window);
    controls.forward_key(true, true, 1.3, motion.sprint_double_tap_window);
    assert!(controls.sprint);
    controls.forward_key(false, false, 1.4, motion.sprint_double_tap_window);
    controls.forward_key(true, true, 1.6, motion.sprint_double_tap_window);
    assert!(
        !controls.sprint,
        "Third press is outside the window anchored at1.0"
    );
    controls.forward_key(false, false, 1.7, motion.sprint_double_tap_window);
    controls.forward_key(true, true, 1.9, motion.sprint_double_tap_window);
    assert!(controls.sprint);
}

#[test]
fn recognized_dodge_direction_survives_key_release_before_the_fixed_tick() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    let mut controls = Controls::default();
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::KeyA);
    keys.press(KeyCode::Space);
    controls.movement_keys(&keys, 1.0, &motion);
    keys.clear();
    keys.release(KeyCode::KeyA);
    controls.movement_keys(&keys, 1.001, &motion);
    player.step(
        parity_content(),
        &world,
        &motion,
        take_command(&mut controls),
        DT,
    );
    assert_eq!(player.action, MovementAction::DodgeLeft);
    assert_eq!(player.dodges, 1);
    assert_eq!(player.jumps, 0);
}

#[test]
fn absent_dodge_direction_cannot_turn_zero_into_a_right_dodge() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.dodges, 0);
    assert_eq!(player.jumps, 0);
    assert_eq!(player.sp, 100.0);
}

#[test]
fn rejected_dodge_fallback_jumps_on_ground_without_adding_an_airborne_impulse() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut grounded = standing(&world, &motion, 0.0);
    grounded.sp = 19.0;
    let command = MovementInput {
        dodge: true,
        dodge_side: -1.0,
        ..default()
    };
    grounded.step(parity_content(), &world, &motion, command, DT);
    assert_eq!(grounded.dodges, 0);
    assert_eq!(grounded.jumps, 1);
    assert!(grounded.vertical_speed > 26.0);
    let mut airborne = Player::new(Vec3::Y * 10.0, &motion);
    airborne.sp = 19.0;
    airborne.velocity.y = -5.0;
    airborne.step(parity_content(), &world, &motion, command, DT);
    assert_eq!(airborne.dodges, 0);
    assert_eq!(airborne.jumps, 0);
    assert!((airborne.vertical_speed - (-5.0 + motion.gravity * DT)).abs() < 0.001);
}

#[test]
fn airborne_dodge_preserves_ballistic_y_and_wins_over_jump_or_walljump() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    for incoming_y in [12.0, -12.0] {
        let mut player = Player::new(Vec3::new(-motion.radius - 0.025, 10.0, 0.0), &motion);
        player.velocity.y = incoming_y;
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                dodge: true,
                dodge_side: -1.0,
                jump: true,
                ..default()
            },
            DT,
        );
        assert_eq!(player.action, MovementAction::DodgeLeft);
        assert_eq!(player.air_dodges, 1);
        assert_eq!(player.jumps, 0);
        assert_eq!(player.wall_jumps, 0);
        assert!((player.vertical_speed - (incoming_y + motion.gravity * DT)).abs() < 0.001);
        assert_eq!(player.sp, 80.0);
    }
}

#[test]
fn airborne_sprint_cancels_rise_once_and_preserves_fall_then_gravity_accumulates() {
    let motion = state_motion();
    let world = CollisionWorld::new(Vec::new());
    for incoming_y in [12.0, -12.0] {
        let mut player = Player::new(Vec3::Y * 20.0, &motion);
        player.velocity.y = incoming_y;
        let command = MovementInput {
            movement: Vec2::Y,
            sprint: true,
            ..default()
        };
        player.step(parity_content(), &world, &motion, command, DT);
        assert_eq!(player.action, MovementAction::Sprint);
        assert_eq!(player.air_sprints, 1);
        assert!(
            (player.vertical_speed - (incoming_y.min(0.0) + motion.gravity * DT)).abs() < 0.001
        );
        let prior_y = player.vertical_speed;
        player.step(parity_content(), &world, &motion, command, DT);
        assert_eq!(player.air_sprints, 1);
        assert!((player.vertical_speed - (prior_y + motion.gravity * DT)).abs() < 0.001);
        assert!((player.velocity.z + 16.5).abs() < 0.001);
    }
}

#[test]
fn sprint_landing_keeps_full_speed_and_does_not_leave_a_latent_jump_land_state() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::Y * 0.01, &motion);
    player.velocity.y = -5.0;
    let command = MovementInput {
        movement: Vec2::Y,
        sprint: true,
        ..default()
    };
    player.step(parity_content(), &world, &motion, command, DT);
    assert!(player.grounded);
    assert_eq!(player.action, MovementAction::Sprint);
    assert!(player.landing_age >= motion.land_delay);
    let before = player.feet;
    player.step(parity_content(), &world, &motion, command, DT);
    assert!((before.z - player.feet.z - 16.5 * DT).abs() < 0.001);
    assert_eq!(player.action, MovementAction::Sprint);
    let before = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::Y,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
    assert!((before.z - player.feet.z - 11.0 * DT).abs() < 0.001);
    assert!(player.landing_age >= motion.land_delay);
}

#[test]
fn double_w_sprint_requires_a_second_press_and_ends_on_release() {
    let mut controls = Controls::default();
    controls.forward_key(true, true, 1.0, 0.25);
    assert!(!controls.sprint);
    controls.forward_key(false, false, 1.05, 0.25);
    controls.forward_key(true, true, 1.16, 0.25);
    assert!(controls.sprint);
    controls.forward_key(false, false, 1.3, 0.25);
    assert!(!controls.sprint);
    controls.forward_key(true, true, 2.0, 0.25);
    assert!(!controls.sprint);
    controls.clear_actions();
    controls.forward_key(true, true, 2.1, 0.25);
    assert!(!controls.sprint, "Chat/cursor reset clears the tap history");
}

#[test]
fn sprint_uses_native_state_multiplier_and_sp_drain() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    let start = player.feet;
    for _ in 0..240 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                sprint: true,
                ..default()
            },
            DT,
        );
    }
    assert!((player.feet.distance(start) - 16.5).abs() < 0.01);
    assert!((player.sp - 90.0).abs() < 0.01);
    assert!((player.sprint_distance - 16.5).abs() < 0.01);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
}

#[test]
fn sprint_entry_threshold_does_not_prematurely_end_an_active_sprint() {
    let motion = Motion {
        sp_recovery_per_second: 0.0,
        ..default()
    };
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.sp = 1.0;
    let command = MovementInput {
        movement: Vec2::Y,
        sprint: true,
        ..default()
    };
    player.step(parity_content(), &world, &motion, command, DT);
    assert_eq!(player.action, MovementAction::Sprint);
    assert!(player.sp < 1.0);
    player.step(parity_content(), &world, &motion, command, DT);
    assert_eq!(player.action, MovementAction::Sprint);
    for _ in 0..23 {
        player.step(parity_content(), &world, &motion, command, DT);
    }
    assert_eq!(player.action, MovementAction::None);
    assert!(player.sp < motion.sprint_sp_per_second * DT);
    assert!(player.sp >= 0.0);
}

#[test]
fn dodge_uses_native_sp_cost_and_two_phase_speed() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            dodge: true,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::DodgeRight);
    assert_eq!(player.dodges, 1);
    assert_eq!(player.sp, 80.0);
    for _ in 0..23 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: -Vec2::Y,
                ..default()
            },
            DT,
        );
    }
    assert!((player.feet.x - motion.dodge_speed * 0.1).abs() < 0.005);
    assert!(player.feet.z.abs() < 0.005);
    assert_eq!(player.jumps, 0);
    assert_eq!(player.dodges, 1);
    for _ in 24..145 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    let expected = motion.dodge_speed * (motion.dodge_time + motion.dodge_delay)
        - 0.5 * motion.dodge_deceleration * motion.dodge_delay.powi(2);
    assert!(
        (player.feet.x - expected).abs() < 0.08,
        "{} vs {expected}",
        player.feet.x
    );
    assert_eq!(player.action, MovementAction::None);
    assert!((player.sp - (80.0 + 144.0 * 7.5 * DT)).abs() < 0.001);
}

#[test]
fn a_fresh_space_cancels_dodge_even_with_the_side_key_still_held() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    for delay in [0.05, 0.3] {
        for held_side in [false, true] {
            let mut player = standing(&world, &motion, 0.0);
            let mut controls = Controls::default();
            let mut keys = ButtonInput::<KeyCode>::default();
            keys.press(KeyCode::KeyA);
            keys.press(KeyCode::Space);
            controls.movement_keys(&keys, 1.0, &motion);
            player.step(
                parity_content(),
                &world,
                &motion,
                take_command(&mut controls),
                DT,
            );
            keys.clear();
            keys.release(KeyCode::Space);
            if !held_side {
                keys.release(KeyCode::KeyA);
            }
            for _ in 0..(delay / DT) as usize {
                controls.movement_keys(&keys, 1.01, &motion);
                player.step(
                    parity_content(),
                    &world,
                    &motion,
                    take_command(&mut controls),
                    DT,
                );
                keys.clear();
            }
            keys.press(KeyCode::Space);
            controls.movement_keys(&keys, 1.0 + delay, &motion);
            player.step(
                parity_content(),
                &world,
                &motion,
                take_command(&mut controls),
                DT,
            );
            assert_eq!(player.action, MovementAction::None);
            assert_eq!(player.dodges, 1);
            assert_eq!(player.jumps, 1);
            assert!(
                player.sp >= 80.0 && player.sp < 83.0,
                "Cancellation must not charge another dodge cost"
            );
            assert!(player.vertical_speed > 26.0);
        }
    }
}

#[test]
fn cancelling_an_air_dodge_does_not_add_a_midair_jump() {
    let motion = state_motion();
    let world = CollisionWorld::new(Vec::new());
    let mut player = Player::new(Vec3::Y * 10.0, &motion);
    player.velocity.y = -5.0;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            dodge_side: -1.0,
            ..default()
        },
        DT,
    );
    let before = player.vertical_speed;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            dodge_side: -1.0,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.jumps, 0);
    assert_eq!(player.dodges, 1);
    assert!((player.vertical_speed - before - motion.gravity * DT).abs() < 0.001);
}

#[test]
fn cancelling_dodge_does_not_reset_the_native_400ms_entry_cooldown() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    let chord = MovementInput {
        dodge: true,
        dodge_side: -1.0,
        ..default()
    };
    player.step(parity_content(), &world, &motion, chord, DT);
    for _ in 0..12 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    player.step(parity_content(), &world, &motion, chord, DT);
    assert_eq!(player.action, MovementAction::None);
    player.step(parity_content(), &world, &motion, chord, DT);
    assert_eq!(player.dodges, 1, "cancel cannot bypass the entry cooldown");
    for _ in 0..84 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    player.step(parity_content(), &world, &motion, chord, DT);
    assert_eq!(player.dodges, 2);
}

#[test]
fn dodge_cannot_cross_a_thin_wall() {
    let motion = state_motion();
    let mut triangles = floor();
    triangles.extend(wall(0.0));
    let world = CollisionWorld::new(triangles);
    let mut player = standing(&world, &motion, -4.0);
    for tick in 0..150 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::X,
                dodge: tick == 0,
                ..default()
            },
            DT,
        );
        assert!(
            player.feet.x <= -motion.radius,
            "Dodge tunneled at tick {tick}: {:?}",
            player.feet
        );
        assert!(player.grounded);
    }
    assert_eq!(player.dodges, 1);
}

#[test]
fn active_dodge_follows_camera_yaw_then_recovery_keeps_last_direction() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            dodge: true,
            ..default()
        },
        DT,
    );
    let initial_x = player.feet.x;
    for _ in 0..58 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                yaw: std::f32::consts::FRAC_PI_2,
                ..default()
            },
            DT,
        );
    }
    assert!((player.feet.x - initial_x).abs() < 0.001);
    assert!(player.feet.z > 5.0);
    let before = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            yaw: 0.0,
            ..default()
        },
        DT,
    );
    assert!((player.feet.x - before.x).abs() < 0.001);
    assert!(
        player.feet.z > before.z,
        "Recovery retains the last active dodge direction"
    );
}

#[test]
fn insufficient_sp_blocks_actions_and_recovers_when_idle() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.sp = 0.0;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::X,
            dodge: true,
            sprint: true,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.dodges, 0);
    for _ in 0..(240 * 14) {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    assert_eq!(player.sp, motion.max_sp);
}

#[test]
fn native_sp_recovers_without_idle_delay_during_dodge_and_walljump() {
    let world = CollisionWorld::new(Vec::new());
    for max_sp in [100.0, 200.0] {
        let motion = Motion {
            max_sp,
            ..default()
        };
        for action in [
            MovementAction::None,
            MovementAction::DodgeRight,
            MovementAction::WallJump,
        ] {
            let mut player = Player::new(Vec3::Y * 10.0, &motion);
            player.spend_sp(20.0);
            player.set_action(action);
            for _ in 0..3 {
                player.step(
                    parity_content(),
                    &world,
                    &motion,
                    MovementInput::default(),
                    0.016,
                );
            }
            assert_eq!(player.action, action);
            let expected = max_sp - 20.0 + max_sp * 0.075 * 0.048;
            assert!(
                (player.sp - expected).abs() < 0.001,
                "{action:?}: {} vs {expected}",
                player.sp
            );
        }
    }
}

#[test]
fn native_sp_recovery_is_disabled_by_sprint_and_resumes_on_release() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.sp = 80.0;
    for _ in 0..10 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                sprint: true,
                ..default()
            },
            0.016,
        );
    }
    assert_eq!(player.action, MovementAction::Sprint);
    assert!((player.sp - 78.4).abs() < 0.001);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert_eq!(player.action, MovementAction::None);
    assert!((player.sp - 78.52).abs() < 0.001);
    for _ in 0..200 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            0.016,
        );
    }
    assert_eq!(player.sp, motion.max_sp);
}

#[test]
fn leaving_support_seeds_native_fall_speed_then_accumulates_gravity_to_terminal() {
    let motion = state_motion();
    let world = CollisionWorld::new(Vec::new());
    let mut player = Player::new(Vec3::Y * 1000.0, &motion);
    player.grounded = true;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert!(!player.grounded);
    assert!((player.vertical_speed + 7.5).abs() < 0.001);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert!((player.vertical_speed + 8.7).abs() < 0.001);
    for _ in 0..120 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            0.016,
        );
        assert!(player.vertical_speed >= motion.terminal_velocity);
    }
    assert_eq!(player.vertical_speed, motion.terminal_velocity);
}

#[test]
fn airborne_and_sprint_directions_use_full_speed_instead_of_run_angle_rates() {
    let motion = state_motion();
    let world = CollisionWorld::new(Vec::new());
    for movement in [Vec2::X, -Vec2::Y, Vec2::new(1.0, -1.0)] {
        let mut player = Player::new(Vec3::Y * 100.0, &motion);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement,
                ..default()
            },
            0.016,
        );
        assert!((Vec2::new(player.velocity.x, player.velocity.z).length() - 11.0).abs() < 0.001);
    }
    let floor_world = CollisionWorld::new(floor());
    let mut player = standing(&floor_world, &motion, 0.0);
    player.step(
        parity_content(),
        &floor_world,
        &motion,
        MovementInput {
            movement: Vec2::new(1.0, 1.0),
            sprint: true,
            ..default()
        },
        0.016,
    );
    assert!((Vec2::new(player.velocity.x, player.velocity.z).length() - 16.5).abs() < 0.001);
}

#[test]
fn ledge_fall_landing_retains_full_control_for_its_separate_125ms_phase() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::Y * 0.01, &motion);
    player.velocity.y = -5.0;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput::default(),
        0.016,
    );
    assert!(player.grounded && player.fall_landing);
    assert_eq!(player.landing_age, 0.0);
    for _ in 0..7 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: -Vec2::Y,
                ..default()
            },
            0.016,
        );
        assert!((player.velocity.z - 11.0).abs() < 0.001);
    }
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: -Vec2::Y,
            ..default()
        },
        0.016,
    );
    assert!(
        (player.velocity.z - 9.9).abs() < 0.001,
        "Run's backward rate resumes after Fall's125ms landing"
    );
}

#[test]
fn dodge_touchdown_does_not_create_a_latent_jump_landing_slowdown() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = Player::new(Vec3::Y * 0.01, &motion);
    player.velocity.y = -5.0;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            dodge_side: 1.0,
            ..default()
        },
        0.016,
    );
    assert!(player.grounded);
    assert_eq!(player.action, MovementAction::DodgeRight);
    assert!(!player.fall_landing && player.landing_age >= motion.land_delay);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: Vec2::Y,
            jump: true,
            ..default()
        },
        0.016,
    );
    assert_eq!(player.action, MovementAction::None);
    assert!((player.velocity.z + 11.0).abs() < 0.001);
    assert_eq!(player.jumps, 1);
}

fn approach_test_wall(player: &mut Player, world: &CollisionWorld, motion: &Motion) {
    player.step(
        parity_content(),
        world,
        motion,
        MovementInput {
            movement: Vec2::Y,
            yaw: std::f32::consts::FRAC_PI_2,
            ..default()
        },
        0.016,
    );
    assert!(
        player.wall_contact.is_some(),
        "Approach must actually hit the wall"
    );
}

fn begin_test_walljump(world: &CollisionWorld, motion: &Motion) -> Player {
    let mut player = Player::new(
        Vec3::new(-motion.collision_axis_scales[0] - 0.025, 2.0, 0.0),
        motion,
    );
    approach_test_wall(&mut player, world, motion);
    player.step(
        parity_content(),
        world,
        motion,
        MovementInput {
            jump: true,
            yaw: std::f32::consts::FRAC_PI_2,
            ..default()
        },
        DT,
    );
    assert_eq!(player.walljump_phase, WallJumpPhase::Windup);
    player
}

fn finish_test_walljump_windup(
    player: &mut Player,
    world: &CollisionWorld,
    motion: &Motion,
    yaw: f32,
) {
    for _ in 0..64 {
        if player.walljump_phase != WallJumpPhase::Windup {
            return;
        }
        player.step(
            parity_content(),
            world,
            motion,
            MovementInput { yaw, ..default() },
            DT,
        );
    }
    panic!("Wall jump did not leave windup within64ticks");
}

#[test]
fn bound_windup_blocks_dodge_then_launch_cancel_clears_all_rebound_momentum() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    let mut player = begin_test_walljump(&world, &motion);
    let start = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            dodge_side: 1.0,
            ..default()
        },
        DT,
    );
    assert_eq!(player.walljump_phase, WallJumpPhase::Windup);
    assert_eq!(player.feet, start);
    assert_eq!(player.dodges, 0);
    finish_test_walljump_windup(&mut player, &world, &motion, std::f32::consts::FRAC_PI_2);
    assert_eq!(player.walljump_phase, WallJumpPhase::LaunchA);
    assert!(player.vertical_speed > 30.0 && player.velocity.x < -23.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            dodge: true,
            dodge_side: 1.0,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::DodgeRight);
    assert!((player.vertical_speed - motion.gravity * DT).abs() < 0.001);
    assert!((player.velocity.x - motion.dodge_speed).abs() < 0.001);
    assert!(player.velocity.z.abs() < 0.001);
}

#[test]
fn bound_launch_variant_uses_camera_at_launch_and_air_keeps_ballistic_inertia() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    for (yaw, expected) in [
        (0.0, WallJumpPhase::LaunchB),
        (std::f32::consts::FRAC_PI_2, WallJumpPhase::LaunchA),
    ] {
        let mut player = begin_test_walljump(&world, &motion);
        finish_test_walljump_windup(&mut player, &world, &motion, yaw);
        assert_eq!(player.walljump_phase, expected);
        let launch = player.walljump_velocity;
        for _ in 0..128 {
            if !player.walljump_phase.launches() {
                break;
            }
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput { yaw, ..default() },
                DT,
            );
        }
        assert_eq!(player.action, MovementAction::WallJump);
        assert!(matches!(
            player.walljump_phase,
            WallJumpPhase::AirA | WallJumpPhase::AirB
        ));
        assert!((player.velocity.x - launch.x).abs() < 0.001);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                ..default()
            },
            DT,
        );
        assert!((player.velocity.x - launch.x).abs() < 0.001);
        assert!((player.velocity.z + motion.base_speed).abs() < 0.001);
        assert!(player.vertical_speed < 0.0);
    }
}

#[test]
fn glancing_launch_cannot_reuse_its_departing_wall_at_any_tick_rate() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    for hz in [60.0, 62.5, 120.0, 240.0] {
        let dt = 1.0 / hz;
        let yaw = 0.21_f32.asin();
        let mut player = Player::new(Vec3::new(-1.025, 2.0, 0.0), &motion);
        approach_test_wall(&mut player, &world, &motion);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                jump: true,
                yaw,
                ..default()
            },
            dt,
        );
        for _ in 0..64 {
            if player.walljump_phase != WallJumpPhase::Windup {
                break;
            }
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput { yaw, ..default() },
                dt,
            );
        }
        assert_eq!(player.wall_jumps, 1);
        assert!(player.walljump_phase.launches());
        assert!(
            player.feet.x.abs() < 1.12,
            "Old proximity check would accept this wall at {hz}Hz"
        );
        assert!(
            player.wall_contact.is_none(),
            "Departing sweep must clear contact"
        );
        for _ in 0..4 {
            player.step(
                parity_content(),
                &world,
                &motion,
                MovementInput {
                    jump: true,
                    yaw,
                    ..default()
                },
                dt,
            );
            assert_ne!(player.walljump_phase, WallJumpPhase::Windup, "{hz}Hz");
            assert_eq!(player.wall_jumps, 1);
        }
    }
}

#[test]
fn a_nearby_wall_without_a_sweep_contact_does_not_allow_a_jump() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    let mut player = Player::new(Vec3::new(-1.025, 2.0, 0.0), &motion);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            jump: true,
            yaw: std::f32::consts::FRAC_PI_2,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.wall_jumps, 0);
}

#[test]
fn bound_jump_can_rebound_from_another_contact_during_its_launch_phase() {
    let motion = state_motion();
    let mut triangles = wall(0.0);
    triangles.extend(wall(-8.0));
    let world = CollisionWorld::new(triangles);
    let mut player = begin_test_walljump(&world, &motion);
    finish_test_walljump_windup(&mut player, &world, &motion, 0.0);
    for _ in 0..100 {
        if player.wall_contact.is_some() {
            break;
        }
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    assert!(player.walljump_phase.launches());
    assert!(
        player.wall_contact.is_some(),
        "Second jump needs a swept contact"
    );
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            jump: true,
            ..default()
        },
        DT,
    );
    assert_eq!(player.walljump_phase, WallJumpPhase::Windup);
    assert_eq!(player.wall_jumps, 1);
    finish_test_walljump_windup(&mut player, &world, &motion, 0.0);
    assert_eq!(player.wall_jumps, 2);
    assert!(
        player.velocity.x > 23.0,
        "Second wall must launch away from its face"
    );
    assert!(player.sp < 65.0, "Each accepted rebound pays its own20SP");
}

#[test]
fn bound_rechecks_sp_after_windup_and_holds_landing_before_accepting_actions() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    let mut player = begin_test_walljump(&world, &motion);
    player.sp = 0.0;
    for _ in 0..50 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    assert_eq!(player.walljump_phase, WallJumpPhase::Failed);
    assert_eq!(player.wall_jumps, 0);
    assert!(player.vertical_speed < 0.0);
    let floor_world = CollisionWorld::new(floor());
    player.feet = Vec3::Y * 0.01;
    player.velocity.y = -5.0;
    player.walljump_velocity = Vec3::new(7.0, -5.0, 4.0);
    player.step(
        parity_content(),
        &floor_world,
        &motion,
        MovementInput::default(),
        DT,
    );
    assert_eq!(player.walljump_phase, WallJumpPhase::Landing);
    assert_eq!(player.walljump_velocity, Vec3::ZERO);
    let touchdown = player.feet;
    for _ in 0..47 {
        player.step(
            parity_content(),
            &floor_world,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                dodge: true,
                dodge_side: 1.0,
                ..default()
            },
            DT,
        );
        assert_eq!(player.action, MovementAction::WallJump);
        assert!((player.feet - touchdown).length() < 0.001);
    }
    player.step(
        parity_content(),
        &floor_world,
        &motion,
        MovementInput {
            movement: Vec2::Y,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::None);
    assert!((player.velocity.z + 11.0).abs() < 0.001);
}

#[test]
fn walljump_waits_then_reflects_with_native_horizontal_vertical_forces() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    let mut player = Player::new(
        Vec3::new(-motion.collision_axis_scales[0] - 0.025, 2.0, 0.0),
        &motion,
    );
    approach_test_wall(&mut player, &world, &motion);
    let start = player.feet;
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            jump: true,
            yaw: std::f32::consts::FRAC_PI_2,
            ..default()
        },
        DT,
    );
    assert_eq!(player.action, MovementAction::WallJump);
    for _ in 0..46 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    assert_eq!(player.wall_jumps, 0);
    assert_eq!(player.sp, 100.0, "Native cost is checked after windup");
    assert_eq!(player.feet, start);
    for _ in 0..4 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    assert_eq!(player.wall_jumps, 1);
    assert!(player.sp >= 80.0 && player.sp < 80.13);
    assert!(player.vertical_speed > 30.0);
    assert!((player.velocity.x + 23.4375).abs() < 0.001);
    assert!(player.feet.x < start.x && player.feet.y > start.y);
    for _ in 0..130 {
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                jump: true,
                ..default()
            },
            DT,
        );
    }
    assert_eq!(
        player.wall_jumps, 1,
        "Further Space presses cannot rebound after leaving wall contact"
    );
}

#[test]
fn airborne_jump_needs_a_real_upright_wall() {
    let motion = state_motion();
    for (world, feet) in [
        (CollisionWorld::new(floor()), Vec3::new(0.0, 0.025, 0.0)),
        (CollisionWorld::new(wall(0.0)), Vec3::new(-2.0, 2.0, 0.0)),
    ] {
        let mut player = Player::new(feet, &motion);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                jump: true,
                ..default()
            },
            DT,
        );
        assert_eq!(player.action, MovementAction::None);
        assert_eq!(player.wall_jumps, 0);
        assert_eq!(player.sp, 100.0);
    }
}

#[test]
fn walljump_requires_native_facing_dot_greater_than_point_two() {
    let motion = state_motion();
    let world = CollisionWorld::new(wall(0.0));
    for approach in [-1.0_f32, 0.0, 0.19, 0.21, 0.8] {
        let mut player = Player::new(Vec3::new(-1.025, 2.0, 0.0), &motion);
        approach_test_wall(&mut player, &world, &motion);
        player.step(
            parity_content(),
            &world,
            &motion,
            MovementInput {
                jump: true,
                yaw: approach.asin(),
                ..default()
            },
            DT,
        );
        assert_eq!(
            player.action == MovementAction::WallJump,
            approach > 0.2,
            "approach {approach}"
        );
    }
}

#[test]
fn fully_supported_landing_stays_on_circle2_ring() {
    let motion = Motion::default();
    let map = scene::read_map_for(scene::MapId::Circle2).unwrap();
    let level = scene::make_level(&map);
    for hz in [60.0_f32, 62.5, 120.0, 240.0] {
        let dt = 1.0 / hz;
        for side in [-1.0_f32, 1.0] {
            let mut player = Player::new(Vec3::new(side * 2.0, 2.0, -19.75), &motion);
            let mut landed = false;
            for _ in 0..(hz as usize * 2) {
                player.step(
                    parity_content(),
                    &level.collision,
                    &motion,
                    MovementInput::default(),
                    dt,
                );
                if player.grounded && player.feet.y > -1.0 {
                    landed = true;
                    break;
                }
            }
            assert!(
                landed,
                "{hz}Hz side {side}: {:?}, wall jumps {}",
                player.feet, player.wall_jumps
            );
            let touchdown = player.feet;
            for _ in 0..hz as usize {
                player.step(
                    parity_content(),
                    &level.collision,
                    &motion,
                    MovementInput::default(),
                    dt,
                );
            }
            assert!(
                player.grounded && player.feet.y > -1.1,
                "fell off ring: {touchdown:?} -> {:?}",
                player.feet
            );
            assert!(Vec2::new(player.feet.x, player.feet.z).length() > 18.0);
        }
    }
}

#[test]
fn circle2_jump_against_round_wall_keeps_rising() {
    let motion = state_motion();
    let map = scene::read_map_for(scene::MapId::Circle2).unwrap();
    let level = scene::make_level(&map);
    let mut player = Player::new(Vec3::new(2.0, -8.0, -21.0), &motion);
    for _ in 0..240 {
        player.step(
            parity_content(),
            &level.collision,
            &motion,
            MovementInput::default(),
            DT,
        );
    }
    let start = player.feet;
    let mut peak = start.y;
    for tick in 0..120 {
        player.step(
            parity_content(),
            &level.collision,
            &motion,
            MovementInput {
                movement: Vec2::Y,
                yaw: std::f32::consts::PI,
                jump: tick == 0,
                ..default()
            },
            DT,
        );
        peak = peak.max(player.feet.y);
    }
    assert!(
        peak - start.y > 4.7,
        "snagged: {start:?}, peak {peak}, final {:?}",
        player.feet
    );
}

#[test]
fn respawn_clears_actions_and_advances_pose_reset_without_losing_counters() {
    let motion = state_motion();
    let world = CollisionWorld::new(floor());
    let mut player = standing(&world, &motion, 0.0);
    player.step(
        parity_content(),
        &world,
        &motion,
        MovementInput {
            movement: -Vec2::X,
            dodge: true,
            ..default()
        },
        DT,
    );
    player.reset_motion(Vec3::new(30.0, 4.0, 5.0));
    assert_eq!(player.action, MovementAction::None);
    assert_eq!(player.pose_reset_serial, 1);
    assert_eq!(player.dodges, 1);
    assert_eq!(player.sp, 100.0);
    assert_eq!(player.previous, player.feet);
    assert!(!player.walljump_used);
}
