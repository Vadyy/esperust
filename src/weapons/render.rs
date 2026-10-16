use super::*;

pub(super) struct TrailSample {
    pub(super) born: f32,
    pub(super) points: [Vec3; 2],
}
pub(super) struct LiveTrail {
    pub(super) name: String,
    pub(super) weapon: String,
    pub(super) serial: u64,
    pub(super) owner: Option<Entity>,
    pub(super) start: f32,
    pub(super) end: f32,
    pub(super) emission_start: f32,
    pub(super) emission_end: f32,
    pub(super) samples: Vec<TrailSample>,
}

pub(super) struct ParticleTemplate {
    pub(super) sequence: String,
    pub(super) component: usize,
    pub(super) source: ParticleDefinition,
    pub(super) meshes: Vec<Handle<Mesh>>,
}
pub(super) struct Particle {
    pub(super) sequence_id: u32,
    pub(super) owner: Option<Entity>,
    pub(super) template: usize,
    pub(super) texture: usize,
    pub(super) position: Vec3,
    pub(super) velocity: Vec3,
    pub(super) gravity: Vec3,
    pub(super) center: Vec3,
    pub(super) attractive: f32,
    pub(super) age: f32,
    pub(super) lifetime: f32,
    pub(super) size_offset: Vec2,
    pub(super) rotation_offset: f32,
}
pub(super) struct LiveSequence {
    pub(super) id: u32,
    pub(super) name: String,
    pub(super) age: f32,
    pub(super) origin: Transform,
    pub(super) serial: Option<u64>,
    pub(super) remove_on_exit: bool,
    pub(super) carry: Vec<f32>,
    pub(super) owner: Option<Entity>,
    pub(super) bone_poses: HashMap<String, Mat4>,
}

#[derive(Default)]
pub(super) struct Geometry {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) normals: Vec<[f32; 3]>,
    pub(super) colors: Vec<[f32; 4]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) indices: Vec<u32>,
}
impl Geometry {
    fn upload(self, meshes: &mut Assets<Mesh>, handle: &Handle<Mesh>) {
        if crate::dynamic_mesh::needs_upload(meshes, handle, !self.indices.is_empty()) {
            if let Some(mesh) = meshes.get_mut(handle) {
                self.apply(mesh);
            }
        }
    }
    pub(super) fn apply(self, mesh: &mut Mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
    }
}

use crate::effect_core::{random, sample_key, source_vector};

pub(super) fn component_origin(
    manifest: &Manifest,
    sequence: &LiveSequence,
    index: usize,
    avatar: &Transform,
    animation: &Animation,
) -> Mat4 {
    component_origin_inner(manifest, sequence, index, avatar, animation, 0)
}

pub(super) fn component_origin_inner(
    manifest: &Manifest,
    sequence: &LiveSequence,
    index: usize,
    avatar: &Transform,
    animation: &Animation,
    depth: usize,
) -> Mat4 {
    let root = if sequence.owner.is_none() && sequence.serial.is_some() {
        avatar.to_matrix()
    } else {
        sequence.origin.to_matrix()
    };
    let definition = &manifest.effects.sequences[&sequence.name];
    let component = &definition.components[index];
    let attached = if component.flags[0] == 1 && depth < definition.components.len() {
        let parent_index = component.flags[1] as usize;
        definition
            .components
            .get(parent_index)
            .filter(|_| parent_index != index)
            .and_then(|parent| parent.asset_model.as_deref())
            .and_then(|name| manifest.effects.models.get(&asset_stem(name)))
            .map(|model| {
                let parent = &definition.components[parent_index];
                let tick = ((sequence.age * 1000. - parent.start_ms as f32).max(0.) as f64
                    * model.ticks_per_ms)
                    .min(model.duration_ticks) as f32;
                let local = model
                    .node_frames
                    .get(&component.name)
                    .map_or(Mat4::IDENTITY, |frames| sample_node(frames, tick));
                component_origin_inner(
                    manifest,
                    sequence,
                    parent_index,
                    avatar,
                    animation,
                    depth + 1,
                ) * local
            })
            .unwrap_or(root)
    } else if !component.name.is_empty() {
        root * if sequence.owner.is_some() {
            sequence
                .bone_poses
                .get(&component.name)
                .copied()
                .unwrap_or(Mat4::IDENTITY)
        } else if sequence.serial.is_some() {
            animation
                .bone_matrix(&component.name)
                .unwrap_or(Mat4::IDENTITY)
        } else {
            Mat4::IDENTITY
        }
    } else {
        root
    };
    let attached = apply_transform_sources(attached, root, component.actor_transform_sources);
    apply_transform_sources(
        attached,
        Mat4::from_translation(root.w_axis.truncate()),
        component.self_transform_sources,
    )
}

pub(super) fn apply_transform_sources(
    attached: Mat4,
    root: Mat4,
    sources: Option<[u32; 3]>,
) -> Mat4 {
    if let Some(sources) = sources {
        let (mut scale, mut rotation, mut position) = attached.to_scale_rotation_translation();
        let (actor_scale, actor_rotation, actor_position) = root.to_scale_rotation_translation();
        if sources[0] == 1 {
            position = actor_position;
        }
        if sources[1] == 1 {
            rotation = actor_rotation;
        }
        if sources[2] == 1 {
            scale = actor_scale;
        }
        Mat4::from_scale_rotation_translation(scale, rotation, position)
    } else {
        attached
    }
}

pub(super) fn sample_node(frames: &[NodeFrame], tick: f32) -> Mat4 {
    if frames.is_empty() {
        return Mat4::IDENTITY;
    }
    let i = frames
        .partition_point(|frame| frame.tick < tick)
        .min(frames.len() - 1);
    let a = &frames[i.saturating_sub(1)];
    let b = &frames[i];
    let t = if b.tick > a.tick {
        ((tick - a.tick) / (b.tick - a.tick)).clamp(0., 1.)
    } else {
        0.
    };
    let (sa, ra, pa) = Mat4::from_cols_array(&a.matrix).to_scale_rotation_translation();
    let (sb, rb, pb) = Mat4::from_cols_array(&b.matrix).to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(sa.lerp(sb, t), ra.slerp(rb, t), pa.lerp(pb, t))
}

pub(super) fn effect_mesh_positions(
    positions: &[[f32; 3]],
    origin: Mat4,
    camera: &Transform,
    billboard: bool,
) -> Vec<[f32; 3]> {
    if !billboard || positions.is_empty() {
        return positions
            .iter()
            .map(|p| origin.transform_point3(Vec3::from_array(*p)).to_array())
            .collect();
    }
    let center =
        positions.iter().map(|p| Vec3::from_array(*p)).sum::<Vec3>() / positions.len() as f32;
    let world_center = origin.transform_point3(center);
    let (scale, _, _) = origin.to_scale_rotation_translation();
    positions
        .iter()
        .map(|p| {
            (world_center + camera.rotation * ((Vec3::from_array(*p) - center) * scale)).to_array()
        })
        .collect()
}

pub(super) fn emit_particle(
    weapons: &mut Weapons,
    index: usize,
    sequence_id: u32,
    origin: Mat4,
    stats: &mut WeaponStats,
) {
    let source = &weapons.particle_templates[index].source;
    if weapons
        .particles
        .iter()
        .filter(|p| p.template == index && p.sequence_id == sequence_id)
        .count()
        >= source.max_particles
    {
        return;
    }
    let owner = weapons
        .live_sequences
        .iter()
        .find(|sequence| sequence.id == sequence_id)
        .and_then(|sequence| sequence.owner);
    let emitter = &source.emitter_keys[0];
    let mut rng = || random(&mut weapons.random);
    let offset = crate::particle_geometry::volume(
        emitter.emitter_type,
        emitter.min_width,
        emitter.max_width,
        emitter.min_height,
        emitter.max_height,
        &mut rng,
    );
    let position = source_vector(emitter.position) + Vec3::new(offset.x, offset.y, -offset.z);
    let speed = emitter.speed + crate::particle_geometry::variance(&mut rng, source.speed_var);
    let velocity = crate::particle_geometry::velocity(
        Vec3::from(emitter.cone_direction),
        speed,
        emitter.cone_theta_degrees,
        &mut rng,
    );
    let lifetime = (emitter.lifetime_ms as f32
        + crate::particle_geometry::variance(&mut rng, source.lifetime_var as f32))
    .max(1.)
        * 0.001;
    let shared_size = crate::particle_geometry::variance(&mut rng, source.size_var);
    let size_offset = Vec2::splat(shared_size)
        + Vec2::new(
            crate::particle_geometry::variance(&mut rng, source.size_x_var),
            crate::particle_geometry::variance(&mut rng, source.size_y_var),
        );
    let rotation_offset = crate::particle_geometry::variance(&mut rng, source.rotate_var);
    weapons.particles.push(Particle {
        sequence_id,
        owner,
        template: index,
        texture: (random(&mut weapons.random) * source.textures.len() as f32) as usize,
        position: origin.transform_point3(position * 0.01),
        velocity: origin.transform_vector3(Vec3::new(velocity.x, velocity.y, -velocity.z) * 0.01),
        gravity: origin.transform_vector3(source_vector(emitter.gravity) * 0.01),
        center: origin.transform_point3(source_vector(emitter.position) * 0.01),
        attractive: emitter.attractive * 0.01,
        age: 0.,
        lifetime,
        size_offset,
        rotation_offset,
    });
    stats.peak_effects = stats.peak_effects.max(weapons.particles.len());
}

pub(super) fn update_effects(
    weapons: &mut Weapons,
    animation: &Animation,
    avatar: &Transform,
    camera: &Transform,
    player: &Player,
    delta: f32,
    meshes: &mut Assets<Mesh>,
    stats: &mut WeaponStats,
) {
    weapons.live_sequences.retain(|sequence| {
        sequence.owner.is_some()
            || !sequence.remove_on_exit
            || (sequence.serial == Some(weapons.active_id) && player.melee.attack.is_some())
    });
    weapons.live_sequences.retain(|sequence| {
        let source = &weapons.manifest.effects.sequences[&sequence.name];
        source.flags[1] != 0 || sequence.age * 1000. < source.duration_ms as f32
    });
    let mut emission = Vec::new();
    for sequence in &mut weapons.live_sequences {
        let source = &weapons.manifest.effects.sequences[&sequence.name];
        if source.flags[1] != 0 && source.duration_ms > 0 {
            if sequence.age >= source.duration_ms as f32 * 0.001 {
                sequence.age %= source.duration_ms as f32 * 0.001;
                sequence.carry.fill(0.);
            }
        }
        let old = sequence.age;
        sequence.age += delta;
        for (index, template) in weapons
            .particle_templates
            .iter()
            .enumerate()
            .filter(|(_, template)| template.sequence == sequence.name)
        {
            let component = &source.components[template.component];
            let emitter = &template.source.emitter_keys[0];
            let count = crate::particle_clock::emission_count(
                emitter.continuance != 0,
                emitter.emit_per_second,
                old,
                sequence.age,
                component.start_ms as f32 * 0.001,
                component.end_ms as f32 * 0.001,
                &mut sequence.carry[template.component],
            );
            let origin = component_origin(
                &weapons.manifest,
                sequence,
                template.component,
                avatar,
                animation,
            );
            for _ in 0..count {
                emission.push((index, sequence.id, origin));
            }
        }
    }
    for particle in &mut weapons.particles {
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
    weapons
        .particles
        .retain(|particle| particle.age < particle.lifetime);
    for (index, sequence_id, origin) in emission {
        emit_particle(weapons, index, sequence_id, origin, stats);
    }
    for (name, handles) in &weapons.effect_meshes {
        let source = &weapons.manifest.effects.models[name];
        for (source_mesh, handle) in source.meshes.iter().zip(handles) {
            let mut geometry = Geometry::default();
            for sequence in &weapons.live_sequences {
                let definition = &weapons.manifest.effects.sequences[&sequence.name];
                for (index, component) in definition.components.iter().enumerate() {
                    if !component
                        .asset_model
                        .as_deref()
                        .is_some_and(|path| asset_stem(path) == *name)
                        || sequence.age * 1000. < component.start_ms as f32
                        || component.opacity(sequence.age * 1000.) <= 0.
                    {
                        continue;
                    }
                    let clock = (sequence.age * 1000. - component.start_ms as f32) as f64
                        * source.ticks_per_ms;
                    let tick = if component.animation_loop {
                        clock % source.duration_ticks
                    } else {
                        clock.min(source.duration_ticks)
                    } as f32;
                    let (positions, alpha) = sample_mesh(source_mesh, tick);
                    let alpha = alpha * component.opacity(sequence.age * 1000.);
                    let origin =
                        component_origin(&weapons.manifest, sequence, index, avatar, animation);
                    let first = geometry.positions.len() as u32;
                    geometry.positions.extend(effect_mesh_positions(
                        &positions,
                        origin,
                        camera,
                        source_mesh.shader_flags & 16 != 0,
                    ));
                    geometry.normals.extend(vec![[0., 1., 0.]; positions.len()]);
                    geometry
                        .colors
                        .extend(vec![[1., 1., 1., alpha]; positions.len()]);
                    geometry.uvs.extend(&source_mesh.uvs);
                    geometry
                        .indices
                        .extend(source_mesh.indices.iter().map(|i| i + first));
                }
            }
            geometry.upload(meshes, handle);
        }
    }
    for (index, template) in weapons.particle_templates.iter().enumerate() {
        for (texture, handle) in template.meshes.iter().enumerate() {
            let mut geometry = Geometry::default();
            for particle in weapons
                .particles
                .iter()
                .filter(|p| p.template == index && p.texture == texture)
            {
                let key = sample_key(
                    &template.source.particle_keys,
                    particle.age / particle.lifetime,
                );
                let width =
                    (key[1] + particle.size_offset.x).max(0.) * 0.005 * scene::PLAYER_MODEL_SCALE;
                let height =
                    (key[2] + particle.size_offset.y).max(0.) * 0.005 * scene::PLAYER_MODEL_SCALE;
                let (right, up) = crate::effect_core::billboard_axes(
                    template.source.render_type,
                    particle.velocity,
                    particle.position,
                    camera,
                );
                let rotation = Quat::from_axis_angle(
                    right.cross(up).normalize_or(Vec3::Z),
                    (key[7] + particle.rotation_offset).to_radians(),
                );
                let right = rotation * right * width;
                let up = rotation * up * height;
                let first = geometry.positions.len() as u32;
                for (corner, uv_index) in [
                    (-right + up, 0),
                    (-right - up, 1),
                    (right + up, 2),
                    (right - up, 3),
                ] {
                    geometry
                        .positions
                        .push((particle.position + corner).to_array());
                    geometry.normals.push([0., 1., 0.]);
                    geometry
                        .colors
                        .push([key[3], key[4], key[5], key[6].clamp(0., 1.)]);
                    let uv = template.source.textures[texture].uv;
                    geometry.uvs.push([uv[uv_index * 2], uv[uv_index * 2 + 1]]);
                }
                geometry.indices.extend([
                    first,
                    first + 1,
                    first + 2,
                    first + 2,
                    first + 1,
                    first + 3,
                ]);
            }
            geometry.upload(meshes, handle);
        }
    }
    stats.live_effects = weapons.live_sequences.len() + weapons.particles.len();
    stats.peak_effects = stats.peak_effects.max(stats.live_effects);
}

pub(super) fn update_trails(
    weapons: &mut Weapons,
    animation: &Animation,
    avatar: &Transform,
    player: &Player,
    actor_poses: &HashMap<Entity, ActorPresentationPose>,
    delta: f32,
    meshes: &mut Assets<Mesh>,
    stats: &mut WeaponStats,
) {
    weapons.clock += delta;
    let clock = weapons.clock;
    if weapons
        .trails
        .iter()
        .any(|trail| !trail.samples.is_empty() && weapons.manifest.ghosttrails[&trail.name].haze)
    {
        stats.haze_frames += 1;
    }
    for trail in weapons
        .trails
        .iter()
        .filter(|trail| !trail.samples.is_empty() && weapons.manifest.ghosttrails[&trail.name].haze)
    {
        *stats.haze_trails.entry(trail.name.clone()).or_default() += 1;
    }
    for trail in &mut weapons.trails {
        let source = &weapons.manifest.ghosttrails[&trail.name];
        let lifetime = source.lifetime_ms * 0.001;
        trail
            .samples
            .retain(|sample| clock - sample.born < lifetime);
        let actor = trail.owner.and_then(|owner| actor_poses.get(&owner));
        let active = match trail.owner {
            Some(owner) => weapons
                .actor_presentations
                .get(&owner)
                .is_some_and(|state| state.active && state.active_id == trail.serial),
            None => trail.serial == weapons.active_id && player.melee.attack.is_some(),
        };
        let elapsed = actor.map_or(player.melee.elapsed, |pose| pose.elapsed);
        if !active {
            trail.emission_end = trail.emission_end.min(clock);
        }
        if !active
            || elapsed < trail.start
            || elapsed > trail.end
            || trail
                .samples
                .last()
                .is_some_and(|last| clock - last.born < 1. / 120.)
        {
            continue;
        }
        let weapon = weapons.manifest.weapons[&trail.weapon].trail_part(source.part.as_deref());
        let bone = if let Some(actor) = actor {
            actor.bones.get(&weapon.attach_bone).copied()
        } else {
            animation.bone_matrix(&weapon.attach_bone)
        };
        let Some(bone) = bone else {
            continue;
        };
        let origin = actor.map_or(avatar, |pose| &pose.origin).to_matrix()
            * bone
            * Mat4::from_cols_array(&weapon.attach_matrix);
        let cursor = actor
            .map(|pose| (pose.clip.as_str(), pose.tick))
            .or_else(|| animation.upper_cursor());
        let points = cursor
            .and_then(|(clip, tick)| {
                weapon
                    .mesh_animations
                    .get(clip)
                    .map(|source| (source, tick))
            })
            .filter(|(source, _)| !source.trail_frames.is_empty())
            .map_or(source.points, |(source, tick)| {
                let i = source
                    .trail_frames
                    .partition_point(|frame| frame.tick < tick as f32)
                    .min(source.trail_frames.len() - 1);
                let a = &source.trail_frames[i.saturating_sub(1)];
                let b = &source.trail_frames[i];
                let weight = if b.tick > a.tick {
                    ((tick as f32 - a.tick) / (b.tick - a.tick)).clamp(0., 1.)
                } else {
                    0.
                };
                std::array::from_fn(|i| {
                    Vec3::from_array(a.points[i])
                        .lerp(Vec3::from_array(b.points[i]), weight)
                        .to_array()
                })
            })
            .map(|point| origin.transform_point3(Vec3::from_array(point)));
        trail.samples.push(TrailSample {
            born: clock,
            points,
        });
    }
    weapons.trails.retain(|trail| {
        let (active, elapsed) = if let Some(owner) = trail.owner {
            (
                weapons
                    .actor_presentations
                    .get(&owner)
                    .is_some_and(|state| state.active && state.active_id == trail.serial),
                actor_poses
                    .get(&owner)
                    .map_or(f32::INFINITY, |pose| pose.elapsed),
            )
        } else {
            (
                trail.serial == weapons.active_id && player.melee.attack.is_some(),
                player.melee.elapsed,
            )
        };
        !trail.samples.is_empty() || (active && elapsed < trail.end)
    });
    for (name, handles) in &weapons.ghost_meshes {
        for handle in handles {
            let mut geometry = Geometry::default();
            let source = &weapons.manifest.ghosttrails[name];
            for trail in weapons.trails.iter().filter(|trail| trail.name == *name) {
                for (sample_index, pair) in trail.samples.windows(2).enumerate() {
                    let first = geometry.positions.len() as u32;
                    for (offset, sample) in pair.iter().enumerate() {
                        let age = (clock - sample.born) * 1000.;
                        let alpha = trail_alpha(source, trail, clock, age);
                        for (i, point) in sample.points.iter().enumerate() {
                            geometry.positions.push(point.to_array());
                            geometry.normals.push([0., 1., 0.]);
                            geometry.colors.push([1., 1., 1., alpha]);
                            geometry.uvs.push(trail_uv(
                                sample_index + offset,
                                trail.samples.len(),
                                i,
                            ));
                        }
                    }
                    geometry.indices.extend([
                        first,
                        first + 1,
                        first + 2,
                        first + 2,
                        first + 1,
                        first + 3,
                    ]);
                }
            }
            geometry.upload(meshes, handle);
        }
    }
    stats.live_effects += weapons
        .trails
        .iter()
        .map(|trail| trail.samples.len())
        .sum::<usize>();
    stats.peak_effects = stats.peak_effects.max(stats.live_effects);
}

pub(super) fn trail_uv(index: usize, sample_count: usize, endpoint: usize) -> [f32; 2] {
    [
        1. - index as f32 / sample_count.saturating_sub(1).max(1) as f32,
        1. - endpoint as f32,
    ]
}

pub(super) fn trail_alpha(
    source: &GhostTrailSource,
    trail: &LiveTrail,
    clock: f32,
    sample_age_ms: f32,
) -> f32 {
    let emission = if clock <= trail.emission_end {
        ((clock - trail.emission_start) * 1000. / source.fade_in_ms.max(1.)).clamp(0., 1.)
    } else {
        (1. - (clock - trail.emission_end) * 1000. / source.fade_out_ms.max(1.)).clamp(0., 1.)
    };
    emission * (1. - sample_age_ms / source.lifetime_ms.max(1.)).clamp(0., 1.)
}
