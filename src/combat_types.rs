#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Weapon {
    #[default]
    Unarmed,
    PlasmaSword,
    Dagger,
    CounterSword,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attack {
    PlasmaWeak(u8),
    PlasmaStrong,
    PlasmaDash,
    PlasmaDashStrong,
    PlasmaJump,
    DaggerSlash(u8),
    DaggerDash,
    DaggerAir,
    #[allow(dead_code)]
    CounterDash,
    CounterCombo(u8),
    CounterJump,
    CounterGuard,
    CounterRetaliation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AttackPhase {
    #[default]
    Normal,
    Delay,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitSpec {
    pub hit_index: u8,
    pub range: f32,
    pub damage: f32,
    pub push: f32,
    pub lift: f32,
    pub stun: f32,
    pub shape: HitShape,
    pub attribute: AttackAttributeId,
    pub critical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitShape {
    Ray {
        origin: [f32; 3],
        direction: [f32; 3],
        length: f32,
    },
    Cylinder {
        origin: [f32; 3],
        radius: f32,
        height: f32,
    },
    Fan {
        origin: [f32; 3],
        height: f32,
        inner_radius: f32,
        outer_radius: f32,
        angle: f32,
    },
    Box {
        origin: [f32; 3],
        width: f32,
        height: f32,
        length: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttackAttributeId {
    PlasmaCritical,
    PlasmaStandWeak,
    PlasmaStandStrong,
    PlasmaAttack2Weak,
    PlasmaAttack2,
    PlasmaJumpCritical,
    PlasmaJump,
    CounterCounterCritical,
    CounterCounterAttack,
    CounterCritical,
    CounterAttack1,
    CounterAttack2,
    CounterAttack3,
    CounterAttack4,
    CounterJumpDash,
    DaggerAttack1,
    DaggerAttack2,
}
impl AttackAttributeId {
    pub const ALL: [Self; 17] = [
        Self::PlasmaCritical,
        Self::PlasmaStandWeak,
        Self::PlasmaStandStrong,
        Self::PlasmaAttack2Weak,
        Self::PlasmaAttack2,
        Self::PlasmaJumpCritical,
        Self::PlasmaJump,
        Self::CounterCounterCritical,
        Self::CounterCounterAttack,
        Self::CounterCritical,
        Self::CounterAttack1,
        Self::CounterAttack2,
        Self::CounterAttack3,
        Self::CounterAttack4,
        Self::CounterJumpDash,
        Self::DaggerAttack1,
        Self::DaggerAttack2,
    ];
    pub const fn source_name(self) -> &'static str {
        match self {
            Self::PlasmaCritical => "ATTACKATTRIB_PLASMA_SWORD_CRITICAL",
            Self::PlasmaStandWeak => "ATTACKATTRIB_PLASMA_SWORD_STAND_WEAK",
            Self::PlasmaStandStrong => "ATTACKATTRIB_PLASMA_SWORD_STAND_STRONG",
            Self::PlasmaAttack2Weak => "ATTACKATTRIB_PLASMA_SWORD_ATTACK2_WEAK",
            Self::PlasmaAttack2 => "ATTACKATTRIB_PLASMA_SWORD_ATTACK2",
            Self::PlasmaJumpCritical => "ATTACKATTRIB_PLASMA_SWORD_JUMP_CRITICAL",
            Self::PlasmaJump => "ATTACKATTRIB_PLASMA_SWORD_JUMP",
            Self::CounterCounterCritical => "ATTACKATTRIB_COUNTER_SWORD_COUNTER_CRITICAL",
            Self::CounterCounterAttack => "ATTACKATTRIB_COUNTER_SWORD_COUNTER_ATTACK",
            Self::CounterCritical => "ATTACKATTRIB_COUNTER_SWORD_CRITICAL",
            Self::CounterAttack1 => "ATTACKATTRIB_COUNTER_SWORD_ATTACK1",
            Self::CounterAttack2 => "ATTACKATTRIB_COUNTER_SWORD_ATTACK2",
            Self::CounterAttack3 => "ATTACKATTRIB_COUNTER_SWORD_ATTACK3",
            Self::CounterAttack4 => "ATTACKATTRIB_COUNTER_SWORD_ATTACK4",
            Self::CounterJumpDash => "ATTACKATTRIB_COUNTER_SWORD_JUMPDASH",
            Self::DaggerAttack1 => "ATTACKATTRIB_SPYDAGGER_ATTACK1",
            Self::DaggerAttack2 => "ATTACKATTRIB_SPYDAGGER_ATTACK2",
        }
    }
    pub fn from_source_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.source_name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::AttackAttributeId as Id;
    #[test]
    fn selected_attribute_names_are_unique_and_round_trip_without_empty_ids() {
        let names: std::collections::HashSet<_> =
            Id::ALL.into_iter().map(Id::source_name).collect();
        assert_eq!(names.len(), Id::ALL.len());
        for id in Id::ALL {
            assert_eq!(Id::from_source_name(id.source_name()), Some(id));
            assert!(id.source_name().starts_with("ATTACKATTRIB_"));
        }
        assert_eq!(Id::from_source_name(""), None);
        assert_eq!(Id::from_source_name("ATTACKATTRIB_UNKNOWN"), None);
    }
}
