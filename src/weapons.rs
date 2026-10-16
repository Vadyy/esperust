use crate::{
    animation::{self, Animation},
    gameplay::{self, Avatar, Player, View},
    melee::{Attack, AttackPhase, Weapon},
    scene,
};
use crate::{combat_geometry::melee_surface_hit, simulation::SimulationSet};
use bevy::{
    asset::RenderAssetUsages, audio::Volume, camera::visibility::NoFrustumCulling, mesh::Indices,
    prelude::*, render::render_resource::PrimitiveTopology,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

pub struct WeaponsPlugin;
impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponStats>()
            .init_resource::<StrikeEvents>()
            .init_resource::<ActorAttackFrames>()
            .add_systems(crate::loading::PrepareWorld, setup.after(gameplay::setup))
            .add_systems(
                Update,
                (update_practice_equipment.after(crate::practice_animation::animate))
                    .run_if(crate::loading::ready),
            )
            .add_systems(
                Update,
                (cleanup_actor_attack_audio.after(present)).run_if(crate::loading::ready),
            )
            .add_systems(
                Update,
                ((
                    present
                        .after(animation::animate)
                        .after(crate::practice_animation::animate),
                    update_weapon_ui,
                ))
                    .run_if(crate::loading::ready),
            );
        install_simulation(app);
    }
}

fn install_simulation(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        capture_strikes
            .in_set(SimulationSet::CaptureAttacks)
            .run_if(crate::loading::ready),
    )
    .add_systems(
        FixedUpdate,
        capture_actor_actions
            .in_set(SimulationSet::CapturePresentation)
            .run_if(crate::loading::ready),
    );
}

#[derive(Resource, Default, serde::Serialize)]
pub struct WeaponStats {
    pub models_loaded: usize,
    pub visible_models: usize,
    pub attacks_presented: u32,
    pub surface_hits: u32,
    pub sounds_played: u32,
    pub sound_counts: BTreeMap<String, u32>,
    pub effect_sequences: u32,
    pub sequence_counts: BTreeMap<String, u32>,
    pub live_effects: usize,
    pub peak_effects: usize,
    pub haze_passes_loaded: usize,
    pub haze_frames: u32,
    pub haze_trails: BTreeMap<String, u32>,
}

#[derive(Resource)]
pub(crate) struct Weapons {
    manifest: Manifest,
    last_serial: u64,
    last_reset: u32,
    last_player_death: u32,
    last_player_hit: u64,
    last_attack: Option<Attack>,
    sounds: HashMap<String, Handle<AudioSource>>,
    sound_events: Vec<(f32, Handle<AudioSource>)>,
    age: f32,
    weapon_meshes:
        HashMap<(String, Option<usize>), Vec<(String, Handle<Mesh>, Vec<[f32; 3]>, Vec<[f32; 3]>)>>,
    effect_meshes: HashMap<String, Vec<Handle<Mesh>>>,
    particle_templates: Vec<ParticleTemplate>,
    particles: Vec<Particle>,
    live_sequences: Vec<LiveSequence>,
    sequence_events: Vec<(f32, String, bool)>,
    random: u32,
    ghost_meshes: HashMap<String, Vec<Handle<Mesh>>>,
    trails: Vec<LiveTrail>,
    active_id: u64,
    clock: f32,
    actor_presentations: HashMap<Entity, ActorPresentation>,
}

#[cfg(test)]
fn attribute_key(attack: Attack, hit_index: u8) -> &'static str {
    crate::content::attacks::hit_spec(attack, hit_index as usize)
        .map_or("", |spec| spec.attribute.source_name())
}

mod assets;
mod capture;
mod equipment;
mod hud;
mod playback;
mod render;
mod source;

use assets::*;
use capture::*;
use equipment::*;
use hud::*;
#[cfg(test)]
use playback::attack_sound_enabled;
use render::*;
use source::*;

pub(crate) use equipment::{WeaponModel, spawn_practice_equipment, weapon_key};
pub use playback::present;

#[cfg(test)]
mod tests;
