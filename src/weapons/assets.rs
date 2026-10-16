use super::*;

pub(super) fn asset_stem(source: &str) -> String {
    source
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(source)
        .split('.')
        .next()
        .unwrap_or(source)
        .to_ascii_lowercase()
}

pub(super) fn empty_mesh() -> Mesh {
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

pub(super) fn effect_material(
    materials: &mut Assets<StandardMaterial>,
    assets: &AssetServer,
    texture: &str,
    alpha_mode: AlphaMode,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color_texture: Some(assets.load(texture.to_owned())),
        unlit: true,
        alpha_mode,
        cull_mode: None,
        double_sided: true,
        ..default()
    })
}

pub(super) fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut haze_materials: ResMut<Assets<crate::materials::TrailHaze>>,
    assets: Res<AssetServer>,
    avatar: Single<Entity, With<Avatar>>,
    mut stats: ResMut<WeaponStats>,
) {
    let manifest: Manifest = match std::fs::read("assets/melee/melee.json")
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        Some(value) => value,
        None => {
            warn!("Melee source export unavailable; run tools/asset_melee.py");
            return;
        }
    };
    assert_eq!(manifest.format_version, 1);
    let mut manifest = manifest;
    #[derive(Deserialize)]
    struct ReactionManifest {
        effects: EffectManifest,
        audio: HashMap<String, AudioSourceRecord>,
    }
    let reactions: ReactionManifest = serde_json::from_slice(
        &std::fs::read("assets/melee/reactions.json").expect("Export reaction effects"),
    )
    .expect("Invalid original reaction effects");
    manifest
        .effects
        .sequences
        .extend(reactions.effects.sequences);
    manifest.effects.models.extend(reactions.effects.models);
    manifest.audio.extend(reactions.audio);
    let mut weapon_meshes = HashMap::new();
    for (key, equipment) in &mut manifest.weapons {
        let part_count = equipment.parts.len();
        let parts: Vec<_> = if part_count == 0 {
            vec![(None, equipment)]
        } else {
            equipment
                .parts
                .iter_mut()
                .enumerate()
                .map(|(index, part)| (Some(index), &mut part.source))
                .collect()
        };
        for (part, source) in parts {
            let root = commands
                .spawn((
                    Name::new(format!("Original {key} equipment")),
                    WeaponModel {
                        key: key.clone(),
                        part,
                    },
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id();
            commands.entity(*avatar).add_child(root);
            let parts = std::mem::take(&mut source.meshes);
            let bind: HashMap<_, _> = parts
                .iter()
                .map(|part| {
                    (
                        part.name.clone(),
                        (part.positions.clone(), part.normals.clone()),
                    )
                })
                .collect();
            let handles = scene::spawn_meshes(
                parts,
                &mut commands,
                &mut meshes,
                &mut materials,
                &assets,
                Some(root),
                None,
            );
            if !handles.is_empty() {
                stats.models_loaded += 1;
            }
            weapon_meshes.insert(
                (key.clone(), part),
                handles
                    .into_iter()
                    .map(|(name, handle)| {
                        let (positions, normals) = bind[&name].clone();
                        (name, handle, positions, normals)
                    })
                    .collect(),
            );
        }
    }
    let sounds = manifest
        .audio
        .iter()
        .map(|(stem, source)| (stem.clone(), assets.load(source.path.clone())))
        .collect();
    let mut effect_meshes = HashMap::new();
    for (name, model) in &manifest.effects.models {
        let mut handles = Vec::new();
        for source in &model.meshes {
            let mesh = meshes.add(empty_mesh());
            commands.spawn((
                Name::new(format!("Original {name} effect")),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(effect_material(
                    &mut materials,
                    &assets,
                    &source.texture,
                    if source.shader_flags & 32 != 0 {
                        AlphaMode::Add
                    } else {
                        AlphaMode::Blend
                    },
                )),
                Transform::default(),
                NoFrustumCulling,
                crate::dynamic_mesh::EffectMesh,
            ));
            handles.push(mesh);
        }
        effect_meshes.insert(name.clone(), handles);
    }
    let mut particle_templates = Vec::new();
    for (name, sequence) in &manifest.effects.sequences {
        for (index, component) in sequence.components.iter().enumerate() {
            let Some(source) = &component.particle else {
                continue;
            };
            let mut handles = Vec::new();
            for texture in &source.textures {
                let mesh = meshes.add(empty_mesh());
                commands.spawn((
                    Name::new(format!("Original {name} particles")),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(effect_material(
                        &mut materials,
                        &assets,
                        &texture.path,
                        AlphaMode::Add,
                    )),
                    Transform::default(),
                    NoFrustumCulling,
                    crate::dynamic_mesh::EffectMesh,
                ));
                handles.push(mesh);
            }
            particle_templates.push(ParticleTemplate {
                sequence: name.clone(),
                component: index,
                source: source.clone(),
                meshes: handles,
            });
        }
    }
    let mut ghost_meshes = HashMap::new();
    for (name, source) in &manifest.ghosttrails {
        let mut handles = Vec::new();
        for texture in [&source.texture] {
            let mesh = meshes.add(empty_mesh());
            commands.spawn((
                Name::new(format!("Original {name} ribbon")),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(effect_material(
                    &mut materials,
                    &assets,
                    texture,
                    AlphaMode::Add,
                )),
                Transform::default(),
                NoFrustumCulling,
                crate::dynamic_mesh::EffectMesh,
            ));
            handles.push(mesh);
        }
        if source.haze {
            let mesh = meshes.add(empty_mesh());
            let path = source
                .haze_texture
                .as_ref()
                .unwrap_or(&source.texture)
                .clone();
            let gradient = assets.load_with_settings(
                path,
                |settings: &mut bevy::image::ImageLoaderSettings| {
                    settings.is_srgb = false;
                    settings.sampler = bevy::image::ImageSampler::linear();
                },
            );
            commands.spawn((
                Name::new(format!("Original {name} haze")),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(haze_materials.add(crate::materials::TrailHaze { gradient })),
                Transform::default(),
                NoFrustumCulling,
                crate::dynamic_mesh::EffectMesh,
            ));
            handles.push(mesh);
            stats.haze_passes_loaded += 1;
        }
        ghost_meshes.insert(name.clone(), handles);
    }
    setup_weapon_ui(&mut commands, &assets, &manifest);
    commands.insert_resource(Weapons {
        manifest,
        last_serial: 0,
        last_reset: 0,
        last_player_death: 0,
        last_player_hit: 0,
        last_attack: None,
        sounds,
        sound_events: Vec::new(),
        age: 0.,
        weapon_meshes,
        effect_meshes,
        particle_templates,
        particles: Vec::new(),
        live_sequences: Vec::new(),
        sequence_events: Vec::new(),
        random: 0x51e9bd31,
        ghost_meshes,
        trails: Vec::new(),
        active_id: 0,
        clock: 0.,
        actor_presentations: HashMap::new(),
    });
}
