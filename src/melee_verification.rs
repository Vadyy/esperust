use crate::{Options, gameplay::Player};
use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::collections::{BTreeMap, BTreeSet};

pub struct MeleeVerificationPlugin;
impl Plugin for MeleeVerificationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeleeSmoke>()
            .add_systems(PreUpdate, (drive.before(InputSystems)).run_if(crate::loading::ready))
            .add_systems(Update, (inspect.after(crate::weapons::present)).run_if(crate::loading::ready));
    }
}

#[derive(Resource, Default)]
struct MeleeSmoke {
    stage: usize,
    attacks: BTreeSet<String>,
    captured: BTreeSet<String>,
    chat_serial: u64,
    checks: Vec<String>,
    isolated_audio: Option<(BTreeMap<String, u32>, u32, u32)>,
    saved: bool,
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

fn key(world: &mut World, code: KeyCode, logical_key: Key, pressed: bool, text: Option<&str>) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world
        .resource_mut::<Messages<KeyboardInput>>()
        .write(KeyboardInput {
            key_code: code,
            logical_key,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: text.map(Into::into),
            repeat: false,
            window,
        });
}

fn tap(world: &mut World, code: KeyCode, logical: Key, text: Option<&str>) {
    key(world, code, logical.clone(), true, text);
    key(world, code, logical, false, None);
}

fn drive(world: &mut World) {
    if !world.resource::<Options>().melee_smoke {
        return;
    }
    for mut window in world.query::<&mut Window>().iter_mut(world) {
        window.focused = true;
    }
    let elapsed = world.resource::<Time>().elapsed_secs();
    let stage = world.resource::<MeleeSmoke>().stage;
    let schedule = [
        1.1, 1.15, 1.7, 1.8, 1.9, 2.4, 2.45, 3.3, 4.1, 5.4, 5.45, 8.4, 8.45, 8.55, 8.60, 11.5,
        12.3, 12.35, 13.5, 13.55, 17.0, 17.05, 17.15, 17.20, 19.0, 19.05, 19.12, 19.16, 19.22,
        19.27, 19.4, 21.0, 21.1, 21.15, 21.8, 22.5, 23.0, 23.15, 23.20, 23.40, 23.45, 23.65, 25.0,
        25.05, 25.90, 26.15, 26.20, 27.8, 28.0, 28.6, 29.8, 29.9, 30.0, 30.3, 30.8, 31.0, 31.05,
        31.8, 31.85, 32.6, 32.65, 33.4,
    ];
    if stage >= schedule.len() || elapsed < schedule[stage] {
        return;
    }
    match stage {
        0 | 5 | 7 | 13 | 16 | 22 | 28 | 32 => mouse(world, MouseButton::Left, true),
        1 | 6 | 8 | 14 | 17 | 23 | 29 | 33 => mouse(world, MouseButton::Left, false),
        2 | 4 | 31 => tap(world, KeyCode::Enter, Key::Enter, None),
        3 => tap(
            world,
            KeyCode::KeyR,
            Key::Character("/dummy".into()),
            Some("/dummy"),
        ),
        9 | 18 => mouse(world, MouseButton::Right, true),
        10 | 19 => mouse(world, MouseButton::Right, false),
        11 | 20 | 24 => key(world, KeyCode::Space, Key::Space, true, None),
        12 | 21 | 25 | 27 => key(world, KeyCode::Space, Key::Space, false, None),
        15 => tap(world, KeyCode::Digit2, Key::Character("2".into()), None),
        26 => {
            key(world, KeyCode::KeyA, Key::Character("a".into()), true, None);
            key(world, KeyCode::Space, Key::Space, true, None);
        }
        30 => {
            let player = world.resource::<Player>();
            assert_eq!(player.action, crate::gameplay::MovementAction::None);
            assert_eq!(player.melee.attack, Some(crate::melee::Attack::DaggerAir));
            assert!(player.air_dodges > 0);
            world
                .resource_mut::<MeleeSmoke>()
                .checks
                .push("Dagger left click cancels an airborne dodge through mouse input".into());
            key(
                world,
                KeyCode::KeyA,
                Key::Character("a".into()),
                false,
                None,
            );
        }
        34 => {
            assert!(world.resource::<crate::chat::ChatState>().active);
            let serial = world.resource::<Player>().melee.serial;
            assert_eq!(
                serial,
                world.resource::<MeleeSmoke>().chat_serial,
                "Chat click leaked into melee"
            );
            tap(world, KeyCode::Escape, Key::Escape, None);
            world
                .resource_mut::<MeleeSmoke>()
                .checks
                .push("Chat blocks mouse attacks".into());
        }
        35 => {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/melee-equipped.png"));
        }
        36 => tap(world, KeyCode::Digit1, Key::Character("1".into()), None),
        37 | 42 => mouse(world, MouseButton::Right, true),
        38 | 43 => mouse(world, MouseButton::Right, false),
        39 | 44 => mouse(world, MouseButton::Left, true),
        40 | 46 => mouse(world, MouseButton::Left, false),
        41 | 45 => {
            assert_eq!(
                world.resource::<Player>().melee.attack,
                Some(crate::melee::Attack::PlasmaDashStrong)
            );
            world
                .resource_mut::<MeleeSmoke>()
                .checks
                .push(if stage == 41 {
                    "Plasma dash tap interrupts normal phase into full-body heavy".into()
                } else {
                    "Plasma dash hold interrupts recovery into full-body heavy".into()
                });
        }
        47 | 49 => {
            let name = if stage == 47 { "plasma" } else { "dagger" };
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("logs/melee-idle-{name}.png")));
        }
        48 => tap(world, KeyCode::Digit2, Key::Character("2".into()), None),
        50 | 52 => tap(world, KeyCode::Enter, Key::Enter, None),
        51 => tap(
            world,
            KeyCode::KeyR,
            Key::Character("/dummy".into()),
            Some("/dummy"),
        ),
        53 => tap(world, KeyCode::Digit1, Key::Character("1".into()), None),
        54 => {
            assert!(
                world
                    .query::<&crate::combat::PracticeTarget>()
                    .iter(world)
                    .next()
                    .is_none()
            );
            let sounds = world
                .resource::<crate::weapons::WeaponStats>()
                .sound_counts
                .clone();
            let steps = world
                .resource::<crate::audio::MovementAudioStats>()
                .footsteps;
            let surfaces = world.resource::<crate::weapons::WeaponStats>().surface_hits;
            world.resource_mut::<MeleeSmoke>().isolated_audio = Some((sounds, steps, surfaces));
        }
        55 | 57 | 59 => mouse(world, MouseButton::Left, true),
        56 | 58 | 60 => mouse(world, MouseButton::Left, false),
        61 => {
            let smoke = world.resource::<MeleeSmoke>();
            let (before, footsteps, surfaces) = smoke.isolated_audio.as_ref().unwrap();
            let stats = world.resource::<crate::weapons::WeaponStats>();
            let emitted: BTreeMap<_, _> = stats
                .sound_counts
                .iter()
                .filter_map(|(name, count)| {
                    let delta = count - before.get(name).copied().unwrap_or(0);
                    (delta > 0).then_some((name.as_str(), delta))
                })
                .collect();
            let mut expected = BTreeMap::from([("sword_s", 3)]);
            let impacts = stats.surface_hits - surfaces;
            if impacts > 0 { expected.insert("sword_attack", impacts); }
            assert_eq!(emitted, expected,
                "Stationary weak cuts emit one slash each plus source audio for confirmed map impacts");
            assert_eq!(
                *footsteps,
                world
                    .resource::<crate::audio::MovementAudioStats>()
                    .footsteps,
                "Idle melee must not generate footsteps"
            );
            world.resource_mut::<MeleeSmoke>().checks.push("All three stationary weak cuts play one slash each, confirmed map impacts play their source cue, and footsteps remain silent".into());
        }
        _ => unreachable!(),
    }
    if stage == 31 {
        world.resource_mut::<MeleeSmoke>().chat_serial = world.resource::<Player>().melee.serial;
    }
    world.resource_mut::<MeleeSmoke>().stage += 1;
}

fn inspect(
    options: Res<Options>,
    time: Res<Time>,
    player: Res<Player>,
    stats: Res<crate::weapons::WeaponStats>,
    combat: Res<crate::combat::CombatStats>,
    mut smoke: ResMut<MeleeSmoke>,
    mut commands: Commands,
) {
    if !options.melee_smoke {
        return;
    }
    if let Some(attack) = player.melee.attack {
        let name = format!("{attack:?}");
        smoke.attacks.insert(name.clone());
        if player.melee.elapsed >= 0.16 && smoke.captured.insert(name.clone()) {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!(
                    "logs/melee-{}.png",
                    name.replace(['(', ')'], "")
                )));
        }
        if attack == crate::melee::Attack::PlasmaJump
            && player.melee.elapsed >= 0.58
            && smoke.captured.insert("PlasmaJump-impact".into())
        {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/melee-PlasmaJump-impact.png"));
        }
        if attack == crate::melee::Attack::PlasmaDash
            && player.melee.elapsed >= 0.62
            && smoke.captured.insert("PlasmaDash-effect".into())
        {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/melee-PlasmaDash-effect.png"));
        }
        if attack == crate::melee::Attack::DaggerDash
            && player.melee.elapsed >= 0.48
            && smoke.captured.insert("DaggerDash-effect".into())
        {
            commands.spawn(Screenshot::primary_window())
                .observe(save_to_disk("logs/melee-DaggerDash-effect.png"));
        }
    }
    if time.elapsed_secs() >= 11.85 && smoke.captured.insert("weapon-switch".into()) {
        commands.spawn(Screenshot::primary_window())
            .observe(save_to_disk("logs/parity-weapon-switch.png"));
    }
    if time.elapsed_secs() < 35.5 || smoke.saved {
        return;
    }
    for expected in [
        "PlasmaWeak",
        "PlasmaStrong",
        "PlasmaDash",
        "PlasmaDashStrong",
        "PlasmaJump",
        "DaggerSlash",
        "DaggerDash",
        "DaggerAir",
    ] {
        assert!(
            smoke
                .attacks
                .iter()
                .any(|attack| attack.starts_with(expected)),
            "Melee smoke never entered {expected}: {:?}",
            smoke.attacks
        );
    }
    assert_eq!(stats.models_loaded, 4, "Three weapons, including both Counter Sword hand carriers, must load");
    assert!(stats.visible_models >= 1);
    assert!(stats.sounds_played > 0 && stats.effect_sequences > 0);
    assert_eq!(
        stats.haze_passes_loaded, 6,
        "All six separately named source trails must retain their haze pass"
    );
    for name in ["GhostTrail_two_hand_sword_01", "GhostTrail_SpyDagger_01"] {
        assert!(
            stats.haze_trails.get(name).copied().unwrap_or(0) > 0,
            "Both weapons' selected cuts must present their source haze: {name}"
        );
    }
    assert_eq!(
        stats.sound_counts.get("spydagger_poke"),
        Some(&1),
        "Dagger thrust must play its swing sound only once"
    );
    assert_eq!(
        stats.sound_counts.get("sword_power"),
        Some(&1),
        "Plasma jump must play its impact sound"
    );
    assert_eq!(
        stats.sequence_counts.get("skill_swordman_strike"),
        Some(&1),
        "Plasma jump must present the original shockwave"
    );
    assert!(
        combat.hits > 0,
        "Original attack timeline must damage the practice target"
    );
    let report = serde_json::json!({
        "passed": true, "attacks": smoke.attacks, "checks": smoke.checks,
        "models_loaded":stats.models_loaded,"visible_models":stats.visible_models,
        "sounds_played":stats.sounds_played,"effect_sequences":stats.effect_sequences,"surface_hits":stats.surface_hits,
        "sound_counts":stats.sound_counts,"sequence_counts":stats.sequence_counts,
        "haze_passes_loaded":stats.haze_passes_loaded,"haze_frames":stats.haze_frames,"haze_trails":stats.haze_trails,
        "peak_effects":stats.peak_effects,"practice_hits":combat.hits,
        "practice_damage":combat.total_damage,"captures":smoke.captured,
        "scope":"Recreation integration checks; original-client trajectory/actor-hit parity remains separate",
    });
    std::fs::write(
        "logs/melee-smoke.json",
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    info!("Melee smoke passed: {report}");
    smoke.saved = true;
}
