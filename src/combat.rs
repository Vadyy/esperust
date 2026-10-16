#[cfg(test)]
use crate::combat_geometry::in_hit_volume;
use crate::simulation::SimulationSet;
use crate::{
    chat::ChatState,
    gameplay::{Motion, Player},
    melee::{Attack, HitSpec},
    practice_animation::{self, PracticeAnimator, PracticeAssets},
    scene::{self, Level, MapMesh},
};
use bevy::prelude::*;
use serde::Deserialize;

mod attacks;
mod contact;
mod cues;
mod presentation;
mod resolution;
mod target;
pub use target::PracticeTarget;
use target::{PRACTICE_RESPAWN_SECONDS, Reaction};
pub(crate) use target::{PracticeFighter, SparringMode};

#[cfg(test)]
mod pipeline_tests;

mod practice_verification;
pub(crate) use practice_verification::install as install_verification;

pub struct CombatPlugin;
impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatStats>()
            .init_resource::<ActorSweepStart>()
            .init_resource::<cues::CombatCues>()
            .add_message::<CombatHit>()
            .add_message::<CombatAttack>()
            .add_message::<ActorEffect>()
            .add_message::<crate::gameplay::CameraShakeRequest>()
            .add_systems(crate::loading::PrepareWorld, practice_animation::load)
            .add_systems(
                Update,
                ((toggle_target, target_status)
                    .chain()
                    .after(scene::switch_maps))
                .run_if(crate::loading::ready),
            )
            .add_systems(
                FixedUpdate,
                (capture_player
                    .in_set(SimulationSet::Prepare)
                    .before(crate::gameplay::prepare_input))
                .run_if(crate::loading::ready),
            )
            .add_systems(
                FixedUpdate,
                ((resolve_combat, presentation::present)
                    .chain()
                    .in_set(SimulationSet::ResolveCombat))
                .run_if(crate::loading::ready),
            )
            .add_systems(
                Update,
                (practice_animation::animate.after(target_status)).run_if(crate::loading::ready),
            );
    }
}

#[derive(Message, Clone, Copy)]
pub struct CombatHit {
    pub position: Vec3,
    pub attack: Attack,
    pub hit_index: u8,
    pub attribute: crate::combat_types::AttackAttributeId,
    pub critical: bool,
    pub local_attacker: bool,
    pub lift: f32,
    pub stun: f32,
}

#[derive(Message, Clone, Copy)]
pub(crate) struct CombatAttack {
    pub owner: Entity,
    pub attack: Attack,
    pub spec: HitSpec,
    pub origin: Transform,
}

#[derive(Message, Clone)]
pub(crate) struct ActorEffect {
    pub owner: Option<Entity>,
    pub name: &'static str,
    pub origin: Transform,
}

#[derive(Resource, Default)]
pub struct CombatStats {
    pub hits: u32,
    pub total_damage: f32,
    pub kills: u32,
    pub animated_frames: u32,
    pub idle_frames: u32,
    pub reaction_frames: u32,
    pub hurt_sounds: u32,
    pub death_sounds: u32,
    pub actor_contacts: u32,
    pub critical_hits: u32,
    pub guarded_hits: u32,
    pub countered_hits: u32,
    pub guard_crushes: u32,
    pub incoming_hits: u32,
    pub reactions: u32,
}

#[derive(Component)]
pub(crate) struct ReactionVoice(Entity);

#[derive(Resource, Default)]
pub(crate) struct ActorSweepStart {
    feet: Vec3,
    reset_serial: u32,
}

fn capture_player(player: Res<Player>, mut start: ResMut<ActorSweepStart>) {
    start.feet = player.feet;
    start.reset_serial = player.pose_reset_serial;
}

#[derive(Component)]
pub(crate) struct PracticeStatus;

#[derive(Deserialize)]
struct Character {
    meshes: Vec<MapMesh>,
}

fn toggle_target(
    mut commands: Commands,
    mut chat: ResMut<ChatState>,
    player: Res<Player>,
    level: Res<Level>,
    targets: Query<Entity, With<PracticeTarget>>,
    labels: Query<Entity, With<PracticeStatus>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
    practice: Res<PracticeAssets>,
    motion: Res<Motion>,
) {
    let sparring = chat.practice_sparring_requested.take();
    if !std::mem::take(&mut chat.practice_toggle_requested) && sparring.is_none() {
        return;
    }
    if !targets.is_empty() {
        for entity in targets.iter().chain(labels.iter()) {
            commands.entity(entity).despawn();
        }
        chat.system("Practice target removed.");
        return;
    }
    let forward = player.actor_facing.normalize_or_zero();
    let requested = player.feet + forward * 3.5 + Vec3::Y * 2.0;
    let position = level
        .collision
        .move_ellipsoid(
            requested,
            Vec3::NEG_Y * 8.0,
            Vec3::from(motion.collision_axis_scales),
            motion.movement_skin_fraction,
        )
        .position;
    let mut receiver = PracticeTarget::new(position, player.pose_reset_serial);
    if let Some(mode) = sparring {
        receiver.fighter = Some(PracticeFighter::new(mode));
    }
    let target = commands
        .spawn((
            Name::new("Practice target"),
            receiver,
            Transform::from_translation(position)
                .with_rotation(Quat::from_rotation_y((-forward.x).atan2(-forward.z)))
                .with_scale(Vec3::splat(scene::PLAYER_MODEL_SCALE)),
            Visibility::default(),
        ))
        .id();
    let character: Character = serde_json::from_slice(
        &std::fs::read("assets/player.json").expect("Practice target requires original character"),
    )
    .expect("Invalid original character export");
    let handles = scene::spawn_meshes(
        character.meshes,
        &mut commands,
        &mut meshes,
        &mut materials,
        &assets,
        Some(target),
        None,
    );
    commands
        .entity(target)
        .insert(PracticeAnimator::new(handles, &practice));
    if sparring.is_some() {
        crate::weapons::spawn_practice_equipment(
            target,
            &mut commands,
            &mut meshes,
            &mut materials,
            &assets,
        );
    }
    commands.spawn((
        PracticeStatus,
        Text::new("Practice target: 100 / 100"),
        TextFont {
            font: assets.load("ui/verdana.ttf"),
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(86),
            left: percent(40),
            ..default()
        },
    ));
    chat.system(match sparring {
        Some(SparringMode::CounterHeavy) => {
            "Counter Sword sparring: heavy attacks every 3 seconds; /dummy removes it."
        }
        _ => "Practice target placed. Attack it to test damage; /dummy removes it.",
    });
}

fn target_status(
    targets: Query<&PracticeTarget>,
    mut labels: Query<&mut Text, With<PracticeStatus>>,
) {
    if let Ok(target) = targets.single() {
        for mut text in &mut labels {
            text.0 = if target.health() <= 0.0 {
                format!(
                    "Practice target respawns in {:.0}s",
                    (PRACTICE_RESPAWN_SECONDS - target.reaction_age)
                        .max(0.0)
                        .ceil()
                )
            } else {
                format!(
                    "Practice target: {:.0} / 100{}",
                    target.health(),
                    if target.stun > 0.0 { " · stunned" } else { "" }
                )
            };
        }
    }
}

fn resolve_combat(
    content: Res<crate::content::simulation::SimulationContent>,
    mut commands: Commands,
    mut player: ResMut<Player>,
    motion: Res<Motion>,
    level: Res<Level>,
    time: Res<Time<Fixed>>,
    mut targets: Query<(Entity, &mut PracticeTarget, &mut Transform)>,
    labels: Query<Entity, With<PracticeStatus>>,
    mut stats: ResMut<CombatStats>,
    start: Res<ActorSweepStart>,
    mut cues: ResMut<cues::CombatCues>,
) {
    let dt = time.delta_secs();
    for (entity, mut target, mut transform) in &mut targets {
        if target.reset_serial != player.pose_reset_serial {
            commands.entity(entity).despawn();
            for label in &labels {
                commands.entity(label).despawn();
            }
            continue;
        }
        let previous_reaction = target.reaction;
        let previous_feet = transform.translation;
        if contact::step_target(
            &content,
            &mut target,
            &mut transform,
            &mut player,
            &motion,
            &level.collision,
            &start,
            dt,
        ) {
            stats.actor_contacts += 1;
        }
        if target.health() > 0.
            && level.death_zones.iter().any(|zone| {
                zone.intersects(
                    previous_feet,
                    transform.translation,
                    Vec3::from(motion.collision_axis_scales),
                    level.local_team,
                )
            })
        {
            target.die();
            cues.0.push(cues::CombatCue::ClearVoices(entity));
            cues.0.push(cues::CombatCue::Voice {
                owner: entity,
                name: "_eu_voice_man_dead_normal",
            });
        }
        if previous_reaction != Reaction::StandUp && target.reaction == Reaction::StandUp {
            cues.0.push(cues::CombatCue::Voice {
                owner: entity,
                name: "get_up",
            });
        }
        if target.respawn_if_ready(&mut transform) {
            cues.0.push(cues::CombatCue::ClearVoices(entity));
        }
        if let Some(batch) = target
            .fighter
            .as_ref()
            .and_then(|fighter| attacks::collect(&content, &fighter.melee))
        {
            resolution::incoming(
                &content,
                &batch,
                entity,
                &mut target,
                &transform,
                &mut player,
                &level.collision,
                &mut stats,
                &mut cues.0,
            );
        }
        if let Some(batch) = attacks::collect(&content, &player.melee) {
            resolution::outgoing(
                &content,
                &batch,
                entity,
                &mut target,
                &mut transform,
                &mut player,
                &level.collision,
                &mut stats,
                &mut cues.0,
            );
        }
    }
    resolution::finish_player(
        &content,
        &mut player,
        &motion,
        &level.collision,
        &mut cues.0,
    );
    for (_, mut target, transform) in &mut targets {
        resolution::finish_target(&content, &mut target, &transform, &level.collision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{contact::actor_contact, resolution::hit_direction};
    use crate::content::simulation::parity_content;
    use crate::melee::HitShape;
    use crate::melee::{Melee, MeleeContext, MeleeInput, Weapon};

    fn weak() -> HitSpec {
        let mut melee = Melee::default();
        melee.equipped = Weapon::PlasmaSword;
        let context = MeleeContext {
            grounded: true,
            ..default()
        };
        melee.tick(
            parity_content(),
            MeleeInput {
                primary_pressed: true,
                primary_released: true,
                ..default()
            },
            context,
            0.016,
            100.0,
        );
        melee.tick(parity_content(), default(), context, 0.208, 100.0);
        melee.hit_spec(parity_content()).unwrap()
    }

    #[test]
    fn counter_combo_and_heavy_camera_turn_redirect_volume_and_critical_ray_in_same_swing() {
        for heavy in [false, true] {
            let mut melee = Melee::default();
            melee.equipped = Weapon::CounterSword;
            melee.aim_direction = Vec3::NEG_Z.to_array();
            let context = MeleeContext {
                grounded: true,
                ..default()
            };
            melee.tick(
                parity_content(),
                MeleeInput {
                    primary_pressed: true,
                    primary_held: true,
                    ..default()
                },
                context,
                0.016,
                100.,
            );
            let input = if heavy {
                MeleeInput {
                    primary_held: true,
                    ..default()
                }
            } else {
                MeleeInput {
                    primary_released: true,
                    ..default()
                }
            };
            melee.tick(
                parity_content(),
                input,
                context,
                if heavy { 0.21 } else { 0.016 },
                100.,
            );
            assert_eq!(
                melee.attack,
                Some(if heavy {
                    Attack::CounterRetaliation
                } else {
                    Attack::CounterCombo(0)
                })
            );
            let serial = melee.serial;
            melee.tick(parity_content(), default(), context, 0.064, 100.);
            melee.aim_direction = [0.6, 0.8, 0.];
            melee.tick(parity_content(), default(), context, 0.3, 100.);
            let specs = melee.hit_specs(parity_content());
            assert!(specs.iter().any(|spec| spec.critical));
            assert!(specs.iter().any(|spec| !spec.critical));
            let forward = Vec3::from(melee.strike_direction());
            for spec in specs {
                assert!(in_hit_volume(
                    &spec,
                    Vec3::ZERO,
                    forward,
                    Vec3::new(3., 2.5, 0.)
                ));
                assert!(!in_hit_volume(
                    &spec,
                    Vec3::ZERO,
                    forward,
                    Vec3::new(0., 2.5, -3.)
                ));
            }
            assert_eq!(melee.serial, serial);
        }
    }
    #[test]
    fn counter_jump_camera_turn_redirects_critical_ray_after_launch() {
        let mut melee = Melee::default();
        melee.equipped = Weapon::CounterSword;
        melee.aim_direction = Vec3::NEG_Z.to_array();
        let context = MeleeContext {
            plasma_jump_attack: true,
            ..default()
        };
        melee.tick(
            parity_content(),
            MeleeInput {
                primary_pressed: true,
                primary_held: true,
                ..default()
            },
            context,
            0.016,
            100.,
        );
        melee.tick(
            parity_content(),
            MeleeInput {
                primary_released: true,
                ..default()
            },
            context,
            0.016,
            100.,
        );
        let serial = melee.serial;
        let launch = melee.tick(parity_content(), default(), context, 0.401, 100.);
        assert_eq!(launch.velocity_world, Some([0., -40., -60.]));
        melee.aim_direction = [0.6, 0.8, 0.];
        let later = melee.tick(parity_content(), default(), context, 0.199, 100.);
        assert_eq!(later.velocity_world, None);
        let specs = melee.hit_specs(parity_content());
        let ray = specs.iter().find(|spec| spec.critical).unwrap();
        let forward = Vec3::from(melee.strike_direction());
        assert!(in_hit_volume(
            ray,
            Vec3::ZERO,
            forward,
            Vec3::new(3., 2.5, 0.)
        ));
        assert!(!in_hit_volume(
            ray,
            Vec3::ZERO,
            forward,
            Vec3::new(0., 2.5, -3.)
        ));
        assert!(
            specs
                .iter()
                .any(|spec| matches!(spec.shape, HitShape::Cylinder { .. }))
        );
        assert_eq!(melee.serial, serial);
    }
    #[test]
    fn source_fan_hits_in_front_and_rejects_behind_far_side_and_above() {
        let spec = weak();
        assert!(in_hit_volume(
            &spec,
            Vec3::ZERO,
            Vec3::NEG_Z,
            Vec3::new(0.0, 1.6, -3.0)
        ));
        for target in [
            Vec3::new(0.0, 1.6, 3.0),
            Vec3::new(5.0, 1.6, -1.0),
            Vec3::new(0.0, 7.0, -3.0),
        ] {
            assert!(!in_hit_volume(&spec, Vec3::ZERO, Vec3::NEG_Z, target));
        }
    }

    #[test]
    fn source_volume_rotates_with_attack_facing() {
        let spec = weak();
        assert!(in_hit_volume(
            &spec,
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(3.0, 1.6, 0.0)
        ));
        assert!(!in_hit_volume(
            &spec,
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(-3.0, 1.6, 0.0)
        ));
    }

    #[test]
    fn native_box_origin_is_the_rear_plane_and_length_extends_forward() {
        let mut spec = weak();
        spec.shape = HitShape::Box {
            origin: [0.0, 2.5, 0.0],
            width: 2.0,
            height: 2.0,
            length: 5.0,
        };
        assert!(in_hit_volume(
            &spec,
            Vec3::ZERO,
            Vec3::NEG_Z,
            Vec3::new(0.0, 1.6, -4.8)
        ));
        assert!(!in_hit_volume(
            &spec,
            Vec3::ZERO,
            Vec3::NEG_Z,
            Vec3::new(0.0, 1.6, 1.0)
        ));
    }

    #[test]
    fn swept_actor_contact_blocks_high_speed_crossings_and_allows_escaping_and_overhead_motion() {
        let world = crate::controller::CollisionWorld::default();
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let (position, normal) = actor_contact(
            &world,
            Vec3::new(-20.0, 0.0, 0.0),
            Vec3::new(20.0, 0.0, 0.0),
            Vec3::ZERO,
            axes,
            0.015,
        )
        .unwrap();
        assert!(position.x < -2.0 && position.x > -2.01, "{position}");
        assert!(normal.x < -0.999);
        assert!(
            actor_contact(
                &world,
                position,
                position - Vec3::X * 4.0,
                Vec3::ZERO,
                axes,
                0.015
            )
            .is_none()
        );
        assert!(
            actor_contact(
                &world,
                Vec3::new(-20.0, 3.3, 0.0),
                Vec3::new(20.0, 3.3, 0.0),
                Vec3::ZERO,
                axes,
                0.015
            )
            .is_none()
        );
        assert!(
            actor_contact(
                &world,
                Vec3::new(-20.0, 0.0, 2.1),
                Vec3::new(20.0, 0.0, 2.1),
                Vec3::ZERO,
                axes,
                0.015
            )
            .is_none()
        );
    }

    #[test]
    fn actor_slide_remains_outside_receiver_and_original_map_wall() {
        let world = crate::controller::CollisionWorld::new(vec![
            [
                Vec3::new(-50.0, -50.0, 2.0),
                Vec3::new(50.0, -50.0, 2.0),
                Vec3::new(50.0, 50.0, 2.0),
            ],
            [
                Vec3::new(-50.0, -50.0, 2.0),
                Vec3::new(50.0, 50.0, 2.0),
                Vec3::new(-50.0, 50.0, 2.0),
            ],
        ]);
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let start = Vec3::new(-5.0, 0.0, 0.0);
        let map_position = world
            .move_ellipsoid(start, Vec3::new(10.0, 0.0, 3.0), axes, 0.015)
            .position;
        let (position, _) =
            actor_contact(&world, start, map_position, Vec3::ZERO, axes, 0.015).unwrap();
        assert!(
            position.z <= 1.001,
            "Actor slide crossed map wall: {position}"
        );
        assert!(
            (position / (axes * 2.0)).length() >= 0.9999,
            "Actor slide entered receiver: {position}"
        );
    }

    #[test]
    fn overlapping_actor_can_move_out_and_stationary_overlap_is_resolved() {
        let world = crate::controller::CollisionWorld::default();
        let axes = Vec3::new(1.0, 1.6, 1.0);
        assert!(actor_contact(&world, Vec3::X, Vec3::X * 3.0, Vec3::ZERO, axes, 0.015).is_none());
        let (position, _) =
            actor_contact(&world, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, axes, 0.015).unwrap();
        assert!(position.is_finite() && position.x >= 2.0);
    }

    #[test]
    fn dead_and_previous_map_or_respawn_receivers_have_no_collision() {
        let mut target = PracticeTarget::new(Vec3::ZERO, 4);
        assert!(target.solid(4));
        assert!(!target.solid(5));
        target.body.health = 0.0;
        assert!(!target.solid(4));
    }

    #[test]
    fn receiver_reaction_direction_tracks_its_own_facing() {
        assert_eq!(hit_direction(Vec3::Z, Vec3::Z), 0);
        assert_eq!(hit_direction(Vec3::Z, Vec3::NEG_Z), 1);
        assert_eq!(hit_direction(Vec3::Z, Vec3::NEG_X), 2);
        assert_eq!(hit_direction(Vec3::Z, Vec3::X), 3);
        assert_eq!(hit_direction(Vec3::X, Vec3::X), 0);
    }
}

#[cfg(test)]
mod actor_contract_tests;

#[cfg(test)]
mod receiver_parity_tests;
