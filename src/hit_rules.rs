use crate::combat_types::AttackAttributeId;
pub(crate) use crate::content::attributes::CameraShakeProfile;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Defense {
    Counter { stun_seconds: f32, crush_cue: bool },
    Block,
    Damage { crush_cue: bool },
}

pub(crate) fn defend(
    content: &crate::content::simulation::SimulationContent,
    receiver: &crate::melee::Melee,
    attribute: AttackAttributeId,
    angle_degrees: f32,
) -> Defense {
    let source = content.profile(attribute);
    let crush_cue = receiver.counter_query(angle_degrees) && source.guard_crush;
    if source.counterable {
        if let Some(milliseconds) = receiver.guard_query(angle_degrees) {
            return Defense::Counter {
                stun_seconds: milliseconds as f32 / 1000.,
                crush_cue,
            };
        }
    }
    if receiver.counter_query(angle_degrees) && !source.guard_crush {
        Defense::Block
    } else {
        Defense::Damage { crush_cue }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::simulation::parity_content;
    fn impact() -> crate::content::simulation::ImpactPresentation {
        crate::content::simulation::load_simulation(std::path::Path::new("assets"))
            .unwrap()
            .1
    }
    use crate::melee::{Attack, Melee, MeleeContext, MeleeInput, Weapon};
    use std::collections::HashMap;
    #[test]
    fn compact_impact_rules_preserve_the_source_calls() {
        let compact_text = std::fs::read_to_string("assets/melee/hit-rules.json").unwrap();
        assert!(
            compact_text.len() < 16 * 1024,
            "Impact config must not embed geometry or animation data"
        );
        let compact: serde_json::Value = serde_json::from_str(&compact_text).unwrap();
        #[derive(serde::Deserialize)]
        struct Calls {
            calls: Vec<serde_json::Value>,
        }
        #[derive(serde::Deserialize)]
        struct Effects {
            attribute_calls: Vec<serde_json::Value>,
        }
        #[derive(serde::Deserialize)]
        struct Source {
            effects: Effects,
            weapon_builders: HashMap<String, Calls>,
        }
        let source: Source = serde_json::from_reader(std::io::BufReader::new(
            std::fs::File::open("assets/melee/melee.json").unwrap(),
        ))
        .unwrap();
        let attributes: Vec<_> = source
            .effects
            .attribute_calls
            .into_iter()
            .filter(|call| {
                matches!(
                    call["method"].as_str(),
                    Some("SetCounterAttack" | "SetGuardData" | "SetDamageData")
                )
            })
            .collect();
        assert_eq!(
            compact["effects"]["attribute_calls"],
            serde_json::json!(attributes)
        );
        for name in [
            "StandardCounterSwordCounterAttack",
            "StandardSpyDaggerAttack1",
        ] {
            let shake: Vec<_> = source.weapon_builders[name]
                .calls
                .iter()
                .filter(|call| {
                    matches!(
                        call["method"].as_str(),
                        Some(
                            "SetCameraShakeEnable"
                                | "SetCameraShakeWhenSuccess"
                                | "SetCameraShakeSetFactor1"
                                | "SetCameraShakeSetFactor2"
                        )
                    )
                })
                .collect();
            assert_eq!(
                compact["weapon_builders"][name]["calls"],
                serde_json::json!(shake)
            );
        }
    }

    #[test]
    fn dagger_cut_uses_its_own_horizontal_success_shake() {
        let shake = impact().attack_shake(Attack::DaggerAir).unwrap();
        assert_eq!(shake.full_seconds, 0.2);
        assert_eq!(shake.fade_seconds, 0.05);
        assert!((shake.amplitude - 0.05).abs() < 0.000001);
        assert_eq!(shake.up_factor, 0.);
        assert_eq!(shake.right_factor, 20.);
        assert!(shake.replace);
        assert!(impact().attack_shake(Attack::DaggerDash).is_none());
        assert_eq!(
            impact()
                .attack_shake(Attack::DaggerSlash(1))
                .unwrap()
                .amplitude,
            shake.amplitude
        );
    }

    #[test]
    #[ignore = "manual cold versus warm impact-path timing; run in a fresh test process"]
    fn cold_impact_timing() {
        use std::time::Instant;
        let start = Instant::now();
        let (content, impact) =
            crate::content::simulation::load_simulation(std::path::Path::new("assets")).unwrap();
        let load_us = start.elapsed().as_micros();
        let start = Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(impact.attack_shake(Attack::CounterRetaliation).unwrap());
            std::hint::black_box(content.profile(AttackAttributeId::CounterCounterAttack));
        }
        println!(
            "impact timing: catalog_load_us={load_us} warm_1000_pairs_us={}",
            start.elapsed().as_micros()
        );
        assert!(
            impact
                .attack_shake(Attack::CounterRetaliation)
                .unwrap()
                .amplitude
                > 0.
        );
        assert!(
            content
                .profile(AttackAttributeId::CounterCounterAttack)
                .guard_crush
        );
    }

    fn guarding(age: f32) -> Melee {
        let mut melee = Melee::default();
        melee.equipped = Weapon::CounterSword;
        let context = MeleeContext {
            grounded: true,
            ..Default::default()
        };
        melee.tick(
            parity_content(),
            MeleeInput {
                secondary_held: true,
                ..Default::default()
            },
            context,
            0.016,
            100.,
        );
        melee.tick(
            parity_content(),
            MeleeInput {
                secondary_held: true,
                ..Default::default()
            },
            context,
            age,
            100.,
        );
        melee
    }
    #[test]
    fn source_defense_priority_uses_counterability_crush_and_independent_arcs() {
        let startup = guarding(0.1);
        let active = guarding(0.6);
        let weak = AttackAttributeId::CounterAttack1;
        let crush = AttackAttributeId::CounterCounterAttack;
        let counterable_crush = AttackAttributeId::CounterJumpDash;
        let block_only = AttackAttributeId::PlasmaJumpCritical;
        assert_eq!(
            defend(parity_content(), &startup, weak, 90.),
            Defense::Counter {
                stun_seconds: 3.,
                crush_cue: false
            }
        );
        assert_eq!(
            defend(parity_content(), &startup, block_only, 0.),
            Defense::Damage { crush_cue: false }
        );
        assert_eq!(
            defend(parity_content(), &active, block_only, 59.9),
            Defense::Block
        );
        assert_eq!(
            defend(parity_content(), &active, block_only, 60.),
            Defense::Damage { crush_cue: false }
        );
        assert_eq!(
            defend(parity_content(), &active, crush, 0.),
            Defense::Damage { crush_cue: true }
        );
        assert_eq!(
            defend(parity_content(), &active, counterable_crush, 0.),
            Defense::Counter {
                stun_seconds: 3.,
                crush_cue: true
            }
        );
        assert_eq!(
            defend(parity_content(), &active, weak, 90.1),
            Defense::Damage { crush_cue: false }
        );
        assert_eq!(active.attack, Some(Attack::CounterGuard));
    }
}
