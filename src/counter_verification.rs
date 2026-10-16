use crate::{
    Options,
    gameplay::{Controls, Player},
    melee::{Attack, AttackPhase, Weapon},
    weapons::WeaponStats,
};
use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::collections::BTreeSet;
pub struct CounterVerificationPlugin;
impl Plugin for CounterVerificationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Check>()
            .add_systems(PreUpdate, (drive.before(InputSystems)).run_if(crate::loading::ready))
            .add_systems(Update, (inspect.after(crate::weapons::present)).run_if(crate::loading::ready));
    }
}
#[derive(Resource, Default)]
struct Check {
    stage: usize,
    next_at: f32,
    last_event: f32,
    actions: BTreeSet<String>,
    captures: BTreeSet<String>,
    saved: bool,
    capture_released: bool,
    strong_probe_stage: u8,
    strong_probe_at: f32,
    uppercut_followup: bool,
    last_attack: Option<Attack>,
    reset_serial: u32,
    guard_jump_observed: bool,
    guard_dodge_observed: bool,
}
fn mouse(world: &mut World, button: MouseButton, pressed: bool) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world
        .resource_mut::<Messages<MouseButtonInput>>()
        .write(MouseButtonInput {
            button,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            window,
        });
}
fn key(world: &mut World, code: KeyCode, pressed: bool) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world
        .resource_mut::<Messages<KeyboardInput>>()
        .write(KeyboardInput {
            key_code: code,
            logical_key: Key::Character("".into()),
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
fn drive(world: &mut World) {
    if !world.resource::<Options>().counter_smoke {
        return;
    }
    for mut window in world.query::<&mut Window>().iter_mut(world) {
        window.focused = true;
    }
    let elapsed = world.resource::<Time>().elapsed_secs();
    let stage = world.resource::<Check>().stage;
    let schedule = [
        0.2, 0.8, 0.85, 1., 1.08, 1.2, 1.28, 1.85, 1.93, 2.5, 2.58, 5., 5.35, 7., 7.8, 8.3, 8.4,
        8.9, 9.1, 9.4, 11., 12., 12.1, 12.2, 12.28, 16., 16.1, 17., 19., 19.1, 20., 21., 21.1, 23.,
        24., 24.15, 24.25, 24.35, 24.43, 24.53, 24.61, 24.8, 26., 26.1,
    ];
    if stage == 1
        && elapsed >= world.resource::<Check>().last_event + 0.05
        && !world.resource::<Check>().capture_released
    {
        mouse(world, MouseButton::Left, false);
        world.resource_mut::<Check>().capture_released = true;
        return;
    }
    if stage == 13 {
        let probe = world.resource::<Check>().strong_probe_stage;
        let at = world.resource::<Check>().strong_probe_at;
        if probe == 0 && elapsed >= world.resource::<Check>().last_event + 0.1 {
            mouse(world, MouseButton::Left, true);
            let mut check = world.resource_mut::<Check>();
            check.strong_probe_stage = 1;
            check.strong_probe_at = elapsed;
            return;
        }
        if probe == 1 && elapsed >= at + 0.05 {
            mouse(world, MouseButton::Left, false);
            world.resource_mut::<Check>().strong_probe_stage = 2;
            return;
        }
        if probe < 2 || world.resource::<Player>().melee.attack.is_some() {
            return;
        }
    }
    if stage >= schedule.len() || elapsed < world.resource::<Check>().next_at.max(3.) {
        return;
    }
    if stage == 1 && world.resource::<Player>().melee.attack.is_some() {
        return;
    }
    eprintln!(
        "counter-check stage={stage} at={elapsed:.3} equip={:?} attack={:?}",
        world.resource::<Player>().melee.equipped,
        world.resource::<Player>().melee.attack
    );
    match stage {
        0 => {
            world.resource_mut::<Controls>().practice_camera(0., -0.45);
            mouse(world, MouseButton::Left, true);
        }
        1 => key(world, KeyCode::Digit3, true),
        2 => key(world, KeyCode::Digit3, false),
        3 | 5 | 7 | 9 | 11 | 18 | 23 => mouse(world, MouseButton::Left, true),
        4 | 6 | 8 | 10 | 12 | 19 | 24 => mouse(world, MouseButton::Left, false),
        13 | 15 | 25 => mouse(world, MouseButton::Right, true),
        14 | 20 | 26 => mouse(world, MouseButton::Right, false),
        16 => key(world, KeyCode::KeyW, true),
        17 => key(world, KeyCode::KeyW, false),
        21 => key(world, KeyCode::Space, true),
        22 => key(world, KeyCode::Space, false),
        27 => {
            world.resource_mut::<Check>().reset_serial =
                world.resource::<Player>().pose_reset_serial;
            key(world, KeyCode::KeyR, true);
        }
        28 => {
            key(world, KeyCode::KeyR, false);
            key(world, KeyCode::Digit1, true);
        }
        29 => key(world, KeyCode::Digit1, false),
        30 => key(world, KeyCode::Digit3, true),
        31 => {
            key(world, KeyCode::Digit3, false);
            mouse(world, MouseButton::Right, true);
        }
        32 => {
            world.resource_mut::<Player>().health = 0.;
        }
        33 => key(world, KeyCode::KeyR, true),
        34 => {
            key(world, KeyCode::KeyR, false);
            mouse(world, MouseButton::Right, false);
        }
        35 => mouse(world, MouseButton::Right, true),
        36 => key(world, KeyCode::KeyW, true),
        37 => key(world, KeyCode::Space, true),
        38 => key(world, KeyCode::Space, false),
        39 => key(world, KeyCode::KeyD, true),
        40 => key(world, KeyCode::Space, true),
        41 => {
            key(world, KeyCode::Space, false);
            key(world, KeyCode::KeyW, false);
            key(world, KeyCode::KeyD, false);
            mouse(world, MouseButton::Right, false);
        }
        42 => key(world, KeyCode::KeyR, true),
        43 => key(world, KeyCode::KeyR, false),
        _ => {}
    }
    let mut check = world.resource_mut::<Check>();
    check.stage += 1;
    check.last_event = elapsed;
    check.next_at = elapsed
        + schedule
            .get(stage + 1)
            .map(|next| *next - schedule[stage])
            .unwrap_or(2.);
}
fn inspect(
    mut commands: Commands,
    options: Res<Options>,
    time: Res<Time>,
    player: Res<Player>,
    stats: Res<WeaponStats>,
    mut check: ResMut<Check>,
) {
    if !options.counter_smoke {
        return;
    }
    if check.last_attack == Some(Attack::CounterRetaliation)
        && player.melee.attack == Some(Attack::CounterCombo(0))
    {
        check.uppercut_followup = true;
    }
    check.last_attack = player.melee.attack;
    if player.melee.attack == Some(Attack::CounterGuard) && player.guard_jump.is_some() {
        check.guard_jump_observed = true;
        if check.captures.insert("guard-lower-jump".into()) {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/counter-guard-lower-jump.png"));
        }
    }
    if check.guard_jump_observed
        && matches!(
            player.action,
            crate::gameplay::MovementAction::DodgeLeft
                | crate::gameplay::MovementAction::DodgeRight
        )
        && player.melee.attack.is_none()
    {
        check.guard_dodge_observed = true;
    }
    if let Some(attack) = player.melee.attack {
        if attack == Attack::CounterGuard && player.melee.phase == AttackPhase::Normal {
            assert_eq!(player.melee.guard_query(90.), Some(3000));
            assert!(!player.melee.counter_query(60.));
        }
        let label = format!("{attack:?}");
        check.actions.insert(label.clone());
        let phase = if player.melee.phase == AttackPhase::Delay {
            "recovery"
        } else {
            "normal"
        };
        let key = format!("{}-{phase}", label.replace(['(', ')'], ""));
        if player.melee.phase_elapsed >= 0.18 && check.captures.insert(key.clone()) {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("logs/counter-{key}.png")));
        }
    } else if player.melee.equipped == Weapon::CounterSword
        && time.elapsed_secs() > 0.9
        && check.captures.insert("idle".into())
    {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("logs/counter-idle.png"));
    }
    if check.stage < 44 || time.elapsed_secs() < check.next_at || check.saved {
        return;
    }
    for action in [
        Attack::CounterCombo(0),
        Attack::CounterCombo(1),
        Attack::CounterCombo(2),
        Attack::CounterCombo(3),
        Attack::CounterGuard,
        Attack::CounterRetaliation,
        Attack::CounterJump,
    ] {
        assert!(
            check.actions.contains(&format!("{action:?}")),
            "Counter input path missing {action:?}: {:?}",
            check.actions
        );
    }
    assert!(
        check.uppercut_followup,
        "uppercut pre-input returns to first combo"
    );
    assert_eq!(stats.models_loaded, 4);
    assert!(
        stats
            .haze_trails
            .contains_key("GhostTrail_counter_sword_01")
    );
    assert!(player.pose_reset_serial > check.reset_serial);
    assert!(
        check.guard_jump_observed,
        "Space selects guard's independent lower jump"
    );
    assert!(
        check.guard_dodge_observed,
        "D+Space exits guard into lateral dodge"
    );
    assert!(
        player.melee.attack.is_none(),
        "respawn clears guard and attack"
    );
    let report = serde_json::json!({"status":"passed", "evidence_kind":"recreation check", "paired_original":false, "actions":check.actions,"captures":check.captures,"weapon_stats": &*stats,"checks":["slot3 through keyboard","short-release four-stage pre-input combo","held-primary uppercut and buffered return to first combo","guard startup, moving guard and release recovery","Space retains guard upper pose with lower jump","D+Space exits guard into lateral dodge","airborne primary jump/drop","respawn and death clear guard","both hand carriers loaded","exact registered counter trail emitted"]});
    std::fs::write(
        "logs/counter-verification.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    check.saved = true;
}
