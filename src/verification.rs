use crate::{
    Options,
    chat::ChatState,
    gameplay::Player,
    scene::{MapEntity, MapId, MapSelection},
};
use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    mesh::VertexAttributeValues,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{CursorGrabMode, CursorOptions},
};

#[derive(Resource, Default)]
struct ChatSmoke {
    stage: usize,
    checks: Vec<String>,
    clicked: bool,
    cursor_before: Option<(CursorGrabMode, bool)>,
}

#[derive(Resource, Default)]
struct MovementSmoke {
    stage: usize,
    checks: Vec<String>,
    chat_action_counts: Option<(u32, u32, f32)>,
    shift_start_dodges: u32,
    cancel_stage: u8,
}

#[derive(Resource, Default)]
struct AnimationSmoke {
    clicked: bool,
    turn: Option<usize>,
    released: bool,
    screenshots: usize,
    frames: usize,
    minimum_body_spread: Option<f64>,
    saved: bool,
    jump_stage: u8,
    jump_captured: bool,
}

pub fn install(app: &mut App) {
    let options = app.world().resource::<Options>().clone();
    if options.emotes_check {
        crate::emotes::install_verification(app);
    }
    if options.profile {
        crate::performance::install(app);
    }
    if options.loading_check {
        crate::loading::install_verification(app);
    }
    if options.hud_smoke || options.movement_smoke {
        crate::hud::install_verification(app);
    }
    if options.movement_smoke {
        crate::effects::install_verification(app);
    }
    if options.melee_smoke {
        app.add_plugins(crate::melee_verification::MeleeVerificationPlugin);
    }
    if options.counter_smoke {
        app.add_plugins(crate::counter_verification::CounterVerificationPlugin);
    }
    if options.counter_combat_check {
        app.add_plugins(crate::counter_combat_verification::CounterCombatVerificationPlugin);
    }
    if options.counter_followup_check {
        app.add_plugins(crate::counter_followup_verification::CounterFollowupVerificationPlugin);
    }
    if options.dagger_effect_check {
        app.add_plugins(crate::dagger_effect_check::DaggerEffectCheckPlugin);
    }
    if options.practice_smoke {
        crate::combat::install_verification(app);
    }
    if options.smoke {
        app.add_systems(
            FixedUpdate,
            scripted_command
                .in_set(crate::simulation::SimulationSet::Prepare)
                .after(crate::gameplay::prepare_input)
                .run_if(crate::loading::ready),
        );
    }
    if !options.chat_smoke && !options.movement_smoke && !options.animation_smoke {
        return;
    }
    app.add_systems(
        PreUpdate,
        scripted_focus
            .before(InputSystems)
            .run_if(crate::loading::ready),
    );
    if options.chat_smoke {
        app.init_resource::<ChatSmoke>().add_systems(
            PreUpdate,
            drive_chat
                .after(scripted_focus)
                .before(InputSystems)
                .run_if(crate::loading::ready),
        );
    }
    if options.movement_smoke {
        app.init_resource::<MovementSmoke>().add_systems(
            PreUpdate,
            drive_movement
                .after(scripted_focus)
                .after(drive_chat)
                .before(InputSystems)
                .run_if(crate::loading::ready),
        );
    }
    if options.animation_smoke {
        app.init_resource::<AnimationSmoke>()
            .add_systems(
                PreUpdate,
                drive_animation
                    .after(scripted_focus)
                    .after(drive_movement)
                    .before(InputSystems)
                    .run_if(crate::loading::ready),
            )
            .add_systems(
                Update,
                inspect_animation
                    .after(crate::animation::animate)
                    .run_if(crate::loading::ready),
            );
    }
}

fn scripted_command(
    time: Res<Time<Fixed>>,
    options: Res<Options>,
    mut command: ResMut<crate::gameplay::MovementInput>,
) {
    if options.profile {
        *command = crate::gameplay::MovementInput {
            yaw: options.yaw.unwrap_or(0.),
            pitch: -0.12,
            ..default()
        };
    }
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    command.movement = if t > 2. && t < 3.5 {
        Vec2::Y
    } else if t > 9. && t < 10.5 {
        Vec2::X
    } else if t > 13. && t < 14. {
        -Vec2::X
    } else if t > 15. && t < 16. {
        Vec2::Y
    } else {
        Vec2::ZERO
    };
    command.jump |= (t > 4. && t - dt <= 4.) || (t > 11. && t - dt <= 11.);
    command.dodge |= t > 13. && t - dt <= 13.;
    command.sprint |= t > 15. && t < 16.;
}

#[cfg(test)]
mod installation_tests {
    use super::*;

    #[test]
    fn ordinary_game_has_no_scripted_input_harness() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Options::default());
        install(&mut app);
        assert!(!app.world().contains_resource::<ChatSmoke>());
        assert!(!app.world().contains_resource::<MovementSmoke>());
        assert!(!app.world().contains_resource::<AnimationSmoke>());
        assert!(!app.is_plugin_added::<crate::melee_verification::MeleeVerificationPlugin>());
        app.update();
    }

    #[test]
    fn selected_weapon_check_installs_only_its_plugin() {
        let mut app = App::new();
        app.insert_resource(Options {
            counter_followup_check: true,
            ..default()
        });
        install(&mut app);
        assert!(app.is_plugin_added::<crate::counter_followup_verification::CounterFollowupVerificationPlugin>());
        assert!(!app.is_plugin_added::<crate::counter_verification::CounterVerificationPlugin>());
        assert!(!app.is_plugin_added::<crate::melee_verification::MeleeVerificationPlugin>());
        assert!(!app.world().contains_resource::<MovementSmoke>());
    }
}

fn scripted_focus(options: Res<Options>, mut windows: Query<&mut Window>) {
    if options.movement_smoke || options.chat_smoke || options.animation_smoke {
        for mut window in &mut windows {
            window.focused = true;
        }
    }
}

fn drive_animation(world: &mut World) {
    if !world.resource::<Options>().animation_smoke {
        return;
    }
    let time = world.resource::<Time>().elapsed_secs();
    if time >= 1.5 && !world.resource::<AnimationSmoke>().clicked {
        let window = world
            .query_filtered::<Entity, With<Window>>()
            .single(world)
            .unwrap();
        for state in [ButtonState::Pressed, ButtonState::Released] {
            world
                .resource_mut::<Messages<MouseButtonInput>>()
                .write(MouseButtonInput {
                    button: MouseButton::Left,
                    state,
                    window,
                });
        }
        world.resource_mut::<AnimationSmoke>().clicked = true;
    }
    if (2.0..5.0).contains(&time) {
        let turn = ((time - 2.0) / 0.065) as usize;
        if world.resource::<AnimationSmoke>().turn != Some(turn) {
            let (release, press) = if turn % 2 == 0 {
                (KeyCode::KeyD, KeyCode::KeyA)
            } else {
                (KeyCode::KeyA, KeyCode::KeyD)
            };
            movement_key(world, release, false);
            movement_key(world, press, true);
            world.resource_mut::<AnimationSmoke>().turn = Some(turn);
        }
    } else if time >= 5.0 && !world.resource::<AnimationSmoke>().released {
        movement_key(world, KeyCode::KeyA, false);
        movement_key(world, KeyCode::KeyD, false);
        world.resource_mut::<AnimationSmoke>().released = true;
    }
    let stage = world.resource::<AnimationSmoke>().jump_stage;
    if time
        >= [5.2, 5.4, 5.45, 6.1]
            .get(stage as usize)
            .copied()
            .unwrap_or(f32::INFINITY)
    {
        match stage {
            0 => movement_key(world, KeyCode::KeyW, true),
            1 => movement_key(world, KeyCode::Space, true),
            2 => movement_key(world, KeyCode::Space, false),
            3 => movement_key(world, KeyCode::KeyW, false),
            _ => unreachable!(),
        }
        world.resource_mut::<AnimationSmoke>().jump_stage += 1;
    }
}

fn inspect_animation(
    options: Res<Options>,
    time: Res<Time>,
    meshes: Res<Assets<Mesh>>,
    parts: Query<(&Name, &Mesh3d, &ChildOf)>,
    player: Res<Player>,
    animation: Res<crate::animation::Animation>,
    mut smoke: ResMut<AnimationSmoke>,
    mut commands: Commands,
) {
    if !options.animation_smoke {
        return;
    }
    let elapsed = time.elapsed_secs();
    if (2.05..5.0).contains(&elapsed) {
        let positions = parts
            .iter()
            .find_map(|(name, handle, _)| {
                if !name.as_str().starts_with("body/") {
                    return None;
                }
                match meshes.get(&handle.0)?.attribute(Mesh::ATTRIBUTE_POSITION)? {
                    VertexAttributeValues::Float32x3(positions) if positions.len() >= 200 => {
                        Some(positions)
                    }
                    _ => None,
                }
            })
            .expect("Animation smoke requires the original jacket mesh");
        assert!(
            positions.iter().flatten().all(|value| value.is_finite()),
            "Animation produced non-finite geometry"
        );
        let count = positions.len() as f64;
        let mean_x = positions.iter().map(|p| p[0] as f64).sum::<f64>() / count;
        let mean_z = positions.iter().map(|p| p[2] as f64).sum::<f64>() / count;
        let spread = (positions
            .iter()
            .map(|p| (p[0] as f64 - mean_x).powi(2) + (p[2] as f64 - mean_z).powi(2))
            .sum::<f64>()
            / count)
            .sqrt();
        assert!(
            spread > 0.13,
            "Rapid A/D collapsed the jacket: horizontal RMS {spread:.6}"
        );
        smoke.minimum_body_spread = Some(
            smoke
                .minimum_body_spread
                .map_or(spread, |old| old.min(spread)),
        );
        smoke.frames += 1;
        if smoke.screenshots < 12 && elapsed >= 2.05 + smoke.screenshots as f32 * 0.22 {
            let path = format!("logs/animation-ad-{:02}.png", smoke.screenshots);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
            smoke.screenshots += 1;
        }
    }
    if elapsed >= 5.95
        && player.normal_jump_active()
        && player.vertical_speed < 0.
        && !smoke.jump_captured
    {
        assert!(player.jump_started_moving);
        let (clip, cursor) = animation.upper_cursor().unwrap();
        assert_eq!(clip, "jump_moving");
        assert!(
            cursor > 2400.,
            "Forward rotation must keep advancing past the apex"
        );
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("logs/parity-forward-jump-descent.png"));
        smoke.jump_captured = true;
    }
    if elapsed >= 6.8 && !smoke.saved {
        assert!(
            smoke.frames > 20,
            "Animation smoke needs enough rendered turns"
        );
        assert!(smoke.jump_captured);
        let report = serde_json::json!({
            "passed":true, "rendered_frames":smoke.frames,
            "turn_interval_seconds":0.065,
            "minimum_jacket_horizontal_rms":smoke.minimum_body_spread,
            "collapse_threshold":0.13,
            "screenshots":smoke.screenshots,
            "forward_jump_descending_clip_continues":smoke.jump_captured,
        });
        std::fs::write(
            "logs/animation-smoke.json",
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        smoke.saved = true;
    }
}

fn drive_movement(world: &mut World) {
    if !world.resource::<Options>().movement_smoke {
        return;
    }
    let time = world.resource::<Time>().elapsed_secs();
    match world.resource::<MovementSmoke>().cancel_stage {
        0 if time >= 7.35 => {
            movement_key(world, KeyCode::Space, true);
            world.resource_mut::<MovementSmoke>().cancel_stage = 1;
        }
        1 if time >= 7.42 => {
            assert!(!matches!(
                world.resource::<Player>().action,
                crate::gameplay::MovementAction::DodgeLeft
                    | crate::gameplay::MovementAction::DodgeRight
            ));
            assert_eq!(world.resource::<Player>().dodges, 1);
            movement_key(world, KeyCode::Space, false);
            movement_key(world, KeyCode::KeyA, false);
            let mut smoke = world.resource_mut::<MovementSmoke>();
            smoke.cancel_stage = 2;
            smoke
                .checks
                .push("Fresh Space cancels dodge while A remains held".into());
        }
        _ => {}
    }
    let stage = world.resource::<MovementSmoke>().stage;
    let schedule = [
        1.5, 2.0, 2.45, 2.90, 3.35, 3.80, 4.25, 4.60, 6.20, 7.0, 7.30, 8.0, 8.08, 8.16, 9.0, 10.0,
        10.08, 10.90, 11.30, 11.40, 11.48, 11.56, 11.80, 12.0, 13.0, 14.0, 15.0, 15.04, 15.12,
        15.20, 15.30, 16.60, 16.68, 16.72, 16.88, 18.0, 19.0, 19.25, 20.0, 20.08, 20.45, 21.0,
        21.35, 22.0,
    ];
    if stage >= schedule.len() || world.resource::<Time>().elapsed_secs() < schedule[stage] {
        return;
    }
    match stage {
        0 => {
            let window = world
                .query_filtered::<Entity, With<Window>>()
                .single(world)
                .unwrap();
            for state in [ButtonState::Pressed, ButtonState::Released] {
                world
                    .resource_mut::<Messages<MouseButtonInput>>()
                    .write(MouseButtonInput {
                        button: MouseButton::Left,
                        state,
                        window,
                    });
            }
        }
        1 => movement_key(world, KeyCode::KeyW, true),
        2..=5 => {
            let directions = [
                KeyCode::KeyW,
                KeyCode::KeyA,
                KeyCode::KeyS,
                KeyCode::KeyD,
                KeyCode::KeyW,
            ];
            movement_key(world, directions[stage - 2], false);
            movement_key(world, directions[stage - 1], true);
        }
        6 => {
            movement_key(world, KeyCode::KeyW, false);
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("W A S D W through keyboard input".into());
        }
        7 => key(world, KeyCode::Space, Key::Space, None),
        8 => {
            let player = world.resource::<Player>();
            assert!(player.jumps > 0, "Space must jump");
            assert!(player.grounded, "Jump must return to the floor");
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Jump and landing complete".into());
        }
        9 => {
            movement_key(world, KeyCode::KeyA, true);
            movement_key(world, KeyCode::Space, true);
        }
        10 => {
            movement_key(world, KeyCode::Space, false);
            assert!(
                world.resource::<Player>().dodges > 0,
                "A + Space must dodge"
            );
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("A + Space lateral dodge".into());
        }
        11 | 13 => movement_key(world, KeyCode::KeyW, true),
        12 => movement_key(world, KeyCode::KeyW, false),
        14 => {
            let projection = world
                .query_filtered::<&Projection, With<crate::gameplay::View>>()
                .single(world)
                .unwrap();
            let Projection::Perspective(perspective) = projection else {
                panic!("Gameplay requires perspective");
            };
            assert!(
                perspective.fov.to_degrees() > 70.0,
                "Sprint must visibly widen the camera FOV"
            );
            movement_key(world, KeyCode::KeyW, false);
            assert!(
                world.resource::<Player>().sprint_distance > 0.1,
                "Double W must sprint"
            );
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Double W sprint widens the rendered camera FOV".into());
        }
        15 => {
            let projection = world
                .query_filtered::<&Projection, With<crate::gameplay::View>>()
                .single(world)
                .unwrap();
            let Projection::Perspective(perspective) = projection else {
                panic!("Gameplay requires perspective");
            };
            assert!(
                (perspective.fov.to_degrees() - 60.0).abs() < 1.0,
                "Released sprint must restore normal FOV"
            );
            prepare_walljump(world);
            movement_key(world, KeyCode::KeyW, true);
        }
        16 => key(world, KeyCode::Space, Key::Space, None),
        17 => {
            movement_key(world, KeyCode::KeyW, false);
            assert!(
                world.resource::<Player>().wall_jumps > 0,
                "Space beside an actual map wall must walljump"
            );
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Wall jump against imported map collision".into());
        }
        18 => key(world, KeyCode::Enter, Key::Enter, None),
        19 => {
            let p = world.resource::<Player>();
            let counts = (p.dodges, p.wall_jumps, p.sprint_distance);
            world.resource_mut::<MovementSmoke>().chat_action_counts = Some(counts);
            movement_key(world, KeyCode::KeyA, true);
            movement_key(world, KeyCode::KeyW, true);
            movement_key(world, KeyCode::Space, true);
        }
        20 => movement_key(world, KeyCode::KeyW, false),
        21 => movement_key(world, KeyCode::KeyW, true),
        22 => {
            let p = world.resource::<Player>();
            assert_eq!(
                Some((p.dodges, p.wall_jumps, p.sprint_distance)),
                world.resource::<MovementSmoke>().chat_action_counts
            );
            assert!(world.resource::<ChatState>().active);
            movement_key(world, KeyCode::KeyA, false);
            movement_key(world, KeyCode::Space, false);
            movement_key(world, KeyCode::KeyW, false);
            key(world, KeyCode::Escape, Key::Escape, None);
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Chat blocks dodge, sprint and wall jump input".into());
        }
        23 => {
            assert!(!world.resource::<ChatState>().active);
            assert!(world.resource::<Player>().feet.is_finite());
        }
        24 => assert!(world.resource::<Player>().feet.is_finite()),
        25 | 35 => {
            world.resource_scope(|world, mut player: Mut<Player>| {
                let level = world.resource::<crate::scene::Level>();
                let mut controls = crate::gameplay::Controls::default();
                crate::gameplay::reset_for_level(&mut player, &mut controls, level);
            });
        }
        26 | 31 => {
            assert!(
                world.resource::<Player>().grounded,
                "Air-action test must begin with an actual jump from the map floor"
            );
            key(world, KeyCode::Space, Key::Space, None);
        }
        27 | 29 => movement_key(world, KeyCode::KeyW, true),
        28 => movement_key(world, KeyCode::KeyW, false),
        30 => {
            assert!(
                world.resource::<Player>().air_sprints > 0,
                "Double W while airborne must enter native FastRun"
            );
            assert!(!world.resource::<Player>().grounded);
            movement_key(world, KeyCode::KeyW, false);
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Double W enters sprint during an actual jump".into());
        }
        32 => movement_key(world, KeyCode::KeyD, true),
        33 => movement_key(world, KeyCode::Space, true),
        34 => {
            assert!(
                world.resource::<Player>().air_dodges > 0,
                "D held plus a new Space press must dodge during a jump"
            );
            movement_key(world, KeyCode::KeyD, false);
            movement_key(world, KeyCode::Space, false);
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("D + fresh Space enters dodge during an actual jump".into());
        }
        36 => {
            let count = world.resource::<Player>().dodges;
            world.resource_mut::<MovementSmoke>().shift_start_dodges = count;
            movement_key(world, KeyCode::KeyA, true);
            movement_key(world, KeyCode::ShiftLeft, true);
        }
        37 => {
            assert_eq!(
                world.resource::<Player>().dodges,
                world.resource::<MovementSmoke>().shift_start_dodges,
                "Shift must not trigger dodge"
            );
            movement_key(world, KeyCode::KeyA, false);
            movement_key(world, KeyCode::ShiftLeft, false);
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Shift has no movement action".into());
        }
        38 => {
            prepare_walljump(world);
            movement_key(world, KeyCode::KeyW, true);
            world.resource_mut::<Player>().sp = 0.;
        }
        39 => key(world, KeyCode::Space, Key::Space, None),
        40 => {
            movement_key(world, KeyCode::KeyW, false);
            assert_eq!(
                world.resource::<Player>().walljump_phase,
                crate::gameplay::WallJumpPhase::Failed
            );
            assert_eq!(world.resource::<Player>().wall_jump_failures, 1);
            assert_eq!(
                world
                    .resource::<crate::audio::MovementAudioStats>()
                    .wall_jump_failures,
                1
            );
            assert!(
                world
                    .resource::<crate::effects::EffectStats>()
                    .wall_haze_frames
                    > 0
            );
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/parity-wall-no-sp.png"));
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Empty-SP wall attempt creates haze, fails and plays warning once".into());
        }
        41 => {
            world.resource_scope(|world, mut player: Mut<Player>| {
                let level = world.resource::<crate::scene::Level>();
                let mut controls = crate::gameplay::Controls::default();
                crate::gameplay::reset_for_level(&mut player, &mut controls, level);
                player.die();
            });
        }
        42 => {
            assert_eq!(world.resource::<Player>().health, 0.);
            assert!(
                world
                    .resource::<crate::weapons::WeaponStats>()
                    .sequence_counts
                    .get("dead_eff")
                    .copied()
                    .unwrap_or(0)
                    > 0
            );
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/parity-player-death.png"));
            world
                .resource_mut::<MovementSmoke>()
                .checks
                .push("Player death starts original dead_eff and blocks movement".into());
        }
        43 => {
            let player = world.resource::<Player>();
            let report = serde_json::json!({
                "passed":true,
                "map":world.resource::<MapSelection>().active.id(),
                "checks":world.resource::<MovementSmoke>().checks,
                "jumps":player.jumps, "dodges":player.dodges,
                "wall_jumps":player.wall_jumps, "wall_jump_failures":player.wall_jump_failures, "sprint_distance_m":player.sprint_distance,
                "air_dodges":player.air_dodges, "air_sprints":player.air_sprints,
            });
            std::fs::write(
                "logs/movement-smoke.json",
                serde_json::to_string_pretty(&report).unwrap(),
            )
            .unwrap();
            info!("Movement input smoke passed: {report}");
        }
        _ => unreachable!(),
    }
    world.resource_mut::<MovementSmoke>().stage += 1;
}

fn movement_key(world: &mut World, code: KeyCode, pressed: bool) {
    let logical = match code {
        KeyCode::ShiftLeft => Key::Shift,
        KeyCode::Space => Key::Space,
        KeyCode::KeyW => Key::Character("w".into()),
        KeyCode::KeyA => Key::Character("a".into()),
        KeyCode::KeyS => Key::Character("s".into()),
        KeyCode::KeyD => Key::Character("d".into()),
        _ => unreachable!(),
    };
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world
        .resource_mut::<Messages<KeyboardInput>>()
        .write(KeyboardInput {
            key_code: code,
            logical_key: logical,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: None,
            repeat: false,
            window,
        });
}

fn prepare_walljump(world: &mut World) {
    let map = crate::scene::read_map_for(world.resource::<MapSelection>().active).unwrap();
    let facing = world
        .query_filtered::<&Transform, With<crate::gameplay::View>>()
        .single(world)
        .unwrap()
        .forward()
        .as_vec3();
    let facing = Vec3::new(facing.x, 0.0, facing.z).normalize();
    let content = world.resource::<crate::content::simulation::SimulationContent>();
    let motion = world.resource::<crate::gameplay::Motion>();
    let level = world.resource::<crate::scene::Level>();
    let mut candidates = Vec::new();
    for mesh in map.meshes.iter().filter(|mesh| mesh.collision) {
        for indices in mesh.indices.chunks_exact(3) {
            let points = [
                Vec3::from_array(mesh.positions[indices[0] as usize]),
                Vec3::from_array(mesh.positions[indices[1] as usize]),
                Vec3::from_array(mesh.positions[indices[2] as usize]),
            ];
            let cross = (points[1] - points[0]).cross(points[2] - points[0]);
            let normal = cross.normalize_or_zero();
            if normal.y.abs() > 0.1 || cross.length() < 12.0 {
                continue;
            }
            let center = (points[0] + points[1] + points[2]) / 3.0;
            for side in [-1.0, 1.0] {
                let outward = normal * side;
                if -facing.dot(outward) <= motion.walljump_min_approach_dot {
                    continue;
                }
                let feet = center + outward * (motion.collision_axis_scales[0] + 0.025)
                    - Vec3::Y * motion.height * 0.5;
                if feet.y < level.min_y + 1.0 {
                    continue;
                }
                let probe = level.collision.move_ellipsoid(
                    feet,
                    Vec3::ZERO,
                    Vec3::from(motion.collision_axis_scales),
                    motion.movement_skin_fraction,
                );
                if probe.position.distance(feet) >= 0.01
                    || probe.grounded
                    || level
                        .collision
                        .ellipsoid_overlaps(feet, Vec3::from(motion.collision_axis_scales))
                {
                    continue;
                }
                candidates.push((feet.distance_squared(level.spawn), feet, outward));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (_, feet, outward) = candidates
        .into_iter()
        .find(|(_, feet, _)| {
            let mut actor = Player::new(*feet, motion);
            let yaw = facing.x.atan2(-facing.z);
            for _ in 0..24 {
                actor.step(
                    content,
                    &level.collision,
                    motion,
                    crate::gameplay::MovementInput {
                        movement: Vec2::Y,
                        yaw,
                        ..default()
                    },
                    1. / 240.,
                );
            }
            actor.step(
                content,
                &level.collision,
                motion,
                crate::gameplay::MovementInput {
                    jump: true,
                    yaw,
                    ..default()
                },
                1. / 240.,
            );
            actor.walljump_phase == crate::gameplay::WallJumpPhase::Windup
        })
        .expect("Map needs an unobstructed vertical wall for walljump smoke");
    let level_spawn = world.resource::<crate::scene::Level>().spawn;
    world.resource_scope(|world, mut player: Mut<Player>| {
        let level = world.resource::<crate::scene::Level>();
        let mut controls = crate::gameplay::Controls::default();
        crate::gameplay::reset_for_level(&mut player, &mut controls, level);
        player.feet = feet;
    });
    info!("Movement smoke wall setup: spawn={level_spawn:?}, feet={feet:?}, outward={outward:?}");
}

fn drive_chat(world: &mut World) {
    if !world.resource::<Options>().chat_smoke {
        return;
    }
    let time = world.resource::<Time>().elapsed_secs();
    if time >= 1.5 && !world.resource::<ChatSmoke>().clicked {
        let window = world
            .query_filtered::<Entity, With<Window>>()
            .single(world)
            .unwrap();
        let mut buttons = world.resource_mut::<Messages<MouseButtonInput>>();
        for state in [ButtonState::Pressed, ButtonState::Released] {
            buttons.write(MouseButtonInput {
                button: MouseButton::Left,
                state,
                window,
            });
        }
        world.resource_mut::<ChatSmoke>().clicked = true;
        return;
    }
    let stage = world.resource::<ChatSmoke>().stage;
    let schedule = [
        2.0, 2.3, 2.8, 3.5, 4.0, 4.3, 4.8, 5.5, 6.0, 6.3, 6.8, 7.3, 7.5, 7.7, 7.9, 8.3, 8.5, 8.7,
        9.0, 9.3, 9.5, 9.8,
    ];
    if stage >= schedule.len() || time < schedule[stage] {
        return;
    }
    match stage {
        0 => {
            let cursor = world.query::<&CursorOptions>().single(world).unwrap();
            let before = (cursor.grab_mode, cursor.visible);
            assert_eq!(before, (CursorGrabMode::Locked, false));
            world.resource_mut::<ChatSmoke>().cursor_before = Some(before);
            key(world, KeyCode::Enter, Key::Enter, None);
        }
        4 | 8 | 12 | 16 | 20 => key(world, KeyCode::Enter, Key::Enter, None),
        1 | 17 | 21 => key(
            world,
            KeyCode::KeyR,
            Key::Character("/map neden-1".into()),
            Some("/map neden-1"),
        ),
        5 => key(
            world,
            KeyCode::KeyR,
            Key::Character("/map station-2".into()),
            Some("/map station-2"),
        ),
        9 => key(
            world,
            KeyCode::KeyR,
            Key::Character("/map circle-2".into()),
            Some("/map circle-2"),
        ),
        13 => key(
            world,
            KeyCode::KeyR,
            Key::Character("/map unknown".into()),
            Some("/map unknown"),
        ),
        2 | 6 | 10 | 14 => key(world, KeyCode::Enter, Key::Enter, None),
        18 => key(world, KeyCode::Escape, Key::Escape, None),
        3 => check_map(world, MapId::Neden1, "Enter submits Neden 1"),
        7 => check_map(
            world,
            MapId::Station2,
            "Enter submits Station 2 and removes old map",
        ),
        11 => check_map(world, MapId::Circle2, "Circle 2 can be restored"),
        15 => check_map(world, MapId::Circle2, "Unknown command preserves map"),
        19 => {
            check_map(world, MapId::Circle2, "Escape cancels pending map command");
            let player = world.resource::<Player>();
            assert_eq!(player.respawns, 0, "Typing R in chat must not respawn");
            assert_eq!(player.jumps, 0);
            assert!(player.feet.is_finite());
            assert!(!world.resource::<ChatState>().active);
            let cursor = world.query::<&CursorOptions>().single(world).unwrap();
            let restored = (cursor.grab_mode, cursor.visible);
            assert_eq!(Some(restored), world.resource::<ChatSmoke>().cursor_before);
            world
                .resource_mut::<ChatSmoke>()
                .checks
                .push("Typing does not trigger gameplay controls".into());
            world
                .resource_mut::<ChatSmoke>()
                .checks
                .push("Cursor capture restores after submitting and cancelling".into());
            let report =
                serde_json::json!({"passed":true,"checks":world.resource::<ChatSmoke>().checks});
            std::fs::write(
                "logs/chat-smoke.json",
                serde_json::to_string_pretty(&report).unwrap(),
            )
            .unwrap();
            info!("Chat/map integration smoke passed: {report}");
        }
        _ => unreachable!(),
    }
    world.resource_mut::<ChatSmoke>().stage += 1;
}

fn check_map(world: &mut World, expected: MapId, label: &str) {
    assert_eq!(world.resource::<MapSelection>().active, expected, "{label}");
    let map = crate::scene::read_map_for(expected).unwrap();
    let parts = map
        .meshes
        .iter()
        .filter(|m| m.visible && !m.indices.is_empty())
        .count();
    let actual = world
        .query_filtered::<Entity, With<MapEntity>>()
        .iter(world)
        .count();
    assert_eq!(
        actual, parts,
        "Map entities must be replaced without accumulation"
    );
    world.resource_mut::<ChatSmoke>().checks.push(label.into());
}

fn key(world: &mut World, code: KeyCode, logical: Key, text: Option<&str>) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    let mut inputs = world.resource_mut::<Messages<KeyboardInput>>();
    for state in [ButtonState::Pressed, ButtonState::Released] {
        inputs.write(KeyboardInput {
            key_code: code,
            logical_key: logical.clone(),
            state,
            text: if state == ButtonState::Pressed {
                text.map(Into::into)
            } else {
                None
            },
            repeat: false,
            window,
        });
    }
}
