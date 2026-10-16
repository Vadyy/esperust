use super::{
    ActorEffect, CombatAttack, CombatHit, ReactionVoice,
    cues::{CombatCue, CombatCues},
};
use crate::{gameplay::CameraShakeRequest, practice_animation::PracticeAssets};
use bevy::{audio::Volume, prelude::*};

pub(super) fn present(
    mut commands: Commands,
    mut cues: ResMut<CombatCues>,
    practice: Res<PracticeAssets>,
    voices: Query<(Entity, &ReactionVoice)>,
    mut hits: MessageWriter<CombatHit>,
    mut attacks: MessageWriter<CombatAttack>,
    mut effects: MessageWriter<ActorEffect>,
    mut camera_shakes: MessageWriter<CameraShakeRequest>,
) {
    for cue in cues.0.drain(..) {
        match cue {
            CombatCue::Hit(hit) => {
                hits.write(hit);
            }
            CombatCue::Attack(attack) => {
                attacks.write(attack);
            }
            CombatCue::Effect(effect) => {
                effects.write(effect);
            }
            CombatCue::CameraShake(shake) => {
                camera_shakes.write(shake);
            }
            CombatCue::ClearVoices(entity) => {
                for (voice, owner) in &voices {
                    if owner.0 == entity {
                        commands.entity(voice).try_despawn();
                    }
                }
            }
            CombatCue::Voice { owner, name } => {
                let voice = commands
                    .spawn((
                        ReactionVoice(owner),
                        AudioPlayer::new(practice.sound(name)),
                        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.9)),
                    ))
                    .id();
                commands.entity(owner).add_child(voice);
            }
        }
    }
}
