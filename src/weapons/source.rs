use super::*;

#[derive(Deserialize)]
pub(super) struct Manifest {
    pub(super) format_version: u32,
    pub(super) weapons: HashMap<String, WeaponSource>,
    #[serde(default)]
    pub(super) effects: EffectManifest,
    #[serde(default)]
    pub(super) audio: HashMap<String, AudioSourceRecord>,
    #[serde(default)]
    pub(super) builders: HashMap<String, Builder>,
    #[serde(default)]
    pub(super) weapon_builders: HashMap<String, Builder>,
    #[serde(default)]
    pub(super) ghosttrails: HashMap<String, GhostTrailSource>,
    #[serde(default)]
    pub(super) ui: HashMap<String, UiSource>,
}

#[derive(Deserialize)]
pub(super) struct UiSource {
    pub(super) path: String,
    #[serde(rename = "size")]
    pub(super) _size: [f32; 2],
}
#[derive(Deserialize)]
pub(super) struct GhostTrailSource {
    #[serde(default)]
    pub(super) part: Option<String>,
    pub(super) texture: String,
    #[serde(default)]
    pub(super) haze: bool,
    #[serde(default)]
    pub(super) haze_texture: Option<String>,
    pub(super) points: [[f32; 3]; 2],
    pub(super) lifetime_ms: f32,
    #[serde(rename = "unit_length_m")]
    pub(super) _unit_length_m: f32,
    pub(super) fade_in_ms: f32,
    pub(super) fade_out_ms: f32,
}
#[derive(Deserialize)]
pub(super) struct Builder {
    pub(super) calls: Vec<BuilderCall>,
}
#[derive(Deserialize)]
pub(super) struct BuilderCall {
    pub(super) method: String,
    #[serde(default)]
    pub(super) target: String,
    pub(super) arguments: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
pub(super) struct AudioSourceRecord {
    #[serde(rename = "source")]
    pub(super) _source: String,
    pub(super) path: String,
}

#[derive(Default, Deserialize)]
pub(super) struct EffectManifest {
    #[serde(default)]
    pub(super) sequences: HashMap<String, Sequence>,
    #[serde(default)]
    pub(super) models: HashMap<String, MeshAnimation>,
    #[serde(default)]
    pub(super) attribute_calls: Vec<BuilderCall>,
}

#[derive(Deserialize)]
pub(super) struct Sequence {
    #[serde(default)]
    pub(super) flags: [u8; 2],
    pub(super) duration_ms: u32,
    pub(super) components: Vec<EffectComponent>,
}
#[derive(Deserialize)]
pub(super) struct EffectComponent {
    #[serde(default)]
    pub(super) animation_loop: bool,
    #[serde(default)]
    pub(super) fade_in_ms: u32,
    #[serde(default)]
    pub(super) fade_out_ms: u32,
    #[serde(default)]
    pub(super) flags: [u32; 2],
    #[serde(default)]
    pub(super) actor_transform_sources: Option<[u32; 3]>,
    #[serde(default)]
    pub(super) self_transform_sources: Option<[u32; 3]>,
    pub(super) start_ms: u32,
    pub(super) end_ms: u32,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) particle: Option<ParticleDefinition>,
    #[serde(default)]
    pub(super) asset_model: Option<String>,
}

impl EffectComponent {
    pub(super) fn opacity(&self, age_ms: f32) -> f32 {
        if age_ms < self.start_ms as f32 {
            return 0.;
        }
        if age_ms <= self.end_ms as f32 {
            if self.fade_in_ms == 0 {
                1.
            } else {
                ((age_ms - self.start_ms as f32) / self.fade_in_ms as f32).min(1.)
            }
        } else if self.fade_out_ms == 0 {
            0.
        } else {
            (1. - (age_ms - self.end_ms as f32) / self.fade_out_ms as f32).max(0.)
        }
    }
}
#[derive(Deserialize)]
pub(super) struct MeshAnimation {
    pub(super) duration_ticks: f64,
    pub(super) ticks_per_ms: f64,
    pub(super) meshes: Vec<AnimatedMesh>,
    #[serde(default)]
    pub(super) trail_frames: Vec<TrailFrame>,
    #[serde(default)]
    pub(super) node_frames: HashMap<String, Vec<NodeFrame>>,
}
#[derive(Deserialize)]
pub(super) struct NodeFrame {
    pub(super) tick: f32,
    pub(super) matrix: [f32; 16],
}
#[derive(Deserialize)]
pub(super) struct TrailFrame {
    pub(super) tick: f32,
    pub(super) points: [[f32; 3]; 2],
}
#[derive(Deserialize)]
pub(super) struct AnimatedMesh {
    pub(super) name: String,
    pub(super) texture: String,
    #[serde(default)]
    pub(super) shader_flags: u32,
    pub(super) indices: Vec<u32>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) frames: Vec<MeshFrame>,
}
#[derive(Deserialize)]
pub(super) struct MeshFrame {
    pub(super) tick: f32,
    pub(super) alpha: f32,
    pub(super) positions: Vec<[f32; 3]>,
    #[serde(default)]
    pub(super) normals: Vec<[f32; 3]>,
}

#[derive(Clone, Deserialize)]
pub(super) struct ParticleDefinition {
    pub(super) render_type: u32,
    #[serde(rename = "blend_type")]
    pub(super) _blend_type: u32,
    pub(super) max_particles: usize,
    pub(super) lifetime_var: u32,
    pub(super) speed_var: f32,
    pub(super) size_x_var: f32,
    pub(super) size_y_var: f32,
    pub(super) size_var: f32,
    pub(super) rotate_var: f32,
    pub(super) emitter_keys: Vec<EmitterKey>,
    pub(super) particle_keys: Vec<[f32; 8]>,
    pub(super) textures: Vec<EffectTexture>,
}
#[derive(Clone, Deserialize)]
pub(super) struct EmitterKey {
    pub(super) continuance: u8,
    pub(super) position: [f32; 3],
    pub(super) cone_direction: [f32; 3],
    pub(super) cone_theta_degrees: f32,
    pub(super) speed: f32,
    pub(super) emitter_type: u32,
    pub(super) min_width: f32,
    pub(super) max_width: f32,
    pub(super) min_height: f32,
    pub(super) max_height: f32,
    pub(super) gravity: [f32; 3],
    pub(super) attractive: f32,
    pub(super) lifetime_ms: u32,
    pub(super) emit_per_second: u32,
}
#[derive(Clone, Deserialize)]
pub(super) struct EffectTexture {
    pub(super) path: String,
    pub(super) uv: [f32; 8],
}

#[derive(Deserialize)]
pub(super) struct WeaponSource {
    #[serde(default)]
    pub(super) parts: Vec<WeaponPart>,
    #[serde(default)]
    pub(super) meshes: Vec<scene::MapMesh>,
    #[serde(default)]
    pub(super) attach_bone: String,
    #[serde(default = "identity")]
    pub(super) attach_matrix: [f32; 16],
    #[serde(default)]
    pub(super) holster_bone: Option<String>,
    #[serde(default)]
    pub(super) holster_matrix: Option<[f32; 16]>,
    #[serde(default)]
    pub(super) mesh_animations: HashMap<String, MeshAnimation>,
}

#[derive(Deserialize)]
pub(super) struct WeaponPart {
    pub(super) key: String,
    #[serde(flatten)]
    pub(super) source: WeaponSource,
}

impl WeaponSource {
    pub(super) fn part(&self, part: Option<usize>) -> &Self {
        part.map_or(self, |index| &self.parts[index].source)
    }
    pub(super) fn trail_part(&self, part: Option<&str>) -> &Self {
        part.map_or(self, |key| {
            &self
                .parts
                .iter()
                .find(|part| part.key == key)
                .expect("Registered trail must name an imported equipment part")
                .source
        })
    }
}

pub(super) fn identity() -> [f32; 16] {
    Mat4::IDENTITY.to_cols_array()
}

impl Manifest {
    pub(super) fn action_calls(&self, attack: Attack) -> impl Iterator<Item = &BuilderCall> {
        let gameplay = match attack {
            Attack::PlasmaWeak(_) => "StandardPlasmaSwordStandAttack_Weak",
            Attack::PlasmaStrong => "StandardPlasmaSwordStandAttack_Strong",
            Attack::PlasmaDashStrong => "StandardPlasmaSwordStandAttack_Strong1",
            Attack::PlasmaDash => "StandardPlasmaSwordAttack2",
            Attack::PlasmaJump => "StandardPlasmaSwordJumpAttack",
            Attack::DaggerSlash(_) | Attack::DaggerAir => "StandardSpyDaggerAttack1",
            Attack::DaggerDash => "StandardSpyDaggerAttack2",
            Attack::CounterDash | Attack::CounterGuard => "",
            Attack::CounterCombo(0) => "StandardCounterSwordAfterDash1",
            Attack::CounterCombo(1) => "StandardCounterSwordAfterDash2",
            Attack::CounterCombo(2) => "StandardCounterSwordAfterDash3",
            Attack::CounterCombo(_) => "StandardCounterSwordAfterDash4",
            Attack::CounterJump => "StandardCounterSwordJumpDash",
            Attack::CounterRetaliation => "StandardCounterSwordCounterAttack",
        };
        self.builders
            .get(attack_builder(attack))
            .into_iter()
            .chain(self.weapon_builders.get(gameplay))
            .flat_map(|builder| &builder.calls)
    }
}

pub(super) fn attack_builder(attack: Attack) -> &'static str {
    match attack {
        Attack::PlasmaWeak(_) => "SwordAtkWeakState_PlasmaSword",
        Attack::PlasmaStrong => "SwordAtkStrongState_PlasmaSword",
        Attack::PlasmaDashStrong => "SwordAtkStrong1State_PlasmaSword",
        Attack::PlasmaDash => "UseWeapon2State_PlasmaSword",
        Attack::PlasmaJump => "SwordJumpAtkState_PlasmaSword",
        Attack::DaggerSlash(_) => "UseWeapon1State_SpyDagger",
        Attack::DaggerDash => "UseWeapon2State_SpyDagger",
        Attack::DaggerAir => "UseWeapon1State_SpyDagger",
        Attack::CounterDash => "UseWeapon1State_CounterSwordDash1",
        Attack::CounterCombo(0) => "SwordAtkAfterDash1State_CounterSword",
        Attack::CounterCombo(1) => "SwordAtkAfterDash2State_CounterSword",
        Attack::CounterCombo(2) => "SwordAtkAfterDash3State_CounterSword",
        Attack::CounterCombo(_) => "SwordAtkAfterDash4State_CounterSword",
        Attack::CounterJump => "SwordJumpAtkState_CounterSword",
        Attack::CounterGuard => "UseWeapon2State_CounterSwordGuard",
        Attack::CounterRetaliation => "SwordAtkCounterAttack_CounterSword",
    }
}
