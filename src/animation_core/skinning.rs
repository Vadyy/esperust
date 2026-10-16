use bevy::prelude::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub(crate) struct SkinMesh {
    pub name: String,
    pub bind_positions: Vec<[f32; 3]>,
    pub bind_normals: Vec<[f32; 3]>,
    pub skin: Vec<Vec<SkinWeight>>,
    pub joints: Vec<Joint>,
    pub attach_bone: Option<usize>,
    pub attach_matrix: Option<[f32; 16]>,
}
#[derive(Clone, Deserialize)]
pub(crate) struct SkinWeight {
    pub joint: usize,
    pub weight: f32,
}
#[derive(Clone, Deserialize)]
pub(crate) struct Joint {
    pub bone: usize,
    pub inverse_bind: [f32; 16],
}
#[derive(Clone, Deserialize)]
pub(crate) struct Bone {
    pub name: String,
    pub parent: Option<usize>,
}
#[derive(Clone, Deserialize)]
pub(crate) struct CharacterSkin {
    pub bones: Vec<Bone>,
    pub meshes: Vec<SkinMesh>,
}

impl CharacterSkin {
    pub(crate) fn validate_and_normalize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (index, bone) in self.bones.iter().enumerate() {
            if bone.parent.is_some_and(|parent| parent >= index) {
                return Err(format!("Invalid skeleton hierarchy at {}", bone.name).into());
            }
        }
        for mesh in &mut self.meshes {
            if mesh.bind_positions.len() != mesh.bind_normals.len()
                || mesh.bind_positions.len() != mesh.skin.len()
                || mesh.joints.iter().any(|j| j.bone >= self.bones.len())
                || mesh.attach_bone.is_some_and(|b| b >= self.bones.len())
                || (mesh.attach_bone.is_some() && mesh.attach_matrix.is_none())
            {
                return Err("Invalid animation skin geometry".into());
            }
            for weights in &mut mesh.skin {
                let total: f32 = weights.iter().map(|w| w.weight).sum();
                if weights
                    .iter()
                    .any(|w| w.joint >= mesh.joints.len() || !w.weight.is_finite() || w.weight < 0.)
                    || (!weights.is_empty() && total <= 0.)
                    || (weights.is_empty() && mesh.attach_bone.is_none())
                {
                    return Err("Invalid animation skin weights".into());
                }
                for weight in weights {
                    weight.weight /= total;
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn skin_mesh(mesh: &SkinMesh, world: &[Mat4]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let joints: Vec<Mat4> = mesh
        .joints
        .iter()
        .map(|joint| world[joint.bone] * Mat4::from_cols_array(&joint.inverse_bind))
        .collect();
    let attach = mesh
        .attach_bone
        .map(|bone| world[bone] * Mat4::from_cols_array(mesh.attach_matrix.as_ref().unwrap()));
    let mut positions = Vec::with_capacity(mesh.bind_positions.len());
    let mut normals = Vec::with_capacity(mesh.bind_positions.len());
    for ((position, normal), weights) in mesh
        .bind_positions
        .iter()
        .zip(&mesh.bind_normals)
        .zip(&mesh.skin)
    {
        let position = Vec3::from_array(*position);
        let normal = Vec3::from_array(*normal);
        let (position, normal) = if weights.is_empty() {
            let matrix = attach.unwrap();
            (
                matrix.transform_point3(position),
                matrix.transform_vector3(normal),
            )
        } else {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::ZERO;
            for weight in weights {
                let matrix = joints[weight.joint];
                p += matrix.transform_point3(position) * weight.weight;
                n += matrix.transform_vector3(normal) * weight.weight;
            }
            (p, n)
        };
        positions.push(position.to_array());
        normals.push(normal.normalize_or_zero().to_array());
    }
    (positions, normals)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_weight_normalization_and_unweighted_attachment_keep_distinct_paths() {
        let mut skin = CharacterSkin {
            bones: vec![Bone {
                name: "root".into(),
                parent: None,
            }],
            meshes: vec![SkinMesh {
                name: "fixture".into(),
                bind_positions: vec![[1., 0., 0.]; 2],
                bind_normals: vec![[0., 2., 0.]; 2],
                skin: vec![
                    vec![
                        SkinWeight {
                            joint: 0,
                            weight: 2.,
                        },
                        SkinWeight {
                            joint: 1,
                            weight: 6.,
                        },
                    ],
                    vec![],
                ],
                joints: vec![
                    Joint {
                        bone: 0,
                        inverse_bind: Mat4::IDENTITY.to_cols_array(),
                    },
                    Joint {
                        bone: 0,
                        inverse_bind: Mat4::from_translation(Vec3::X * 4.).to_cols_array(),
                    },
                ],
                attach_bone: Some(0),
                attach_matrix: Some(Mat4::from_translation(Vec3::Z * 2.).to_cols_array()),
            }],
        };
        skin.validate_and_normalize().unwrap();
        assert_eq!(skin.meshes[0].skin[0][0].weight, 0.25);
        let (positions, normals) =
            skin_mesh(&skin.meshes[0], &[Mat4::from_translation(Vec3::Y * 3.)]);
        assert_eq!(positions, vec![[4., 3., 0.], [1., 3., 2.]]);
        assert_eq!(normals, vec![[0., 1., 0.]; 2]);
        skin.meshes[0].skin[0][0].joint = 2;
        assert!(skin.validate_and_normalize().is_err());
    }
}
