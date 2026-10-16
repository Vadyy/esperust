use crate::combat_types::{Attack, AttackAttributeId, HitShape, HitSpec};

pub(crate) fn attribute_id(attack: Attack) -> Option<AttackAttributeId> {
    Some(match attack {
        Attack::PlasmaWeak(_) => AttackAttributeId::PlasmaStandWeak,
        Attack::PlasmaStrong | Attack::PlasmaDashStrong => AttackAttributeId::PlasmaStandStrong,
        Attack::PlasmaDash => AttackAttributeId::PlasmaAttack2Weak,
        Attack::PlasmaJump => AttackAttributeId::PlasmaJump,
        Attack::DaggerSlash(_) | Attack::DaggerAir => AttackAttributeId::DaggerAttack2,
        Attack::DaggerDash => AttackAttributeId::DaggerAttack1,
        Attack::CounterDash | Attack::CounterGuard => return None,
        Attack::CounterCombo(0) => AttackAttributeId::CounterAttack1,
        Attack::CounterCombo(1) => AttackAttributeId::CounterAttack2,
        Attack::CounterCombo(2) => AttackAttributeId::CounterAttack3,
        Attack::CounterCombo(_) => AttackAttributeId::CounterAttack4,
        Attack::CounterJump => AttackAttributeId::CounterJumpDash,
        Attack::CounterRetaliation => AttackAttributeId::CounterCounterAttack,
    })
}

pub(crate) fn hit_times(attack: Attack) -> &'static [f32] {
    match attack {
        Attack::PlasmaWeak(_) => &[0.2],
        Attack::PlasmaStrong | Attack::PlasmaDashStrong => &[0.4],
        Attack::PlasmaDash => &[0.2, 0.4, 0.5],
        Attack::PlasmaJump => &[0.5],
        Attack::DaggerSlash(_) | Attack::DaggerAir => &[0.26],
        Attack::DaggerDash => &[0.18, 0.36, 0.58],
        Attack::CounterDash | Attack::CounterGuard => &[],
        Attack::CounterCombo(0) => &[0.35, 0.1, 0.35],
        Attack::CounterCombo(1) => &[0.25, 0.1],
        Attack::CounterCombo(2) => &[0.3, 0.3],
        Attack::CounterCombo(_) => &[0.65, 0.05, 0.65],
        Attack::CounterJump => &[0.6, 0.6],
        Attack::CounterRetaliation => &[0.35, 0.35],
    }
}

pub(crate) fn hit_spec(attack: Attack, index: usize) -> Option<HitSpec> {
    let attribute = attribute_id(attack)?;
    let mut spec = HitSpec {
        hit_index: index as u8,
        range: 3.5,
        damage: 17.0,
        push: 0.0,
        lift: 0.0,
        stun: 0.0,
        shape: HitShape::Fan {
            origin: [0.0, 1.4, 0.5],
            height: 2.0,
            inner_radius: 0.5,
            outer_radius: 3.5,
            angle: 120.0_f32.to_radians(),
        },
        attribute,
        critical: false,
    };
    match attack {
        Attack::CounterDash | Attack::CounterGuard => return None,
        Attack::CounterCombo(stage) => {
            spec.damage = [15., 30., 30., 22.5][stage.min(3) as usize];
            spec.range = if stage >= 2 { 5. } else { 3.5 };
            spec.shape = HitShape::Box {
                origin: [0., 2.5, 1.],
                width: 3.,
                height: 3.5,
                length: spec.range,
            };
        }
        Attack::CounterJump => {
            spec.damage = 45.;
            spec.range = 5.;
            spec.push = 20.;
            spec.shape = HitShape::Cylinder {
                origin: [0., 0., 0.],
                radius: 5.,
                height: 6.,
            };
        }
        Attack::CounterRetaliation => {
            spec.damage = 45.;
            spec.range = 6.;
            spec.push = 30.;
            spec.lift = 30.;
            spec.shape = HitShape::Box {
                origin: [0., 2.5, 1.],
                width: 3.,
                height: 6.,
                length: 6.,
            };
        }
        Attack::PlasmaWeak(_) => {}
        Attack::PlasmaStrong | Attack::PlasmaDashStrong => {
            spec.damage = 34.0;
            spec.push = 30.0;
            spec.lift = 20.0;
            spec.range = 5.0;
            spec.shape = HitShape::Box {
                origin: [0.0, 2.5, 0.0],
                width: 2.0,
                height: 2.0,
                length: 5.0,
            };
        }
        Attack::PlasmaDash => {
            spec.damage = 13.6;
            spec.range = 3.5;
            spec.shape = HitShape::Box {
                origin: [0.0, 2.5, -1.0],
                width: 3.0,
                height: 2.5,
                length: 3.5,
            };
            if index == 2 {
                spec.attribute = AttackAttributeId::PlasmaAttack2;
            }
        }
        Attack::PlasmaJump => {
            spec.range = 7.5;
            spec.stun = 1.0;
            spec.shape = HitShape::Box {
                origin: [0.0, 0.0, 0.5],
                width: 3.0,
                height: 7.5,
                length: 7.5,
            };
        }
        Attack::DaggerSlash(_) | Attack::DaggerAir => {
            spec.damage = 25.0;
            spec.range = 4.25;
            spec.shape = HitShape::Fan {
                origin: [0.0, 1.4, 0.5],
                height: 3.5,
                inner_radius: 1.25,
                outer_radius: 4.25,
                angle: 120.0_f32.to_radians(),
            };
        }
        Attack::DaggerDash => {
            spec.damage = [50.0, 25.0, 62.5][index];
            spec.range = 5.0;
            spec.shape = HitShape::Box {
                origin: [0.0, 1.8, -1.0],
                width: 1.25,
                height: 2.5,
                length: 5.0,
            };
        }
    }
    if index == 0
        && matches!(
            attack,
            Attack::CounterCombo(_) | Attack::CounterJump | Attack::CounterRetaliation
        )
    {
        spec.critical = true;
        spec.damage = match attack {
            Attack::CounterCombo(3) | Attack::CounterJump | Attack::CounterRetaliation => 45.,
            _ => 30.,
        };
        spec.attribute = if attack == Attack::CounterRetaliation {
            AttackAttributeId::CounterCounterCritical
        } else {
            AttackAttributeId::CounterCritical
        };
        spec.shape = HitShape::Ray {
            origin: [
                0.,
                2.5,
                if attack == Attack::CounterJump {
                    0.
                } else {
                    1.
                },
            ],
            direction: [0., 0., 1.],
            length: 4.,
        };
        spec.range = 4.;
        spec.push = 0.;
        spec.lift = 0.;
        spec.stun = 0.;
    }
    Some(spec)
}
