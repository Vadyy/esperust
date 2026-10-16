use crate::{
    Options,
    animation::Animation,
    gameplay::{Controls, MovementAction, Player},
    melee::{Attack, AttackPhase, Weapon},
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
pub struct CounterFollowupVerificationPlugin;
impl Plugin for CounterFollowupVerificationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Check>()
            .add_systems(PreUpdate, (drive.before(InputSystems)).run_if(crate::loading::ready))
            .add_systems(Update, (inspect.after(crate::animation::animate)).run_if(crate::loading::ready));
    }
}
#[derive(Resource, Default)]
struct Check {
    stage: u8,
    at: f32,
    heavy_guard: bool,
    left_dodge: bool,
    right_dodge: bool,
    guard_return: bool,
    jump_origin: Option<Vec3>,
    forward: f32,
    jump_heavy: bool,
    heavy_combo: bool,
    combo_continues: bool,
    dodge_origin: Option<Vec3>,
    dodge_peak: f32,
    left_rise: f32,
    right_rise: f32,
    trick_dodge: bool,
    trick_airborne_block: bool,
    trick_distance: f32,
    pose_samples: Vec<(String, f32)>,
    pose_sample_at: f32,
    capture: Option<&'static str>,
    launch_capture: bool,
    guard_air_capture: bool,
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
    if !world.resource::<Options>().counter_followup_check {
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
        elapsed < 28. || world.resource::<Check>().saved,
        "Counter followup timed out at{stage}"
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
            mouse(world, MouseButton::Right, true);
            true
        }
        4 if age >= 0.65 => {
            let p = world.resource::<Player>();
            assert_eq!(p.melee.attack, Some(Attack::CounterGuard));
            assert_eq!(p.melee.counter_stage, 1);
            mouse(world, MouseButton::Left, true);
            true
        }
        5 if age >= 0.25 => {
            assert_eq!(
                world.resource::<Player>().melee.attack,
                Some(Attack::CounterRetaliation)
            );
            world.resource_mut::<Check>().heavy_guard = true;
            world.resource_mut::<Check>().capture = Some("counter-followup-guard-heavy");
            mouse(world, MouseButton::Left, false);
            true
        }
        6 if age >= 1.8
            && world.resource::<Player>().melee.attack == Some(Attack::CounterGuard)
            && world.resource::<Player>().melee.counter_stage == 1 =>
        {
            world.resource_mut::<Check>().dodge_origin=Some(world.resource::<Player>().feet);
            world.resource_mut::<Check>().dodge_peak=0.;
            key(world, KeyCode::KeyA, true);
            true
        }
        7 if age >= 0.05 => {
            key(world, KeyCode::Space, true);
            true
        }
        8 if age >= 0.08 => {
            key(world, KeyCode::Space, false);
            key(world, KeyCode::KeyA, false);
            assert!(
                world.resource::<Check>().left_dodge,
                "held-right chord must present left dodge before returning guard"
            );
            true
        }
        9 if age >= 0.65 && world.resource::<Player>().grounded && world.resource::<Player>().guard_jump.is_none() => {
            assert_eq!(
                world.resource::<Player>().melee.attack,
                Some(Attack::CounterGuard)
            );
            world.resource_mut::<Check>().guard_return = true;
            world.resource_mut::<Check>().capture = Some("counter-followup-guard-return");
            let rise=world.resource::<Check>().dodge_peak;
            assert!(rise>1.,"held-right chord must preserve its native lower jump");
            world.resource_mut::<Check>().left_rise=rise;
            world.resource_mut::<Check>().dodge_origin=Some(world.resource::<Player>().feet);
            world.resource_mut::<Check>().dodge_peak=0.;
            key(world, KeyCode::KeyD, true);
            true
        }
        10 if age >= 0.05 => {
            key(world, KeyCode::Space, true);
            true
        }
        11 if age >= 0.08 => {
            key(world, KeyCode::Space, false);
            key(world, KeyCode::KeyD, false);
            assert!(
                world.resource::<Check>().right_dodge,
                "held-right chord must present right dodge"
            );
            true
        }
        12 if age >= 0.65 && world.resource::<Player>().grounded && world.resource::<Player>().guard_jump.is_none() => {
            assert_eq!(
                world.resource::<Player>().melee.attack,
                Some(Attack::CounterGuard)
            );
            let rise=world.resource::<Check>().dodge_peak;
            assert!(rise>1.);
            world.resource_mut::<Check>().right_rise=rise;
            mouse(world, MouseButton::Right, false);
            true
        }
        13 if age >= 0.3
            && world.resource::<Player>().melee.attack.is_none()
            && world.resource::<Player>().grounded =>
        {
            key(world, KeyCode::Space, true);
            true
        }
        14 if age >= 0.16 => {
            key(world, KeyCode::Space, false);
            mouse(world, MouseButton::Left, true);
            true
        }
        15 if age >= 0.05 => {
            mouse(world, MouseButton::Left, false);
            true
        }
        16 if world.resource::<Player>().melee.attack == Some(Attack::CounterJump) => {
            world.resource_mut::<Check>().jump_origin = Some(world.resource::<Player>().feet);
            true
        }
        17 if age >= 0.15 => {
            mouse(world, MouseButton::Left, true);
            true
        }
        18 if age >= 0.05 => {
            mouse(world, MouseButton::Left, false);
            true
        }
        19 if world.resource::<Player>().melee.attack == Some(Attack::CounterRetaliation) => {
            assert!(
                world.resource::<Check>().forward > 1.,
                "jump must move the actor forward: {}",
                world.resource::<Check>().forward
            );
            world.resource_mut::<Check>().jump_heavy = true;
            mouse(world, MouseButton::Left, true);
            true
        }
        20 if age >= 0.05 => {
            mouse(world, MouseButton::Left, false);
            true
        }
        21 if world.resource::<Player>().melee.attack == Some(Attack::CounterCombo(0)) => {
            world.resource_mut::<Check>().heavy_combo = true;
            mouse(world, MouseButton::Left, true);
            true
        }
        22 if age >= 0.05 => {
            mouse(world, MouseButton::Left, false);
            true
        }
        23 if world.resource::<Player>().melee.attack == Some(Attack::CounterCombo(1)) => {
            world.resource_mut::<Check>().combo_continues = true;
            true
        }
        24 if age>=1.5 && world.resource::<Player>().melee.attack.is_none() => {
            mouse(world,MouseButton::Right,true); true
        }
        25 if age>=0.3 => {
            world.resource_mut::<Check>().dodge_origin=Some(world.resource::<Player>().feet);
            world.resource_mut::<Check>().dodge_peak=0.;
            mouse(world,MouseButton::Right,false);
            key(world,KeyCode::KeyA,true); true
        }
        26 if age>=0.05 => {key(world,KeyCode::Space,true);true}
        27 if age>=0.08 => {key(world,KeyCode::Space,false);true}
        28 if age>=0.15 => {mouse(world,MouseButton::Right,true);true}
        29 if age>=0.1 => {key(world,KeyCode::KeyA,false);true}
        30 if age>=1. && !world.resource::<Check>().saved => {
            let c = world.resource::<Check>();
            assert!(
                c.heavy_guard
                    && c.left_dodge
                    && c.right_dodge
                    && c.guard_return
                    && c.jump_heavy
                    && c.heavy_combo
                    && c.combo_continues
                    && c.trick_dodge && c.trick_airborne_block
            );
            assert!(c.trick_distance>2. && c.dodge_peak>1.);
            let report = serde_json::json!({"status":"passed","evidence_kind":"recreation real-input check; original controls separately retained","paired_original":false,"checks":["heavy while holding active block","held-right left/right chord launches lower jump before automatic guard return","release-dodge-repress restores airborne block","jump applies planar entry-directed ballistic movement on following actor pass","short jump followup selects heavy after700ms","heavy chains into combo1 and combo2"],"forward_displacement_metres":c.forward,"held_chord_rise_metres":[c.left_rise,c.right_rise],"release_repress_trick":{"dodge":c.trick_dodge,"airborne_block":c.trick_airborne_block,"rise_metres":c.dodge_peak,"distance_metres":c.trick_distance},"sampled_root_pose_y":c.pose_samples});
            std::fs::write(
                "logs/counter-followup-verification.json",
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
        eprintln!("counter-followup stage={stage} at={elapsed:.3}");
        let mut c = world.resource_mut::<Check>();
        c.stage += 1;
        c.at = elapsed;
    }
}
fn inspect(
    mut commands: Commands,
    options: Res<Options>,
    player: Res<Player>,
    animation: Option<Res<Animation>>,
    time: Res<Time>,
    mut c: ResMut<Check>,
) {
    if !options.counter_followup_check {
        return;
    }
    assert!(player.melee.equipped == Weapon::CounterSword || c.stage < 4);
    if let Some(origin)=c.dodge_origin {
        c.dodge_peak=c.dodge_peak.max(player.feet.y-origin.y);
        if c.stage>=26 {
            c.trick_distance=c.trick_distance.max((player.feet-origin).with_y(0.).length());
            c.trick_dodge|=matches!(player.action,MovementAction::DodgeLeft|MovementAction::DodgeRight);
            c.trick_airborne_block|=c.stage>=29 && !player.grounded && player.melee.attack==Some(Attack::CounterGuard) && player.guard_jump.is_some();
        }
    }
    if !c.guard_air_capture && (8..=9).contains(&c.stage) && c.dodge_peak>2.
        && !player.grounded && player.melee.attack==Some(Attack::CounterGuard) {
        c.guard_air_capture=true;
        c.capture=Some("counter-followup-airborne-block");
    }
    match player.action {
        MovementAction::DodgeLeft => {
            if !c.left_dodge {
                c.capture = Some("counter-followup-left-dodge");
            }
            c.left_dodge = true;
        }
        MovementAction::DodgeRight => {
            if !c.right_dodge {
                c.capture = Some("counter-followup-right-dodge");
            }
            c.right_dodge = true;
        }
        _ => {}
    }
    if player.melee.attack == Some(Attack::CounterJump) {
        if let Some(origin) = c.jump_origin {
            c.forward = c.forward.max((player.feet - origin).dot(Vec3::NEG_Z));
        }
    }
    if player.melee.attack == Some(Attack::CounterJump)
        && player.melee.phase == AttackPhase::Normal
        && player.melee.elapsed >= 0.42
        && !c.launch_capture
    {
        c.launch_capture = true;
        c.capture = Some("counter-followup-forward-launch");
    }
    if let Some(animation) = animation {
        let label = format!("{:?}", player.action);
        let changed = c.pose_samples.last().is_none_or(|sample| sample.0 != label);
        if c.stage >= 7
            && c.stage <= 13
            && c.pose_samples.len() < 250
            && (changed || time.elapsed_secs() - c.pose_sample_at >= 0.016)
        {
            if let Some(matrix) = animation.bone_matrix("Bip01") {
                c.pose_samples.push((label, matrix.w_axis.y));
                c.pose_sample_at = time.elapsed_secs();
            }
        }
    }
    if let Some(name) = c.capture.take() {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("logs/{name}.png")));
    }
}
