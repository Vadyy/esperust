use crate::{
    Options,
    gameplay::{Controls, Player},
    melee::Attack,
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

pub struct DaggerEffectCheckPlugin;
impl Plugin for DaggerEffectCheckPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CaptureState>()
            .add_systems(
                PreUpdate,
                (drive.before(InputSystems)).run_if(crate::loading::ready),
            )
            .add_systems(
                Update,
                (capture.after(crate::weapons::present)).run_if(crate::loading::ready),
            );
    }
}
#[derive(Resource, Default)]
struct CaptureState {
    stage: usize,
    started: Option<f32>,
    frames: usize,
    sampled_elapsed: Vec<f32>,
    saved: bool,
}
fn drive(world: &mut World) {
    if !world.resource::<Options>().dagger_effect_check {
        return;
    }
    for mut window in world.query::<&mut Window>().iter_mut(world) {
        window.focused = true;
    }
    let yaw = world.resource::<Options>().yaw.unwrap_or(0.);
    world.resource_mut::<Controls>().practice_camera(yaw, -0.65);
    let now = world.resource::<Time>().elapsed_secs();
    if world.resource::<CaptureState>().started.is_none() {
        if !world.resource::<Player>().grounded {
            return;
        }
        world.resource_mut::<CaptureState>().started = Some(now);
    }
    let elapsed = now - world.resource::<CaptureState>().started.unwrap();
    let stage = world.resource::<CaptureState>().stage;
    if elapsed
        < [0.5, 0.55, 0.8, 0.85, 1.5, 1.55]
            .get(stage)
            .copied()
            .unwrap_or(f32::INFINITY)
    {
        return;
    }
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    let state = if stage % 2 == 0 {
        ButtonState::Pressed
    } else {
        ButtonState::Released
    };
    if stage == 2 || stage == 3 {
        world
            .resource_mut::<Messages<KeyboardInput>>()
            .write(KeyboardInput {
                key_code: KeyCode::Digit2,
                logical_key: Key::Character("2".into()),
                state,
                text: None,
                repeat: false,
                window,
            });
    } else {
        world
            .resource_mut::<Messages<MouseButtonInput>>()
            .write(MouseButtonInput {
                button: if stage < 2 {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                },
                state,
                window,
            });
    }
    world.resource_mut::<CaptureState>().stage += 1;
}
fn capture(
    mut commands: Commands,
    options: Res<Options>,
    player: Res<Player>,
    time: Res<Time>,
    mut state: ResMut<CaptureState>,
) {
    if !options.dagger_effect_check {
        return;
    }
    if time.elapsed_secs() >= 8. {
        assert!(
            state.saved,
            "Dagger fixture did not finish before its deadline"
        );
    }
    let Some(started) = state.started else {
        return;
    };
    if time.elapsed_secs() - started >= 3.2 && !state.saved {
        assert_eq!(
            state.frames, 7,
            "Dagger effect check must enter the real attack and capture all seven source times"
        );
        std::fs::write("logs/dagger-effect-check.json", serde_json::to_vec_pretty(&serde_json::json!({
            "passed":true, "captures":state.frames, "sampled_elapsed":state.sampled_elapsed,
            "scope":"Real keyboard equipment selection and mouse attack; recreation captures, not paired native parity"
        })).unwrap()).unwrap();
        state.saved = true;
    }
    if player.melee.attack != Some(Attack::DaggerDash) {
        return;
    }
    let times = [0.36, 0.40, 0.45, 0.50, 0.60, 0.75, 0.90];
    if let Some(&at) = times.get(state.frames) {
        if player.melee.elapsed >= at {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!(
                    "logs/dagger-open-{:04}.png",
                    (at * 1000.) as u32
                )));
            state.sampled_elapsed.push(player.melee.elapsed);
            state.frames += 1;
        }
    }
}
