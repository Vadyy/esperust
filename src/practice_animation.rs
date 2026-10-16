use crate::animation_core::storage::{AnimationStorage, ClipBuffers};
use crate::combat::{CombatStats, PracticeTarget};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
struct Clip {
    duration_ticks: f64,
    frame_count: usize,
    byte_offset: usize,
}

#[derive(Deserialize)]
struct Action {
    clip: String,
    arguments: (f64, u32, u32, bool, f64, bool),
}

#[derive(Deserialize)]
struct Sound {
    path: String,
}

#[derive(Deserialize)]
struct Manifest {
    format_version: u32,
    source_ticks_per_ms: f64,
    bone_count: usize,
    floats_per_frame: usize,
    data_file: String,
    byte_length: usize,
    clips: HashMap<String, Clip>,
    actions: HashMap<String, Action>,
    audio: HashMap<String, Sound>,
}

use crate::animation_core::skinning::CharacterSkin as Skin;

#[derive(Resource)]
pub(crate) struct PracticeAssets {
    manifest: Manifest,
    skin: Arc<Skin>,
    data: ClipBuffers,
    sounds: HashMap<String, Handle<AudioSource>>,
    head_bone: usize,
    pelvis_bone: usize,
}

use crate::animation_core::LocalPose as Pose;

#[derive(Component)]
pub(crate) struct PracticeAnimator {
    meshes: Vec<(usize, Handle<Mesh>)>,
    action: &'static str,
    serial: u32,
    melee_serial: Option<u64>,
    cursor: f64,
    displayed: Vec<Pose>,
    outgoing: Vec<Pose>,
    blend_age: f64,
    world: Vec<Mat4>,
    pub(crate) displayed_head_height: f32,
    pub(crate) displayed_pelvis_height: f32,
}

impl PracticeAnimator {
    pub(crate) fn effect_bones(&self, assets: &PracticeAssets) -> HashMap<String, Mat4> {
        assets
            .skin
            .bones
            .iter()
            .zip(&self.world)
            .map(|(bone, matrix)| (bone.name.clone(), *matrix))
            .collect()
    }

    pub(crate) fn bone_matrix(&self, assets: &PracticeAssets, name: &str) -> Option<Mat4> {
        assets
            .skin
            .bones
            .iter()
            .position(|bone| bone.name == name)
            .and_then(|index| self.world.get(index))
            .copied()
    }
    pub(crate) fn clip_cursor<'a>(&self, assets: &'a PracticeAssets) -> (&'a str, f64) {
        (&assets.manifest.actions[self.action].clip, self.cursor)
    }

    pub(crate) fn new(handles: Vec<(String, Handle<Mesh>)>, assets: &PracticeAssets) -> Self {
        Self {
            meshes: handles
                .into_iter()
                .filter_map(|(name, handle)| {
                    assets
                        .skin
                        .meshes
                        .iter()
                        .position(|mesh| mesh.name == name)
                        .map(|i| (i, handle))
                })
                .collect(),
            action: "idle",
            serial: u32::MAX,
            melee_serial: None,
            cursor: 0.0,
            displayed: Vec::new(),
            outgoing: Vec::new(),
            blend_age: 0.0,
            world: Vec::new(),
            displayed_head_height: f32::NAN,
            displayed_pelvis_height: f32::NAN,
        }
    }
}

impl PracticeAssets {
    #[cfg(test)]
    pub(crate) fn storage_binding(&self) -> (&Arc<Skin>, &ClipBuffers) {
        (&self.skin, &self.data)
    }

    #[cfg(test)]
    pub(crate) fn read() -> Result<Self, Box<dyn std::error::Error>> {
        Self::read_shared(&mut AnimationStorage::default())
    }

    pub(crate) fn read_shared(
        storage: &mut AnimationStorage,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut manifest: Manifest =
            serde_json::from_slice(&std::fs::read("assets/practice/practice.json")?)?;
        let skin = storage.skin(Path::new("assets/player.json"))?;
        let raw = storage.frames(
            &Path::new("assets").join(&manifest.data_file),
            Path::new("assets/player.json"),
            manifest.source_ticks_per_ms,
            manifest.floats_per_frame,
        )?;
        if manifest.format_version != 1
            || manifest.bone_count != skin.bones.len()
            || manifest.floats_per_frame != manifest.bone_count * 10
            || !manifest.source_ticks_per_ms.is_finite()
            || manifest.source_ticks_per_ms <= 0.0
            || raw.len() * 4 != manifest.byte_length
        {
            return Err("Invalid practice pose layout or source clock".into());
        }
        let mut data = ClipBuffers::default();
        data.push(raw);
        if data.iter().any(|v| !v.is_finite())
            || data
                .parts_slices()
                .flat_map(|part| part.chunks_exact(10))
                .any(|v| (Quat::from_slice(&v[3..7]).length_squared() - 1.0).abs() > 0.0001)
        {
            return Err("Invalid practice skeleton pose".into());
        }
        for clip in manifest.clips.values() {
            if !data.valid_clip(
                clip.byte_offset,
                clip.frame_count,
                clip.duration_ticks,
                manifest.floats_per_frame,
            ) {
                return Err("Practice clip outside original frame buffer".into());
            }
        }
        for action in manifest.actions.values() {
            if !manifest.clips.contains_key(&action.clip)
                || !action.arguments.4.is_finite()
                || action.arguments.4 <= 0.0
            {
                return Err("Invalid practice source action".into());
            }
        }
        let melee: serde_json::Value =
            serde_json::from_slice(&std::fs::read("assets/melee/melee.json")?)?;
        let imported = &melee["animations"];
        if imported["bone_count"].as_u64() != Some(manifest.bone_count as u64)
            || imported["floats_per_frame"].as_u64() != Some(manifest.floats_per_frame as u64)
        {
            return Err("Sparring skeleton does not match practice skeleton".into());
        }
        let clock = imported["source_ticks_per_ms"]
            .as_f64()
            .ok_or("Missing sparring clock")?;
        if clock.to_bits() != manifest.source_ticks_per_ms.to_bits() {
            return Err("Sparring source clock does not match practice".into());
        }
        let melee_raw = storage.frames(
            &Path::new("assets").join(
                imported["data_file"]
                    .as_str()
                    .ok_or("Missing sparring buffer")?,
            ),
            Path::new("assets/player.json"),
            clock,
            manifest.floats_per_frame,
        )?;
        let offset = data.len() * 4;
        for (name, value) in imported["clips"].as_object().unwrap() {
            let mut clip: Clip = serde_json::from_value(value.clone())?;
            clip.byte_offset += offset;
            manifest.clips.insert(name.clone(), clip);
        }
        for (name, value) in imported["actions"].as_object().unwrap() {
            let action: Action = serde_json::from_value(value.clone())?;
            if !action.clip.is_empty() {
                manifest.actions.insert(name.clone(), action);
            }
        }
        data.push(melee_raw);
        if data.iter().any(|v| !v.is_finite())
            || manifest.clips.values().any(|clip| {
                !data.valid_clip(
                    clip.byte_offset,
                    clip.frame_count,
                    clip.duration_ticks,
                    manifest.floats_per_frame,
                )
            })
        {
            return Err("Invalid merged practice frame buffer".into());
        }
        let head_bone = skin
            .bones
            .iter()
            .position(|bone| bone.name == "Bip01 Head")
            .ok_or("Practice skeleton has no head")?;
        let pelvis_bone = skin
            .bones
            .iter()
            .position(|bone| bone.name == "Bip01 Pelvis")
            .ok_or("Practice skeleton has no pelvis")?;
        Ok(Self {
            manifest,
            skin,
            data,
            sounds: HashMap::new(),
            head_bone,
            pelvis_bone,
        })
    }

    pub(crate) fn duration(&self, action: &str) -> f32 {
        let action = &self.manifest.actions[action];
        (self.manifest.clips[&action.clip].duration_ticks
            / self.manifest.source_ticks_per_ms
            / 1000.0
            / action.arguments.4) as f32
    }

    pub(crate) fn sound(&self, name: &str) -> Handle<AudioSource> {
        self.sounds[name].clone()
    }

    fn sample(&self, action: &str, cursor: f64) -> Vec<Pose> {
        let clip = &self.manifest.clips[&self.manifest.actions[action].clip];
        let frame = (cursor / clip.duration_ticks * (clip.frame_count - 1) as f64)
            .clamp(0.0, (clip.frame_count - 1) as f64);
        let sample = crate::animation_core::frame_sample(
            clip.byte_offset,
            clip.frame_count,
            self.manifest.floats_per_frame,
            frame,
        );
        self.data.sample(sample, self.manifest.bone_count)
    }

    fn world_pose(&self, local: &[Pose]) -> Vec<Mat4> {
        let mut world = Vec::with_capacity(local.len());
        crate::animation_core::compose_world(
            local,
            self.skin.bones.iter().map(|bone| bone.parent),
            &mut world,
        );
        world
    }

    fn skin(&self, mesh: usize, world: &[Mat4]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        crate::animation_core::skinning::skin_mesh(&self.skin.meshes[mesh], world)
    }
}

pub(crate) fn load(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut storage: ResMut<AnimationStorage>,
) {
    let mut assets = PracticeAssets::read_shared(&mut storage)
        .expect("Export practice assets with tools/asset_practice.py");
    assets.sounds = assets
        .manifest
        .audio
        .iter()
        .map(|(name, sound)| (name.clone(), server.load(sound.path.clone())))
        .collect();
    commands.insert_resource(assets);
}

pub(crate) fn animate(
    assets: Res<PracticeAssets>,
    time: Res<Time>,
    mut targets: Query<(Entity, &PracticeTarget, &mut PracticeAnimator, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut stats: ResMut<CombatStats>,
    mut effects: MessageWriter<crate::combat::ActorEffect>,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("practice_animation"));
    for (entity, target, mut animation, transform) in &mut targets {
        let action = target.animation_action();
        let parameters = &assets.manifest.actions[action];
        let melee = target
            .fighter
            .as_ref()
            .map(|fighter| &fighter.melee)
            .filter(|melee| melee.attack.is_some() && action.starts_with("counter_"));
        let melee_serial = melee.map(|melee| melee.serial);
        if animation.action != action
            || animation.serial != target.reaction_serial
            || animation.melee_serial != melee_serial
        {
            if action == "stun_start" || action == "death" {
                effects.write(crate::combat::ActorEffect {
                    owner: Some(entity),
                    name: if action == "death" {
                        "dead_eff"
                    } else {
                        "stun"
                    },
                    origin: *transform,
                });
            }
            animation.outgoing = animation.displayed.clone();
            animation.blend_age = 0.0;
            animation.cursor = 0.0;
            animation.serial = target.reaction_serial;
            animation.melee_serial = melee_serial;
            animation.action = action;
        }
        let ticks = time.delta_secs_f64() * 1000.0 * assets.manifest.source_ticks_per_ms;
        let requested = target.animation_duration_ms();
        let clip_duration = assets.manifest.clips[&parameters.clip].duration_ticks;
        let numerator = if parameters.arguments.0 > 0. {
            parameters.arguments.0
        } else {
            (clip_duration / assets.manifest.source_ticks_per_ms).trunc()
        };
        let rate = parameters.arguments.4
            * if requested > 0. {
                numerator / requested
            } else {
                1.
            };
        if let Some(melee) = melee {
            animation.cursor = melee.animation_phase_elapsed() as f64
                * 1000.
                * assets.manifest.source_ticks_per_ms
                * rate;
        } else if target.reacting() {
            animation.cursor = target.reaction_elapsed() as f64
                * 1000.
                * assets.manifest.source_ticks_per_ms
                * rate;
        } else {
            animation.cursor += ticks * rate;
        }
        let duration = assets.manifest.clips[&parameters.clip].duration_ticks;
        animation.cursor = if parameters.arguments.3 {
            animation.cursor % duration
        } else {
            animation.cursor.min(duration)
        };
        animation.blend_age += ticks;
        let mut pose = assets.sample(action, animation.cursor);
        if !animation.outgoing.is_empty() && parameters.arguments.1 > 0 {
            let weight =
                (animation.blend_age / parameters.arguments.1 as f64).clamp(0.0, 1.0) as f32;
            for (pose, previous) in pose.iter_mut().zip(&animation.outgoing) {
                *pose = previous.blend(*pose, weight);
            }
        }
        let world = assets.world_pose(&pose);
        animation.displayed_head_height = world[assets.head_bone].w_axis.y;
        animation.displayed_pelvis_height = world[assets.pelvis_bone].w_axis.y;
        for (index, handle) in &animation.meshes {
            if let Some(mesh) = meshes.get_mut(handle) {
                let (positions, normals) = assets.skin(*index, &world);
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            }
        }
        animation.displayed = pose;
        animation.world = world;
        stats.animated_frames += 1;
        if action == "idle" {
            stats.idle_frames += 1;
        } else {
            stats.reaction_frames += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::simulation::parity_content;
    use bevy::audio::Decodable;

    #[test]
    fn original_male_practice_voices_decode() {
        let assets = PracticeAssets::read().unwrap();
        for name in [
            "voice_man_damage_1",
            "voice_man_damage_2",
            "voice_man_damage_3",
            "_eu_voice_man_dead_normal",
            "_eu_voice_man_dead_blow",
            "_eu_voice_man_dead_shock",
        ] {
            let path = std::path::Path::new("assets").join(&assets.manifest.audio[name].path);
            let sound = AudioSource {
                bytes: std::fs::read(path).unwrap().into(),
            };
            assert!(sound.decoder().take(1024).count() > 0, "{name}");
        }
    }

    #[test]
    fn original_idle_and_receiver_poses_animate_and_skin_without_nonfinite_values() {
        let assets = PracticeAssets::read().unwrap();
        assert_eq!(assets.manifest.bone_count, 81);
        let start = assets.sample("idle", 0.0);
        let moved = assets.sample("idle", 2500.0);
        assert!(
            start
                .iter()
                .zip(&moved)
                .any(|(a, b)| a.rotation.dot(b.rotation).abs() < 0.999)
        );
        for action in [
            "flinch_0",
            "flinch_1",
            "flinch_2",
            "flinch_3",
            "stun_start",
            "stun_loop",
            "blow_start",
            "blow_air",
            "blow_fall",
            "get_up",
            "down",
            "down_hit",
            "stand_up",
            "counter_damage_0",
            "counter_damage_1",
            "counter_damage_2",
            "counter_damage_3",
            "death",
        ] {
            for tick in [0.0, 1377.0, 6000.0] {
                let world = assets.world_pose(&assets.sample(action, tick));
                for mesh in 0..assets.skin.meshes.len() {
                    let (positions, normals) = assets.skin(mesh, &world);
                    assert!(
                        positions
                            .iter()
                            .chain(&normals)
                            .flatten()
                            .all(|v| v.is_finite()),
                        "{action}"
                    );
                }
            }
        }
        assert!((assets.manifest.actions["death"].arguments.4 - 1.3).abs() < 0.00001);
        assert!((assets.manifest.actions["blow_air"].arguments.4 - 0.8).abs() < 0.00001);
    }

    #[test]
    fn interrupted_receiver_blends_keep_rotations_normalized() {
        let assets = PracticeAssets::read().unwrap();
        let left = assets.sample("flinch_2", 1600.0);
        let right = assets.sample("flinch_3", 1600.0);
        for (a, b) in left.into_iter().zip(right) {
            let pose = a.blend(b, 0.5).blend(a, 0.5);
            assert!((pose.rotation.length_squared() - 1.0).abs() < 0.000001);
            assert!(pose.matrix().is_finite());
        }
    }

    #[test]
    fn original_death_remains_upright_early_then_falls_before_its_source_endpoint() {
        let assets = PracticeAssets::read().unwrap();
        let seconds = assets.duration("death");
        assert!((seconds - 1.8205128).abs() < 0.00001);
        let pose_at = |age: f32| {
            let ticks = age as f64
                * 1000.0
                * assets.manifest.source_ticks_per_ms
                * assets.manifest.actions["death"].arguments.4;
            assets.world_pose(&assets.sample("death", ticks))
        };
        let early = pose_at(0.7);
        assert!(early[assets.head_bone].w_axis.y > 2.0);
        let fallen = pose_at(seconds * 0.96);
        let head = fallen[assets.head_bone].w_axis.truncate();
        let pelvis = fallen[assets.pelvis_bone].w_axis.truncate();
        assert!(
            head.y < 0.5 && pelvis.y < 0.4,
            "Death clip must lie down: head {head}, pelvis {pelvis}"
        );
        assert!((head.y - pelvis.y).abs() < 0.2);
        assert!(Vec2::new(head.x - pelvis.x, head.z - pelvis.z).length() > 0.7);
    }

    #[test]
    fn simulation_duration_cache_matches_animation_playback_for_every_selected_action() {
        let assets = PracticeAssets::read().unwrap();
        for action in crate::content::reactions::ReactionAction::ALL {
            assert_eq!(
                parity_content().duration(action),
                assets.duration(action.source_name()),
                "{action:?}"
            );
        }
    }
}
