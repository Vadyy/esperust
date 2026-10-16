use super::{FrameSample, LocalPose, skinning::CharacterSkin};
use bevy::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

type Error = Box<dyn std::error::Error>;

#[derive(Resource, Default)]
pub(crate) struct AnimationStorage {
    skins: HashMap<PathBuf, Arc<CharacterSkin>>,
    frames: HashMap<(PathBuf, PathBuf, u64, usize), Arc<[f32]>>,
}
impl AnimationStorage {
    pub(crate) fn skin(&mut self, path: &Path) -> Result<Arc<CharacterSkin>, Error> {
        let id = path.canonicalize()?;
        if let Some(skin) = self.skins.get(&id) {
            return Ok(skin.clone());
        }
        let mut skin: CharacterSkin = serde_json::from_slice(&std::fs::read(&id)?)?;
        skin.validate_and_normalize()?;
        let skin = Arc::new(skin);
        self.skins.insert(id, skin.clone());
        Ok(skin)
    }
    pub(crate) fn frames(
        &mut self,
        path: &Path,
        skin: &Path,
        clock: f64,
        stride: usize,
    ) -> Result<Arc<[f32]>, Error> {
        let key = (
            path.canonicalize()?,
            skin.canonicalize()?,
            clock.to_bits(),
            stride,
        );
        if let Some(frames) = self.frames.get(&key) {
            return Ok(frames.clone());
        }
        let bytes = std::fs::read(&key.0)?;
        if bytes.len() % 4 != 0 {
            return Err("Invalid animation float buffer length".into());
        }
        let frames: Arc<[f32]> = bytes
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        self.frames.insert(key, frames.clone());
        Ok(frames)
    }
}

#[derive(Default)]
pub(crate) struct ClipBuffers {
    parts: Vec<(usize, Arc<[f32]>)>,
    len: usize,
}
impl ClipBuffers {
    pub(crate) fn push(&mut self, frames: Arc<[f32]>) {
        let length = frames.len();
        self.parts.push((self.len, frames));
        self.len += length;
    }
    pub(crate) fn valid_clip(
        &self,
        offset: usize,
        count: usize,
        duration: f64,
        stride: usize,
    ) -> bool {
        self.parts.iter().any(|(start, data)| {
            offset
                .checked_sub(start * 4)
                .is_some_and(|local| super::valid_clip(local, count, duration, stride, data.len()))
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn parts_slices(&self) -> impl Iterator<Item = &[f32]> {
        self.parts.iter().map(|(_, data)| data.as_ref())
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &f32> {
        self.parts.iter().flat_map(|(_, data)| data.iter())
    }
    fn frame(&self, offset: usize, stride: usize) -> &[f32] {
        let (start, data) = self
            .parts
            .iter()
            .rev()
            .find(|(start, _)| *start <= offset)
            .expect("validated frame binding");
        &data[offset - start..offset - start + stride]
    }
    pub(crate) fn sample(&self, sample: FrameSample, bones: usize) -> Vec<LocalPose> {
        let first = self.frame(sample.offset0, bones * 10);
        let second = self.frame(sample.offset1, bones * 10);
        super::sample_pose_frames(first, second, sample.fraction, bones)
    }
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.parts.clear();
        self.len = 0;
    }
    #[cfg(test)]
    pub(crate) fn extend_from_slice(&mut self, values: &[f32]) {
        let mut data: Vec<f32> = self.iter().copied().collect();
        data.extend_from_slice(values);
        self.clear();
        self.push(data.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_offsets_preserve_source_bindings_and_endpoints() {
        let first: Arc<[f32]> = [
            0., 0., 0., 0., 0., 0., 1., 1., 1., 1., 2., 0., 0., 0., 0., 0., 1., 1., 1., 1.,
        ]
        .into();
        let second: Arc<[f32]> = [
            10., 0., 0., 0., 0., 0., 1., 1., 1., 1., 12., 0., 0., 0., 0., 0., 1., 1., 1., 1.,
        ]
        .into();
        let mut data = ClipBuffers::default();
        data.push(first.clone());
        data.push(second.clone());
        assert!(Arc::ptr_eq(&data.parts[0].1, &first));
        assert!(Arc::ptr_eq(&data.parts[1].1, &second));
        assert!(data.valid_clip(80, 2, 100., 10));
        assert!(!data.valid_clip(40, 2, 100., 10));
        assert_eq!(
            data.sample(super::super::frame_sample(80, 2, 10, 0.5), 1)[0]
                .translation
                .x,
            11.
        );
        assert_eq!(
            data.sample(super::super::frame_sample(80, 2, 10, 1.), 1)[0]
                .translation
                .x,
            12.
        );
    }
    #[test]
    fn source_ids_and_exact_bindings_isolate_storage_snapshots() {
        let root = std::env::temp_dir().join(format!("esper-c3-bindings-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let skin_a = root.join("a.json");
        let skin_b = root.join("b.json");
        let frames = root.join("poses.bin");
        std::fs::write(&skin_a, br#"{"bones":[],"meshes":[]}"#).unwrap();
        std::fs::write(&skin_b, br#"{"bones":[],"meshes":[]}"#).unwrap();
        std::fs::write(&frames, 1f32.to_le_bytes()).unwrap();
        let mut storage = AnimationStorage::default();
        let a = storage.skin(&skin_a).unwrap();
        assert!(Arc::ptr_eq(&a, &storage.skin(&skin_a).unwrap()));
        assert!(!Arc::ptr_eq(&a, &storage.skin(&skin_b).unwrap()));
        let source = storage.frames(&frames, &skin_a, 4.8, 10).unwrap();
        assert!(Arc::ptr_eq(
            &source,
            &storage
                .frames(&root.join("./poses.bin"), &skin_a, 4.8, 10)
                .unwrap()
        ));
        for (skin, clock, stride) in [(&skin_b, 4.8, 10), (&skin_a, 4.9, 10), (&skin_a, 4.8, 20)] {
            assert!(!Arc::ptr_eq(
                &source,
                &storage.frames(&frames, skin, clock, stride).unwrap()
            ));
        }
        std::fs::write(&frames, 2f32.to_le_bytes()).unwrap();
        let new = AnimationStorage::default()
            .frames(&frames, &skin_a, 4.8, 10)
            .unwrap();
        assert_eq!(source[0], 1.);
        assert_eq!(new[0], 2.);
        std::fs::write(&frames, [0u8; 3]).unwrap();
        assert!(
            AnimationStorage::default()
                .frames(&frames, &skin_a, 4.8, 10)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn imported_adapters_share_bound_storage_in_either_loading_order() {
        for player_first in [true, false] {
            let mut storage = AnimationStorage::default();
            let (player, practice) = if player_first {
                let player =
                    crate::animation::Animation::load_shared(vec![], &mut storage).unwrap();
                (
                    player,
                    crate::practice_animation::PracticeAssets::read_shared(&mut storage).unwrap(),
                )
            } else {
                let practice =
                    crate::practice_animation::PracticeAssets::read_shared(&mut storage).unwrap();
                (
                    crate::animation::Animation::load_shared(vec![], &mut storage).unwrap(),
                    practice,
                )
            };
            let (player_skin, player_buffers) = player.storage_binding();
            let (practice_skin, practice_buffers) = practice.storage_binding();
            assert!(Arc::ptr_eq(player_skin, practice_skin));
            assert_eq!(storage.skins.len(), 1);
            assert_eq!(storage.frames.len(), 4);
            assert!(storage.frames.keys().any(|(path, _, _, _)| path.ends_with("emotes/poses.bin")));
            let mut saved = 0;
            for (_, source) in &practice_buffers.parts {
                assert!(
                    player_buffers
                        .parts
                        .iter()
                        .any(|(_, other)| Arc::ptr_eq(source, other))
                );
                saved += source.len() * 4;
            }
            assert_eq!(saved, 20_787_840);
            println!("Shared frame allocation saves {saved} bytes plus one normalized skin");
            drop(storage);
            assert!(player_buffers.len() > practice_buffers.len());
        }
    }
}
