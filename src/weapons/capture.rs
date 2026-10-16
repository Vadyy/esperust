use super::*;

pub(super) struct StrikeEvent {
    pub(super) attack: Attack,
    pub(super) hit_index: u8,
    pub(super) origin: Transform,
    pub(super) surface: Option<Transform>,
    pub(super) attribute: crate::combat_types::AttackAttributeId,
}

#[derive(Resource, Default)]
pub(crate) struct StrikeEvents {
    pub(super) pending: Vec<StrikeEvent>,
    pub(super) last: Option<(u64, u8)>,
    pub(super) reset: u32,
}

pub(super) fn capture_strikes(
    content: Res<crate::content::simulation::SimulationContent>,
    player: Res<Player>,
    level: Option<Res<scene::Level>>,
    mut events: ResMut<StrikeEvents>,
) {
    if events.reset != player.pose_reset_serial {
        events.reset = player.pose_reset_serial;
        events.pending.clear();
        events.last = None;
    }
    if let Some(attack) = player.melee.attack {
        for hit in player.melee.hit_specs(&content) {
            let key = (player.melee.serial, hit.hit_index);
            if events.last != Some(key) {
                events.last = Some(key);
                events.pending.push(StrikeEvent {
                    attack,
                    hit_index: hit.hit_index,
                    attribute: hit.attribute,
                    origin: Transform::from_translation(player.feet)
                        .with_rotation(Quat::from_rotation_arc(
                            Vec3::Z,
                            player.actor_facing.normalize_or(Vec3::Z),
                        ))
                        .with_scale(Vec3::splat(scene::PLAYER_MODEL_SCALE)),
                    surface: level.as_ref().and_then(|level| {
                        melee_surface_hit(&level.collision, &hit, player.feet, player.actor_facing)
                    }),
                });
            }
        }
    }
}

pub(super) struct ActorAttackFrame {
    pub(super) owner: Entity,
    pub(super) attack: Option<Attack>,
    pub(super) phase: AttackPhase,
    pub(super) equipped: Weapon,
    pub(super) serial: u64,
    pub(super) elapsed: f32,
    pub(super) origin: Transform,
    pub(super) surface_strikes: Vec<StrikeEvent>,
}
#[derive(Resource, Default)]
pub(crate) struct ActorAttackFrames {
    pub(super) pending: Vec<ActorAttackFrame>,
}

pub(super) fn capture_actor_actions(
    content: Res<crate::content::simulation::SimulationContent>,
    targets: Query<(Entity, &Transform, &crate::combat::PracticeTarget)>,
    level: Option<Res<scene::Level>>,
    mut frames: ResMut<ActorAttackFrames>,
) {
    for (owner, origin, target) in &targets {
        let Some(fighter) = &target.fighter else {
            continue;
        };
        let active = target.health() > 0. && target.animation_action().starts_with("counter_");
        frames.pending.push(ActorAttackFrame {
            owner,
            origin: *origin,
            equipped: fighter.melee.equipped,
            attack: fighter.melee.attack.filter(|_| active),
            phase: fighter.melee.phase,
            serial: fighter.melee.serial,
            elapsed: fighter.melee.elapsed,
            surface_strikes: fighter.melee.attack.filter(|_| active).map_or_else(
                Vec::new,
                |attack| {
                    fighter
                        .melee
                        .hit_specs(&content)
                        .into_iter()
                        .filter_map(|hit| {
                            let surface = melee_surface_hit(
                                &level.as_ref()?.collision,
                                &hit,
                                origin.translation,
                                origin.rotation * Vec3::Z,
                            )?;
                            Some(StrikeEvent {
                                attack,
                                hit_index: hit.hit_index,
                                origin: *origin,
                                surface: Some(surface),
                                attribute: hit.attribute,
                            })
                        })
                        .collect()
                },
            ),
        });
    }
}

#[derive(Default)]
pub(super) struct ActionCues {
    pub(super) sounds: Vec<(f32, Handle<AudioSource>)>,
    pub(super) sequences: Vec<(f32, String, bool)>,
    pub(super) trails: Vec<(String, f32, f32)>,
}
#[derive(Default)]
pub(super) struct ActorPresentation {
    pub(super) last_serial: Option<u64>,
    pub(super) last_attack: Option<Attack>,
    pub(super) active: bool,
    pub(super) active_id: u64,
    pub(super) cues: ActionCues,
}
#[derive(Component)]
pub(super) struct ActorAttackAudio {
    pub(super) owner: Entity,
    pub(super) active_id: u64,
    pub(super) reset: u32,
}

pub(super) fn cleanup_actor_attack_audio(
    mut commands: Commands,
    player: Res<Player>,
    weapons: Option<Res<Weapons>>,
    actors: Query<&crate::combat::PracticeTarget>,
    audio: Query<(Entity, &ActorAttackAudio)>,
) {
    for (entity, sound) in &audio {
        let alive = actors
            .get(sound.owner)
            .is_ok_and(|target| target.health() > 0.);
        let same_action = weapons.as_ref().is_some_and(|weapons| {
            weapons
                .actor_presentations
                .get(&sound.owner)
                .is_some_and(|state| state.active_id == sound.active_id)
        });
        if !alive || !same_action || sound.reset != player.pose_reset_serial {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) struct ActorPresentationPose {
    pub(super) origin: Transform,
    pub(super) bones: HashMap<String, Mat4>,
    pub(super) clip: String,
    pub(super) tick: f64,
    pub(super) elapsed: f32,
}
