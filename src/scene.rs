use crate::controller::CollisionWorld;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use serde::Deserialize;

pub const PLAYER_MODEL_SCALE: f32 = 1.3;
pub const LOCAL_TEAM: u8 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapId {
    #[default]
    Circle2,
    Station2,
    Neden1,
}
impl MapId {
    pub const ALL: [Self; 3] = [Self::Circle2, Self::Station2, Self::Neden1];

    pub fn id(self) -> &'static str {
        match self {
            Self::Circle2 => "circle-2",
            Self::Station2 => "station-2",
            Self::Neden1 => "neden-1",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Circle2 => "Circle 2",
            Self::Station2 => "Station 2",
            Self::Neden1 => "Neden 1",
        }
    }
    pub fn asset_path(self) -> &'static str {
        match self {
            Self::Circle2 => "assets/circle2.json",
            Self::Station2 => "assets/station2.json",
            Self::Neden1 => "assets/neden1.json",
        }
    }
    fn expected_spawns(self) -> usize {
        match self {
            Self::Circle2 => 7,
            Self::Station2 => 8,
            Self::Neden1 => 10,
        }
    }
}
impl std::str::FromStr for MapId {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "circle-2" | "circle2" => Ok(Self::Circle2),
            "station-2" | "station2" => Ok(Self::Station2),
            "neden-1" | "neden1" => Ok(Self::Neden1),
            _ => Err(format!(
                "Unknown map {value}. Available: circle-2, station-2, neden-1"
            )),
        }
    }
}
#[derive(Resource, Default)]
pub struct MapSelection {
    pub active: MapId,
}
#[derive(Component)]
pub struct MapEntity;

#[derive(Deserialize)]
pub struct Map {
    pub format_version: u32,
    #[serde(default)]
    pub id: String,
    pub units: String,
    pub meshes: Vec<MapMesh>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
    pub bounds: Bounds,
    #[serde(default)]
    pub boxes: Vec<MapBox>,
}

#[derive(Deserialize)]
pub struct MapBox {
    pub name: String,
    pub matrix: [f32; 16],
    pub size: [f32; 3],
}

pub struct DeathZone {
    pub name: String,
    pub owner_team: u8,
    inverse: Mat4,
    half_size: Vec3,
}

impl DeathZone {
    fn from_box(source: &MapBox) -> Self {
        let mut matrix = source.matrix;
        for column in 0..4 { for row in 0..4 {
            matrix[column * 4 + row] *= if (column == 2) != (row == 2) { -1. } else { 1. };
        }}
        for value in &mut matrix[12..15] { *value *= 0.01; }
        Self {
            name: source.name.clone(),
            owner_team: if source.name.starts_with("alpha_") { 1 } else { 2 },
            inverse: Mat4::from_cols_array(&matrix).inverse(),
            half_size: Vec3::from(source.size) * 0.5,
        }
    }

    pub fn intersects(&self, before: Vec3, after: Vec3, axes: Vec3, team: u8) -> bool {
        if team == self.owner_team { return false; }
        let a = self.inverse.transform_point3(before + Vec3::Y * axes.y);
        let b = self.inverse.transform_point3(after + Vec3::Y * axes.y);
        let rotation = Mat3::from_mat4(self.inverse);
        let support = Vec3::new(rotation.row(0).mul_add(axes, Vec3::ZERO).length(),
            rotation.row(1).mul_add(axes, Vec3::ZERO).length(), rotation.row(2).mul_add(axes, Vec3::ZERO).length());
        let extent = self.half_size + support;
        let delta = b - a;
        let (mut enter, mut exit) = (0.0_f32, 1.0_f32);
        for axis in 0..3 {
            if delta[axis].abs() < 1e-7 {
                if a[axis].abs() > extent[axis] { return false; }
            } else {
                let near = (-extent[axis] - a[axis]) / delta[axis];
                let far = (extent[axis] - a[axis]) / delta[axis];
                enter = enter.max(near.min(far));
                exit = exit.min(near.max(far));
                if enter > exit { return false; }
            }
        }
        true
    }
}
#[derive(Deserialize)]
pub struct MapMesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub texture: Option<String>,
    #[serde(default)]
    pub enemy_texture: Option<String>,
    #[serde(default)]
    pub team: u8,
    #[serde(default)]
    pub animation: Option<MapAnimation>,
    #[serde(default)]
    pub sector: Option<String>,
    #[serde(default)]
    pub sector_enemy: bool,
    #[serde(default)]
    pub side_texture: Option<String>,
    #[serde(default)]
    pub uvs2: Vec<[f32; 2]>,
    #[serde(default)]
    pub shader_flags: u32,
    #[serde(default = "opaque_alpha")]
    pub alpha: f32,
    #[serde(default)]
    pub colors: Vec<[f32; 4]>,
    pub visible: bool,
    pub collision: bool,
}

#[derive(Component, Clone, Deserialize)]
pub struct MapAnimation {
    duration_ticks: f32,
    alpha: Vec<[f32; 2]>,
}

impl MapAnimation {
    fn opacity(&self, seconds: f32) -> f32 {
        let tick = (seconds * 4800.) % self.duration_ticks;
        for pair in self.alpha.windows(2) {
            if tick <= pair[1][0] {
                let weight = ((tick - pair[0][0]) / (pair[1][0] - pair[0][0])).clamp(0., 1.);
                return pair[0][1] + (pair[1][1] - pair[0][1]) * weight;
            }
        }
        self.alpha.last().map_or(1., |key| key[1])
    }
}

#[derive(Component)]
pub struct SectorGlow { name: String, enemy: bool }

pub fn animate_map(time: Res<Time>, animated: Query<(&MapAnimation, &MeshMaterial3d<StandardMaterial>)>, mut materials: ResMut<Assets<StandardMaterial>>, level: Res<Level>, player: Res<crate::gameplay::Player>, motion: Res<crate::gameplay::Motion>, mut glows: Query<(&SectorGlow, &mut Visibility)>, targets: Query<(&crate::combat::PracticeTarget, &Transform)>) {
    for (animation, handle) in &animated {
        if let Some(material) = materials.get_mut(handle) {
            material.base_color.set_alpha(animation.opacity(time.elapsed_secs()));
        }
    }
    let axes = Vec3::from(motion.collision_axis_scales);
    for (glow, mut visible) in &mut glows {
        let state = level.sectors.iter().find(|s| s.name == glow.name).map_or(0, |sector| {
            if targets.iter().any(|(target, transform)| target.health() > 0. && sector.intersects(transform.translation, transform.translation, axes, 0)) { 2 }
            else if player.health > 0. && sector.intersects(player.feet, player.feet, axes, 0) { 1 }
            else { 0 }
        });
        *visible = if state == if glow.enemy { 2 } else { 1 } { Visibility::Visible } else { Visibility::Hidden };
    }
}
fn opaque_alpha() -> f32 {
    1.0
}
#[derive(Deserialize)]
pub struct Spawn {
    pub name: String,
    pub position: [f32; 3],
}
#[derive(Deserialize)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Resource)]
pub struct Level {
    pub collision: CollisionWorld,
    pub spawn: Vec3,
    pub center: Vec3,
    pub min_y: f32,
    pub triangle_count: usize,
    pub death_zones: Vec<DeathZone>,
    pub local_team: u8,
    pub sectors: Vec<DeathZone>,
}

pub fn read_map_for(id: MapId) -> Result<Map, Box<dyn std::error::Error>> {
    let map: Map = serde_json::from_slice(&std::fs::read(id.asset_path())?)?;
    if map.format_version != 1 || map.units != "meters" {
        return Err("Unsupported map schema/units".into());
    }
    for mesh in &map.meshes {
        if mesh.positions.iter().flatten().any(|v| !v.is_finite())
            || mesh
                .indices
                .iter()
                .any(|&i| i as usize >= mesh.positions.len())
            || mesh.indices.len() % 3 != 0
            || mesh.normals.iter().flatten().any(|v| !v.is_finite())
            || mesh.uvs.iter().flatten().any(|v| !v.is_finite())
            || mesh.uvs2.iter().flatten().any(|v| !v.is_finite())
            || mesh.colors.iter().flatten().any(|v| !v.is_finite())
            || !mesh.alpha.is_finite()
        {
            return Err(format!("Invalid mesh {}", mesh.name).into());
        }
        if mesh.visible
            && (mesh.normals.len() != mesh.positions.len()
                || mesh.uvs.len() != mesh.positions.len())
        {
            return Err(format!("Invalid attributes {}", mesh.name).into());
        }
        if mesh.visible
            && ((!mesh.uvs2.is_empty() && mesh.uvs2.len() != mesh.positions.len())
                || (!mesh.colors.is_empty() && mesh.colors.len() != mesh.positions.len()))
        {
            return Err(format!("Invalid secondary attributes {}", mesh.name).into());
        }
        for texture in [&mesh.texture, &mesh.side_texture].into_iter().flatten() {
            if !std::path::Path::new("assets").join(texture).is_file() {
                return Err(format!("Missing texture {texture}").into());
            }
        }
        if mesh.visible && mesh.side_texture.is_some() && mesh.texture.is_none() {
            return Err(format!("Lightmapped mesh {} has no base texture", mesh.name).into());
        }
    }
    if map
        .bounds
        .min
        .iter()
        .chain(&map.bounds.max)
        .any(|v| !v.is_finite())
        || (0..3).any(|i| map.bounds.min[i] > map.bounds.max[i])
        || map
            .spawns
            .iter()
            .any(|s| s.position.iter().any(|v| !v.is_finite()))
    {
        return Err("Invalid map bounds/spawn coordinates".into());
    }
    if map.spawns.len() != id.expected_spawns()
        || !map
            .meshes
            .iter()
            .any(|m| m.visible && !m.indices.is_empty())
        || !map
            .meshes
            .iter()
            .any(|m| m.collision && !m.indices.is_empty())
    {
        return Err(format!(
            "{} export is missing its scenery, collision, or player spawns",
            id.label()
        )
        .into());
    }
    Ok(map)
}

pub fn make_level(map: &Map) -> Level {
    let triangles: Vec<[Vec3; 3]> = map
        .meshes
        .iter()
        .filter(|m| m.collision)
        .flat_map(|m| {
            m.indices.chunks_exact(3).map(|i| {
                [
                    Vec3::from(m.positions[i[0] as usize]),
                    Vec3::from(m.positions[i[1] as usize]),
                    Vec3::from(m.positions[i[2] as usize]),
                ]
            })
        })
        .collect();
    let center = (Vec3::from(map.bounds.min) + Vec3::from(map.bounds.max)) * 0.5;
    let spawn = map
        .spawns
        .iter()
        .find(|s| map.id == "neden-1" && s.name == "red_spawn_pos_11")
        .or_else(|| map.spawns.iter().find(|s| s.name.to_lowercase().contains("beta")))
        .or(map.spawns.first())
        .map(|s| Vec3::from(s.position))
        .unwrap_or(center + Vec3::Y * 5.0);
    let triangle_count = triangles.len();
    Level {
        collision: CollisionWorld::new(triangles),
        spawn: spawn + Vec3::Y * 0.05,
        center,
        min_y: map.bounds.min[1],
        triangle_count,
        death_zones: map.boxes.iter().filter(|source| source.name.contains("_limited_area")).map(DeathZone::from_box).collect(),
        local_team: LOCAL_TEAM,
        sectors: map.boxes.iter().filter(|source| source.name.starts_with("sector")).map(DeathZone::from_box).collect(),
    }
}

pub fn validate_assets() -> Result<(), Box<dyn std::error::Error>> {
    for id in MapId::ALL {
        validate_map_assets(id)?;
    }
    Ok(())
}

pub fn validate_map_assets(id: MapId) -> Result<(), Box<dyn std::error::Error>> {
    let (content, _) = crate::content::simulation::load_simulation(std::path::Path::new("assets"))?;
    let map = read_map_for(id)?;
    let level = make_level(&map);
    if level.triangle_count == 0 || map.meshes.iter().all(|m| !m.visible) {
        return Err("Empty rendered/collision map".into());
    }
    let motion: crate::gameplay::Motion =
        serde_json::from_slice(&std::fs::read("config/controller.json")?)?;
    if !motion.radius.is_finite()
        || motion.radius <= 0.0
        || !motion.height.is_finite()
        || motion.height < motion.radius * 2.0
        || !motion.gravity.is_finite()
        || motion.gravity >= 0.0
        || !motion.jump_speed.is_finite()
        || motion.jump_speed <= 0.0
        || !motion.terminal_velocity.is_finite()
        || motion.terminal_velocity >= 0.0
        || motion
            .collision_axis_scales
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        || !motion.movement_skin_fraction.is_finite()
        || motion.movement_skin_fraction < 0.0
    {
        return Err("Controller configuration has invalid capsule/jump parameters".into());
    }
    if map.spawns.len() != id.expected_spawns() {
        return Err(format!(
            "{} should export all {} original player spawns, found {}",
            id.label(),
            id.expected_spawns(),
            map.spawns.len()
        )
        .into());
    }
    let visible_triangles = map
        .meshes
        .iter()
        .filter(|m| m.visible && m.alpha >= 0.999 && m.shader_flags & 2 == 0)
        .flat_map(|m| {
            m.indices.chunks_exact(3).map(|i| {
                [
                    Vec3::from(m.positions[i[0] as usize]),
                    Vec3::from(m.positions[i[1] as usize]),
                    Vec3::from(m.positions[i[2] as usize]),
                ]
            })
        })
        .collect();
    let visible_world = CollisionWorld::new(visible_triangles);
    let dt = 1.0 / 240.0;
    let mut settled_spawns = Vec::new();
    let mut spawn_reports = Vec::new();
    for spawn in &map.spawns {
        let initial = Vec3::from(spawn.position) + Vec3::Y * 0.05;
        let mut feet = initial;
        let mut velocity = 0.0_f32;
        let mut supported_frames = 0;
        for _ in 0..1440 {
            velocity = (velocity + motion.gravity * dt).max(motion.terminal_velocity);
            let moved = level.collision.move_ellipsoid(
                feet,
                Vec3::Y * velocity * dt,
                Vec3::from(motion.collision_axis_scales),
                motion.movement_skin_fraction,
            );
            feet = moved.position;
            if !feet.is_finite() {
                return Err(
                    format!("{} produced a non-finite ellipsoid position", spawn.name).into(),
                );
            }
            if moved.grounded {
                velocity = 0.0;
                supported_frames += 1;
                if supported_frames >= 30 {
                    break;
                }
            } else {
                supported_frames = 0;
            }
        }
        if supported_frames < 30 {
            return Err(format!(
                "{} does not settle on original collision geometry",
                spawn.name
            )
            .into());
        }
        let probe_height = motion.radius + 0.1;
        let visible_distance = visible_world
            .raycast(
                feet + Vec3::Y * probe_height,
                -Vec3::Y,
                probe_height + motion.radius + 0.1,
            )
            .ok_or_else(|| {
                format!(
                    "{} settles without a nearby visible floor at {feet:?}",
                    spawn.name
                )
            })?;
        let visible_gap = visible_distance - probe_height;
        if visible_gap.abs() > motion.radius + 0.05 {
            return Err(format!(
                "{} visible/collision floor mismatch: {visible_gap:.3} m",
                spawn.name
            )
            .into());
        }
        let idle_start = feet;
        for _ in 0..240 {
            let moved = level.collision.move_ellipsoid(
                feet,
                Vec3::ZERO,
                Vec3::from(motion.collision_axis_scales),
                motion.movement_skin_fraction,
            );
            feet = moved.position;
            if !moved.grounded || !feet.is_finite() {
                return Err(format!("{} loses support while idle", spawn.name).into());
            }
        }
        let idle_drift = feet.distance(idle_start);
        if idle_drift > 0.001 {
            return Err(format!("{} idle ellipsoid drifts {idle_drift:.4} m", spawn.name).into());
        }
        println!(
            "Spawn {}: supported at ({:.3}, {:.3}, {:.3}) m; visible floor gap {:.3} m",
            spawn.name, feet.x, feet.y, feet.z, visible_gap
        );
        spawn_reports.push(serde_json::json!({ "name":spawn.name, "initial_m":initial.to_array(), "settled_m":feet.to_array(), "grounded":true, "visible_floor_gap_m":visible_gap, "idle_drift_m":idle_drift }));
        settled_spawns.push(feet);
    }

    let hz = crate::DEFAULT_PHYSICS_HZ as f32;
    let dt = 1.0 / hz;
    let vertical_skin = motion.collision_axis_scales[1] * motion.movement_skin_fraction;
    let mut expected_apex = 0.0_f32;
    for tick in 1..=(hz as usize * 3) {
        let rise = (motion.jump_speed + motion.gravity * dt * tick as f32) * dt;
        if rise <= vertical_skin {
            break;
        }
        expected_apex += rise - vertical_skin;
    }
    let expected_return = -2.0 * motion.jump_speed / motion.gravity;
    let jump_start = settled_spawns
        .iter()
        .copied()
        .find(|feet| {
            level
                .collision
                .raycast(
                    *feet + Vec3::Y * (motion.height + 0.05),
                    Vec3::Y,
                    expected_apex + 0.2,
                )
                .is_none()
        })
        .ok_or("All original spawns have insufficient headroom for a free-jump validation")?;
    let mut player = crate::gameplay::Player::new(jump_start, &motion);
    player.step(
        &content,
        &level.collision,
        &motion,
        crate::gameplay::MovementInput::default(),
        dt,
    );
    if !player.grounded {
        return Err("Free-jump validation start is unsupported".into());
    }
    let jump_start = player.feet;
    let mut apex = 0.0_f32;
    let mut landing = None;
    for tick in 1..=(hz as usize * 3) {
        player.step(
            &content,
            &level.collision,
            &motion,
            crate::gameplay::MovementInput {
                jump: tick == 1,
                ..default()
            },
            dt,
        );
        if !player.feet.is_finite() {
            return Err(format!("{hz} Hz Player::step jump produced a non-finite position").into());
        }
        apex = apex.max(player.feet.y - jump_start.y);
        if tick > 1 && player.grounded {
            landing = Some(tick as f32 * dt);
            break;
        }
    }
    let return_time =
        landing.ok_or_else(|| format!("{hz} Hz Player::step jump never returns to the floor"))?;
    let apex_tolerance = 0.025;
    if (apex - expected_apex).abs() > apex_tolerance {
        return Err(format!(
            "{hz} Hz Player::step jump apex {apex:.4} m differs from native free-path prediction {expected_apex:.4} m"
        ).into());
    }
    if (return_time - expected_return).abs() > 2.0 * dt + 0.015
        || (player.feet.y - jump_start.y).abs() > 0.05
    {
        return Err(format!(
            "{hz} Hz Player::step jump return differs from {expected_return:.3} s / original floor: {return_time:.3} s at {:?}",
            player.feet
        ).into());
    }
    println!(
        "Jump {hz} Hz (Player::step): apex {apex:.4} m, return {return_time:.3} s; native free-path prediction {expected_apex:.4} m"
    );
    let jumps = vec![serde_json::json!({
        "physics_hz":hz, "apex_m":apex, "return_seconds":return_time,
        "landing_m":player.feet.to_array(), "apex_tolerance_m":apex_tolerance,
        "integration":"shared Player::step, recreation default update rate"
    })];
    std::fs::create_dir_all("logs")?;
    std::fs::write(
        format!("logs/validation-{}.json", id.id()),
        serde_json::to_string_pretty(&serde_json::json!({
            "map":id.label(), "map_id":id.id(), "mesh_parts":map.meshes.len(), "collision_triangles":level.triangle_count,
            "actor_width_m":motion.radius*2.0, "actor_height_m":motion.height,
            "spawns":spawn_reports, "jump_native_free_predicted_apex_m":expected_apex,
            "jump_nominal_return_seconds":expected_return, "jumps":jumps,
            "collision_axis_scales_m":motion.collision_axis_scales, "movement_skin_fraction":motion.movement_skin_fraction,
            "scope":"all exported spawns plus shared Player::step at the recreation default update rate; recovered swept ellipsoid/map slide and floor passes; exact masks, actor contacts and native parity remain incomplete"
        }))?,
    )?;
    println!(
        "{} validated: {} mesh parts, {} collision triangles, all {} spawns, ellipsoid axes {:.2} / {:.2} m; default-rate free-jump fixture passes.",
        id.label(),
        map.meshes.len(),
        level.triangle_count,
        map.spawns.len(),
        motion.collision_axis_scales[0],
        motion.collision_axis_scales[1]
    );
    Ok(())
}

pub fn spawn_meshes(
    source: Vec<MapMesh>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    assets: &AssetServer,
    parent: Option<Entity>,
    mut dual_materials: Option<&mut Assets<crate::materials::DualTexture>>,
) -> Vec<(String, Handle<Mesh>)> {
    let mut handles = Vec::new();
    for mut source in source
        .into_iter()
        .filter(|m| m.visible && !m.indices.is_empty())
    {
        let usage = if parent.is_some() {
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD
        } else {
            RenderAssetUsages::RENDER_WORLD
        };
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, usage)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, source.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, source.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, source.uvs)
            .with_inserted_indices(Indices::U32(source.indices));
        if !source.colors.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, source.colors);
        }
        let has_lightmap =
            source.side_texture.is_some() && !source.uvs2.is_empty() && dual_materials.is_some();
        if has_lightmap {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, source.uvs2);
        }
        let texture_path = if source.team != 0 && source.team != LOCAL_TEAM {
            source.enemy_texture.take().or(source.texture.take())
        } else { source.texture.take() };
        let texture: Option<Handle<Image>> = texture_path.map(|path| {
            assets.load_with_settings(path, move |settings: &mut ImageLoaderSettings| {
                settings.is_srgb = !has_lightmap;
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
            })
        });
        let handle = meshes.add(mesh);
        handles.push((source.name.clone(), handle.clone()));
        if has_lightmap {
            let blended = source.alpha < 0.999 || source.shader_flags & 2 != 0;
            let lightmap = assets.load_with_settings(
                source.side_texture.unwrap(),
                |settings: &mut ImageLoaderSettings| {
                    settings.is_srgb = false;
                    settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        ..ImageSamplerDescriptor::linear()
                    });
                },
            );
            let entity = commands
                .spawn((
                    Name::new(source.name),
                    Mesh3d(handle),
                    MeshMaterial3d(dual_materials.as_mut().unwrap().add(
                        crate::materials::DualTexture {
                            base: texture.unwrap(),
                            lightmap,
                            alpha_settings: Vec4::new(
                                source.alpha,
                                if blended { 0.0 } else { 0.2 },
                                0.0,
                                0.0,
                            ),
                            alpha_mode: if blended {
                                AlphaMode::Blend
                            } else {
                                AlphaMode::Mask(0.2)
                            },
                        },
                    )),
                    Transform::default(),
                ))
                .id();
            if let Some(parent) = parent {
                commands.entity(parent).add_child(entity);
            } else {
                commands.entity(entity).insert(MapEntity);
            }
            continue;
        }
        let entity = commands
            .spawn((
                Name::new(source.name),
                Mesh3d(handle),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(1., 1., 1., source.alpha),
                    base_color_texture: texture,
                    unlit: true,
                    cull_mode: if source.shader_flags & 8 != 0 {
                        None
                    } else {
                        Some(bevy::render::render_resource::Face::Back)
                    },
                    alpha_mode: if source.shader_flags & 32 != 0 {
                        AlphaMode::Add
                    } else if source.alpha < 0.999 || source.shader_flags & 2 != 0 {
                        AlphaMode::Blend
                    } else {
                        AlphaMode::Mask(0.2)
                    },
                    ..default()
                })),
                Transform::default(),
            ))
            .id();
        if let Some(animation) = source.animation { commands.entity(entity).insert(animation); }
        if let Some(name) = source.sector { commands.entity(entity).insert((SectorGlow { name, enemy: source.sector_enemy }, Visibility::Hidden)); }
        if let Some(parent) = parent {
            commands.entity(parent).add_child(entity);
        } else {
            commands.entity(entity).insert(MapEntity);
        }
    }
    handles
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
    mut dual_materials: ResMut<Assets<crate::materials::DualTexture>>,
    options: Res<crate::Options>,
) {
    let map = read_map_for(options.map)
        .expect("Run prepare-assets.sh to prepare the original map assets");
    let level = make_level(&map);
    info!(
        "{} loaded: {} parts, {} collision triangles, spawn {:?}",
        options.map.label(),
        map.meshes.len(),
        level.triangle_count,
        level.spawn
    );
    spawn_meshes(
        map.meshes,
        &mut commands,
        &mut meshes,
        &mut materials,
        &assets,
        None,
        Some(&mut dual_materials),
    );
    commands.insert_resource(level);
    commands.insert_resource(MapSelection {
        active: options.map,
    });
}

pub fn switch_maps(
    mut requests: MessageReader<crate::chat::MapCommand>,
    mut loading: ResMut<crate::loading::Loading>,
    selection: Res<MapSelection>,
    mut chat: ResMut<crate::chat::ChatState>,
) {
    let Some(request) = requests.read().last() else { return };
    match request.map.parse::<MapId>() {
        Ok(id) if id != selection.active => loading.request_map(id),
        Ok(_) => {},
        Err(error) => chat.system(error),
    }
}

pub(crate) fn load_requested_map(
    mut commands: Commands,
    loading: Res<crate::loading::Loading>,
    old_map: Query<Entity, With<MapEntity>>,
    mut level: ResMut<Level>,
    mut selection: ResMut<MapSelection>,
    mut player: ResMut<crate::gameplay::Player>,
    mut controls: ResMut<crate::gameplay::Controls>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut dual_materials: ResMut<Assets<crate::materials::DualTexture>>,
    assets: Res<AssetServer>,
    mut chat: ResMut<crate::chat::ChatState>,
    mut windows: Query<&mut Window>,
) {
    let id = loading.pending_map.expect("Map loading request");
    let map = match read_map_for(id) {
        Ok(map) => map,
        Err(error) => {
            chat.system(format!("Cannot load {}: {error}", id.label()));
            return;
        }
    };
    let replacement = make_level(&map);
    for entity in &old_map {
        commands.entity(entity).despawn();
    }
    spawn_meshes(
        map.meshes,
        &mut commands,
        &mut meshes,
        &mut materials,
        &assets,
        None,
        Some(&mut dual_materials),
    );
    crate::gameplay::reset_for_level(&mut player, &mut controls, &replacement);
    *level = replacement;
    selection.active = id;
    for mut window in &mut windows {
        window.title = String::from("ESPERUST");
    }
    chat.system(format!("Loaded {}.", id.label()));
    info!(
        "Switched to {}: {} collision triangles",
        id.label(),
        level.triangle_count
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_death_boxes_cover_falls_and_enemy_lasers_only() {
        let axes = Vec3::new(1., 1.6, 1.);
        let circle = read_map_for(MapId::Circle2).unwrap();
        let zones: Vec<_> = circle.boxes.iter().filter(|b| b.name.contains("_limited_area")).map(DeathZone::from_box).collect();
        assert_eq!(zones.len(), 2);
        assert!(zones.iter().any(|z| z.intersects(Vec3::ZERO, Vec3::Y * -50., axes, LOCAL_TEAM)));
        assert!(!zones.iter().any(|z| z.intersects(Vec3::ZERO, Vec3::Y * -20., axes, LOCAL_TEAM)));
        let neden = read_map_for(MapId::Neden1).unwrap();
        assert!(!neden.boxes.iter().any(|b| b.name.contains("_limited_area")));
        let station = read_map_for(MapId::Station2).unwrap();
        let source = station.boxes.iter().find(|b| b.name == "alpha_limited_area02").unwrap();
        let laser = DeathZone::from_box(source);
        let feet = laser.inverse.inverse().transform_point3(Vec3::ZERO) - Vec3::Y * axes.y;
        assert!(laser.intersects(feet - Vec3::Z * 8., feet + Vec3::Z * 8., axes, 2));
        assert!(!laser.intersects(feet - Vec3::Z * 8., feet + Vec3::Z * 8., axes, 1));
        assert!(!laser.intersects(feet + Vec3::Y * 10., feet + Vec3::Y * 12., axes, 2));
    }

    #[test]
    fn map_export_keeps_intact_bridges_and_visible_station_glows() {
        let neden = read_map_for(MapId::Neden1).unwrap();
        for number in [1, 2, 3, 4] {
            assert!(neden.meshes.iter().any(|m| m.visible && m.name.contains(&format!("/ds4_light_bridge{number:02}/"))));
        }
        for fragment in ["Cylinder40", "Cylinder106", "Box2274"] {
            assert!(neden.meshes.iter().filter(|m| m.name.contains(&format!("/{fragment}/"))).all(|m| !m.visible));
        }
        let station = read_map_for(MapId::Station2).unwrap();
        let glows: Vec<_> = station.meshes.iter().filter(|m| m.name.contains("sector") && m.name.contains("_nolight_")).collect();
        assert_eq!(glows.len(), 20);
        assert!(glows.iter().all(|m| m.visible && !m.collision && m.shader_flags & 32 != 0));
        let lasers: Vec<_> = station.meshes.iter().filter(|m| m.name.starts_with("ds7_safeline.scn/") && m.team != 0).collect();
        assert_eq!(lasers.len(), 8);
        assert!(lasers.iter().all(|m| m.enemy_texture.is_some() && m.animation.is_some()));
    }

    #[test]
    fn map_command_ids_resolve_to_bundled_exports() {
        for id in MapId::ALL {
            assert_eq!(id.id().parse::<MapId>().unwrap(), id);
        }
        assert_eq!("neden1".parse::<MapId>().unwrap(), MapId::Neden1);
        assert_eq!("station_2".parse::<MapId>().unwrap(), MapId::Station2);
        assert!("../../other".parse::<MapId>().is_err());
    }
}
