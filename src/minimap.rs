use crate::{
    Options,
    gameplay::{Player, View},
    hud::{Anchor, SourceRect, source_node},
    scene::{LOCAL_TEAM, MapId, MapSelection},
};
use bevy::{
    asset::{embedded_asset, embedded_path, AssetPath}, prelude::*, render::render_resource::AsBindGroup, shader::ShaderRef,
};
use serde::Deserialize;
use std::collections::HashMap;

fn register_shader(app: &mut App) {
    embedded_asset!(app, "minimap.wgsl");
}

pub struct MinimapPlugin;
impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        register_shader(app);
        app.add_plugins(UiMaterialPlugin::<RadarMaterial>::default())
            .init_resource::<RadarChecks>()
            .add_systems(crate::loading::PrepareWorld, setup)
            .add_systems(
                Update, ((
                    update.after(crate::gameplay::present),
                    inspect.after(update),
                )).run_if(crate::loading::ready));
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct RadarMaterial {
    #[texture(0)]
    #[sampler(1)]
    map: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    mask: Handle<Image>,
    #[uniform(4)]
    region: Vec4,
    #[uniform(5)]
    rotation: Vec4,
}
impl UiMaterial for RadarMaterial {
    fn fragment_shader() -> ShaderRef {
        AssetPath::from_path_buf(embedded_path!("minimap.wgsl"))
            .with_source("embedded")
            .into()
    }
}

pub(crate) fn warmup(world: &mut World) {
    let handles: Vec<_> = world.query::<&MaterialNode<RadarMaterial>>().iter(world)
        .map(|node| node.0.clone()).collect();
    for handle in handles {
        world.spawn((crate::loading::Warmup, MaterialNode(handle), Node {
            position_type: PositionType::Absolute, width: px(2), height: px(2), ..default()
        }));
    }
}

#[derive(Deserialize)]
struct SpriteSource {
    file: String,
}
#[derive(Deserialize)]
struct MapSource {
    image: String,
    enemy_image: String,
    bounds: [f32; 4],
    range_radius: f32,
}
impl MapSource {
    fn region(&self, feet: Vec3) -> Vec4 {
        let [left, right, top, bottom] = self.bounds;
        Vec4::new(
            (feet.x - left) / (right - left),
            (top + feet.z) / (top - bottom),
            self.range_radius / (right - left),
            self.range_radius / (top - bottom),
        )
    }
    fn image(&self) -> &str {
        if LOCAL_TEAM == 2 {
            &self.enemy_image
        } else {
            &self.image
        }
    }
}
#[derive(Resource, Deserialize)]
struct RadarSource {
    rect: [f32; 4],
    player_size: [f32; 2],
    sprites: HashMap<String, SpriteSource>,
    maps: HashMap<String, MapSource>,
}
#[derive(Resource)]
struct Radar {
    material: Handle<RadarMaterial>,
    images: HashMap<String, Handle<Image>>,
    current_map: Option<MapId>,
}
#[derive(Component)]
struct RadarRoot;

fn visible_for_map(map: MapId) -> bool {
    crate::hud::HudMode::for_map(map) != crate::hud::HudMode::Chaser
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<RadarMaterial>>,
) {
    let source: RadarSource = serde_json::from_slice(
        &std::fs::read("assets/ui/minimap/minimap.json")
            .expect("Run tools/asset_minimap.py to import the original radar"),
    )
    .expect("Invalid original radar manifest");
    let images: HashMap<_, _> = source
        .maps
        .iter()
        .map(|(id, spec)| (id.clone(), assets.load(spec.image().to_owned())))
        .collect();
    let material = materials.add(RadarMaterial {
        map: images["circle-2"].clone(),
        mask: assets.load(source.sprites["mask"].file.clone()),
        region: Vec4::new(0.5, 0.5, 0.4, 0.4),
        rotation: Vec4::new(1., 0., 0., 0.),
    });
    let canvas = commands.spawn((
        Name::new("Original radar logical canvas"),
        crate::hud::DesignCanvas,
        source_node([0., 0., 1024., 768.]),
        UiTransform::default(),
    )).id();
    let [_, _, width, height] = source.rect;
    let root = commands
        .spawn((
            Name::new("Original EU1162 circular radar"),
            RadarRoot,
            source_node(source.rect),
            SourceRect {
                rect: source.rect,
                anchor: Anchor::TopLeft,
            },
            GlobalZIndex(11),
        ))
        .id();
    let background = commands
        .spawn((
            Name::new("Original radar map and circular mask"),
            MaterialNode(material.clone()),
            source_node([0., 0., width, height]),
        ))
        .id();
    let [player_width, player_height] = source.player_size;
    let player = commands
        .spawn((
            Name::new("Original radar player arrow"),
            ImageNode::new(assets.load(source.sprites["player"].file.clone())),
            source_node([
                (width - player_width) * 0.5,
                (height - player_height) * 0.5,
                player_width,
                player_height,
            ]),
        ))
        .id();
    let rim = commands
        .spawn((
            Name::new("Original radar blue rim"),
            ImageNode::new(assets.load(source.sprites["rim"].file.clone())),
            source_node([0., 0., width, height]),
        ))
        .id();
    commands.entity(canvas).add_child(root);
    commands
        .entity(root)
        .add_children(&[background, player, rim]);
    commands.insert_resource(source);
    commands.insert_resource(Radar {
        material,
        images,
        current_map: None,
    });
}

fn update(
    source: Res<RadarSource>,
    mut radar: ResMut<Radar>,
    mut materials: ResMut<Assets<RadarMaterial>>,
    selection: Res<MapSelection>,
    player: Res<Player>,
    camera: Single<&Transform, With<View>>,
    options: Res<Options>,
    mut roots: Query<&mut Visibility, With<RadarRoot>>,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("minimap"));
    let map_changed = radar.current_map != Some(selection.active);
    let region = source.maps[selection.active.id()].region(player.feet);
    let forward = camera.forward();
    let yaw = forward.x.atan2(-forward.z);
    let (sin, cos) = yaw.sin_cos();
    let rotation = Vec4::new(cos, sin, 0., 0.);
    let material = materials.get(&radar.material).expect("Radar material");
    if map_changed || material.region != region || material.rotation != rotation {
        let material = materials.get_mut(&radar.material).expect("Radar material");
        if map_changed {
            material.map = radar.images[selection.active.id()].clone();
            radar.current_map = Some(selection.active);
        }
        material.region = region;
        material.rotation = rotation;
    }
    for mut visible in &mut roots {
        visible.set_if_neq(if !visible_for_map(selection.active) || options.overview || player.health <= 0.
        {
            Visibility::Hidden
        } else {
            Visibility::Visible
        });
    }
}

#[derive(Resource, Default)]
struct RadarChecks {
    maps: std::collections::BTreeSet<String>,
    frames: usize,
    saved: bool,
}

fn inspect(
    options: Res<Options>,
    time: Res<Time>,
    selection: Res<MapSelection>,
    source: Res<RadarSource>,
    radar: Res<Radar>,
    player: Res<Player>,
    materials: Res<Assets<RadarMaterial>>,
    roots: Query<(&Visibility, &Node), With<RadarRoot>>,
    mut checks: ResMut<RadarChecks>,
) {
    if !options.hud_smoke {
        return;
    }
    let material = materials.get(&radar.material).expect("Radar material");
    assert_eq!(radar.current_map, Some(selection.active));
    assert_eq!(material.map, radar.images[selection.active.id()]);
    assert!(material.region.abs_diff_eq(
        source.maps[selection.active.id()].region(player.feet),
        0.00001
    ));
    for (visibility, node) in &roots {
        assert_eq!(
            *visibility == Visibility::Visible,
            visible_for_map(selection.active) && !options.overview && player.health > 0.
        );
        assert_eq!(
            (node.left, node.top, node.width, node.height),
            (px(30.), px(30.), px(150.), px(150.))
        );
    }
    checks.maps.insert(selection.active.id().into());
    checks.frames += 1;
    if time.elapsed_secs() >= 12. && !checks.saved {
        assert_eq!(checks.maps.len(), 3);
        let report = serde_json::json!({"passed": true, "maps": checks.maps, "frames": checks.frames,
            "checks": ["Original map textures update on map changes", "UV center follows live player coordinates", "150x150 source size and 30px top-left margins survive resizing", "Deathmatch and Touchdown show radar; Chaser hides it"]});
        std::fs::write(
            "logs/minimap-smoke.json",
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        checks.saved = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radar_shader_path_resolves_to_the_registered_embedded_asset() {
        use bevy::asset::io::{embedded::EmbeddedAssetRegistry, AssetSourceId};
        let mut app = App::new();
        app.init_resource::<EmbeddedAssetRegistry>();
        register_shader(&mut app);
        let ShaderRef::Path(path) = RadarMaterial::fragment_shader() else {
            panic!("Expected an embedded shader path")
        };
        assert_eq!(path.source(), &AssetSourceId::from("embedded"));
        assert!(app.world().resource::<EmbeddedAssetRegistry>().remove_asset(path.path()).is_some());
    }

    #[test]
    fn source_map_coordinates_keep_player_centered_without_mirroring_z() {
        let source: RadarSource =
            crate::content::required_json("ui/minimap/minimap.json");
        assert_eq!(source.rect, [30., 30., 150., 150.]);
        for id in MapId::ALL {
            let map = &source.maps[id.id()];
            let [left, right, top, bottom] = map.bounds;
            assert_eq!(
                map.region(Vec3::new(left, 0., -top)).truncate().truncate(),
                Vec2::ZERO
            );
            assert_eq!(
                map.region(Vec3::new(right, 0., -bottom))
                    .truncate()
                    .truncate(),
                Vec2::ONE
            );
            let center = map.region(Vec3::new((left + right) * 0.5, 0., -(top + bottom) * 0.5));
            assert!(Vec2::new(center.x, center.y).abs_diff_eq(Vec2::splat(0.5), 0.00001));
            assert_eq!(map.range_radius, 40.);
        }
        let map = &source.maps["station-2"];
        assert!(map.image().ends_with("station-2-enemy.png"));
    }

    #[test]
    fn source_modes_show_radar_in_deathmatch_and_touchdown_but_hide_chaser() {
        assert!(!visible_for_map(MapId::Circle2));
        assert!(visible_for_map(MapId::Neden1));
        assert!(visible_for_map(MapId::Station2));
    }
}
