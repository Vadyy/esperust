use super::actor_contract_tests::{actors, hit_pair, tick_pair};
use crate::{
    combat_types::Attack, content::simulation::parity_content, controller::CollisionWorld,
    gameplay::Motion,
};
use bevy::prelude::*;

fn floor(y: f32) -> Vec<[Vec3; 3]> {
    let a = Vec3::new(-100., y, -100.);
    let b = Vec3::new(100., y, -100.);
    let c = Vec3::new(100., y, 100.);
    let d = Vec3::new(-100., y, 100.);
    vec![[a, b, c], [a, c, d]]
}

#[test]
fn counter_and_plasma_heavy_share_full_launch_collision_and_recovery_trajectories() {
    let content = parity_content();
    for sparring in [false, true] {
        for attack in [Attack::CounterRetaliation, Attack::PlasmaStrong] {
            let index = usize::from(attack == Attack::CounterRetaliation);
            let mut hit = content.hit_spec(attack, index).unwrap();
            hit.damage = 1.;
            assert!(hit.lift > 0. && hit.push > 0.);
            for smooth in [false, true] {
                for hz in [30., 60., 240.] {
                    for obstacles in 0..3 {
                        let motion = Motion {
                            smooth_physics: smooth,
                            ..default()
                        };
                        let mut triangles = floor(0.);
                        if obstacles >= 1 {
                            triangles.extend([
                                [
                                    Vec3::new(-100., 0., 8.),
                                    Vec3::new(100., 0., 8.),
                                    Vec3::new(100., 100., 8.),
                                ],
                                [
                                    Vec3::new(-100., 0., 8.),
                                    Vec3::new(100., 100., 8.),
                                    Vec3::new(-100., 100., 8.),
                                ],
                            ]);
                        }
                        if obstacles == 2 {
                            triangles.extend(floor(5.));
                        }
                        let world = CollisionWorld::new(triangles);
                        let (mut player, mut target, mut transform) = actors(&motion, Vec3::ZERO);
                        if sparring {
                            target.fighter =
                                Some(super::PracticeFighter::new(super::SparringMode::Attack));
                            player.melee.equipped = crate::melee::Weapon::CounterSword;
                        }
                        tick_pair(
                            content,
                            &world,
                            &motion,
                            &mut player,
                            &mut target,
                            &mut transform,
                            1. / hz,
                        );
                        assert!(target.body.grounded);
                        hit_pair(
                            content,
                            &mut player,
                            &mut target,
                            &hit,
                            Vec3::new(0., 0.6, 0.8),
                        );
                        assert!(!target.body.grounded);
                        assert_eq!(target.body.velocity(), Vec3::new(0., hit.lift, hit.push));
                        transform.rotation =
                            Quat::from_rotation_arc(Vec3::Z, target.body.actor_facing);
                        let mut actions =
                            std::collections::BTreeSet::from([target.animation_action()]);
                        let mut max_y = transform.translation.y;
                        for _ in 0..(hz * 7.) as usize {
                            tick_pair(
                                content,
                                &world,
                                &motion,
                                &mut player,
                                &mut target,
                                &mut transform,
                                1. / hz,
                            );
                            actions.insert(if target.reacting() {
                                target.animation_action()
                            } else {
                                "idle"
                            });
                            max_y = max_y.max(transform.translation.y);
                            if obstacles >= 1 {
                                assert!(transform.translation.z < 8.);
                            }
                            if obstacles == 2 {
                                assert!(
                                    transform.translation.y + motion.collision_axis_scales[1] * 2.
                                        < 5.01
                                );
                            }
                            if !target.reacting() {
                                break;
                            }
                        }
                        assert!(max_y > 0.1, "heavy must actually launch");
                        for action in [
                            "blow_start",
                            "blow_fall",
                            "get_up",
                            "down",
                            "stand_up",
                            "idle",
                        ] {
                            assert!(actions.contains(action), "missing {action}: {actions:?}");
                        }
                        assert!(target.body.grounded);
                    }
                }
            }
        }
    }
}
