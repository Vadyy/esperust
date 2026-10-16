use super::{ContentError, attacks, attributes, reactions, root_motion};
use crate::combat_types::{Attack, AttackAttributeId, HitShape, HitSpec};
use std::path::Path;

#[derive(Debug, bevy::prelude::Resource)]
pub(crate) struct SimulationContent {
    reactions: reactions::ReactionTimings,
    roots: root_motion::RootMotion,
    profiles: [attributes::AttackProfile; 17],
    attacks: AttackDefinitions,
    surface_success: [bool; 2],
    emote_durations: Vec<f32>,
}

#[derive(Debug)]
struct AttackDefinitions;

#[derive(Debug, bevy::prelude::Resource)]
pub(crate) struct ImpactPresentation {
    shakes: [attributes::CameraShakeProfile; 2],
}
fn impact_index(attack: Attack) -> Option<usize> {
    match attack {
        Attack::CounterRetaliation => Some(0),
        Attack::DaggerSlash(_) | Attack::DaggerAir => Some(1),
        _ => None,
    }
}
impl ImpactPresentation {
    pub(crate) fn attack_shake(&self, attack: Attack) -> Option<attributes::CameraShakeProfile> {
        impact_index(attack).map(|i| self.shakes[i])
    }
}
impl SimulationContent {
    pub(crate) fn emote_duration(&self, index: usize) -> Option<f32> {
        self.emote_durations.get(index).copied()
    }
    pub(crate) fn duration(&self, action: reactions::ReactionAction) -> f32 {
        self.reactions.duration(action)
    }
    pub(crate) fn profile(&self, id: AttackAttributeId) -> attributes::AttackProfile {
        self.profiles[id as usize]
    }
    pub(crate) fn surface_success(&self, attack: Attack) -> bool {
        impact_index(attack).is_some_and(|i| self.surface_success[i])
    }
    pub(crate) fn hit_times(&self, attack: Attack) -> &'static [f32] {
        self.attacks.hit_times(attack)
    }
    pub(crate) fn hit_spec(&self, attack: Attack, index: usize) -> Option<HitSpec> {
        self.attacks.hit_spec(attack, index)
    }
    pub(crate) fn root_speed(
        &self,
        attack: Attack,
        previous: f32,
        current: f32,
        dt: f32,
    ) -> (f32, f32) {
        if dt <= 0. {
            return (0., 0.);
        }
        let (name, duration, multiplier) = match attack {
            Attack::PlasmaDash => ("B0000", 0.75, 2.0),
            Attack::CounterCombo(0) => ("R0000", 0.8, 1.7),
            Attack::CounterCombo(1) => ("R0001", 0.6, 1.0),
            Attack::CounterCombo(2) => ("R0002", 0.8, 1.0),
            Attack::CounterCombo(_) => ("R0003", 1.15, 1.0),
            _ => ("SN001", 1.0, 2.0),
        };
        self.roots
            .clip(name)
            .speed(previous, current, dt, duration, multiplier)
    }
}
impl AttackDefinitions {
    fn hit_times(&self, attack: Attack) -> &'static [f32] {
        attacks::hit_times(attack)
    }
    fn hit_spec(&self, attack: Attack, index: usize) -> Option<HitSpec> {
        (index < self.hit_times(attack).len())
            .then(|| attacks::hit_spec(attack, index))
            .flatten()
    }
}

pub(crate) fn load_simulation(
    root: &Path,
) -> Result<(SimulationContent, ImpactPresentation), ContentError> {
    let (mut simulation, presentation) = assemble(
        root,
        reactions::load(root)?,
        root_motion::load(root)?,
        attributes::load(root)?,
    )?;
    simulation.emote_durations = super::emotes::load(root)?
        .entries
        .iter()
        .map(|e| e.duration)
        .collect();
    Ok((simulation, presentation))
}
fn assemble(
    root: &Path,
    reactions: reactions::ReactionTimings,
    roots: root_motion::RootMotion,
    attributes: attributes::Attributes,
) -> Result<(SimulationContent, ImpactPresentation), ContentError> {
    let profiles = AttackAttributeId::ALL.map(|id| attributes.profile(id));
    for (id, profile) in AttackAttributeId::ALL.into_iter().zip(profiles) {
        if [profile.damage_time, profile.guard_time]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err(ContentError::invalid(
                root,
                "melee/hit-rules.json",
                id.source_name(),
                "duration",
                "must be finite and nonnegative",
            ));
        }
    }
    let names = [
        "StandardCounterSwordCounterAttack",
        "StandardSpyDaggerAttack1",
    ];
    let mut shakes = Vec::new();
    for name in names {
        let shake = attributes.shake(name).ok_or_else(|| {
            ContentError::invalid(
                root,
                "melee/hit-rules.json",
                name,
                "builder",
                "missing selected impact definition",
            )
        })?;
        if [
            shake.full_seconds,
            shake.fade_seconds,
            shake.amplitude,
            shake.up_factor,
            shake.right_factor,
            shake.near,
            shake.far,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || shake.full_seconds < 0.
            || shake.fade_seconds <= 0.
            || shake.near < 0.
            || shake.far <= shake.near
        {
            return Err(ContentError::invalid(
                root,
                "melee/hit-rules.json",
                name,
                "camera bounds",
                "requires finite values, nonnegative full time/near, positive fade and far > near",
            ));
        }
        shakes.push(shake);
    }
    let content = SimulationContent {
        reactions,
        roots,
        profiles,
        attacks: AttackDefinitions,
        surface_success: [true; 2],
        emote_durations: Vec::new(),
    };
    let mut selected = vec![
        Attack::PlasmaStrong,
        Attack::PlasmaDash,
        Attack::PlasmaDashStrong,
        Attack::PlasmaJump,
        Attack::DaggerDash,
        Attack::DaggerAir,
        Attack::CounterDash,
        Attack::CounterJump,
        Attack::CounterGuard,
        Attack::CounterRetaliation,
    ];
    for stage in 0..=u8::MAX {
        selected.extend([
            Attack::PlasmaWeak(stage),
            Attack::DaggerSlash(stage),
            Attack::CounterCombo(stage),
        ]);
    }
    for attack in selected {
        for (index, time) in content.hit_times(attack).iter().enumerate() {
            let spec = content
                .hit_spec(attack, index)
                .expect("scheduled code-authored hit");
            let mut values = vec![
                *time,
                spec.range,
                spec.damage,
                spec.push,
                spec.lift,
                spec.stun,
            ];
            match spec.shape {
                HitShape::Ray {
                    origin,
                    direction,
                    length,
                } => {
                    values.extend(origin);
                    values.extend(direction);
                    values.push(length);
                }
                HitShape::Cylinder {
                    origin,
                    radius,
                    height,
                } => {
                    values.extend(origin);
                    values.extend([radius, height]);
                }
                HitShape::Fan {
                    origin,
                    height,
                    inner_radius,
                    outer_radius,
                    angle,
                } => {
                    values.extend(origin);
                    values.extend([height, inner_radius, outer_radius, angle]);
                }
                HitShape::Box {
                    origin,
                    width,
                    height,
                    length,
                } => {
                    values.extend(origin);
                    values.extend([width, height, length]);
                }
            }
            if values.iter().any(|v| !v.is_finite()) || *time < 0. {
                return Err(ContentError::invalid(
                    root,
                    "code-authored attacks",
                    &format!("{attack:?}/{index}"),
                    "hit definition",
                    "must be finite with nonnegative event time",
                ));
            }
            let _ = content.profile(spec.attribute);
        }
    }
    Ok((
        content,
        ImpactPresentation {
            shakes: [shakes[0], shakes[1]],
        },
    ))
}

#[cfg(test)]
pub(crate) fn from_fixture(
    reactions: serde_json::Value,
    roots: serde_json::Value,
    attributes: serde_json::Value,
) -> Result<(SimulationContent, ImpactPresentation), ContentError> {
    assemble(
        Path::new("synthetic"),
        reactions::synthetic(reactions)?,
        root_motion::synthetic(roots)?,
        attributes::synthetic(attributes)?,
    )
}
#[cfg(test)]
pub(crate) fn synthetic() -> (SimulationContent, ImpactPresentation) {
    from_fixture(
        super::tests::synthetic_reactions(),
        super::tests::synthetic_roots(),
        super::tests::synthetic_attributes(),
    )
    .unwrap()
}

#[cfg(test)]
pub(crate) fn parity_content() -> &'static SimulationContent {
    static CONTENT: std::sync::OnceLock<SimulationContent> = std::sync::OnceLock::new();
    CONTENT.get_or_init(|| load_simulation(Path::new("assets")).unwrap().0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_catalog_runs_player_and_melee_without_files_or_playback() {
        let (content, impact) = synthetic();
        let motion = crate::gameplay::Motion::default();
        let mut player = crate::gameplay::Player::new(bevy::prelude::Vec3::ZERO, &motion);
        player.step(
            &content,
            &crate::controller::CollisionWorld::default(),
            &motion,
            crate::gameplay::MovementInput::default(),
            0.016,
        );
        let spec = content.hit_spec(Attack::CounterRetaliation, 1).unwrap();
        player.receive_melee_hit(&content, &spec, bevy::prelude::Vec3::Z);
        assert!(player.health < 100.);
        assert!(player.hit_reaction.is_some());
        assert_eq!(
            content
                .profile(AttackAttributeId::CounterAttack1)
                .damage_time,
            125.5_f32 * 0.001
        );
        assert_eq!(
            content.root_speed(Attack::PlasmaDash, 0., 0.75, 0.75),
            (8. / 0.75, 4. / 0.75)
        );
        for attack in [
            Attack::CounterRetaliation,
            Attack::DaggerSlash(255),
            Attack::DaggerAir,
            Attack::PlasmaDash,
        ] {
            assert_eq!(
                content.surface_success(attack),
                impact.attack_shake(attack).is_some()
            );
        }
        assert!(
            content
                .hit_spec(Attack::CounterCombo(0), usize::MAX)
                .is_none()
        );
    }

    #[test]
    fn catalog_matches_independent_imports_and_all_code_authored_definitions() {
        let root = Path::new("assets");
        let (catalog, impact) = load_simulation(root).unwrap();
        let attributes = attributes::load(root).unwrap();
        let reactions = reactions::load(root).unwrap();
        for id in AttackAttributeId::ALL {
            assert_eq!(catalog.profile(id), attributes.profile(id));
        }
        for action in reactions::ReactionAction::ALL {
            assert_eq!(catalog.duration(action), reactions.duration(action));
        }
        assert_eq!(
            impact.attack_shake(Attack::CounterRetaliation),
            attributes.shake("StandardCounterSwordCounterAttack")
        );
        assert_eq!(
            impact.attack_shake(Attack::DaggerAir),
            attributes.shake("StandardSpyDaggerAttack1")
        );
        for stage in 0..=255 {
            for attack in [
                Attack::CounterCombo(stage),
                Attack::PlasmaWeak(stage),
                Attack::DaggerSlash(stage),
            ] {
                assert_eq!(catalog.hit_times(attack), attacks::hit_times(attack));
                for index in 0..catalog.hit_times(attack).len() {
                    assert_eq!(
                        catalog.hit_spec(attack, index),
                        attacks::hit_spec(attack, index)
                    );
                }
            }
        }
    }
}
