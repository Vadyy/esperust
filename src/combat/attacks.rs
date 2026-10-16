use crate::melee::{Attack, HitSpec, Melee};

pub(super) struct AttackBatch {
    pub attack: Attack,
    pub serial: u64,
    pub specs: Vec<HitSpec>,
}

pub(super) fn collect(
    content: &crate::content::simulation::SimulationContent,
    melee: &Melee,
) -> Option<AttackBatch> {
    Some(AttackBatch {
        attack: melee.attack?,
        serial: melee.serial,
        specs: melee.hit_specs(content),
    })
}
