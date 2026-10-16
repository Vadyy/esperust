use super::*;
use crate::{
    animation::Animation,
    content::emotes::{EmoteCatalog, HUG},
    melee::Weapon,
};
use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    render::view::screenshot::{Screenshot, save_to_disk},
};

#[derive(Resource, Default)]
struct Check {
    stage: u8,
    slot: usize,
    next: f32,
    weapon: Option<Weapon>,
    sinks: usize,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Check>().add_systems(
        PreUpdate,
        drive.before(InputSystems).run_if(crate::loading::ready),
    );
}

fn key(world: &mut World, code: KeyCode, text: &str, down: bool) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world
        .resource_mut::<Messages<KeyboardInput>>()
        .write(KeyboardInput {
            key_code: code,
            logical_key: Key::Character(text.into()),
            state: if down {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: if down && !text.is_empty() {
                Some(text.into())
            } else {
                None
            },
            repeat: false,
            window,
        });
}
fn snap(world: &mut World, name: &str) {
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("logs/emotes-{name}.png")));
}
fn digit(index: usize) -> KeyCode {
    [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ][index]
}

fn drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < world.resource::<Check>().next {
        return;
    }
    for mut window in world.query::<&mut Window>().iter_mut(world) {
        window.focused = true;
    }
    let stage = world.resource::<Check>().stage;
    let slot = world.resource::<Check>().slot;
    match stage {
        0 => {
            if !world.resource::<Player>().grounded {
                return;
            }
            let window = world
                .query_filtered::<Entity, With<Window>>()
                .single(world)
                .unwrap();
            world
                .resource_mut::<Messages<MouseButtonInput>>()
                .write(MouseButtonInput {
                    button: MouseButton::Left,
                    state: ButtonState::Pressed,
                    window,
                });
            let equipped = world.resource::<Player>().melee.equipped;
            world.resource_mut::<Check>().weapon = Some(equipped);
        }
        1 => {
            let window = world
                .query_filtered::<Entity, With<Window>>()
                .single(world)
                .unwrap();
            world
                .resource_mut::<Messages<MouseButtonInput>>()
                .write(MouseButtonInput {
                    button: MouseButton::Left,
                    state: ButtonState::Released,
                    window,
                });
        }
        2 => {
            if !world.resource::<Player>().can_emote() {
                return;
            }
            key(world, KeyCode::KeyE, "e", true);
        }
        3 => {
            key(world, KeyCode::KeyE, "e", false);
            assert!(world.resource::<EmoteMenu>().open);
            let (text, background) = world
                .query_filtered::<(&Text, Option<&BackgroundColor>), With<MenuRoot>>()
                .single(world)
                .unwrap();
            assert!(
                background.is_none_or(|background| background.0.alpha() == 0.),
                "original menu background must remain transparent"
            );
            assert!(text.0.starts_with("\n1.Greet\n2.Win\n3.Upset"));
            assert!(text.0.ends_with("0.Cancel") && !text.0.contains("Esc"));
            if slot == 0 {
                snap(world, "menu");
            }
        }
        4 => key(world, digit(slot), "", true),
        5 => {
            key(world, digit(slot), "", false);
            let p = world.resource::<Player>();
            assert_eq!(p.emote.map(|e| e.index), Some(slot));
            assert_eq!(
                Some(p.melee.equipped),
                world.resource::<Check>().weapon,
                "emote selection must not equip a weapon"
            );
            assert!(!world.resource::<EmoteMenu>().open);
            world.resource_mut::<Check>().next = now + 0.5;
            world.resource_mut::<Check>().stage = 6;
            return;
        }
        6 => {
            let expected = world.resource::<EmoteCatalog>().entries[slot]
                .action
                .clone();
            let clip = world
                .resource::<Animation>()
                .upper_cursor()
                .unwrap()
                .0
                .to_owned();
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read("assets/emotes/emotes.json").unwrap())
                    .unwrap();
            assert_eq!(
                clip,
                manifest["actions"][&expected]["clip"].as_str().unwrap()
            );
            assert_eq!(
                world.resource::<EmoteAudio>().voices_played as usize,
                slot + 1
            );
            assert!(
                world
                    .query_filtered::<&AudioSink, With<EmoteVoice>>()
                    .iter(world)
                    .next()
                    .is_some(),
                "original emote voice must reach an audio sink"
            );
            world.resource_mut::<Check>().sinks += 1;
            if slot == 0 {
                snap(world, "greeting");
            }
            key(
                world,
                if slot % 2 == 0 {
                    KeyCode::KeyW
                } else {
                    KeyCode::KeyS
                },
                "",
                true,
            );
        }
        7 => {
            key(
                world,
                if slot % 2 == 0 {
                    KeyCode::KeyW
                } else {
                    KeyCode::KeyS
                },
                "",
                false,
            );
            assert!(world.resource::<Player>().emote.is_none());
            let mut check = world.resource_mut::<Check>();
            check.slot += 1;
            if check.slot < MENU_COUNT {
                check.stage = 2;
                check.next = now + 0.12;
                return;
            }
        }
        8 => key(world, KeyCode::Digit2, "2", true),
        9 => {
            key(world, KeyCode::Digit2, "2", false);
            assert_eq!(world.resource::<Player>().melee.equipped, Weapon::Dagger);
        }
        10 => key(world, KeyCode::Enter, "", true),
        11 => {
            key(world, KeyCode::Enter, "", false);
            assert!(world.resource::<crate::chat::ChatState>().active);
            key(world, KeyCode::Slash, "/hug", true);
        }
        12 => {
            key(world, KeyCode::Slash, "", false);
            key(world, KeyCode::Enter, "", true);
        }
        13 => {
            key(world, KeyCode::Enter, "", false);
            assert_eq!(world.resource::<Player>().emote.map(|e| e.index), Some(HUG));
            world.resource_mut::<Check>().next = now + 0.7;
            world.resource_mut::<Check>().stage = 14;
            return;
        }
        14 => {
            snap(world, "hug");
            assert_eq!(world.resource::<EmoteAudio>().voices_played, 10);
            let duration = world.resource::<EmoteCatalog>().entries[HUG].duration;
            world.resource_mut::<Check>().next = now + duration;
        }
        15 => {
            assert!(
                world.resource::<Player>().emote.is_none(),
                "hug ends at natural clip duration"
            );
            key(world, KeyCode::Enter, "", true);
        }
        16 => {
            key(world, KeyCode::Enter, "", false);
            key(world, KeyCode::KeyE, "e", true);
            key(world, KeyCode::Digit1, "1", true);
        }
        17 => {
            key(world, KeyCode::KeyE, "e", false);
            key(world, KeyCode::Digit1, "1", false);
            assert!(!world.resource::<EmoteMenu>().open);
            assert!(world.resource::<Player>().emote.is_none());
            assert_eq!(world.resource::<Player>().melee.equipped, Weapon::Dagger);
            key(world, KeyCode::Escape, "", true);
        }
        18 => key(world, KeyCode::Escape, "", false),
        19 => key(world, KeyCode::KeyE, "e", true),
        20 => {
            key(world, KeyCode::KeyE, "e", false);
            assert!(world.resource::<EmoteMenu>().open);
            key(world, KeyCode::Escape, "", true);
        }
        21 => {
            key(world, KeyCode::Escape, "", false);
            assert!(!world.resource::<EmoteMenu>().open);
            let cursor = world
                .query::<&bevy::window::CursorOptions>()
                .single(world)
                .unwrap();
            assert_eq!(cursor.grab_mode, bevy::window::CursorGrabMode::Locked);
            key(world, KeyCode::KeyE, "e", true);
        }
        22 => {
            key(world, KeyCode::KeyE, "e", false);
            assert!(world.resource::<EmoteMenu>().open);
            key(world, KeyCode::Digit0, "0", true);
        }
        23 => {
            key(world, KeyCode::Digit0, "0", false);
            assert!(!world.resource::<EmoteMenu>().open);
            let report = serde_json::json!({ "status": "passed", "menu_slots": MENU_COUNT,
                "voices_played": world.resource::<EmoteAudio>().voices_played,
                "audio_sinks_verified": world.resource::<Check>().sinks,
                "checks": ["all nine keyboard selections", "original upper/lower poses", "original voices",
                    "no weapon switch on selection", "movement cancellation", "normal weapon hotkeys",
                    "/hug chat command", "natural completion", "chat isolation", "Escape preserves capture",
                    "0.Cancel", "transparent original menu text and order"] });
            std::fs::write(
                "logs/emotes-check.json",
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            world.write_message(bevy::app::AppExit::Success);
        }
        _ => return,
    }
    let mut check = world.resource_mut::<Check>();
    check.stage += 1;
    if stage != 14 {
        check.next = now + 0.12;
    }
}
