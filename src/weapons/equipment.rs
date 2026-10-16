use super::*;

#[derive(Component)]
pub(crate) struct WeaponModel {
    pub(super) key: String,
    pub(super) part: Option<usize>,
}

pub(crate) fn weapon_key(weapon: Weapon) -> Option<&'static str> {
    match weapon {
        Weapon::Unarmed => None,
        Weapon::PlasmaSword => Some("plasma"),
        Weapon::Dagger => Some("dagger"),
        Weapon::CounterSword => Some("counter"),
    }
}

pub(super) fn equipment_holstered(
    player: &Player,
    motion: &gameplay::Motion,
    source: &WeaponSource,
) -> bool {
    plasma_holstered(player, motion)
        || (player.melee.equipped == Weapon::CounterSword
            && source.holster_bone.is_some()
            && movement_holstered(player, motion))
}

pub(super) fn plasma_holstered(player: &Player, motion: &gameplay::Motion) -> bool {
    player.melee.equipped == Weapon::PlasmaSword && movement_holstered(player, motion)
}

pub(super) fn movement_holstered(player: &Player, motion: &gameplay::Motion) -> bool {
    let landing_time = if player.walljump_landing {
        motion.walljump_landing_time
    } else if player.fall_landing {
        motion.fall_landing_time
    } else {
        motion.land_delay
    };
    player.melee.attack.is_none()
        && (!player.grounded
            || player.action != gameplay::MovementAction::None
            || player.landing_age < landing_time)
}

pub(super) fn sample_mesh(source: &AnimatedMesh, tick: f32) -> (Vec<[f32; 3]>, f32) {
    let (first, second, weight) =
        crate::effect_core::frame_span(&source.frames, tick, |frame| frame.tick);
    let a = &source.frames[first];
    let b = &source.frames[second];
    (
        a.positions
            .iter()
            .zip(&b.positions)
            .map(|(a, b)| {
                Vec3::from_array(*a)
                    .lerp(Vec3::from_array(*b), weight)
                    .to_array()
            })
            .collect(),
        a.alpha + (b.alpha - a.alpha) * weight,
    )
}

pub(super) fn sample_mesh_normals(source: &AnimatedMesh, tick: f32) -> Option<Vec<[f32; 3]>> {
    let (first, second, weight) =
        crate::effect_core::frame_span(&source.frames, tick, |frame| frame.tick);
    let a = &source.frames[first];
    let b = &source.frames[second];
    if a.normals.len() != a.positions.len() || b.normals.len() != b.positions.len() {
        return None;
    }
    Some(
        a.normals
            .iter()
            .zip(&b.normals)
            .map(|(a, b)| {
                Vec3::from_array(*a)
                    .lerp(Vec3::from_array(*b), weight)
                    .normalize_or_zero()
                    .to_array()
            })
            .collect(),
    )
}

pub(super) fn update_weapon_meshes(
    weapons: &Weapons,
    animation: &Animation,
    meshes: &mut Assets<Mesh>,
) {
    for ((key, part), handles) in &weapons.weapon_meshes {
        let clip = animation.upper_cursor().and_then(|(name, tick)| {
            weapons.manifest.weapons[key]
                .part(*part)
                .mesh_animations
                .get(name)
                .map(|source| (source, tick))
        });
        for (name, handle, bind, normals) in handles {
            let Some(mesh) = meshes.get_mut(handle) else {
                continue;
            };
            let source = clip.and_then(|(clip, tick)| {
                clip.meshes
                    .iter()
                    .find(|source| source.name == *name)
                    .map(|source| (source, tick))
            });
            let positions = source.map_or_else(
                || bind.clone(),
                |(source, tick)| sample_mesh(source, tick as f32).0,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            let sampled_normals =
                source.and_then(|(source, tick)| sample_mesh_normals(source, tick as f32));
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_NORMAL,
                sampled_normals.unwrap_or_else(|| normals.clone()),
            );
        }
    }
}

#[derive(Component)]
pub(super) struct PracticeEquipment {
    pub(super) owner: Entity,
    pub(super) source: WeaponSource,
    pub(super) meshes: Vec<(String, Handle<Mesh>, Vec<[f32; 3]>, Vec<[f32; 3]>)>,
}

pub(crate) fn spawn_practice_equipment(
    owner: Entity,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    assets: &AssetServer,
) {
    #[derive(Deserialize)]
    struct EquipmentManifest {
        weapons: HashMap<String, WeaponSource>,
    }
    let mut manifest: EquipmentManifest = serde_json::from_slice(
        &std::fs::read("assets/melee/melee.json").expect("Sparring requires checked melee assets"),
    )
    .unwrap();
    let mut source = manifest.weapons.remove("counter").unwrap();
    for part in std::mem::take(&mut source.parts) {
        let mut source = part.source;
        let root = commands
            .spawn((
                Name::new(format!("Sparring Counter {}", part.key)),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        commands.entity(owner).add_child(root);
        let geometry = std::mem::take(&mut source.meshes);
        let bind: HashMap<_, _> = geometry
            .iter()
            .map(|mesh| {
                (
                    mesh.name.clone(),
                    (mesh.positions.clone(), mesh.normals.clone()),
                )
            })
            .collect();
        let handles = scene::spawn_meshes(
            geometry,
            commands,
            meshes,
            materials,
            assets,
            Some(root),
            None,
        );
        let handles = handles
            .into_iter()
            .map(|(name, handle)| {
                let (positions, normals) = bind[&name].clone();
                (name, handle, positions, normals)
            })
            .collect();
        commands.entity(root).insert(PracticeEquipment {
            owner,
            source,
            meshes: handles,
        });
    }
}

pub(super) fn update_practice_equipment(
    practice: Res<crate::practice_animation::PracticeAssets>,
    actors: Query<&crate::practice_animation::PracticeAnimator>,
    mut equipment: Query<(&mut Transform, &PracticeEquipment)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (mut transform, equipment) in &mut equipment {
        let Ok(actor) = actors.get(equipment.owner) else {
            continue;
        };
        let source = &equipment.source;
        if let Some(bone) = actor.bone_matrix(&practice, &source.attach_bone) {
            *transform =
                Transform::from_matrix(bone * Mat4::from_cols_array(&source.attach_matrix));
        }
        let (clip, tick) = actor.clip_cursor(&practice);
        let local = source.mesh_animations.get(clip);
        for (name, handle, bind, normals) in &equipment.meshes {
            let Some(mesh) = meshes.get_mut(handle) else {
                continue;
            };
            let local = local.and_then(|clip| clip.meshes.iter().find(|mesh| mesh.name == *name));
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                local.map_or_else(|| bind.clone(), |source| sample_mesh(source, tick as f32).0),
            );
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_NORMAL,
                local
                    .and_then(|source| sample_mesh_normals(source, tick as f32))
                    .unwrap_or_else(|| normals.clone()),
            );
        }
    }
}
