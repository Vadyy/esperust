use crate::{
    Options,
    combat::{CombatStats, PracticeTarget, SparringMode},
    gameplay::{Controls, Player},
    melee::Weapon,
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
pub struct CounterCombatVerificationPlugin;
impl Plugin for CounterCombatVerificationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Check>()
            .add_systems(PreUpdate, (drive.before(InputSystems)).run_if(crate::loading::ready))
            .add_systems(
                Update, (capture
                    .after(crate::weapons::present)
                    .after(crate::critical_feedback::CriticalFeedbackSet)).run_if(crate::loading::ready));
    }
}
#[derive(Resource, Default)]
struct Check {
    stage: u8,
    at: f32,
    sp: f32,
    capture: Option<&'static str>,
    critical_captured: bool,
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
    if !world.resource::<Options>().counter_combat_check {
        return;
    }
    for mut window in world.query::<&mut Window>().iter_mut(world) {
        window.focused = true;
    }
    world.resource_mut::<Controls>().practice_camera(0., 0.);
    let elapsed = world.resource::<Time>().elapsed_secs();
    let stage = world.resource::<Check>().stage;
    let age = elapsed - world.resource::<Check>().at;
    assert!(
        elapsed < 33. || stage >= 11,
        "Counter combat check timed out at stage{stage}"
    );
    let advance = match stage {
        0 if elapsed >= 3. => {
            mouse(world, MouseButton::Left, true);
            true
        }
        1 if age >= 0.05 => {
            mouse(world, MouseButton::Left, false);
            true
        }
        2 if world.resource::<Player>().melee.attack.is_none() => {
            key(world, KeyCode::Digit3, true);
            true
        }
        3 if age >= 0.05 => {
            key(world, KeyCode::Digit3, false);
            assert_eq!(
                world.resource::<Player>().melee.equipped,
                Weapon::CounterSword
            );
            world
                .resource_mut::<crate::chat::ChatState>()
                .practice_sparring_requested = Some(SparringMode::Attack);
            mouse(world, MouseButton::Right, true);
            world.resource_mut::<Check>().sp = world.resource::<Player>().sp;
            true
        }
        4 if world.resource::<CombatStats>().countered_hits > 0 => {
            assert_eq!(world.resource::<Player>().health, 100.);
            assert_eq!(world.resource::<Player>().sp, world.resource::<Check>().sp);
            let stunned = world.query::<&PracticeTarget>().single(world).unwrap().stun;
            assert!(
                stunned > 2.8,
                "Source counter applies three seconds of stun"
            );
            world.resource_mut::<Check>().capture = Some("counter-combat-counter-stun");
            true
        }
        5 if age >= 3.05 => {
            assert_eq!(
                world.query::<&PracticeTarget>().single(world).unwrap().stun,
                0.
            );
            world
                .query::<&mut PracticeTarget>()
                .single_mut(world)
                .unwrap()
                .prepare_sparring_attack(true);
            true
        }
        6 if world.resource::<CombatStats>().incoming_hits >= 2 => {
            assert_eq!(
                world.resource::<Player>().health,
                10.,
                "Critical ray45 and ordinary45 each apply"
            );
            assert_eq!(world.resource::<Player>().sp, world.resource::<Check>().sp);
            assert!(world.resource::<CombatStats>().guard_crushes >= 1);
            world.resource_mut::<Check>().capture = Some("counter-combat-guard-crush");
            true
        }
        7 if age >= 1. => {
            mouse(world, MouseButton::Right, false);
            let feet = world.resource::<Player>().feet;
            let mut query = world.query::<(&mut PracticeTarget, &mut Transform)>();
            let (mut target, mut transform) = query.single_mut(world).unwrap();
            transform.translation = feet + Vec3::NEG_Z * 3.5;
            target.prepare_passive(transform.translation);
            true
        }
        8 if age >= 0.25
            && world.resource::<Player>().hit_reaction.is_none()
            && world.resource::<Player>().melee.attack.is_none() =>
        {
            mouse(world, MouseButton::Left, true);
            true
        }
        9 if age >= 0.05 => {
            mouse(world, MouseButton::Left, false);
            true
        }
        10 if world.resource::<CombatStats>().hits >= 3 => {
            let stats = world.resource::<CombatStats>();
            assert_eq!(stats.critical_hits, 1);
            assert_eq!(stats.total_damage, 60.);
            assert_eq!(
                world
                    .query::<&PracticeTarget>()
                    .single(world)
                    .unwrap()
                    .health(),
                40.
            );
            true
        }
        11 if age >= 0.15 && !world.resource::<Check>().critical_captured => {
            let feedback = world.resource::<crate::critical_feedback::CriticalFeedbackStats>();
            assert_eq!((feedback.accepted, feedback.sounds_played), (1, 1));
            assert!(feedback.layer_opacity[0] > 0.);
            let mut check = world.resource_mut::<Check>();
            check.critical_captured = true;
            check.capture = Some("counter-combat-critical");
            false
        }
        11 if age >= 0.6 && !world.resource::<Check>().saved => {
            let stats = world.resource::<CombatStats>();
            let report = serde_json::json!({"status":"passed","evidence_kind":"recreation combat check","paired_original":false,"countered_hits":stats.countered_hits,"guard_crushes":stats.guard_crushes,"incoming_hits":stats.incoming_hits,"critical_hits":stats.critical_hits,"outgoing_damage":stats.total_damage,"checks":["actual incoming Counter combo countered with no defender HP/SP loss","three-second attacker stun","counter-disabled uppercut crushes active guard and applies both45-damage events","critical ray and both ordinary first-stage events apply total60 without extra head multiplier","source-local critical feedback after confirmed hit"]});
            std::fs::write(
                "logs/counter-combat-verification.json",
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            world.resource_mut::<Check>().saved = true;
            world
                .resource_mut::<Messages<bevy::app::AppExit>>()
                .write(bevy::app::AppExit::Success);
            false
        }
        _ => false,
    };
    if advance {
        let mut check = world.resource_mut::<Check>();
        check.stage += 1;
        check.at = elapsed;
    }
}
fn capture(mut commands: Commands, options: Res<Options>, mut check: ResMut<Check>) {
    if !options.counter_combat_check {
        return;
    }
    if let Some(name) = check.capture.take() {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("logs/{name}.png")));
    }
}
