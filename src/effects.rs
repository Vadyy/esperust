use crate::{
    gameplay::{self, Avatar, MovementAction, Player, View},
    scene::PLAYER_MODEL_SCALE,
};
use bevy::{
    asset::RenderAssetUsages, camera::visibility::NoFrustumCulling, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use serde::Deserialize;
use std::collections::HashMap;

pub struct MovementEffectsPlugin;

impl Plugin for MovementEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EffectStats>()
            .add_systems(crate::loading::PrepareWorld, setup.after(gameplay::setup))
            .add_systems(
                Update,
                (update.after(gameplay::present)).run_if(crate::loading::ready),
            );
    }
}

#[derive(Resource, Default, serde::Serialize)]
pub struct EffectStats {
    pub jump_sequences: u32,
    pub dodge_sequences: u32,
    pub sprint_sequences: u32,
    pub wall_jump_sequences: u32,
    pub wall_haze_frames: u32,
    pub particles_emitted: u32,
    pub live_particles: usize,
    pub peak_particles: usize,
}

#[derive(Deserialize)]
struct Manifest {
    format_version: u32,
    sequences: HashMap<String, Sequence>,
    wall_jump: WallJump,
}

#[derive(Deserialize)]
struct Sequence {
    duration_ms: u32,
    components: Vec<EffectComponent>,
}

#[derive(Deserialize)]
struct EffectComponent {
    start_ms: u32,
    end_ms: u32,
    particle: Option<ParticleDefinition>,
}

#[derive(Clone, Deserialize)]
struct ParticleDefinition {
    render_type: u32,
    blend_type: u32,
    max_particles: usize,
    lifetime_var: u32,
    speed_var: f32,
    size_x_var: f32,
    size_y_var: f32,
    size_var: f32,
    rotate_var: f32,
    emitter_keys: Vec<EmitterKey>,
    particle_keys: Vec<[f32; 8]>,
    textures: Vec<EffectTexture>,
}

#[derive(Clone, Deserialize)]
struct EmitterKey {
    continuance: u8,
    position: [f32; 3],
    cone_direction: [f32; 3],
    cone_theta_degrees: f32,
    speed: f32,
    emitter_type: u32,
    min_width: f32,
    max_width: f32,
    min_height: f32,
    max_height: f32,
    gravity: [f32; 3],
    attractive: f32,
    lifetime_ms: u32,
    emit_per_second: u32,
}

#[derive(Clone, Deserialize)]
struct EffectTexture {
    path: String,
    uv: [f32; 8],
}

#[derive(Deserialize)]
struct WallJump {
    duration_ticks: f32,
    ticks_per_ms: f32,
    meshes: Vec<WallMesh>,
}

#[derive(Deserialize)]
struct WallMesh {
    texture: String,
    shader_flags: u32,
    haze_texture: Option<String>,
    indices: Vec<u32>,
    uvs: Vec<[f32; 2]>,
    frames: Vec<WallFrame>,
}

#[derive(Deserialize)]
struct WallFrame {
    tick: f32,
    alpha: f32,
    positions: Vec<[f32; 3]>,
}

struct Definition {
    source: ParticleDefinition,
    start: f32,
    end: f32,
    mesh: Handle<Mesh>,
}

struct Emitter {
    id: u32,
    definition: usize,
    action: MovementAction,
    age: f32,
    carry: f32,
}

struct Particle {
    emitter_id: u32,
    definition: usize,
    position: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    age: f32,
    lifetime: f32,
    size_offset: Vec2,
    rotation_offset: f32,
    center: Vec3,
    attractive: f32,
}

#[derive(Resource)]
struct MovementEffects {
    next_emitter: u32,
    manifest: Manifest,
    definitions: Vec<Definition>,
    sequences: HashMap<String, Vec<usize>>,
    emitters: Vec<Emitter>,
    particles: Vec<Particle>,
    wall_meshes: Vec<Handle<Mesh>>,
    wall_origin: Transform,
    wall_age: f32,
    last_serial: u32,
    last_jumps: u32,
    last_reset: u32,
    random: u32,
}

fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));
    mesh
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut haze_materials: ResMut<Assets<crate::materials::TrailHaze>>,
    assets: Res<AssetServer>,
) {
    let manifest: Manifest = match std::fs::read("assets/movement-effects.json")
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(value) => value,
        None => {
            warn!("Movement effects export absent or invalid; run tools/asset_effects.py");
            return;
        }
    };
    assert_eq!(manifest.format_version, 1);
    let mut definitions = Vec::new();
    let mut sequences = HashMap::new();
    let mut shared_materials = HashMap::<String, Handle<StandardMaterial>>::new();
    let mut make_material = |path: &str| {
        shared_materials
            .entry(path.to_owned())
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color_texture: Some(assets.load(path.to_owned())),
                    unlit: true,
                    alpha_mode: AlphaMode::Add,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                })
            })
            .clone()
    };
    for (name, sequence) in &manifest.sequences {
        let mut indices = Vec::new();
        for component in &sequence.components {
            let Some(particle) = &component.particle else {
                continue;
            };
            assert_eq!(
                particle.blend_type, 1,
                "Unimplemented source particle blend mode"
            );
            assert_eq!(particle.emitter_keys.len(), 1);
            assert_eq!(particle.textures.len(), 1);
            let handle = meshes.add(empty_mesh());
            commands.spawn((
                Name::new(format!("Original {name} particles")),
                Mesh3d(handle.clone()),
                MeshMaterial3d(make_material(&particle.textures[0].path)),
                Transform::default(),
                NoFrustumCulling,
                crate::dynamic_mesh::EffectMesh,
            ));
            indices.push(definitions.len());
            definitions.push(Definition {
                source: particle.clone(),
                start: component.start_ms as f32 * 0.001,
                end: component.end_ms.min(sequence.duration_ms) as f32 * 0.001,
                mesh: handle,
            });
        }
        sequences.insert(name.clone(), indices);
    }
    let mut wall_meshes = Vec::new();
    for source in &manifest.wall_jump.meshes {
        let handle = meshes.add(empty_mesh());
        commands.spawn((
            Name::new("Original wall-jump effect mesh"),
            Mesh3d(handle.clone()),
            MeshMaterial3d(make_material(&source.texture)),
            Transform::default(),
            NoFrustumCulling,
            crate::dynamic_mesh::EffectMesh,
        ));
        if source.shader_flags & 0x8000 != 0 {
            let path = source
                .haze_texture
                .as_ref()
                .expect("Export wall haze gradient");
            let gradient = assets.load_with_settings(
                path.clone(),
                |settings: &mut bevy::image::ImageLoaderSettings| {
                    settings.is_srgb = false;
                    settings.sampler = bevy::image::ImageSampler::linear();
                },
            );
            commands.spawn((
                Name::new("Original wall-jump Haze2 pass"),
                Mesh3d(handle.clone()),
                MeshMaterial3d(haze_materials.add(crate::materials::TrailHaze { gradient })),
                Transform::default(),
                NoFrustumCulling,
                crate::dynamic_mesh::EffectMesh,
            ));
        }
        wall_meshes.push(handle);
    }
    commands.insert_resource(MovementEffects {
        next_emitter: 0,
        manifest,
        definitions,
        sequences,
        emitters: Vec::new(),
        particles: Vec::new(),
        wall_meshes,
        wall_origin: Transform::default(),
        wall_age: f32::INFINITY,
        last_serial: 0,
        last_jumps: 0,
        last_reset: 0,
        random: 0xa491e72f,
    });
}

use crate::effect_core::{random, sample_key, source_vector};

impl MovementEffects {
    fn start(&mut self, name: &str, action: MovementAction, _jump: bool) {
        self.next_emitter = self.next_emitter.wrapping_add(1);
        if let Some(definitions) = self.sequences.get(name) {
            for &definition in definitions {
                self.emitters.push(Emitter {
                    id: self.next_emitter,
                    definition,
                    action,
                    age: 0.,
                    carry: 0.,
                });
            }
        }
    }

    fn emit(&mut self, index: usize, emitter_id: u32, avatar: &Transform) {
        let definition = &self.definitions[index].source;
        if self
            .particles
            .iter()
            .filter(|p| p.definition == index && p.emitter_id == emitter_id)
            .count()
            >= definition.max_particles
        {
            return;
        }
        let emitter = &definition.emitter_keys[0];
        let scale = 0.01 * PLAYER_MODEL_SCALE;
        let mut rng = || random(&mut self.random);
        let offset = crate::particle_geometry::volume(
            emitter.emitter_type,
            emitter.min_width,
            emitter.max_width,
            emitter.min_height,
            emitter.max_height,
            &mut rng,
        );
        let local = source_vector(emitter.position) + Vec3::new(offset.x, offset.y, -offset.z);
        let speed =
            emitter.speed + crate::particle_geometry::variance(&mut rng, definition.speed_var);
        let velocity = crate::particle_geometry::velocity(
            Vec3::from(emitter.cone_direction),
            speed,
            emitter.cone_theta_degrees,
            &mut rng,
        );
        let lifetime = (emitter.lifetime_ms as f32
            + crate::particle_geometry::variance(&mut rng, definition.lifetime_var as f32))
        .max(1.)
            * 0.001;
        let shared_size = crate::particle_geometry::variance(&mut rng, definition.size_var);
        let size_offset = Vec2::splat(shared_size)
            + Vec2::new(
                crate::particle_geometry::variance(&mut rng, definition.size_x_var),
                crate::particle_geometry::variance(&mut rng, definition.size_y_var),
            );
        let rotation_offset = crate::particle_geometry::variance(&mut rng, definition.rotate_var);
        self.particles.push(Particle {
            emitter_id,
            definition: index,
            position: avatar.translation + avatar.rotation * (local * scale),
            velocity: avatar.rotation * (Vec3::new(velocity.x, velocity.y, -velocity.z) * scale),
            gravity: avatar.rotation * source_vector(emitter.gravity) * scale,
            age: 0.,
            lifetime,
            size_offset,
            rotation_offset,
            center: avatar.translation + avatar.rotation * source_vector(emitter.position) * scale,
            attractive: emitter.attractive * scale,
        });
    }
}

fn update(
    effects: Option<ResMut<MovementEffects>>,
    player: Res<Player>,
    avatar: Single<&Transform, (With<Avatar>, Without<View>)>,
    camera: Single<&Transform, (With<View>, Without<Avatar>)>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut stats: ResMut<EffectStats>,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("movement_effects"));
    let Some(mut effects) = effects else { return };
    let delta = time.delta_secs();
    if player.pose_reset_serial != effects.last_reset {
        effects.emitters.clear();
        effects.particles.clear();
        effects.wall_age = f32::INFINITY;
        effects.last_jumps = player.jumps;
        effects.last_serial = player.action_serial;
        effects.last_reset = player.pose_reset_serial;
    }
    if player.jumps != effects.last_jumps {
        effects.start("jump", MovementAction::None, true);
        stats.jump_sequences += 1;
        effects.last_jumps = player.jumps;
    }
    if player.action_serial != effects.last_serial {
        effects.last_serial = player.action_serial;
        match player.action {
            MovementAction::Sprint => {
                effects.start("sprint", player.action, false);
                stats.sprint_sequences += 1;
            }
            MovementAction::DodgeLeft => {
                effects.start("dodge_left", player.action, false);
                stats.dodge_sequences += 1;
            }
            MovementAction::DodgeRight => {
                effects.start("dodge_right", player.action, false);
                stats.dodge_sequences += 1;
            }
            MovementAction::WallJump => {
                effects.wall_origin = **avatar;
                effects.wall_origin.rotation =
                    Quat::from_rotation_arc(Vec3::Z, -player.wall_normal);
                effects.wall_age = 0.;
                stats.wall_jump_sequences += 1;
            }
            MovementAction::None => {}
        }
    }
    let mut emit = Vec::new();
    for index in 0..effects.emitters.len() {
        let definition_index = effects.emitters[index].definition;
        let definition = &effects.definitions[definition_index];
        let start = definition.start;
        let end = definition.end;
        let (continuous, rate) = (
            definition.source.emitter_keys[0].continuance != 0,
            definition.source.emitter_keys[0].emit_per_second,
        );
        let emitter = &mut effects.emitters[index];
        if emitter.action == MovementAction::Sprint && emitter.action != player.action {
            continue;
        }
        let old = emitter.age;
        emitter.age += delta;
        let duration = if emitter.action == MovementAction::Sprint {
            f32::INFINITY
        } else {
            end
        };
        let count = crate::particle_clock::emission_count(
            continuous,
            rate,
            old,
            emitter.age,
            start,
            duration,
            &mut emitter.carry,
        );
        for _ in 0..count {
            emit.push((definition_index, emitter.id));
        }
    }
    for particle in &mut effects.particles {
        particle.age += delta;
        crate::particle_geometry::integrate(
            &mut particle.position,
            &mut particle.velocity,
            particle.gravity,
            particle.center,
            particle.attractive,
            delta,
        );
    }
    effects.particles.retain(|p| p.age < p.lifetime);
    for (index, emitter_id) in emit {
        let before = effects.particles.len();
        effects.emit(index, emitter_id, &avatar);
        stats.particles_emitted += (effects.particles.len() - before) as u32;
    }
    let definitions_ends: Vec<f32> = effects.definitions.iter().map(|d| d.end).collect();
    effects.emitters.retain(|emitter| {
        (emitter.action != MovementAction::Sprint || emitter.action == player.action)
            && (emitter.action == MovementAction::Sprint
                || emitter.age < definitions_ends[emitter.definition])
    });
    stats.live_particles = effects.particles.len();
    stats.peak_particles = stats.peak_particles.max(stats.live_particles);
    let scale = 0.01 * PLAYER_MODEL_SCALE;
    for (index, definition) in effects.definitions.iter().enumerate() {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut colors = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();
        for p in effects.particles.iter().filter(|p| p.definition == index) {
            let key = sample_key(&definition.source.particle_keys, p.age / p.lifetime);
            let width = (key[1] + p.size_offset.x).max(0.) * scale * 0.5;
            let height = (key[2] + p.size_offset.y).max(0.) * scale * 0.5;
            let (right, up) = crate::effect_core::billboard_axes(
                definition.source.render_type,
                p.velocity,
                p.position,
                &camera,
            );
            let rotation = Quat::from_axis_angle(
                right.cross(up).normalize_or(Vec3::Z),
                (key[7] + p.rotation_offset).to_radians(),
            );
            let right = rotation * right * width;
            let up = rotation * up * height;
            let first = positions.len() as u32;
            for (corner, uv_index) in [
                (-right + up, 0),
                (-right - up, 1),
                (right + up, 2),
                (right - up, 3),
            ] {
                positions.push((p.position + corner).to_array());
                normals.push([0., 1., 0.]);
                colors.push([key[3], key[4], key[5], key[6].clamp(0., 1.)]);
                let uv = definition.source.textures[0].uv;
                uvs.push([uv[uv_index * 2], uv[uv_index * 2 + 1]]);
            }
            indices.extend([first, first + 1, first + 2, first + 2, first + 1, first + 3]);
        }
        if !crate::dynamic_mesh::needs_upload(&meshes, &definition.mesh, !indices.is_empty()) {
            continue;
        }
        if let Some(mesh) = meshes.get_mut(&definition.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_indices(Indices::U32(indices));
        }
    }
    effects.wall_age += delta;
    let wall_ticks = effects.wall_age * effects.manifest.wall_jump.ticks_per_ms * 1000.;
    if wall_ticks <= effects.manifest.wall_jump.duration_ticks {
        stats.wall_haze_frames += 1;
    }
    for (source, handle) in effects
        .manifest
        .wall_jump
        .meshes
        .iter()
        .zip(&effects.wall_meshes)
    {
        let alive = wall_ticks <= effects.manifest.wall_jump.duration_ticks
            && effects.wall_age * 1000.
                <= effects.manifest.sequences["wall_jump"].duration_ms as f32;
        if !crate::dynamic_mesh::needs_upload(&meshes, handle, alive) {
            continue;
        }
        let Some(mesh) = meshes.get_mut(handle) else {
            continue;
        };
        if !alive {
            mesh.insert_indices(Indices::U32(Vec::new()));
            continue;
        }
        let (first, second, t) =
            crate::effect_core::frame_span(&source.frames, wall_ticks, |frame| frame.tick);
        let before = &source.frames[first];
        let after = &source.frames[second];
        let alpha = before.alpha + (after.alpha - before.alpha) * t;
        let positions: Vec<[f32; 3]> = before
            .positions
            .iter()
            .zip(&after.positions)
            .map(|(a, b)| {
                let position = Vec3::from_array(*a).lerp(Vec3::from_array(*b), t);
                (effects.wall_origin.translation + effects.wall_origin.rotation * position)
                    .to_array()
            })
            .collect();
        let count = positions.len();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 1., 0.]; count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1., 1., 1., alpha]; count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, source.uvs.clone());
        mesh.insert_indices(Indices::U32(source.indices.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_movement_carriers_and_tracks_are_valid() {
        let manifest: Manifest = crate::content::required_json("movement-effects.json");
        assert_eq!(manifest.sequences.len(), 5);
        assert_eq!(manifest.wall_jump.meshes.len(), 27);
        for sequence in manifest.sequences.values() {
            for component in &sequence.components {
                assert!(component.start_ms < component.end_ms);
                if let Some(p) = &component.particle {
                    assert!(p.max_particles > 0 && p.max_particles <= 500);
                    assert_eq!(p.emitter_keys.len(), 1);
                    assert_eq!(p.textures.len(), 1);
                    assert!(
                        std::path::Path::new("assets")
                            .join(&p.textures[0].path)
                            .is_file()
                    );
                    assert!(p.particle_keys.windows(2).all(|v| v[0][0] < v[1][0]));
                    assert!(p.particle_keys.iter().flatten().all(|v| v.is_finite()));
                }
            }
        }
        for mesh in &manifest.wall_jump.meshes {
            assert_eq!(mesh.frames.len(), 75);
            for frame in &mesh.frames {
                assert_eq!(frame.positions.len(), mesh.uvs.len());
                assert!(
                    mesh.indices
                        .iter()
                        .all(|i| (*i as usize) < frame.positions.len())
                );
            }
        }
    }

    #[test]
    fn source_particle_key_interpolation_keeps_lifetime_endpoints() {
        let keys = [
            [0., 10., 20., 1., 1., 1., 0., 0.],
            [0.25, 8., 20., 1., 1., 1., 0.5, 0.],
            [1., 8., 20., 1., 1., 1., 0., 0.],
        ];
        assert_eq!(sample_key(&keys, 0.)[6], 0.);
        assert_eq!(sample_key(&keys, 0.25)[6], 0.5);
        assert_eq!(sample_key(&keys, 1.)[6], 0.);
        assert!((sample_key(&keys, 0.125)[6] - 0.25).abs() < 1e-6);
    }
}

mod verification;
pub(crate) fn install_verification(app: &mut App) {
    verification::install(app);
}
