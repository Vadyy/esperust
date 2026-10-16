use crate::animation_core::storage::{AnimationStorage, ClipBuffers};
use std::{path::Path, sync::Arc};
use crate::gameplay::{Motion, MovementAction, Player, WallJumpPhase};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Manifest {
    format_version: u32,
    source_ticks_per_ms: f64,
    floats_per_frame: usize,
    bone_count: usize,
    clips: HashMap<String, Clip>,
    actions: HashMap<String, Action>,
    data_file: String,
}
use crate::animation_core::skinning::{Bone, CharacterSkin};

impl Bone {
    fn is_lower(&self) -> bool {
        self.name.contains("Bip01")
            && (self.name == "Bip01"
                || ["Thigh", "Calf", "Foot", "Toe", "Pelvis"]
                    .iter()
                    .any(|part| self.name.contains(*part)))
    }
}
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
    #[serde(default)]
    upper_arguments: Option<(f64, u32, u32, bool, f64, bool)>,
}
#[derive(Resource)]
pub struct Animation {
    manifest: Manifest,
    skin: Arc<CharacterSkin>,
    data: ClipBuffers,
    meshes: Vec<(Handle<Mesh>, usize)>,
    current: String,
    cursor_ticks: f64,
    last_jumps: u32,
    last_respawns: u32,
    last_action_serial: u32,
    last_pose_reset_serial: u32,
    displayed_pose: Vec<LocalPose>,
    world_pose: Vec<Mat4>,
    current_looping: bool,
    transition: Option<PoseTransition>,
    upper: LayerPlayback,
    last_melee_serial: u64,
    last_hit_serial: u64,
    last_emote_serial: u64,
    run_playback_rate: f64,
    lower_walk_playback_rate: f64,
}

#[derive(Default)]
struct LayerPlayback {
    current: String,
    cursor_ticks: f64,
    looping: bool,
    transition: Option<PoseTransition>,
}
use crate::animation_core::FrameSample;

use crate::animation_core::LocalPose;

enum TransitionSource {
    Playback {
        clip: String,
        cursor_ticks: f64,
        looping: bool,
    },
    Snapshot(Vec<LocalPose>),
}

struct PoseTransition {
    source: TransitionSource,
    elapsed_ticks: f64,
    duration_ticks: f64,
    upper_duration_ticks: f64,
}

impl PoseTransition {
    fn weight(&self, lower: bool) -> f32 {
        let duration = if lower {
            self.duration_ticks
        } else {
            self.upper_duration_ticks
        };
        if duration == 0. {
            1.
        } else {
            (self.elapsed_ticks / duration).clamp(0., 1.) as f32
        }
    }

    fn complete(&self) -> bool {
        self.elapsed_ticks >= self.duration_ticks.max(self.upper_duration_ticks)
    }
}

fn playback_rate(
    duration_ticks: f64,
    ticks_per_ms: f64,
    duration_override_ms: f64,
    requested_duration_ms: f64,
    rate: f64,
) -> f64 {
    if requested_duration_ms > 0. {
        let duration_ms = if duration_override_ms > 0. {
            duration_override_ms
        } else {
            (duration_ticks / ticks_per_ms).trunc()
        };
        rate * duration_ms / requested_duration_ms
    } else {
        rate
    }
}

fn advance_cursor(cursor: f64, delta_ticks: f64, duration: f64, looping: bool) -> f64 {
    let cursor = cursor + delta_ticks;
    if looping {
        if cursor > duration {
            cursor % duration
        } else {
            cursor
        }
    } else {
        cursor.min(duration)
    }
}

impl Animation {
    #[cfg(test)]
    pub(crate) fn storage_binding(&self) -> (&Arc<CharacterSkin>, &ClipBuffers) {
        (&self.skin, &self.data)
    }

    pub(crate) fn bone_matrix(&self, name: &str) -> Option<Mat4> {
        let index = self.skin.bones.iter().position(|bone| bone.name == name)?;
        self.world_pose.get(index).copied()
    }

    pub(crate) fn upper_cursor(&self) -> Option<(&str, f64)> {
        let action = self.manifest.actions.get(&self.upper.current)?;
        Some((&action.clip, self.upper.cursor_ticks))
    }

    pub(crate) fn footstep_cursor_ms(&self) -> Option<u32> {
        (self.current.starts_with("run_")
            || self.current.contains("_run_")
            || (self.current.contains("_walk_") && !self.current.ends_with("_walk_idle_lower"))
            || self.current == "sprint"
            || self.current.ends_with("_sprint"))
        .then(|| (self.cursor_ticks / self.manifest.source_ticks_per_ms).trunc() as u32)
    }

    #[cfg(test)]
    pub fn load(handles: Vec<(String, Handle<Mesh>)>) -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_shared(handles, &mut AnimationStorage::default())
    }

    pub(crate) fn load_shared(
        handles: Vec<(String, Handle<Mesh>)>,
        storage: &mut AnimationStorage,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut manifest: Manifest =
            serde_json::from_slice(&std::fs::read("assets/player-animations.json")?)?;
        let skin = storage.skin(Path::new("assets/player.json"))?;
        if manifest.format_version != 3
            || manifest.bone_count != skin.bones.len()
            || manifest.floats_per_frame != manifest.bone_count * 10
            || !manifest.source_ticks_per_ms.is_finite()
            || manifest.source_ticks_per_ms <= 0.
        {
            return Err("Invalid animation source clock".into());
        }
        let mut data = ClipBuffers::default();
        data.push(storage.frames(
            &Path::new("assets").join(&manifest.data_file),
            Path::new("assets/player.json"),
            manifest.source_ticks_per_ms,
            manifest.floats_per_frame,
        )?);
        if let Ok(bytes) = std::fs::read("assets/melee/melee.json") {
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if let Some(animations) = value.get("animations") {
                let mut melee: Manifest = serde_json::from_value(animations.clone())?;
                if melee.format_version != 3
                    || melee.bone_count != manifest.bone_count
                    || melee.floats_per_frame != manifest.floats_per_frame
                    || (melee.source_ticks_per_ms - manifest.source_ticks_per_ms).abs() > 0.00001
                {
                    return Err("Melee animation skeleton/clock does not match player".into());
                }
                let byte_offset = data.len() * 4;
                for clip in melee.clips.values_mut() {
                    clip.byte_offset += byte_offset;
                }
                data.push(storage.frames(
                    &Path::new("assets").join(&melee.data_file),
                    Path::new("assets/player.json"),
                    melee.source_ticks_per_ms,
                    melee.floats_per_frame,
                )?);
                melee.actions.retain(|_, action| !action.clip.is_empty());
                manifest.clips.extend(melee.clips);
                manifest.actions.extend(melee.actions);
            }
        }
        let mut emotes: Manifest = serde_json::from_slice(&std::fs::read("assets/emotes/emotes.json")?)?;
        if emotes.format_version != 3 || emotes.bone_count != manifest.bone_count
            || emotes.floats_per_frame != manifest.floats_per_frame
            || emotes.source_ticks_per_ms.to_bits() != manifest.source_ticks_per_ms.to_bits() {
            return Err("Emote skeleton or source clock does not match player".into());
        }
        let offset = data.len() * 4;
        for clip in emotes.clips.values_mut() { clip.byte_offset += offset; }
        data.push(storage.frames(&Path::new("assets").join(&emotes.data_file),
            Path::new("assets/player.json"), emotes.source_ticks_per_ms, emotes.floats_per_frame)?);
        manifest.clips.extend(emotes.clips);
        manifest.actions.extend(emotes.actions);
        let mut reactions: serde_json::Value =
            serde_json::from_slice(&std::fs::read("assets/practice/practice.json")?)?;
        reactions["format_version"] = 3.into();
        let mut reactions: Manifest = serde_json::from_value(reactions)?;
        if reactions.bone_count != manifest.bone_count
            || reactions.floats_per_frame != manifest.floats_per_frame
        {
            return Err("Reaction skeleton does not match player".into());
        }
        if reactions.source_ticks_per_ms.to_bits() != manifest.source_ticks_per_ms.to_bits() {
            return Err("Reaction source clock does not match player".into());
        }
        let offset = data.len() * 4;
        for clip in reactions.clips.values_mut() {
            clip.byte_offset += offset;
        }
        data.push(storage.frames(
            &Path::new("assets").join(&reactions.data_file),
            Path::new("assets/player.json"),
            reactions.source_ticks_per_ms,
            reactions.floats_per_frame,
        )?);
        manifest.clips.extend(reactions.clips);
        manifest.actions.extend(
            reactions
                .actions
                .into_iter()
                .map(|(key, value)| (format!("reaction_{key}"), value)),
        );
        if data.iter().any(|v| !v.is_finite()) {
            return Err("Invalid animation transform value".into());
        }
        for pose in data.parts_slices().flat_map(|part| part.chunks_exact(10)) {
            if Quat::from_slice(&pose[3..7]).length_squared() < 0.5 {
                return Err("Invalid animation quaternion".into());
            }
        }
        for clip in manifest.clips.values() {
            if !data.valid_clip(
                clip.byte_offset,
                clip.frame_count,
                clip.duration_ticks,
                manifest.floats_per_frame,
            ) {
                return Err("Animation frame outside buffer".into());
            }
        }
        for action in manifest.actions.values() {
            if !manifest.clips.contains_key(&action.clip)
                || !action.arguments.4.is_finite()
                || action.arguments.4 <= 0.
            {
                return Err("Invalid animation action".into());
            }
        }
        let meshes = handles
            .into_iter()
            .filter_map(|(name, handle)| {
                skin.meshes
                    .iter()
                    .position(|l| l.name == name)
                    .map(|index| (handle, index))
            })
            .collect();
        Ok(Self {
            manifest,
            skin,
            data,
            meshes,
            current: String::new(),
            cursor_ticks: 0.,
            last_jumps: 0,
            last_respawns: 0,
            last_action_serial: 0,
            last_pose_reset_serial: 0,
            displayed_pose: Vec::new(),
            world_pose: Vec::new(),
            current_looping: true,
            transition: None,
            upper: LayerPlayback::default(),
            last_melee_serial: 0,
            last_hit_serial: 0,
            last_emote_serial: 0,
            run_playback_rate: 1.,
            lower_walk_playback_rate: 1.,
        })
    }

    fn advance_upper(
        &mut self,
        name: &str,
        delta_secs: f64,
        force_reset: bool,
        snap_pose: bool,
        requested_duration_ms: f64,
    ) -> Option<FrameSample> {
        let name = if self.manifest.actions.contains_key(name) {
            name
        } else {
            "idle"
        };
        let action = self.manifest.actions.get(name)?;
        let clip = self.manifest.clips.get(&action.clip)?;
        let args = action.upper_arguments.as_ref().unwrap_or(&action.arguments);
        if snap_pose {
            self.upper.transition = None;
        } else if (self.upper.current != name || force_reset) && !self.displayed_pose.is_empty() {
            let source = if self.upper.transition.is_some() {
                TransitionSource::Snapshot(self.displayed_pose.clone())
            } else if let Some(previous) = self.manifest.actions.get(&self.upper.current) {
                TransitionSource::Playback {
                    clip: previous.clip.clone(),
                    cursor_ticks: self.upper.cursor_ticks,
                    looping: self.upper.looping,
                }
            } else {
                TransitionSource::Snapshot(self.displayed_pose.clone())
            };
            self.upper.transition = (args.1 > 0).then_some(PoseTransition {
                source,
                elapsed_ticks: 0.,
                duration_ticks: args.1 as f64,
                upper_duration_ticks: args.1 as f64,
            });
        }
        if force_reset || (self.upper.current != name && args.5) {
            self.upper.cursor_ticks = 0.;
        }
        self.upper.current = name.into();
        self.upper.looping = args.3;
        let delta_ticks = delta_secs * 1000. * self.manifest.source_ticks_per_ms;
        self.upper.cursor_ticks = advance_cursor(
            self.upper.cursor_ticks,
            delta_ticks
                * if name == "sprint" { 0.5 } else if (name.starts_with("counter_UseWeapon2_walk_") || name.starts_with("dagger_UseWeapon1_walk_")) && !name.ends_with("idle_lower") { self.lower_walk_playback_rate } else if name.starts_with("run_") || name.contains("_run_") { self.run_playback_rate } else { 1. }
                * playback_rate(
                    clip.duration_ticks,
                    self.manifest.source_ticks_per_ms,
                    args.0,
                    requested_duration_ms,
                    args.4,
                ),
            clip.duration_ticks,
            args.3,
        );
        let sample = self.frame_sample(clip, self.upper.cursor_ticks);
        if let Some(transition) = &mut self.upper.transition {
            transition.elapsed_ticks += delta_ticks;
            if let TransitionSource::Playback {
                clip,
                cursor_ticks,
                looping,
            } = &mut transition.source
            {
                *cursor_ticks = advance_cursor(
                    *cursor_ticks,
                    delta_ticks,
                    self.manifest.clips[clip].duration_ticks,
                    *looping,
                );
            }
            if transition.complete() {
                self.upper.transition = None;
            }
        }
        Some(sample)
    }

    fn advance(
        &mut self,
        name: &str,
        delta_secs: f64,
        force_reset: bool,
        snap_pose: bool,
        requested_duration_ms: f64,
    ) -> Option<FrameSample> {
        let name = if self.manifest.actions.contains_key(name) {
            name
        } else {
            "idle"
        };
        let action = self
            .manifest
            .actions
            .get(name)
            .or_else(|| self.manifest.actions.get("idle"))?;
        let clip = self.manifest.clips.get(&action.clip)?;
        if snap_pose {
            self.displayed_pose.clear();
            self.transition = None;
        } else if (self.current != name || force_reset) && !self.displayed_pose.is_empty() {
            let source = if self.transition.is_some() {
                TransitionSource::Snapshot(self.displayed_pose.clone())
            } else {
                TransitionSource::Playback {
                    clip: self.manifest.actions.get(&self.current)?.clip.clone(),
                    cursor_ticks: self.cursor_ticks,
                    looping: self.current_looping,
                }
            };
            let upper_duration = action
                .upper_arguments
                .as_ref()
                .map_or(action.arguments.1, |args| args.1);
            self.transition =
                (action.arguments.1 > 0 || upper_duration > 0).then(|| PoseTransition {
                    source,
                    elapsed_ticks: 0.,
                    duration_ticks: action.arguments.1 as f64,
                    upper_duration_ticks: upper_duration as f64,
                });
        }
        if force_reset || (self.current != name && action.arguments.5) {
            self.cursor_ticks = 0.;
        }
        self.current = name.into();
        self.current_looping = action.arguments.3;
        let rate = (if name == "sprint" { 0.5 } else if (name.starts_with("counter_UseWeapon2_walk_") || name.starts_with("dagger_UseWeapon1_walk_")) && !name.ends_with("idle_lower") { self.lower_walk_playback_rate } else if name.starts_with("run_") || name.contains("_run_") { self.run_playback_rate } else { 1. }) * playback_rate(
            clip.duration_ticks,
            self.manifest.source_ticks_per_ms,
            action.arguments.0,
            requested_duration_ms,
            action.arguments.4,
        );
        self.cursor_ticks = advance_cursor(
            self.cursor_ticks,
            delta_secs * 1000. * self.manifest.source_ticks_per_ms * rate,
            clip.duration_ticks,
            action.arguments.3,
        );
        let sample = self.frame_sample(clip, self.cursor_ticks);
        if let Some(transition) = &mut self.transition {
            let delta_ticks = delta_secs * 1000. * self.manifest.source_ticks_per_ms;
            transition.elapsed_ticks += delta_ticks;
            if let TransitionSource::Playback {
                clip,
                cursor_ticks,
                looping,
            } = &mut transition.source
            {
                *cursor_ticks = advance_cursor(
                    *cursor_ticks,
                    delta_ticks,
                    self.manifest.clips[clip.as_str()].duration_ticks,
                    *looping,
                );
            }
            if transition.complete() {
                self.transition = None;
            }
        }
        Some(sample)
    }

    fn frame_sample(&self, clip: &Clip, cursor_ticks: f64) -> FrameSample {
        let sample = cursor_ticks / clip.duration_ticks * (clip.frame_count - 1) as f64;
        crate::animation_core::frame_sample(
            clip.byte_offset,
            clip.frame_count,
            self.manifest.floats_per_frame,
            sample,
        )
    }

    fn attack_sample(
        &mut self,
        name: &str,
        elapsed: f64,
        duration_ms: f64,
        upper: bool,
    ) -> Option<FrameSample> {
        let action = self.manifest.actions.get(name)?;
        let clip = self.manifest.clips.get(&action.clip)?;
        let args = if upper {
            action.upper_arguments.as_ref().unwrap_or(&action.arguments)
        } else {
            &action.arguments
        };
        let ticks = advance_cursor(
            0.,
            elapsed
                * 1000.
                * self.manifest.source_ticks_per_ms
                * playback_rate(
                    clip.duration_ticks,
                    self.manifest.source_ticks_per_ms,
                    args.0,
                    duration_ms,
                    args.4,
                ),
            clip.duration_ticks,
            args.3,
        );
        if upper {
            self.upper.cursor_ticks = ticks;
        } else {
            self.cursor_ticks = ticks;
        }
        Some(self.frame_sample(clip, ticks))
    }

    fn sample_pose(&self, sample: FrameSample) -> Vec<LocalPose> {
        self.data.sample(sample, self.manifest.bone_count)
    }

    fn render_pose(&mut self, sample: FrameSample) {
        let mut pose = self.sample_pose(sample);
        if let Some(transition) = &self.transition {
            let source = match &transition.source {
                TransitionSource::Playback {
                    clip, cursor_ticks, ..
                } => self.sample_pose(self.frame_sample(&self.manifest.clips[clip], *cursor_ticks)),
                TransitionSource::Snapshot(pose) => pose.clone(),
            };
            for ((target, source), bone) in pose.iter_mut().zip(source).zip(&self.skin.bones) {
                *target = source.blend(*target, transition.weight(bone.is_lower()));
            }
        }
        self.displayed_pose = pose;
        self.rebuild_world_pose();
    }

    fn render_layered_pose(&mut self, lower: FrameSample, upper: FrameSample) {
        self.render_pose(lower);
        let mut upper_pose = self.sample_pose(upper);
        if let Some(transition) = &self.upper.transition {
            let source = match &transition.source {
                TransitionSource::Playback {
                    clip, cursor_ticks, ..
                } => self.sample_pose(self.frame_sample(&self.manifest.clips[clip], *cursor_ticks)),
                TransitionSource::Snapshot(pose) => pose.clone(),
            };
            for (target, source) in upper_pose.iter_mut().zip(source) {
                *target = source.blend(*target, transition.weight(false));
            }
        }
        for ((target, upper), bone) in self
            .displayed_pose
            .iter_mut()
            .zip(upper_pose)
            .zip(&self.skin.bones)
        {
            if !bone.is_lower() {
                *target = upper;
            }
        }
        self.rebuild_world_pose();
    }

    fn rebuild_world_pose(&mut self) {
        crate::animation_core::compose_world(
            &self.displayed_pose,
            self.skin.bones.iter().map(|bone| bone.parent),
            &mut self.world_pose,
        );
    }

    fn skin_mesh(&self, index: usize) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        crate::animation_core::skinning::skin_mesh(&self.skin.meshes[index], &self.world_pose)
    }
}

fn select_action(player: &Player, motion: &Motion) -> &'static str {
    if player.health <= 0. { return "reaction_death"; }
    if let Some(reaction)=player.hit_reaction {
        return match reaction.kind {
            crate::gameplay::HitReactionKind::Stun => if reaction.age<=0.7 {"reaction_stun_start"} else {"reaction_stun_loop"},
            crate::gameplay::HitReactionKind::Blow => match reaction.phase {
                1=>"reaction_blow_start",
                2=>"reaction_blow_fall",
                _=>"reaction_get_up",
            },
            crate::gameplay::HitReactionKind::StandUp => "reaction_stand_up",
            crate::gameplay::HitReactionKind::Down => "reaction_down",
            crate::gameplay::HitReactionKind::DownDamage => "reaction_down_hit",
            crate::gameplay::HitReactionKind::Damage(direction) => if player.melee.equipped == crate::melee::Weapon::CounterSword {
                ["counter_damage_0","counter_damage_1","counter_damage_2","counter_damage_3"][direction.min(3) as usize]
            } else {["reaction_flinch_0","reaction_flinch_1","reaction_flinch_2","reaction_flinch_3"][direction.min(3) as usize]},
        };
    }
    let moving = player.movement.length_squared() > 0.01;
    let x = player.movement.x;
    let y = player.movement.y;
    if player.action == MovementAction::DodgeLeft {
        "dodge_left"
    } else if player.action == MovementAction::DodgeRight {
        "dodge_right"
    } else if player.action == MovementAction::WallJump {
        match player.walljump_phase {
            WallJumpPhase::Windup => "wall_jump_start",
            WallJumpPhase::LaunchA => "wall_jump_rise",
            WallJumpPhase::LaunchB => "wall_jump_rise_b",
            WallJumpPhase::AirA | WallJumpPhase::AirB | WallJumpPhase::Failed => "wall_jump_fall",
            WallJumpPhase::Landing => "wall_jump_land",
            WallJumpPhase::None => "jump_fall",
        }
    } else if player.action == MovementAction::Sprint {
        "sprint"
    } else if !player.grounded {
        if !player.normal_jump_active() && player.vertical_speed < 0. {
            "jump_fall"
        } else if player.jump_started_moving {
            "jump_moving"
        } else {
            "jump_rise"
        }
    } else if player.landing_age
        < if player.walljump_landing {
            motion.walljump_landing_time
        } else if player.fall_landing {
            motion.fall_landing_time
        } else {
            motion.land_delay
        }
    {
        if player.walljump_landing {
            "wall_jump_land"
        } else if player.fall_landing {
            "fall_land"
        } else if player.jump_started_moving {
            "jump_land_moving"
        } else {
            "jump_land"
        }
    } else if !moving {
        "idle"
    } else if y > 0.3 {
        if x < -0.3 {
            "run_left_forward"
        } else if x > 0.3 {
            "run_right_forward"
        } else {
            "run_forward"
        }
    } else if y < -0.3 {
        if x < -0.3 {
            "run_left_backward"
        } else if x > 0.3 {
            "run_right_backward"
        } else {
            "run_backward"
        }
    } else if x < 0. {
        "run_left"
    } else {
        "run_right"
    }
}

fn requested_duration_ms(name: &str, motion: &Motion) -> f64 {
    match name {
        "reaction_stun_start" => 700.,
        "reaction_blow_fall" => 200.,
        "reaction_get_up" => 400.,
        "reaction_down" => 1000.,
        "reaction_down_hit" => 500.,
        "dodge_left" | "dodge_right" => (motion.dodge_time + motion.dodge_delay) as f64 * 1000.,
        "fall_land" => motion.fall_landing_time as f64 * 1000.,
        "wall_jump_land" => motion.walljump_landing_time as f64 * 1000.,
        _ => 0.,
    }
}

fn melee_actions(player: &Player, motion: &Motion) -> (String, String, bool) {
    use crate::melee::{Attack, AttackPhase};
    let movement = select_action(player, motion);
    if player.hit_reaction.is_some() { return (movement.into(), movement.into(), true); }
    let Some(prefix) = crate::weapons::weapon_key(player.melee.equipped) else {
        return (movement.into(), movement.into(), false);
    };
    if let Some(attack) = player.melee.attack {
        let delay = player.melee.phase == AttackPhase::Delay;
        let (upper, walk_set) = match attack {
            Attack::CounterDash => (
                if delay { "counter_dash_recovery" }
                else if player.melee.counter_stage == 1 { "counter_dash_active" }
                else { "counter_dash" }.into(),
                None,
            ),
            Attack::CounterCombo(index) => (
                format!("counter_combo_{}{}", index as usize + 1, if delay { "_recovery" } else { "" }),
                None,
            ),
            Attack::CounterJump => (
                if delay { "counter_jump_recovery" } else { "counter_jump" }.into(),
                None,
            ),
            Attack::CounterGuard => (
                if delay { "counter_guard_recovery" } else { "counter_guard" }.into(),
                (!delay || player.guard_recovery_lower_override).then_some("counter_UseWeapon2"),
            ),
            Attack::CounterRetaliation => (
                if delay { "counter_retaliation_recovery" } else { "counter_retaliation" }.into(),
                None,
            ),
            Attack::PlasmaWeak(index) => (
                format!("plasma_weak_{}", index as usize + 1),
                Some("plasma_SwordAtkWeak"),
            ),
            Attack::PlasmaStrong => ("plasma_strong_1".into(), Some("plasma_SwordAtkStrong")),
            Attack::PlasmaDashStrong => ("plasma_charged".into(), None),
            Attack::PlasmaDash => (
                if delay {
                    "plasma_dash_recovery"
                } else {
                    "plasma_dash"
                }
                .into(),
                None,
            ),
            Attack::PlasmaJump => (
                if delay {
                    "plasma_jump_recovery"
                } else {
                    "plasma_jump"
                }
                .into(),
                None,
            ),
            Attack::DaggerSlash(index) => (
                format!("dagger_cut_{}", index as usize + 1),
                Some("dagger_UseWeapon1"),
            ),
            Attack::DaggerDash => (
                if delay {
                    "dagger_poke_recovery"
                } else {
                    "dagger_poke_1"
                }
                .into(),
                None,
            ),
            Attack::DaggerAir => (
                format!(
                    "dagger_cut_{}",
                    player.melee.dagger_slash_index() as usize + 1
                ),
                Some("dagger_UseWeapon1"),
            ),
        };
        if let Some(set) = walk_set.filter(|_| !player.melee.full_body) {
            if attack == Attack::CounterGuard {
                if let Some(jump)=player.guard_jump {
                    return (jump.action().into(),upper,false);
                }
                let direction=["idle","forward","right_forward","right","right_backward","backward","left_backward","left","left_forward"][player.guard_walk_mode() as usize];
                return (format!("{set}_walk_{direction}_lower"),upper,false);
            }
            if attack.is_dagger_cut() {
                if let Some(jump)=player.dagger_jump {
                    return (jump.action().into(), upper, false);
                }
                let direction=["idle","forward","right_forward","right","right_backward","backward","left_backward","left","left_forward"][player.guard_walk_mode() as usize];
                return (format!("{set}_walk_{direction}_lower"), upper, false);
            }
            if !player.grounded {
                return (movement.into(), upper, false);
            }
            let direction = movement.strip_prefix("run_").unwrap_or("idle");
            (format!("{set}_walk_{direction}_lower"), upper, false)
        } else {
            (upper.clone(), upper, true)
        }
    } else if movement == "idle" {
        let idle = format!("{prefix}_idle_upper");
        (idle.clone(), idle, false)
    } else if movement.starts_with("run_") {
        (
            format!("{prefix}_{movement}_lower"),
            format!("{prefix}_{movement}_upper"),
            false,
        )
    } else {
        (movement.into(), movement.into(), false)
    }
}

pub fn animate(
    animation: Option<ResMut<Animation>>,
    player: Res<Player>,
    motion: Res<Motion>,
    time: Res<Time>,
    emotes: Res<crate::content::emotes::EmoteCatalog>,
    mut meshes: ResMut<Assets<Mesh>>,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("player_animation"));
    let Some(mut animation) = animation else {
        return;
    };
    let movement_name = select_action(&player, &motion);
    animation.run_playback_rate =
        (motion.base_speed * player.melee.equipped.move_speed_rate() / 9.15) as f64;
    animation.lower_walk_playback_rate=
        (motion.base_speed * player.melee.equipped.attack_speed_rate()/9.15) as f64;
    let (mut lower_name, mut upper_name, full_body_attack) = melee_actions(&player, &motion);
    if let Some(emote) = player.emote {
        lower_name = emotes.entries[emote.index].action.clone();
        upper_name = lower_name.clone();
    }
    let respawn = animation.last_respawns != player.respawns
        || animation.last_pose_reset_serial != player.pose_reset_serial;
    let hit_reset=animation.last_hit_serial!=player.hit_serial;
    animation.last_hit_serial=player.hit_serial;
    let emote_reset = animation.last_emote_serial != player.emote_serial;
    animation.last_emote_serial = player.emote_serial;
    let force_reset = emote_reset || hit_reset || animation.last_jumps != player.jumps
        || respawn
        || (animation.last_action_serial != player.action_serial
            && player.action != MovementAction::None);
    animation.last_jumps = player.jumps;
    animation.last_respawns = player.respawns;
    animation.last_action_serial = player.action_serial;
    animation.last_pose_reset_serial = player.pose_reset_serial;
    let melee_reset = animation.last_melee_serial != player.melee.serial;
    animation.last_melee_serial = player.melee.serial;
    let retain_dash_strong = player.melee.attack == Some(crate::melee::Attack::PlasmaDashStrong)
        && player.melee.phase == crate::melee::AttackPhase::Delay;
    let retain_dagger_cut=player.melee.attack.is_some_and(crate::melee::Attack::is_dagger_cut)
        && player.melee.phase==crate::melee::AttackPhase::Delay;
    let attack_duration = if player.emote.is_some() { 0. } else if retain_dagger_cut { 550. } else if retain_dash_strong {
        1000.0
    } else if player.melee.attack.is_some() {
        player.melee.animation_duration_ms()
    } else {
        requested_duration_ms(movement_name, &motion)
    };
    let mut upper_sample = animation.advance_upper(
        &upper_name,
        time.delta_secs_f64(),
        force_reset || (melee_reset && player.melee.attack.is_some() && !retain_dash_strong && !retain_dagger_cut),
        respawn,
        attack_duration,
    );
    let Some(mut sample) = animation.advance(
        &lower_name,
        time.delta_secs_f64(),
        force_reset || (melee_reset && full_body_attack && !retain_dash_strong)
            || (melee_reset && player.melee.attack.is_some_and(crate::melee::Attack::is_dagger_cut)
                && !retain_dagger_cut && player.dagger_jump.is_some()),
        respawn,
        if full_body_attack {
            attack_duration
        } else if let Some(jump)=player.melee_lower_jump() {
            jump.duration_ms()
        } else {
            requested_duration_ms(movement_name, &motion)
        },
    ) else {
        return;
    };
    if player.melee.attack.is_some() {
        let elapsed = if retain_dagger_cut { 0.55 } else if retain_dash_strong {
            1.0
        } else if player.melee.attack==Some(crate::melee::Attack::CounterGuard)
            && player.melee.phase==crate::melee::AttackPhase::Normal {
            player.melee.elapsed as f64
        } else {
            player.melee.animation_phase_elapsed() as f64
        };
        upper_sample = animation.attack_sample(&upper_name, elapsed, attack_duration, true);
        if full_body_attack {
            if let Some(attack) =
                animation.attack_sample(&lower_name, elapsed, attack_duration, false)
            {
                sample = attack;
            }
        }
    }
    if let Some(emote) = player.emote {
        upper_sample = animation.attack_sample(&upper_name, emote.age as f64, 0., true);
        if let Some(pose) = animation.attack_sample(&lower_name, emote.age as f64, 0., false) {
            sample = pose;
        }
    }
    if let Some(upper) = upper_sample {
        animation.render_layered_pose(sample, upper);
    } else {
        animation.render_pose(sample);
    }
    let _skin_span = profile.as_deref().and_then(|p| p.span("player_skinning"));
    for (handle, index) in &animation.meshes {
        let (positions, normals) = animation.skin_mesh(*index);
        if let Some(mesh) = meshes.get_mut(handle) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::content::simulation::parity_content;
    use super::*;

    #[test]
    fn counter_guard_lower_jump_uses_native_layers_and_resets_natural_fall() {
        let mut animation=fixture();let motion=Motion::default();
        let mut player=Player::new(Vec3::ZERO,&motion);
        player.melee.equipped=crate::melee::Weapon::CounterSword;
        player.melee.attack=Some(crate::melee::Attack::CounterGuard);
        player.melee.full_body=false;
        for (phase,age,name,blend,looping) in [
            (1,0.1,"lower_jump_rise",200,false),
            (2,0.4,"lower_jump_fall",1000,true),
            (2,0.701,"lower_jump_fall_natural",1000,true),
            (3,0.1,"lower_jump_land",200,false),
        ] {
            player.guard_jump=Some(crate::gameplay::LowerJump {phase,age});
            let (lower,upper,full)=melee_actions(&player,&motion);
            assert_eq!(lower,name);assert_eq!(upper,"counter_guard");assert!(!full);
            assert_eq!(animation.manifest.actions[name].arguments,(0.,blend,0,looping,1.,true));
        }
        animation.advance("lower_jump_fall",0.2,true,true,700.).unwrap();
        assert!(animation.cursor_ticks>0.);
        animation.advance("lower_jump_fall_natural",0.,false,false,0.).unwrap();
        assert_eq!(animation.cursor_ticks,0.,"700ms assignment restarts fall rather than retaining the apex cursor");
    }
    #[test]
    fn counter_guard_walking_uses_attack_speed_clock_and_recovery_is_five_hundred_ms() {
        let mut animation=fixture();
        animation.run_playback_rate=11.*1.1/9.15;
        animation.lower_walk_playback_rate=11./9.15;
        animation.advance("counter_UseWeapon2_walk_forward_lower",0.1,true,true,0.).unwrap();
        let ticks_per_ms=animation.manifest.source_ticks_per_ms;
        assert!((animation.cursor_ticks-100.*ticks_per_ms*(11./9.15)).abs()<0.001);
        animation.advance_upper("counter_guard",0.1,true,true,0.).unwrap();
        assert!((animation.upper.cursor_ticks-100.*ticks_per_ms).abs()<0.001);
        animation.advance("counter_guard_recovery",0.502,true,true,500.).unwrap();
        let duration=animation.manifest.clips["R0032"].duration_ticks;
        assert!((animation.cursor_ticks-duration).abs()<0.001);
    }

    #[test]
    fn local_sprint_plays_at_native_half_rate_on_both_layers() {
        let mut animation = fixture();
        animation.run_playback_rate = 1.443;
        animation.advance("sprint", 0.2, true, true, 0.).unwrap();
        animation.advance_upper("sprint", 0.2, true, true, 0.).unwrap();
        assert!((animation.cursor_ticks - 480.).abs() < 0.001);
        assert!((animation.upper.cursor_ticks - 480.).abs() < 0.001);
    }

    #[test]
    fn airborne_dagger_enters_shared_lower_jump_and_changes_at_apex() {
        use crate::{controller::CollisionWorld, gameplay::MovementInput, melee::{Attack, MeleeInput, Weapon}};
        let motion=Motion::default();
        let world=CollisionWorld::default();
        let mut player=Player::new(Vec3::Y*100., &motion);
        player.melee.equipped=Weapon::Dagger;
        player.grounded=true;
        player.step(parity_content(), &world, &motion, MovementInput {jump:true, movement:Vec2::Y, ..default()}, 0.016);
        assert_eq!(select_action(&player,&motion),"jump_moving");
        player.step(parity_content(), &world, &motion, MovementInput {movement:Vec2::Y,
            melee:MeleeInput {primary_pressed:true,primary_held:true,primary_down_sample:true,..default()}, ..default()},0.016);
        assert!(player.melee.attack.is_none(),"new-down5 stops before held4");
        player.step(parity_content(), &world,&motion,MovementInput {movement:Vec2::Y,
            melee:MeleeInput {primary_held:true,..default()},..default()},0.016);
        assert_eq!(player.melee.attack,Some(Attack::DaggerAir));
        let (lower,upper,full)=melee_actions(&player,&motion);
        assert_eq!(lower,"lower_jump_rise");assert_eq!(upper,"dagger_cut_1");assert!(!full);
        let mut animation=fixture();
        animation.advance("jump_moving",0.1,true,true,0.).unwrap();
        animation.advance(&lower,0.,true,false,700.).unwrap();
        assert_eq!(animation.cursor_ticks,0.,"lower Jump Enter assigns a fresh cursor");
        assert_eq!(animation.manifest.actions[&lower].clip,"jump_rise");
        for _ in 0..22 {player.step(parity_content(), &world,&motion,MovementInput::default(),0.016);}
        assert_eq!(melee_actions(&player,&motion).0,"lower_jump_fall");
        assert_eq!(player.melee.attack,Some(Attack::DaggerAir));
    }

    #[test]
    fn equipment_run_rate_does_not_change_dagger_attack_idle_or_jump() {
        for action in ["dagger_poke_1", "dagger_poke_recovery", "dagger_cut_1", "dagger_idle_upper", "jump_rise"] {
            let mut ordinary = fixture();
            let mut equipped = fixture();
            equipped.run_playback_rate = 1.443;
            let duration = match action { "dagger_poke_1" => 1000., "dagger_poke_recovery" => 1500., "dagger_cut_1" => 550., _ => 0. };
            ordinary.advance(action, 0.1, true, true, duration).unwrap();
            equipped.advance(action, 0.1, true, true, duration).unwrap();
            ordinary.advance_upper(action, 0.1, true, true, duration).unwrap();
            equipped.advance_upper(action, 0.1, true, true, duration).unwrap();
            assert_eq!(ordinary.cursor_ticks, equipped.cursor_ticks, "{action}");
            assert_eq!(ordinary.upper.cursor_ticks, equipped.upper.cursor_ticks, "{action}");
        }
    }

    #[test]
    fn equipped_run_scales_both_cursors_without_speeding_up_blends() {
        let mut a = fixture();
        a.run_playback_rate = (11. * 1.2 / 9.15) as f64;
        a.advance("dagger_idle_upper", 0., true, true, 0.).unwrap();
        let pose = a.advance("dagger_idle_upper", 0., false, false, 0.).unwrap();
        a.render_pose(pose);
        a.advance("dagger_run_forward_lower", 0.1, false, false, 0.).unwrap();
        a.advance_upper("dagger_run_forward_upper", 0.1, false, false, 0.).unwrap();
        let expected = 480.000019 * a.run_playback_rate;
        assert!((a.cursor_ticks - expected).abs() < 0.01);
        assert!((a.upper.cursor_ticks - expected).abs() < 0.01);
        assert!((a.transition.as_ref().unwrap().elapsed_ticks - 480.000019).abs() < 0.01);
    }

    #[test]
    fn jump_pose_keeps_takeoff_variant_while_air_steering_changes() {
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.vertical_speed = 12.;
        for moving_at_takeoff in [false, true] {
            player.jump_started_moving = moving_at_takeoff;
            for direction in [Vec2::ZERO, Vec2::Y, -Vec2::Y, Vec2::X, -Vec2::X] {
                player.movement = direction;
                player.grounded = false;
                assert_eq!(select_action(&player, &motion), if moving_at_takeoff { "jump_moving" } else { "jump_rise" });
                player.grounded = true;
                player.landing_age = 0.;
                assert_eq!(select_action(&player, &motion), if moving_at_takeoff { "jump_land_moving" } else { "jump_land" });
            }
        }
    }

    #[test]
    fn normal_jump_rotation_survives_apex_and_reaches_clip_end_before_landing() {
        use crate::{controller::CollisionWorld, gameplay::MovementInput};
        let world = CollisionWorld::new(vec![
            [Vec3::new(-100., 0., -100.), Vec3::new(100., 0., -100.), Vec3::new(100., 0., 100.)],
            [Vec3::new(-100., 0., -100.), Vec3::new(100., 0., 100.), Vec3::new(-100., 0., 100.)],
        ]);
        let motion = Motion::default();
        for moving in [false, true] {
            let mut player = Player::new(Vec3::Y * 0.001, &motion);
            player.step(parity_content(), &world, &motion, MovementInput::default(), 0.016);
            player.landing_age = 10.;
            let mut animation = fixture();
            let name = if moving { "jump_moving" } else { "jump_rise" };
            let mut descending_frames = 0;
            let mut last_cursor = 0.;
            for tick in 0..50 {
                player.step(parity_content(), &world, &motion, MovementInput {
                    movement: if moving { Vec2::Y } else { Vec2::ZERO },
                    jump: tick == 0,
                    ..default()
                }, 0.016);
                if player.grounded {
                    assert_eq!(select_action(&player, &motion), if moving { "jump_land_moving" } else { "jump_land" });
                    break;
                }
                assert_eq!(select_action(&player, &motion), name);
                animation.advance(name, 0.016, tick == 0, tick == 0, 0.).unwrap();
                assert!(animation.cursor_ticks >= last_cursor);
                last_cursor = animation.cursor_ticks;
                if player.vertical_speed < 0. { descending_frames += 1; }
            }
            assert!(player.grounded);
            assert!(descending_frames > 15);
            let clip = &animation.manifest.clips[&animation.manifest.actions[name].clip];
            assert!(last_cursor / clip.duration_ticks as f64 > 0.95, "{name}: {last_cursor}");
        }
    }

    #[test]
    fn lateral_dodge_uses_source_600ms_duration_without_equipment_rate() {
        let motion = Motion::default();
        for name in ["dodge_left", "dodge_right"] {
            let mut animation = fixture();
            animation.run_playback_rate = 1.443;
            let duration = requested_duration_ms(name, &motion);
            assert!((duration - 600.).abs() < 0.001);
            animation.advance(name, 0.3, true, true, duration).unwrap();
            animation.advance_upper(name, 0.3, true, true, duration).unwrap();
            let clip_duration = animation.manifest.clips[&animation.manifest.actions[name].clip].duration_ticks;
            assert!((animation.cursor_ticks / clip_duration - 0.5).abs() < 0.002);
            assert!((animation.upper.cursor_ticks / clip_duration - 0.5).abs() < 0.002);
            animation.advance(name, 0.3, false, false, duration).unwrap();
            assert!(animation.cursor_ticks / clip_duration > 0.998);
        }
    }

    fn fixture() -> Animation {
        Animation::load(Vec::new()).unwrap()
    }

    #[test]
    fn native_sprint_keeps_its_source_clip_during_ascent_fall_and_landing() {
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.action = MovementAction::Sprint;
        player.movement = Vec2::Y;
        let mut animation = fixture();
        let exported: serde_json::Value =
            serde_json::from_slice(&std::fs::read("assets/player-animations.json").unwrap())
                .unwrap();
        assert_eq!(exported["clips"]["sprint"]["source_clip"], "00068");
        for (grounded, vertical_speed, landing_age) in [
            (false, 10., 10.),
            (false, -10., 10.),
            (true, 0., 0.),
            (true, 0., 10.),
        ] {
            player.grounded = grounded;
            player.vertical_speed = vertical_speed;
            player.landing_age = landing_age;
            let name = select_action(&player, &motion);
            assert_eq!(name, "sprint");
            assert_eq!(animation.manifest.actions[name].clip, "sprint");
            animation.advance(name, 1. / 60., false, false, 0.).unwrap();
            assert_eq!(animation.current, "sprint");
        }
        player.action = MovementAction::None;
        player.grounded = false;
        player.vertical_speed = -10.;
        assert_eq!(select_action(&player, &motion), "jump_fall");
    }

    #[test]
    fn wall_jump_pose_tracks_native_phase_including_glancing_variant_and_post_apex_launch() {
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.action = MovementAction::WallJump;
        let exported: serde_json::Value =
            serde_json::from_slice(&std::fs::read("assets/player-animations.json").unwrap())
                .unwrap();
        for (phase, name, source) in [
            (WallJumpPhase::Windup, "wall_jump_start", "00066"),
            (WallJumpPhase::LaunchA, "wall_jump_rise", "00067"),
            (WallJumpPhase::LaunchB, "wall_jump_rise_b", "00067_2"),
            (WallJumpPhase::AirA, "wall_jump_fall", "00002"),
            (WallJumpPhase::AirB, "wall_jump_fall", "00002"),
            (WallJumpPhase::Landing, "wall_jump_land", "00003"),
            (WallJumpPhase::Failed, "wall_jump_fall", "00002"),
        ] {
            player.walljump_phase = phase;
            for vertical_speed in [-8., 8.] {
                player.vertical_speed = vertical_speed;
                assert_eq!(select_action(&player, &motion), name);
            }
            let clip = exported["actions"][name]["clip"].as_str().unwrap();
            assert_eq!(exported["clips"][clip]["source_clip"], source);
        }
    }

    #[test]
    fn wall_jump_non_landing_clips_keep_native_lua_rates_instead_of_fitting_state_timers() {
        let motion = Motion::default();
        let mut animation = fixture();
        for (name, rate) in [
            ("wall_jump_start", 1.),
            ("wall_jump_rise", 1.),
            ("wall_jump_rise_b", 0.8),
            ("wall_jump_fall", 1.),
        ] {
            let duration = requested_duration_ms(name, &motion);
            assert_eq!(duration, 0.);
            animation.advance(name, 0.1, true, true, duration).unwrap();
            assert!((animation.cursor_ticks - 480. * rate).abs() < 0.001);
        }
        let duration = requested_duration_ms("wall_jump_land", &motion);
        assert!((duration - 200.).abs() < 0.001);
        animation
            .advance("wall_jump_land", 0.1, true, true, duration)
            .unwrap();
        assert!((animation.cursor_ticks - 919.2).abs() < 0.001);
    }

    #[test]
    fn ledge_fall_landing_uses_its_own_clip_parameters_and_125ms_timer() {
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.grounded = true;
        player.fall_landing = true;
        player.landing_age = 0.;
        player.movement = Vec2::Y;
        let mut animation = fixture();
        let name = select_action(&player, &motion);
        assert_eq!(name, "fall_land");
        let action = &animation.manifest.actions[name];
        assert_eq!(action.clip, "jump_land");
        assert_eq!(action.arguments, (380., 200, 300, false, 1., true));
        animation
            .advance(name, 0.05, true, true, requested_duration_ms(name, &motion))
            .unwrap();
        assert!((animation.cursor_ticks - 729.6).abs() < 0.001);
        player.landing_age = motion.fall_landing_time;
        assert_eq!(select_action(&player, &motion), "run_forward");
    }

    #[test]
    fn equipped_idle_uses_its_authored_root_and_pelvis_on_both_layers() {
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.grounded = true;
        for (weapon, clip) in [
            (crate::melee::Weapon::PlasmaSword, "A0013"),
            (crate::melee::Weapon::Dagger, "SN002"),
        ] {
            player.melee.equipped = weapon;
            let (lower, upper, _) = melee_actions(&player, &motion);
            let mut animation = fixture();
            assert_eq!(animation.manifest.actions[&lower].clip, clip);
            assert_eq!(lower, upper);
            let lower_sample = animation.advance(&lower, 0., true, true, 0.).unwrap();
            let upper_sample = animation.advance_upper(&upper, 0., true, true, 0.).unwrap();
            animation.render_layered_pose(lower_sample, upper_sample);
            let layered = animation.world_pose.clone();
            animation.render_pose(lower_sample);
            for (actual, expected) in layered.iter().zip(&animation.world_pose) {
                assert!(actual.abs_diff_eq(*expected, 1e-5));
            }
        }
    }

    #[test]
    fn original_source_clock_yields_natural_run_and_idle_periods() {
        let a = fixture();
        let seconds = |name: &str| {
            a.manifest.clips[name].duration_ticks / a.manifest.source_ticks_per_ms / 1000.
        };
        assert!((seconds("run_forward") - 2. / 3.).abs() < 0.000001);
        assert!((seconds("idle") - 5. / 3.).abs() < 0.000001);
    }

    #[test]
    fn direction_changes_preserve_source_cursor_without_resetting_stride() {
        let mut a = fixture();
        a.advance("run_forward", 0.2, false, false, 0.).unwrap();
        a.advance("run_left", 0.1, false, false, 0.).unwrap();
        assert!((a.cursor_ticks - 1440.).abs() < 0.001);
        a.advance("jump_rise", 0.1, false, false, 0.).unwrap();
        assert!((a.cursor_ticks - 480.).abs() < 0.001);
    }

    #[test]
    fn falling_loops_and_landing_uses_the_original_rate() {
        let mut a = fixture();
        a.advance("jump_fall", 1., false, false, 0.).unwrap();
        assert!((a.cursor_ticks - 1600.).abs() < 0.001);
        a.advance("jump_land", 0.1, false, false, 0.).unwrap();
        assert!((a.cursor_ticks - 384.).abs() < 0.001);
        assert_eq!(advance_cursor(3000., 200., 3200., true), 3200.);
        assert_eq!(advance_cursor(3000., 300., 3200., true), 100.);
        assert_eq!(advance_cursor(3000., 300., 3200., false), 3200.);
    }

    #[test]
    fn duration_override_is_a_rate_numerator_and_never_a_start_offset() {
        let ticks_per_ms = 4.800000190734863;
        assert!((playback_rate(3360., ticks_per_ms, 600., 700., 1.) - 6. / 7.).abs() < 1e-6);
        assert_eq!(playback_rate(3360., ticks_per_ms, 600., 0., 1.), 1.);
        assert_eq!(
            playback_rate(2640., ticks_per_ms, 0., 500., 1.),
            549. / 500.
        );
    }

    fn pose_fixture() -> Animation {
        let mut a = fixture();
        a.manifest.floats_per_frame = 10;
        a.manifest.bone_count = 1;
        Arc::make_mut(&mut a.skin).bones.truncate(1);
        Arc::make_mut(&mut a.skin).meshes.clear();
        a.manifest.clips.clear();
        a.manifest.actions.clear();
        a.data.clear();
        for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
            for _ in 0..2 {
                a.data
                    .extend_from_slice(&[i as f32 * 10., 0., 0., 0., 0., 0., 1., 1., 1., 1.]);
            }
            a.manifest.clips.insert(
                name.into(),
                Clip {
                    duration_ticks: 10000.,
                    frame_count: 2,
                    byte_offset: i * 80,
                },
            );
            a.manifest.actions.insert(
                name.into(),
                Action {
                    clip: name.into(),
                    arguments: (0., 1000, 0, true, 1., false),
                    upper_arguments: None,
                },
            );
        }
        a
    }

    fn display(a: &mut Animation, name: &str, seconds: f64, reset: bool, snap: bool) -> f32 {
        let sample = a.advance(name, seconds, reset, snap, 0.).unwrap();
        a.render_pose(sample);
        a.displayed_pose[0].translation.x
    }

    #[test]
    fn interrupted_direction_transitions_start_from_the_displayed_pose() {
        let mut a = pose_fixture();
        let half_blend = 500. / a.manifest.source_ticks_per_ms / 1000.;
        assert_eq!(display(&mut a, "a", 0., false, false), 0.);
        assert_eq!(display(&mut a, "b", 0., false, false), 0.);
        assert_eq!(display(&mut a, "b", half_blend, false, false), 5.);
        assert_eq!(display(&mut a, "c", 0., false, false), 5.);
        assert_eq!(display(&mut a, "c", half_blend, false, false), 12.5);
        assert_eq!(display(&mut a, "c", half_blend, false, false), 20.);
        assert!(a.transition.is_none());
    }

    #[test]
    fn action_resets_blend_but_respawns_discard_the_old_pose() {
        let mut a = pose_fixture();
        let half_blend = 500. / a.manifest.source_ticks_per_ms / 1000.;
        display(&mut a, "a", 0.1, false, false);
        display(&mut a, "b", half_blend, false, false);
        assert_eq!(display(&mut a, "b", 0., true, false), 5.);
        assert_eq!(a.cursor_ticks, 0.);
        assert_eq!(display(&mut a, "c", 0., true, true), 20.);
        assert!(a.transition.is_none());
    }

    #[test]
    fn lateral_dodge_uses_native_active_plus_recovery_duration() {
        let mut a = fixture();
        assert_eq!(a.manifest.actions["dodge_left"].clip, "dodge_left");
        a.advance("dodge_left", 0.3, true, false, 600.).unwrap();
        assert!((a.cursor_ticks - 3597.60014).abs() < 0.001);
        a.advance("dodge_left", 0.3, false, false, 600.).unwrap();
        assert!(a.cursor_ticks > 7190. && a.cursor_ticks <= 7200.);
    }

    #[test]
    fn outgoing_clip_advances_and_interruption_preserves_local_pose() {
        let mut a = fixture();
        let sample = a.advance("run_left", 0.1, true, true, 0.).unwrap();
        a.render_pose(sample);
        let sample = a.advance("run_right", 0.02, false, false, 0.).unwrap();
        a.render_pose(sample);
        let transition = a.transition.as_ref().unwrap();
        let TransitionSource::Playback { cursor_ticks, .. } = &transition.source else {
            panic!("outgoing playback must keep moving");
        };
        assert!((*cursor_ticks - 576.).abs() < 0.001);
        let displayed = a.displayed_pose.clone();
        let sample = a.advance("run_left", 0., false, false, 0.).unwrap();
        a.render_pose(sample);
        assert!(matches!(
            a.transition.as_ref().unwrap().source,
            TransitionSource::Snapshot(_)
        ));
        for (before, after) in displayed.iter().zip(&a.displayed_pose) {
            assert!(before.translation.distance(after.translation) < 1e-6);
            assert!(before.rotation.dot(after.rotation).abs() > 0.999999);
            assert!(before.scale.distance(after.scale) < 1e-6);
        }
    }

    #[test]
    fn native_per_node_masks_use_distinct_upper_and_lower_run_blends() {
        let mut a = fixture();
        assert_eq!(
            a.skin.bones.iter().filter(|bone| bone.is_lower()).count(),
            13
        );
        let bone = |name: &str| a.skin.bones.iter().find(|bone| bone.name == name).unwrap();
        assert!(bone("Bip01 L Calf").is_lower());
        assert!(bone("Bip01 Footsteps").is_lower());
        assert!(!bone("ber_leg_L").is_lower());
        assert!(!bone("Bip01 Spine").is_lower());
        let sample = a.advance("run_left", 0.1, true, true, 0.).unwrap();
        a.render_pose(sample);
        let lower_duration = 1000. / a.manifest.source_ticks_per_ms / 1000.;
        let sample = a
            .advance("run_right", lower_duration, false, false, 0.)
            .unwrap();
        a.render_pose(sample);
        let transition = a.transition.as_ref().unwrap();
        assert!((transition.weight(true) - 1.).abs() < 1e-6);
        assert!((transition.weight(false) - 0.5).abs() < 1e-6);
        let sample = a
            .advance("run_right", lower_duration, false, false, 0.)
            .unwrap();
        a.render_pose(sample);
        assert!(a.transition.is_none());
    }

    #[test]
    fn independent_melee_upper_track_preserves_lower_cursor_and_pose() {
        let mut a = pose_fixture();
        Arc::make_mut(&mut a.skin).bones[0].name = "Bip01".into();
        Arc::make_mut(&mut a.skin).bones.push(Bone {
            name: "Bip01 Spine".into(),
            parent: Some(0),
        });
        a.manifest.bone_count = 2;
        a.manifest.floats_per_frame = 20;
        a.data.clear();
        for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
            a.manifest.clips.get_mut(name).unwrap().byte_offset = i * 160;
            for _ in 0..4 {
                a.data
                    .extend_from_slice(&[i as f32 * 10., 0., 0., 0., 0., 0., 1., 1., 1., 1.]);
            }
        }
        let upper = a.advance_upper("c", 0.1, true, true, 0.).unwrap();
        let lower = a.advance("a", 0.1, true, true, 0.).unwrap();
        a.render_layered_pose(lower, upper);
        assert_eq!(a.displayed_pose[0].translation.x, 0.);
        assert_eq!(a.displayed_pose[1].translation.x, 20.);
        let cursor = a.cursor_ticks;
        let upper = a.advance_upper("b", 0., true, false, 0.).unwrap();
        let lower = a.advance("a", 0.05, false, false, 0.).unwrap();
        a.render_layered_pose(lower, upper);
        assert!(a.cursor_ticks > cursor);
        assert_eq!(a.upper.cursor_ticks, 0.);
        assert_eq!(a.displayed_pose[0].translation.x, 0.);
        assert_eq!(a.displayed_pose[1].translation.x, 20.);
        assert!(a.bone_matrix("Bip01 Spine").is_some());
    }

    #[test]
    fn every_equipped_posture_and_melee_phase_has_an_imported_source_action() {
        use crate::melee::{Attack, AttackPhase, Weapon};
        let animation = fixture();
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.grounded = true;
        player.landing_age = 10.;
        let directions = [
            Vec2::ZERO,
            Vec2::Y,
            Vec2::NEG_Y,
            Vec2::X,
            Vec2::NEG_X,
            Vec2::new(1., 1.),
            Vec2::new(-1., 1.),
            Vec2::new(1., -1.),
            Vec2::new(-1., -1.),
        ];
        let check = |player: &Player| {
            let (lower, upper, _) = melee_actions(player, &motion);
            assert!(
                animation.manifest.actions.contains_key(&lower),
                "missing source lower action {lower}"
            );
            assert!(
                animation.manifest.actions.contains_key(&upper),
                "missing source upper action {upper}"
            );
        };
        for weapon in [Weapon::PlasmaSword, Weapon::Dagger, Weapon::CounterSword] {
            player.melee.equipped = weapon;
            for direction in directions {
                player.movement = direction;
                player.melee.attack = None;
                check(&player);
                let attacks = match weapon {
                    Weapon::PlasmaSword => vec![
                        Attack::PlasmaWeak(0),
                        Attack::PlasmaWeak(1),
                        Attack::PlasmaWeak(2),
                        Attack::PlasmaStrong,
                        Attack::PlasmaDash,
                        Attack::PlasmaJump,
                    ],
                    Weapon::CounterSword => vec![
                        Attack::CounterDash,
                        Attack::CounterCombo(0), Attack::CounterCombo(1),
                        Attack::CounterCombo(2), Attack::CounterCombo(3),
                        Attack::CounterJump, Attack::CounterGuard, Attack::CounterRetaliation,
                    ],
                    _ => vec![
                        Attack::DaggerSlash(0),
                        Attack::DaggerSlash(1),
                        Attack::DaggerDash,
                        Attack::DaggerAir,
                    ],
                };
                for attack in attacks {
                    player.melee.attack = Some(attack);
                    for phase in [AttackPhase::Normal, AttackPhase::Delay] {
                        player.melee.phase = phase;
                        check(&player);
                    }
                }
            }
        }
    }

    #[derive(Deserialize)]
    struct GeometryReference {
        data_file: String,
        floats_per_frame: usize,
        mesh_layout: Vec<ReferenceLayout>,
        poses: Vec<ReferencePose>,
    }
    #[derive(Deserialize)]
    struct ReferenceLayout {
        name: String,
        vertex_count: usize,
        positions_float_offset: usize,
        normals_float_offset: usize,
    }
    #[derive(Deserialize)]
    struct ReferencePose {
        clip: String,
        ticks: f64,
        byte_offset: usize,
    }

    #[test]
    fn original_sampled_bone_skinning_matches_previous_baked_geometry() {
        let mut a = fixture();
        let reference: GeometryReference = serde_json::from_slice(
            &std::fs::read("assets/player-animation-reference.json").unwrap(),
        )
        .unwrap();
        let bytes =
            std::fs::read(std::path::Path::new("assets").join(&reference.data_file)).unwrap();
        let data: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert_eq!(
            data.len(),
            reference.poses.len() * reference.floats_per_frame
        );
        assert_eq!(reference.poses.len(), 60);
        for pose in &reference.poses {
            let sample = a.frame_sample(&a.manifest.clips[&pose.clip], pose.ticks);
            a.render_pose(sample);
            for layout in &reference.mesh_layout {
                let index = a
                    .skin
                    .meshes
                    .iter()
                    .position(|mesh| mesh.name == layout.name)
                    .unwrap();
                let (positions, normals) = a.skin_mesh(index);
                assert_eq!(positions.len(), layout.vertex_count);
                for (values, offset) in [
                    (&positions, layout.positions_float_offset),
                    (&normals, layout.normals_float_offset),
                ] {
                    let start = pose.byte_offset / 4 + offset;
                    for (actual, expected) in values
                        .iter()
                        .flatten()
                        .zip(&data[start..start + layout.vertex_count * 3])
                    {
                        assert!(
                            (actual - expected).abs() < 0.0001,
                            "{} at {} ticks, {}: {actual} != {expected}",
                            pose.clip,
                            pose.ticks,
                            layout.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rapid_actual_left_right_transitions_preserve_bone_lengths_and_torso_width() {
        let mut a = fixture();
        let torso = a
            .skin
            .meshes
            .iter()
            .position(|m| m.name == "body/00_Male_BodyPart/0")
            .unwrap();
        let sample = a.advance("run_forward", 0.1, true, true, 0.).unwrap();
        a.render_pose(sample);
        let lengths: Vec<f32> = a
            .displayed_pose
            .iter()
            .map(|p| p.translation.length())
            .collect();
        for frame in 0..1440 {
            let period = [1, 2, 4][(frame / 240) % 3];
            let name = if (frame / period) % 2 == 0 {
                "run_left"
            } else {
                "run_right"
            };
            let sample = a.advance(name, 1. / 240., false, false, 0.).unwrap();
            a.render_pose(sample);
            for (i, pose) in a.displayed_pose.iter().enumerate() {
                assert!((pose.rotation.length() - 1.).abs() < 1e-5);
                assert!(pose.scale.distance(Vec3::ONE) < 0.00001);
                let name = &a.skin.bones[i].name;
                if [
                    "Thigh", "Calf", "UpperArm", "Forearm", " Hand", " Neck", " Head",
                ]
                .iter()
                .any(|part| name.contains(*part))
                    && lengths[i] > 0.01
                {
                    assert!(
                        (pose.translation.length() / lengths[i] - 1.).abs() < 0.01,
                        "bone {} shortened at frame {frame}",
                        a.skin.bones[i].name
                    );
                }
            }
            let (positions, normals) = a.skin_mesh(torso);
            assert!(
                positions
                    .iter()
                    .chain(&normals)
                    .flatten()
                    .all(|v| v.is_finite())
            );
            let center = positions
                .iter()
                .map(|p| Vec2::new(p[0], p[2]))
                .sum::<Vec2>()
                / positions.len() as f32;
            let rms = (positions
                .iter()
                .map(|p| Vec2::new(p[0], p[2]).distance_squared(center))
                .sum::<f32>()
                / positions.len() as f32)
                .sqrt();
            assert!(rms > 0.13, "torso collapsed to RMS {rms} at frame {frame}");
        }
    }

    #[test]
    fn counter_reflected_stun_uses_fixed_seven_hundred_ms_startup() {
        let motion=Motion::default(); let mut player=Player::new(Vec3::ZERO,&motion);
        player.receive_counter_stun(3.);
        let mut reaction=player.hit_reaction.unwrap(); reaction.age=0.7; player.hit_reaction=Some(reaction);
        assert_eq!(select_action(&player,&motion),"reaction_stun_start");
        assert_eq!(requested_duration_ms("reaction_stun_start",&motion),700.);
        reaction.age=0.701; player.hit_reaction=Some(reaction);
        assert_eq!(select_action(&player,&motion),"reaction_stun_loop");
    }
    #[test]
    fn native_plasma_hold_spins_both_layers_before_starting_the_walking_lower_track() {
        use crate::melee::{Attack, MeleeContext, MeleeInput, Weapon};
        let animation = fixture();
        let motion = Motion::default();
        let mut player = Player::new(Vec3::ZERO, &motion);
        player.grounded = true;
        player.landing_age = 10.;
        player.movement = Vec2::Y;
        player.melee.equipped = Weapon::PlasmaSword;
        let context = MeleeContext {
            grounded: true,
            ..default()
        };
        player.melee.tick(parity_content(), MeleeInput {
                primary_pressed: true,
                primary_held: true,
                ..default()
            },
            context,
            0.016,
            100.,
        );
        for _ in 0..13 {
            player.melee.tick(parity_content(), MeleeInput {
                    primary_held: true,
                    ..default()
                },
                context,
                0.016,
                100.,
            );
        }
        assert_eq!(player.melee.attack, Some(Attack::PlasmaStrong));
        let (lower, upper, full_body) = melee_actions(&player, &motion);
        assert!(full_body);
        assert_eq!(lower, "plasma_strong_1");
        assert_eq!(lower, upper);
        for _ in 0..32 {
            player.melee.tick(parity_content(), default(), context, 0.016, 100.);
        }
        let (lower, upper, full_body) = melee_actions(&player, &motion);
        assert!(!full_body);
        assert_eq!(lower, "plasma_SwordAtkStrong_walk_forward_lower");
        assert_eq!(upper, "plasma_strong_1");
        assert!(animation.manifest.actions[&lower].arguments.3);
        assert!(!animation.manifest.actions[&upper].arguments.3);
    }

    #[test]
    fn stationary_melee_lower_clips_do_not_generate_walking_sound_cursors() {
        let mut animation = fixture();
        for name in [
            "plasma_SwordAtkWeak_walk_idle_lower",
            "plasma_SwordAtkStrong_walk_idle_lower",
            "dagger_UseWeapon1_walk_idle_lower",
        ] {
            animation.advance(name, 0.2, true, true, 0.).unwrap();
            assert_eq!(animation.footstep_cursor_ms(), None, "{name}");
        }
        animation
            .advance(
                "plasma_SwordAtkWeak_walk_forward_lower",
                0.2,
                true,
                true,
                0.,
            )
            .unwrap();
        assert!(animation.footstep_cursor_ms().is_some());
    }

    #[test]
    fn airborne_dagger_cancels_use_alternating_upper_cut_clips() {
        use crate::{controller::CollisionWorld, gameplay::MovementInput, melee::{Attack, MeleeInput, Weapon}};
        let animation=fixture();let motion=Motion::default();let world=CollisionWorld::default();
        let mut player=Player::new(Vec3::Y*100.,&motion);
        player.melee.equipped=Weapon::Dagger;
        for (name,clip) in [("dagger_cut_1","SN013"),("dagger_cut_2","SN014")] {
            player.step(parity_content(), &world,&motion,MovementInput {melee:MeleeInput {primary_held:true,..default()},..default()},0.016);
            assert_eq!(player.melee.attack,Some(Attack::DaggerAir));
            let (lower,upper,full)=melee_actions(&player,&motion);
            assert!(!full);assert!(lower.starts_with("lower_jump_"));
            assert_eq!(upper,name);assert_eq!(animation.manifest.actions[&upper].clip,clip);
            for _ in 0..40 {player.step(parity_content(), &world,&motion,MovementInput::default(),0.016);}
            assert!(player.melee.attack.is_none());assert!(player.dagger_jump.is_none());
        }
    }
}
