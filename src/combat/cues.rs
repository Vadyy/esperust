use super::{ActorEffect, CombatAttack, CombatHit};
use crate::gameplay::CameraShakeRequest;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(super) struct CombatCues(pub Vec<CombatCue>);

pub(super) enum CombatCue {
    Attack(CombatAttack),
    Hit(CombatHit),
    Effect(ActorEffect),
    CameraShake(CameraShakeRequest),
    ClearVoices(Entity),
    Voice { owner: Entity, name: &'static str },
}
