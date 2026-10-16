use super::*;

pub(super) fn play_sound(
    commands: &mut Commands,
    source: Handle<AudioSource>,
    stats: &mut WeaponStats,
) {
    spawn_sound(commands, source, stats);
}

pub(super) fn spawn_sound(
    commands: &mut Commands,
    source: Handle<AudioSource>,
    stats: &mut WeaponStats,
) -> Entity {
    if let Some(path) = source.path() {
        *stats
            .sound_counts
            .entry(asset_stem(&path.to_string()))
            .or_default() += 1;
    }
    let sound = commands
        .spawn((
            AudioPlayer::new(source),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.9)),
        ))
        .id();
    stats.sounds_played += 1;
    sound
}

impl Weapons {
    pub(super) fn action_cues(&self, attack: Attack) -> ActionCues {
        let mut cues = ActionCues::default();
        for call in self.manifest.action_calls(attack) {
            match call.method.as_str() {
                "AddSoundBySex" | "AddSequence" => {
                    let index = if call.method == "AddSoundBySex" { 0 } else { 2 };
                    let when = call
                        .arguments
                        .get(3)
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.) as f32
                        * 0.001;
                    if let Some(sound) = call
                        .arguments
                        .get(index)
                        .and_then(|value| value.as_str())
                        .and_then(|name| self.sounds.get(&asset_stem(name)))
                    {
                        cues.sounds.push((when, sound.clone()));
                    }
                    if call.method == "AddSequence" {
                        if let Some(name) = call
                            .arguments
                            .get(1)
                            .and_then(|value| value.as_str())
                            .filter(|name| !name.is_empty())
                        {
                            let cleanup = call
                                .arguments
                                .first()
                                .and_then(|value| value.as_f64())
                                .unwrap_or(0.)
                                != 0.;
                            cues.sequences.push((when, name.to_owned(), cleanup));
                        }
                    }
                }
                "AddGhostTrail" => {
                    if let (Some(name), Some(start), Some(end)) = (
                        call.arguments.first().and_then(|value| value.as_str()),
                        call.arguments.get(1).and_then(|value| value.as_f64()),
                        call.arguments.get(2).and_then(|value| value.as_f64()),
                    ) {
                        cues.trails.push((
                            name.to_owned(),
                            start as f32 * 0.001,
                            end as f32 * 0.001,
                        ));
                    }
                }
                _ => {}
            }
        }
        cues
    }

    pub(super) fn present_actor_frame(
        &mut self,
        frame: ActorAttackFrame,
        commands: &mut Commands,
        stats: &mut WeaponStats,
    ) {
        let mut state = self
            .actor_presentations
            .remove(&frame.owner)
            .unwrap_or_default();
        if state.last_serial != Some(frame.serial) {
            let entry = frame.attack.is_some()
                && (frame.phase == AttackPhase::Normal || state.last_attack != frame.attack);
            if entry {
                state.active_id = state.active_id.wrapping_add(1);
                state.cues = self.action_cues(frame.attack.unwrap());
                self.live_sequences.retain(|sequence| {
                    sequence.owner != Some(frame.owner) || !sequence.remove_on_exit
                });
                stats.attacks_presented += 1;
                for (name, start, end) in &state.cues.trails {
                    if self.manifest.ghosttrails.contains_key(name) {
                        self.trails.push(LiveTrail {
                            name: name.clone(),
                            weapon: weapon_key(frame.equipped).unwrap().into(),
                            owner: Some(frame.owner),
                            serial: state.active_id,
                            start: *start,
                            end: *end,
                            emission_start: self.clock + *start - frame.elapsed,
                            emission_end: self.clock + *end - frame.elapsed,
                            samples: Vec::new(),
                        });
                        stats.effect_sequences += 1;
                    }
                }
            }
            if !entry && state.active && frame.attack.is_none() {
                state.active_id = state.active_id.wrapping_add(1);
            }
            state.last_serial = Some(frame.serial);
            state.last_attack = frame.attack;
        }
        state.active = frame.attack.is_some();
        if !state.active {
            state.cues = ActionCues::default();
        }
        state.cues.sounds.retain(|(when, sound)| {
            if frame.elapsed < *when {
                return true;
            }
            let voice = spawn_sound(commands, sound.clone(), stats);
            commands.entity(voice).insert(ActorAttackAudio {
                owner: frame.owner,
                active_id: state.active_id,
                reset: self.last_reset,
            });
            commands.entity(frame.owner).add_child(voice);
            false
        });
        let ready: Vec<_> = state
            .cues
            .sequences
            .iter()
            .filter(|(when, _, _)| frame.elapsed >= *when)
            .map(|(_, name, cleanup)| (name.clone(), *cleanup))
            .collect();
        state
            .cues
            .sequences
            .retain(|(when, _, _)| frame.elapsed < *when);
        for (name, cleanup) in ready {
            let before = self.live_sequences.len();
            self.start_sequence(&name, frame.origin, Some(state.active_id), cleanup, stats);
            if self.live_sequences.len() > before {
                self.live_sequences.last_mut().unwrap().owner = Some(frame.owner);
            }
        }
        self.actor_presentations.insert(frame.owner, state);
    }

    pub(super) fn start_sequence(
        &mut self,
        name: &str,
        origin: Transform,
        serial: Option<u64>,
        remove_on_exit: bool,
        stats: &mut WeaponStats,
    ) {
        let name = asset_stem(name);
        let Some(source) = self.manifest.effects.sequences.get(&name) else {
            return;
        };
        if !source
            .components
            .iter()
            .any(|c| c.particle.is_some() || c.asset_model.is_some())
        {
            return;
        }
        *stats.sequence_counts.entry(name.clone()).or_default() += 1;
        self.live_sequences.push(LiveSequence {
            id: stats.effect_sequences,
            name,
            age: 0.,
            origin,
            serial,
            remove_on_exit,
            carry: vec![0.; source.components.len()],
            owner: None,
            bone_poses: HashMap::new(),
        });
        stats.effect_sequences += 1;
    }

    pub(super) fn attribute_event(
        &mut self,
        commands: &mut Commands,
        attack: Attack,
        hit_index: u8,
        attribute: &str,
        damage: bool,
        origin: Transform,
        sound_owner: Option<Entity>,
        stats: &mut WeaponStats,
    ) {
        let sound_enabled = damage || attack_sound_enabled(attack, hit_index);
        let method = if damage {
            "SetDamageSequence"
        } else {
            "SetAttackSequence"
        };
        let events: Vec<_> = self
            .manifest
            .effects
            .attribute_calls
            .iter()
            .filter(|call| call.method == method && call.target.contains(&format!("({attribute})")))
            .map(|call| call.arguments.clone())
            .collect();
        for event in events {
            let sound_index = if damage { 2 } else { 1 };
            if let Some(sound) = event
                .get(sound_index)
                .and_then(|v| v.as_str())
                .and_then(|name| self.sounds.get(&asset_stem(name)))
                .filter(|_| sound_enabled)
            {
                if let Some(owner) = sound_owner {
                    if let Some(state) = self
                        .actor_presentations
                        .get(&owner)
                        .filter(|state| state.last_attack == Some(attack))
                    {
                        let audio = spawn_sound(commands, sound.clone(), stats);
                        commands.entity(audio).insert(ActorAttackAudio {
                            owner,
                            active_id: state.active_id,
                            reset: self.last_reset,
                        });
                        commands.entity(owner).add_child(audio);
                    }
                } else {
                    play_sound(commands, sound.clone(), stats);
                }
            }
            if let Some(sequence) = event.first().and_then(|v| v.as_str()) {
                self.start_sequence(sequence, origin, None, false, stats);
            }
        }
    }
}

pub(super) fn attack_sound_enabled(attack: Attack, hit_index: u8) -> bool {
    attack != Attack::DaggerDash || hit_index == 1
}

pub fn present(
    mut commands: Commands,
    weapons: Option<ResMut<Weapons>>,
    animation: Option<Res<Animation>>,
    (player, content, profile): (
        Res<Player>,
        Res<crate::content::simulation::SimulationContent>,
        Option<Res<crate::performance::SystemTimings>>,
    ),
    motion: Res<gameplay::Motion>,
    time: Res<Time>,
    avatar: Single<&Transform, (With<Avatar>, Without<View>, Without<WeaponModel>)>,
    camera: Single<&Transform, (With<View>, Without<Avatar>, Without<WeaponModel>)>,
    mut equipment: Query<
        (&WeaponModel, &mut Transform, &mut Visibility),
        (
            Without<Avatar>,
            Without<View>,
            Without<crate::combat::PracticeTarget>,
        ),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut combat_hits: (
        MessageReader<crate::combat::CombatHit>,
        MessageReader<crate::combat::CombatAttack>,
    ),
    mut actor_effects: MessageReader<crate::combat::ActorEffect>,
    targets: Query<
        (
            Entity,
            &Transform,
            &crate::combat::PracticeTarget,
            &crate::practice_animation::PracticeAnimator,
        ),
        Without<WeaponModel>,
    >,
    practice: Res<crate::practice_animation::PracticeAssets>,
    (mut strikes, mut actor_frames): (ResMut<StrikeEvents>, ResMut<ActorAttackFrames>),
    mut stats: ResMut<WeaponStats>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("weapons"));
    let (Some(mut weapons), Some(animation)) = (weapons, animation) else {
        return;
    };
    stats.visible_models = 0;
    for (model, mut transform, mut visible) in &mut equipment {
        if player.emote.is_some() || weapon_key(player.melee.equipped) != Some(model.key.as_str()) {
            *visible = Visibility::Hidden;
            continue;
        }
        let source = weapons.manifest.weapons[&model.key].part(model.part);
        let movement_holster = equipment_holstered(&player, &motion, source);
        let active = !movement_holster;
        let attachment = if active {
            animation
                .bone_matrix(&source.attach_bone)
                .map(|bone| bone * Mat4::from_cols_array(&source.attach_matrix))
        } else {
            source
                .holster_bone
                .as_deref()
                .and_then(|name| animation.bone_matrix(name))
                .zip(source.holster_matrix)
                .map(|(bone, offset)| bone * Mat4::from_cols_array(&offset))
        };
        if let Some(matrix) = attachment {
            *transform = Transform::from_matrix(matrix);
            *visible = Visibility::Visible;
            stats.visible_models += 1;
        } else {
            *visible = Visibility::Hidden;
        }
    }
    if weapons.last_reset != player.pose_reset_serial {
        weapons.last_reset = player.pose_reset_serial;
        weapons.last_serial = player.melee.serial;
        weapons.sound_events.clear();
        weapons.live_sequences.clear();
        weapons.particles.clear();
        weapons.sequence_events.clear();
        weapons.trails.clear();
        weapons.actor_presentations.clear();
        actor_frames.pending.clear();
    }
    if weapons.last_serial != player.melee.serial {
        weapons.last_serial = player.melee.serial;
        let entry = player.melee.attack.is_some()
            && (player.melee.phase == AttackPhase::Normal
                || weapons.last_attack != player.melee.attack);
        if entry {
            weapons.sound_events.clear();
            weapons.sequence_events.clear();
            weapons
                .live_sequences
                .retain(|sequence| sequence.owner.is_some() || !sequence.remove_on_exit);
            weapons.active_id = weapons.active_id.wrapping_add(1);
        }
        if let Some(attack) = player.melee.attack.filter(|_| entry) {
            stats.attacks_presented += 1;
            let cues = weapons.action_cues(attack);
            weapons.sound_events = cues.sounds;
            weapons.sequence_events = cues.sequences;
            for (name, start, end) in cues.trails {
                if weapons.manifest.ghosttrails.contains_key(&name) {
                    let serial = weapons.active_id;
                    let clock = weapons.clock;
                    weapons.trails.push(LiveTrail {
                        name,
                        weapon: weapon_key(player.melee.equipped).unwrap().into(),
                        serial,
                        owner: None,
                        start,
                        end,
                        emission_start: clock + start,
                        emission_end: clock + end,
                        samples: Vec::new(),
                    });
                    stats.effect_sequences += 1;
                }
            }
        }
        weapons.last_attack = player.melee.attack;
    }
    for mut frame in actor_frames.pending.drain(..) {
        if targets.get(frame.owner).is_ok() {
            let owner = frame.owner;
            let surface_strikes = std::mem::take(&mut frame.surface_strikes);
            weapons.present_actor_frame(frame, &mut commands, &mut stats);
            for strike in surface_strikes {
                if let Some(surface) = strike.surface {
                    weapons.attribute_event(
                        &mut commands,
                        strike.attack,
                        strike.hit_index,
                        strike.attribute.source_name(),
                        true,
                        surface,
                        Some(owner),
                        &mut stats,
                    );
                    stats.surface_hits += 1;
                }
            }
        }
    }
    let actor_poses: HashMap<_, _> = targets
        .iter()
        .map(|(entity, transform, target, animator)| {
            let (clip, tick) = animator.clip_cursor(&practice);
            (
                entity,
                ActorPresentationPose {
                    origin: *transform,
                    bones: animator.effect_bones(&practice),
                    clip: clip.to_owned(),
                    tick,
                    elapsed: target
                        .fighter
                        .as_ref()
                        .map_or(0., |fighter| fighter.melee.elapsed),
                },
            )
        })
        .collect();
    weapons
        .actor_presentations
        .retain(|owner, _| actor_poses.contains_key(owner));
    weapons.trails.retain(|trail| {
        trail.owner.is_none_or(|owner| {
            actor_poses.contains_key(&owner)
                && targets
                    .get(owner)
                    .is_ok_and(|(_, _, target, _)| target.health() > 0.)
        })
    });
    if weapons.last_player_hit != player.hit_serial {
        weapons.last_player_hit = player.hit_serial;
        if player
            .hit_reaction
            .is_some_and(|reaction| reaction.kind == crate::gameplay::HitReactionKind::StandUp)
        {
            play_sound(&mut commands, practice.sound("get_up"), &mut stats);
        }
    }
    if weapons.last_player_death != player.death_serial {
        weapons.last_player_death = player.death_serial;
        if player.health <= 0. {
            let serial = weapons.active_id;
            weapons.start_sequence("dead_eff", **avatar, Some(serial), false, &mut stats);
            play_sound(
                &mut commands,
                practice.sound("_eu_voice_man_dead_normal"),
                &mut stats,
            );
        }
    }
    if player.melee.attack.is_none() {
        weapons.sound_events.clear();
    }
    weapons.age = player.melee.elapsed;
    let age = weapons.age;
    weapons.sound_events.retain(|(when, source)| {
        if age >= *when {
            play_sound(&mut commands, source.clone(), &mut stats);
            false
        } else {
            true
        }
    });
    if player.melee.attack.is_none() {
        weapons.sequence_events.clear();
    }
    let ready: Vec<_> = weapons
        .sequence_events
        .iter()
        .filter(|(when, _, _)| age >= *when)
        .map(|(_, name, cleanup)| (name.clone(), *cleanup))
        .collect();
    weapons.sequence_events.retain(|(when, _, _)| age < *when);
    for (name, cleanup) in ready {
        let serial = weapons.active_id;
        weapons.start_sequence(&name, **avatar, Some(serial), cleanup, &mut stats);
    }
    if strikes.reset != player.pose_reset_serial {
        strikes.pending.clear();
    }
    for strike in strikes.pending.drain(..) {
        weapons.attribute_event(
            &mut commands,
            strike.attack,
            strike.hit_index,
            strike.attribute.source_name(),
            false,
            strike.origin,
            None,
            &mut stats,
        );
        if let Some(surface) = strike.surface {
            weapons.attribute_event(
                &mut commands,
                strike.attack,
                strike.hit_index,
                strike.attribute.source_name(),
                true,
                surface,
                None,
                &mut stats,
            );
            stats.surface_hits += 1;
        }
    }
    for strike in combat_hits.1.read() {
        weapons.attribute_event(
            &mut commands,
            strike.attack,
            strike.spec.hit_index,
            strike.spec.attribute.source_name(),
            false,
            strike.origin,
            Some(strike.owner),
            &mut stats,
        );
    }
    for hit in combat_hits.0.read() {
        if !hit.local_attacker
            && player.health > 0.
            && content.profile(hit.attribute).damage_time > 0.
        {
            let voice = if hit.lift > 0. {
                "voice_man_damage_1"
            } else if hit.stun > 0. {
                "voice_man_damage_3"
            } else {
                "voice_man_damage_2"
            };
            play_sound(&mut commands, practice.sound(voice), &mut stats);
        }
        weapons.attribute_event(
            &mut commands,
            hit.attack,
            hit.hit_index,
            hit.attribute.source_name(),
            true,
            Transform::from_translation(hit.position).with_rotation(avatar.rotation),
            None,
            &mut stats,
        );
    }
    for effect in actor_effects.read() {
        let before = weapons.live_sequences.len();
        let serial = effect.owner.is_none().then_some(weapons.active_id);
        weapons.start_sequence(effect.name, effect.origin, serial, false, &mut stats);
        if weapons.live_sequences.len() > before {
            weapons.live_sequences.last_mut().unwrap().owner = effect.owner;
        }
        if effect.name == "guard_crush" {
            if let Some(sound) = weapons.sounds.get("guard_crush") {
                play_sound(&mut commands, sound.clone(), &mut stats);
            }
        }
        if effect.name == "stun" {
            if let Some(sound) = weapons.sounds.get("stun") {
                play_sound(&mut commands, sound.clone(), &mut stats);
            }
        }
    }
    let mut live_sequences = std::mem::take(&mut weapons.live_sequences);
    let actor_states = &weapons.actor_presentations;
    live_sequences.retain_mut(|sequence| {
        let Some(owner) = sequence.owner else {
            return true;
        };
        let Ok((_, transform, target, animator)) = targets.get(owner) else {
            return false;
        };
        if (target.health() <= 0. && sequence.name != "dead_eff")
            || (sequence.name == "stun" && (target.stun <= 0. || target.health() <= 0.))
            || (sequence.name == "dead_eff" && target.health() > 0.)
        {
            return false;
        }
        if sequence.remove_on_exit
            && !actor_states
                .get(&owner)
                .is_some_and(|state| state.active && sequence.serial == Some(state.active_id))
        {
            return false;
        }
        sequence.origin = *transform;
        sequence.bone_poses = animator.effect_bones(&practice);
        true
    });
    weapons.live_sequences = live_sequences;
    let mut particles = std::mem::take(&mut weapons.particles);
    particles.retain(|particle| {
        particle.owner.is_none_or(|owner| {
            targets.get(owner).is_ok_and(|(_, _, target, _)| {
                target.health() > 0.
                    || weapons.particle_templates[particle.template].sequence == "dead_eff"
            })
        })
    });
    weapons.particles = particles;
    update_weapon_meshes(&weapons, &animation, &mut meshes);
    update_effects(
        &mut weapons,
        &animation,
        &avatar,
        &camera,
        &player,
        time.delta_secs(),
        &mut meshes,
        &mut stats,
    );
    update_trails(
        &mut weapons,
        &animation,
        &avatar,
        &player,
        &actor_poses,
        time.delta_secs(),
        &mut meshes,
        &mut stats,
    );
}
