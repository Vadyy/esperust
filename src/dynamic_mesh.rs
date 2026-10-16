use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct EffectMesh;

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        PostUpdate,
        update_visibility.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
}

fn update_visibility(
    meshes: Res<Assets<Mesh>>,
    mut effects: Query<(&Mesh3d, &mut Visibility), With<EffectMesh>>,
) {
    for (mesh, mut visibility) in &mut effects {
        visibility.set_if_neq(if needs_upload(&meshes, &mesh.0, false) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

pub(crate) fn needs_upload(
    meshes: &Assets<Mesh>,
    handle: &Handle<Mesh>,
    has_geometry: bool,
) -> bool {
    has_geometry
        || meshes
            .get(handle)
            .is_some_and(|mesh| mesh.indices().is_some_and(|indices| !indices.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology,
    };

    #[test]
    fn inactive_mesh_uploads_only_on_expiry_and_can_reactivate() {
        let mut meshes = Assets::<Mesh>::default();
        let handle = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
                .with_inserted_indices(Indices::U32(vec![])),
        );
        assert!(!needs_upload(&meshes, &handle, false));
        assert!(needs_upload(&meshes, &handle, true));
        meshes
            .get_mut(&handle)
            .unwrap()
            .insert_indices(Indices::U32(vec![0, 1, 2]));
        assert!(
            needs_upload(&meshes, &handle, false),
            "expired geometry must disappear"
        );
        meshes
            .get_mut(&handle)
            .unwrap()
            .insert_indices(Indices::U32(vec![]));
        assert!(!needs_upload(&meshes, &handle, false));
        assert!(
            needs_upload(&meshes, &handle, true),
            "later emissions must render"
        );
    }

    #[test]
    fn empty_effects_leave_render_queues_and_return_when_geometry_reappears() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .add_systems(Update, update_visibility);
        let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
                .with_inserted_indices(Indices::U32(vec![])),
        );
        let effect = app
            .world_mut()
            .spawn((EffectMesh, Mesh3d(handle.clone()), Visibility::Inherited))
            .id();
        let warmup = app
            .world_mut()
            .spawn((Mesh3d(handle.clone()), Visibility::Inherited))
            .id();
        for active in [false, true, false, false, true] {
            app.world_mut()
                .resource_mut::<Assets<Mesh>>()
                .get_mut(&handle)
                .unwrap()
                .insert_indices(Indices::U32(if active { vec![0, 1, 2] } else { vec![] }));
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(effect).unwrap(),
                if active {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            );
            assert_eq!(
                *app.world().get::<Visibility>(warmup).unwrap(),
                Visibility::Inherited
            );
        }
    }
}
