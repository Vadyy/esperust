use super::{Avatar, Controls, FpsCounter, Help, MapTitle, Motion, Player, Status, View};
use crate::{
    Options,
    melee::Weapon,
    scene::{self, Level, MapMesh},
};
use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*};
use serde::Deserialize;

#[derive(Deserialize)]
struct Character {
    meshes: Vec<MapMesh>,
}

pub fn setup(
    mut commands: Commands,
    level: Res<Level>,
    mut controls: ResMut<Controls>,
    options: Res<Options>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
    mut animation_storage: ResMut<crate::animation_core::storage::AnimationStorage>,
) {
    let motion: Motion = match std::fs::read("config/controller.json") {
        Ok(bytes) => serde_json::from_slice(&bytes).expect("Invalid controller configuration"),
        Err(_) => Motion::default(),
    };
    let spawn = options.spawn.unwrap_or(level.spawn);
    let direction = level.center - spawn;
    controls.yaw = options.yaw.unwrap_or_else(|| {
        if Vec2::new(direction.x, direction.z).length_squared() > 0.0001 {
            direction.x.atan2(-direction.z)
        } else {
            0.0
        }
    });
    controls.pitch = -0.12;
    controls.diagnostics = false;
    let mut player = Player::with_skill(spawn, &motion, options.skill);
    if !options.unarmed {
        player.melee.equipped = Weapon::PlasmaSword;
    }
    commands.insert_resource(player);
    let avatar = commands
        .spawn((
            Name::new("Original starter character"),
            Avatar,
            Transform::from_translation(spawn).with_scale(Vec3::splat(scene::PLAYER_MODEL_SCALE)),
            Visibility::default(),
        ))
        .id();
    match std::fs::read("assets/player.json")
        .ok()
        .and_then(|b| serde_json::from_slice::<Character>(&b).ok())
    {
        Some(character) => {
            let handles = scene::spawn_meshes(
                character.meshes,
                &mut commands,
                &mut meshes,
                &mut materials,
                &assets,
                Some(avatar),
                None,
            );
            match crate::animation::Animation::load_shared(handles, &mut animation_storage) {
                Ok(animation) => {
                    commands.insert_resource(animation);
                }
                Err(error) => warn!("Character animation unavailable: {error}"),
            }
        }
        None => {
            warn!("Original character export absent: showing diagnostic capsule");
            let child = commands
                .spawn((
                    Mesh3d(meshes.add(Capsule3d::new(
                        motion.radius,
                        motion.height - motion.radius * 2.,
                    ))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgb(0.3, 0.75, 0.85),
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_xyz(0., motion.height * 0.5 / scene::PLAYER_MODEL_SCALE, 0.)
                        .with_scale(Vec3::splat(1. / scene::PLAYER_MODEL_SCALE)),
                ))
                .id();
            commands.entity(avatar).add_child(child);
        }
    }
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: motion.camera_fov_degrees.to_radians(),
            near: 0.03,
            far: 20000.,
            ..default()
        }),
        Tonemapping::None,
        View {
            last_pose_reset_serial: 0,
        },
        Transform::from_xyz(0., 10., 30.).looking_at(level.center, Vec3::Y),
    ));
    let fps_root = commands.spawn((
        Name::new("Always visible FPS counter"),
        GlobalZIndex(100),
        Node {
            display: Display::Grid,
            position_type: PositionType::Absolute,
            left: px(2),
            top: px(2),
            ..default()
        },
    )).id();
    for entity in crate::ui_text::styled_text(
        &mut commands, fps_root, "0",
        TextFont {
            font: assets.load("ui/verdana.ttf"), font_size: 11., ..default()
        },
        TextLayout::new_with_no_wrap(), Color::WHITE, 2,
        Color::srgb(0.5, 0.5, 0.5),
    ) {
        commands.entity(entity).insert(FpsCounter);
    }
    commands.spawn((
        MapTitle,
        Text::new(options.map.label()),
        Visibility::Hidden,
        TextFont {
            font_size: 18.,
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.95, 1.)),
        BackgroundColor(Color::srgba(0.02, 0.04, 0.07, 0.75)),
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            top: px(38),
            padding: UiRect::all(px(4)),
            ..default()
        },
    ));
    commands.spawn((
        Status,
        Text::new(""),
        Visibility::Hidden,
        TextFont {
            font_size: 14.,
            ..default()
        },
        TextColor(Color::srgb(0.75, 0.85, 0.9)),
        BackgroundColor(Color::srgba(0.02, 0.04, 0.07, 0.75)),
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            top: px(69),
            padding: UiRect::all(px(4)),
            ..default()
        },
    ));
    commands.spawn((
        Help,
        Text::new("F3 diagnostics · F12 screenshot"),
        TextFont {
            font_size: 13.,
            ..default()
        },
        TextColor(Color::srgba(1., 1., 1., 0.65)),
        Node {
            position_type: PositionType::Absolute,
            right: px(16),
            top: px(72),
            ..default()
        },
    ));
    commands.insert_resource(motion);
}
