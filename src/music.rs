pub(crate) fn catalog_json() -> String {
    std::fs::read_to_string("assets/music.json")
        .unwrap_or_else(|_| include_str!("../config/music.json").to_string())
}

use crate::scene::{MapId, MapSelection};
use bevy::{audio::Volume, prelude::*};
use serde::Deserialize;

pub struct MusicPlugin;
impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MusicCatalog::load()).add_systems(
            Update,
            (follow_map.after(crate::scene::switch_maps)).run_if(crate::loading::ready),
        );
    }
}

#[derive(Resource, Deserialize)]
struct MusicCatalog {
    format_version: u32,
    volume: f32,
    maps: Vec<MusicMap>,
}
#[derive(Deserialize)]
struct MusicMap {
    id: String,
    tracks: Vec<String>,
}
impl MusicCatalog {
    fn load() -> Self {
        let catalog: Self =
            serde_json::from_str(&catalog_json()).expect("Invalid original music catalog");
        assert_eq!(catalog.format_version, 1);
        assert!(catalog.volume.is_finite() && (0.0..=1.0).contains(&catalog.volume));
        for map in MapId::ALL {
            assert_eq!(
                catalog
                    .maps
                    .iter()
                    .filter(|entry| entry.id == map.id())
                    .count(),
                1
            );
            assert!(!catalog.track(map).is_empty());
        }
        catalog
    }

    fn track(&self, map: MapId) -> &str {
        &self
            .maps
            .iter()
            .find(|entry| entry.id == map.id())
            .expect("Map missing from original music catalog")
            .tracks[0]
    }
}

#[derive(Component)]
pub(crate) struct MapMusic {
    map: MapId,
}

fn follow_map(
    mut commands: Commands,
    selection: Res<MapSelection>,
    catalog: Res<MusicCatalog>,
    assets: Res<AssetServer>,
    players: Query<(Entity, &MapMusic)>,
) {
    let mut existing = players.iter();
    if let Some((_, playing)) = existing.next()
        && playing.map == selection.active
        && existing.next().is_none()
    {
        return;
    }
    for (entity, _) in &players {
        commands.entity(entity).despawn();
    }
    let track = catalog.track(selection.active);
    commands.spawn((
        Name::new(format!("{} original music", selection.active.label())),
        MapMusic {
            map: selection.active,
        },
        AudioPlayer::new(assets.load(track.to_owned())),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(catalog.volume)),
    ));
    info!("Original music: {} → {track}", selection.active.label());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::audio::Decodable;

    #[test]
    fn music_catalog_selects_the_recovered_map_tracks() {
        let catalog = MusicCatalog::load();
        assert_eq!(
            catalog.track(MapId::Circle2),
            "original/resources/bgm/nanorisk.ogg"
        );
        assert_eq!(
            catalog.track(MapId::Station2),
            "original/resources/bgm/battle4.ogg"
        );
        assert_eq!(
            catalog.track(MapId::Neden1),
            "original/resources/bgm/battle.ogg"
        );
        let manifest: serde_json::Value = serde_json::from_str(&catalog_json()).unwrap();
        let files = manifest["resources"].as_array().unwrap();
        assert_eq!(files.len(), 4);
        for file in files {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets")
                .join(file["asset_path"].as_str().unwrap());
            let bytes = std::fs::read(path).expect("Original music was not exported");
            assert_eq!(&bytes[..4], b"OggS");
            let source = AudioSource {
                bytes: bytes.into(),
            };
            assert!(source.decoder().take(1024).count() > 0);
        }
    }

    #[test]
    fn music_replaces_a_map_once_and_repairs_duplicates() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<AudioSource>()
            .init_resource::<MapSelection>()
            .insert_resource(MusicCatalog::load())
            .add_systems(Update, follow_map);
        app.update();
        let first = app
            .world_mut()
            .query_filtered::<Entity, With<MapMusic>>()
            .single(app.world())
            .unwrap();
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<MapMusic>>()
                .single(app.world())
                .unwrap(),
            first
        );
        app.world_mut().resource_mut::<MapSelection>().active = MapId::Station2;
        app.update();
        assert!(app.world().get_entity(first).is_err());
        let replacement = app
            .world_mut()
            .query_filtered::<Entity, With<MapMusic>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().get::<MapMusic>(replacement).unwrap().map,
            MapId::Station2
        );
        assert!(matches!(
            app.world()
                .get::<PlaybackSettings>(replacement)
                .unwrap()
                .mode,
            bevy::audio::PlaybackMode::Loop
        ));
        app.world_mut().spawn(MapMusic {
            map: MapId::Station2,
        });
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&MapMusic>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
