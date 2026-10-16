use super::contact::actor_contact;
use super::*;

pub(crate) fn install(app: &mut App) {
    app.insert_resource(PracticeSmoke {
        active: Some(true),
        ..default()
    })
    .add_systems(
        FixedUpdate,
        practice_walk
            .in_set(SimulationSet::Prepare)
            .after(capture_player)
            .before(crate::gameplay::prepare_input)
            .run_if(crate::loading::ready),
    )
    .add_systems(
        Update,
        practice_smoke
            .after(practice_animation::animate)
            .run_if(crate::loading::ready),
    );
}

#[derive(Resource, Default)]
struct PracticeSmoke {
    active: Option<bool>,
    stage: usize,
    actions: std::collections::BTreeSet<String>,
    captures: std::collections::BTreeSet<String>,
    checks: Vec<String>,
    walk_started: bool,
    walk_until: f32,
    walk_direction: Vec3,
    walk_start: Vec3,
    contacts_before_walk: u32,
    contact_separation: Option<f32>,
    death_head_height: Option<f32>,
    death_pelvis_height: Option<f32>,
    death_capture_age: Option<f32>,
    saved: bool,
}

fn practice_walk(
    time: Res<Time<Fixed>>,
    smoke: Res<PracticeSmoke>,
    mut controls: ResMut<crate::gameplay::Controls>,
) {
    if smoke.active != Some(true) || !smoke.walk_started || smoke.saved {
        return;
    }
    let movement = if time.elapsed_secs() < smoke.walk_until {
        Vec2::Y
    } else {
        Vec2::ZERO
    };
    let yaw = smoke.walk_direction.x.atan2(-smoke.walk_direction.z);
    controls.practice_movement(movement, yaw);
}

fn practice_smoke(
    content: Res<crate::content::simulation::SimulationContent>,
    mut commands: Commands,
    time: Res<Time>,
    mut smoke: ResMut<PracticeSmoke>,
    mut chat: ResMut<ChatState>,
    mut player: ResMut<Player>,
    motion: Res<Motion>,
    level: Res<Level>,
    practice: Res<PracticeAssets>,
    animators: Query<&PracticeAnimator>,
    mut targets: Query<(&mut PracticeTarget, &mut Transform)>,
    stats: Res<CombatStats>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if smoke.saved {
        return;
    }
    let elapsed = time.elapsed_secs();
    if let Ok((target, _)) = targets.single() {
        let action = target.animation_action().to_owned();
        smoke.actions.insert(action.clone());
        if (action != "idle" && target.reaction_age >= 0.06 || action == "idle" && elapsed >= 1.2)
            && smoke.captures.insert(action.clone())
        {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(format!(
                    "logs/practice-{action}.png"
                )));
        }
        if target.reaction == Reaction::Dead
            && target.reaction_age >= practice.duration("death") * 0.96
            && smoke.captures.insert("death-fall".into())
        {
            let animation = animators
                .single()
                .expect("Receiver must have its own displayed pose");
            assert!(
                animation.displayed_head_height < 0.5
                    && animation.displayed_pelvis_height < 0.4
                    && (animation.displayed_head_height - animation.displayed_pelvis_height).abs()
                        < 0.2,
                "Displayed death pose must have fallen: head {}, pelvis {}",
                animation.displayed_head_height,
                animation.displayed_pelvis_height,
            );
            smoke.death_head_height = Some(animation.displayed_head_height);
            smoke.death_pelvis_height = Some(animation.displayed_pelvis_height);
            smoke.death_capture_age = Some(target.reaction_age);
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(
                    "logs/practice-death-fall.png",
                ));
        }
    }
    let schedule = [
        0.6, 1.4, 2.5, 4.4, 6.8, 7.3, 8.1, 10.9, 11.2, 13.1, 13.5, 14.1,
    ];
    if smoke.stage >= schedule.len() || elapsed < schedule[smoke.stage] {
        return;
    }
    use crate::melee::{MeleeContext, MeleeInput, Weapon};
    match smoke.stage {
        0 => chat.practice_toggle_requested = true,
        1 | 2 | 3 | 5 => {
            let (mut target, mut transform) = targets
                .single_mut()
                .expect("Practice target was not spawned");
            if smoke.stage != 5 {
                target.body.health = 100.0;
            }
            let health = target.health();
            target.body = Player::new(target.home, &motion);
            target.body.health = health;
            target.set_reaction(Reaction::Idle);
            transform.translation = target.home;
            player.melee.reset();
            player.melee.equipped = Weapon::PlasmaSword;
            let airborne = smoke.stage == 2;
            let heavy = smoke.stage == 3;
            let sp = player.sp;
            player.melee.tick(
                &content,
                MeleeInput {
                    primary_pressed: true,
                    primary_held: heavy,
                    primary_released: !heavy,
                    ..default()
                },
                MeleeContext {
                    grounded: !airborne,
                    plasma_jump_attack: airborne,
                    guard_lower_busy: false,
                },
                if heavy { 0.21 } else { 0.016 },
                sp,
            );
        }
        4 => {
            let (mut target, mut transform) = targets.single_mut().unwrap();
            target.body = Player::new(target.home, &motion);
            target.body.health = 1.0;
            target.stun = 0.0;
            target.set_reaction(Reaction::Idle);
            transform.translation = target.home;
        }
        6 => {
            let (target, _) = targets.single().unwrap();
            assert_eq!(
                target.health(),
                0.0,
                "Lethal confirmed hit must enter death"
            );
            assert!(!target.solid(player.pose_reset_serial));
            smoke
                .checks
                .push("Dead receiver no longer blocks the player".into());
        }
        7 => {
            let (target, _) = targets.single_mut().unwrap();
            assert_eq!(target.health(), 100.0);
            assert_eq!(target.reaction, Reaction::Idle);
            smoke
                .checks
                .push("Practice target respawns automatically after three seconds".into());
        }
        8 => {
            let (target, transform) = targets.single().unwrap();
            assert!(target.solid(player.pose_reset_serial));
            let axes = Vec3::from(motion.collision_axis_scales);
            let from = transform.translation + Vec3::X * -8.0;
            let to = transform.translation + Vec3::X * 8.0;
            let (contact, _) = actor_contact(
                &level.collision,
                from,
                to,
                transform.translation,
                axes,
                motion.movement_skin_fraction,
            )
            .unwrap();
            assert!(contact.x <= transform.translation.x - axes.x * 1.99);
            smoke.checks.push(
                "Long swept actor movement stops on receiver's near side without tunneling".into(),
            );
            let direction =
                Vec3::new(player.actor_facing.x, 0.0, player.actor_facing.z).normalize_or_zero();
            assert!(direction.length_squared() > 0.99);
            let from = level
                .collision
                .move_ellipsoid(
                    transform.translation - direction * 6.0 + Vec3::Y * 2.0,
                    Vec3::NEG_Y * 8.0,
                    axes,
                    motion.movement_skin_fraction,
                )
                .position;
            let reset_serial = player.pose_reset_serial;
            let respawns = player.respawns;
            *player = Player::new(from, &motion);
            player.pose_reset_serial = reset_serial;
            player.respawns = respawns;
            player.melee.equipped = Weapon::PlasmaSword;
            smoke.walk_started = true;
            smoke.walk_until = 13.0;
            smoke.walk_direction = direction;
            smoke.walk_start = from;
            smoke.contacts_before_walk = stats.actor_contacts;
        }
        9 => {
            let (target, transform) = targets.single().unwrap();
            assert!(target.solid(player.pose_reset_serial));
            assert!(
                stats.actor_contacts > smoke.contacts_before_walk,
                "Normal player simulation must actually collide with receiver"
            );
            assert!(
                player.feet.distance(smoke.walk_start) > 2.5,
                "Player controller never walked toward the target"
            );
            let axes = Vec3::from(motion.collision_axis_scales);
            let separation = transform.translation - player.feet;
            let radius = 2.0 / (smoke.walk_direction / axes).length();
            let forward_separation = separation.dot(smoke.walk_direction);
            assert!(
                forward_separation >= radius * 0.99
                    && forward_separation <= radius + 0.2
                    && (separation / (axes * 2.0)).length() >= 0.9999,
                "Player must stop on near side of the actual receiver: {separation:?}"
            );
            smoke.contact_separation = Some(forward_separation);
            smoke.checks.push(
                "Normal fixed-step player controller walks into receiver and stops at its near-side body boundary".into(),
            );
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(
                    "logs/practice-solid-contact.png",
                ));
            smoke.captures.insert("solid-contact".into());
        }
        10 => {
            player.pose_reset_serial = player.pose_reset_serial.wrapping_add(1);
        }
        11 => {
            assert!(
                targets.is_empty(),
                "Player reset must remove receiver and its collision"
            );
            assert!(stats.idle_frames > 0 && stats.reaction_frames > 0);
            assert!(
                smoke.death_head_height.is_some(),
                "Death must be verified from its displayed fallen pose"
            );
            assert!(
                stats.hurt_sounds >= 3 && stats.death_sounds >= 1,
                "Confirmed reactions must play original voices"
            );
            for action in ["flinch_0", "stun_start", "blow_start", "death"] {
                assert!(
                    smoke.actions.contains(action),
                    "Missing rendered reaction {action}: {:?}",
                    smoke.actions
                );
            }
            smoke
                .checks
                .push("Player reset removes receiver, voice children and collision".into());
            let report = serde_json::json!({ "passed": true, "actions": smoke.actions, "captures": smoke.captures,
                "checks": smoke.checks, "animated_frames": stats.animated_frames, "idle_frames": stats.idle_frames,
                "reaction_frames": stats.reaction_frames, "hurt_sounds": stats.hurt_sounds, "death_sounds": stats.death_sounds,
                "hits": stats.hits, "actor_contacts": stats.actor_contacts,
                "controller_contact_separation_m": smoke.contact_separation,
                "controller_contact_count": stats.actor_contacts - smoke.contacts_before_walk,
                "death_displayed_head_height_m": smoke.death_head_height,
                "death_displayed_pelvis_height_m": smoke.death_pelvis_height,
                "death_capture_age_s": smoke.death_capture_age,
                "scope": "Local receiver fixture using original clips/voices and normal player controller; melee input is covered separately" });
            std::fs::write(
                "logs/practice-smoke.json",
                serde_json::to_string_pretty(&report).unwrap(),
            )
            .unwrap();
            info!("Practice smoke passed: {report}");
            smoke.saved = true;
            exit.write(bevy::app::AppExit::Success);
        }
        _ => unreachable!(),
    }
    smoke.stage += 1;
}
