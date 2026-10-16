use super::{Controls, Motion, MovementAction, MovementInput, Player};
use crate::{Options, melee::MeleeInput, scene::Level};
use bevy::prelude::*;

pub fn reset_for_level(player: &mut Player, controls: &mut Controls, level: &Level) {
    player.reset_motion(level.spawn);
    controls.movement = Vec2::ZERO;
    controls.jump = false;
    controls.crouch = false;
    controls.respawn = false;
    controls.clear_actions();
    controls.emote = None;
    controls.cancel_emote = false;
    let direction = level.center - level.spawn;
    controls.yaw = if Vec2::new(direction.x, direction.z).length_squared() > 0.0001 {
        direction.x.atan2(-direction.z)
    } else {
        0.0
    };
    controls.pitch = -0.12;
}

pub fn prepare_input(
    mut player: ResMut<Player>,
    mut controls: ResMut<Controls>,
    level: Res<Level>,
    mut command: ResMut<MovementInput>,
) {
    if controls.respawn || player.death_age.is_some_and(|age| age >= 3.) || !player.feet.is_finite()
    {
        player.reset_motion(level.spawn);
        player.respawns += 1;
        controls.respawn = false;
        controls.jump = false;
        controls.clear_actions();
        controls.emote = None;
        controls.cancel_emote = false;
    }
    *command = MovementInput {
        emote: controls.emote.take(),
        cancel_emote: std::mem::take(&mut controls.cancel_emote),
        movement: controls.movement,
        jump: std::mem::take(&mut controls.jump),
        dodge: std::mem::take(&mut controls.dodge),
        dodge_side: std::mem::take(&mut controls.dodge_side),
        sprint: controls.sprint,
        crouch: controls.crouch,
        yaw: controls.yaw,
        pitch: controls.pitch,
        melee: MeleeInput {
            dispatch_blocked: false,
            dodging: false,
            recognition_deferred: controls.melee.recognition_deferred,
            secondary_down_sample: controls.melee.secondary_down_sample,
            primary_down_sample: controls.melee.primary_down_sample,
            movement_requested: controls.movement.length_squared() > 0.01,
            primary_pressed: std::mem::take(&mut controls.melee.primary_pressed),
            primary_held: controls.melee.primary_held,
            primary_released: std::mem::take(&mut controls.melee.primary_released),
            secondary_pressed: std::mem::take(&mut controls.melee.secondary_pressed),
            secondary_held: controls.melee.secondary_held,
            secondary_released: std::mem::take(&mut controls.melee.secondary_released),
            equip: controls.melee.equip.take(),
        },
    };
}

pub fn simulate(
    content: Res<crate::content::simulation::SimulationContent>,
    mut player: ResMut<Player>,
    mut controls: ResMut<Controls>,
    level: Res<Level>,
    motion: Res<Motion>,
    time: Res<Time<Fixed>>,
    options: Res<Options>,
    selection: Res<crate::scene::MapSelection>,
    command: Res<MovementInput>,
    mut telemetry: Option<ResMut<crate::parity::Telemetry>>,
) {
    let dt = time.delta_secs();
    let command = *command;
    let before = player.feet;
    player.step(&content, &level.collision, &motion, command, dt);
    if player.health > 0. && !player.god_mode {
        if let Some(zone) = level.death_zones.iter().find(|zone| {
            zone.intersects(
                before,
                player.feet,
                Vec3::from(motion.collision_axis_scales),
                level.local_team,
            )
        }) {
            info!("Original death volume: {}", zone.name);
            player.die();
        }
    }
    if let Some(telemetry) = telemetry.as_mut() {
        telemetry
            .record(
                &time,
                selection.active,
                options.physics_hz,
                motion.smooth_physics,
                &player,
                before,
                command,
            )
            .expect("Cannot write parity telemetry");
    }
    if player.action != MovementAction::Sprint {
        controls.sprint = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controller::CollisionWorld, melee::Weapon};

    fn input_app(controls: Controls) -> App {
        let mut app = App::new();
        let motion = Motion::default();
        app.insert_resource(Player::new(Vec3::Y, &motion))
            .insert_resource(controls)
            .init_resource::<MovementInput>()
            .insert_resource(Level {
                collision: CollisionWorld::new(Vec::new()),
                spawn: Vec3::new(10., 20., 30.),
                center: Vec3::ZERO,
                min_y: -100.,
                triangle_count: 0,
                death_zones: Vec::new(),
                local_team: 0,
                sectors: Vec::new(),
            })
            .add_systems(Update, prepare_input);
        app
    }

    #[test]
    fn hug_command_survives_uncaptured_device_samples_and_is_consumed_once() {
        let mut controls = Controls::default();
        Player::queue_hug(&mut controls);
        controls.clear_actions();
        let mut app = input_app(controls);
        app.update();
        assert_eq!(
            app.world().resource::<MovementInput>().emote,
            Some(crate::content::emotes::HUG)
        );
        assert!(!app.world().resource::<MovementInput>().cancel_emote);
        app.update();
        assert!(app.world().resource::<MovementInput>().emote.is_none());
    }

    #[test]
    fn fixed_input_consumes_edges_once_but_retains_held_and_sample_state() {
        let mut app = input_app(Controls {
            movement: Vec2::X,
            jump: true,
            dodge: true,
            dodge_side: -1.,
            sprint: true,
            melee: MeleeInput {
                primary_pressed: true,
                primary_held: true,
                primary_released: true,
                secondary_pressed: true,
                secondary_held: true,
                secondary_released: true,
                primary_down_sample: true,
                secondary_down_sample: true,
                recognition_deferred: true,
                equip: Some(Weapon::Dagger),
                ..default()
            },
            ..default()
        });
        app.update();
        let first = *app.world().resource::<MovementInput>();
        assert!(first.jump && first.dodge);
        assert_eq!(first.dodge_side, -1.);
        assert!(first.melee.primary_pressed && first.melee.primary_released);
        assert!(first.melee.secondary_pressed && first.melee.secondary_released);
        assert_eq!(first.melee.equip, Some(Weapon::Dagger));
        app.update();
        let second = *app.world().resource::<MovementInput>();
        assert!(!second.jump && !second.dodge);
        assert_eq!(second.dodge_side, 0.);
        assert!(!second.melee.primary_pressed && !second.melee.primary_released);
        assert!(!second.melee.secondary_pressed && !second.melee.secondary_released);
        assert_eq!(second.melee.equip, None);
        assert!(second.melee.primary_held && second.melee.secondary_held);
        assert!(second.melee.primary_down_sample && second.melee.secondary_down_sample);
        assert!(second.melee.recognition_deferred);
        assert!(second.sprint && second.melee.movement_requested);
        assert_eq!(second.movement, Vec2::X);
    }

    #[test]
    fn respawn_clears_actions_before_preparing_the_fixed_command() {
        for explicit_respawn in [false, true] {
            let mut app = input_app(Controls {
                respawn: explicit_respawn,
                movement: Vec2::Y,
                jump: true,
                dodge: true,
                sprint: true,
                melee: MeleeInput {
                    primary_pressed: true,
                    primary_held: true,
                    ..default()
                },
                ..default()
            });
            if !explicit_respawn {
                app.world_mut().resource_mut::<Player>().death_age = Some(3.);
            }
            app.update();
            let player = app.world().resource::<Player>();
            assert_eq!(player.feet, Vec3::new(10., 20., 30.));
            assert_eq!(player.respawns, 1);
            assert_eq!(player.death_age, None);
            let command = app.world().resource::<MovementInput>();
            assert!(!command.jump && !command.dodge && !command.sprint);
            assert!(!command.melee.primary_pressed && !command.melee.primary_held);
            assert_eq!(command.movement, Vec2::Y);
        }
    }
}
