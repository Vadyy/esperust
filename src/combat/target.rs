use crate::content::simulation::SimulationContent;
use crate::gameplay::{HitReaction, HitReactionKind, Motion, MovementInput, Player};
use crate::melee::{Attack, AttackPhase, HitSpec, Melee, MeleeInput, Weapon};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SparringMode {
    Attack,
    Guard,
    CounterHeavy,
}

pub(crate) struct PracticeFighter {
    pub melee: Melee,
    pub mode: SparringMode,
    age: f32,
    pressed_at: f32,
    input_phase: u8,
    attack_count: u32,
}
impl PracticeFighter {
    pub(super) fn new(mode: SparringMode) -> Self {
        let mut melee = Melee::default();
        melee.equipped = Weapon::CounterSword;
        Self {
            melee,
            mode,
            age: 0.,
            pressed_at: 0.,
            input_phase: 0,
            attack_count: 0,
        }
    }
    pub(super) fn reset(&mut self) {
        self.melee.reset();
        self.age = 0.;
        self.pressed_at = 0.;
        self.input_phase = 0;
    }
    fn input(&mut self, dt: f32) -> MeleeInput {
        self.age += dt;
        let mut input = MeleeInput::default();
        if self.mode == SparringMode::Guard {
            input.secondary_pressed = self.melee.attack.is_none();
            input.secondary_held = true;
        } else if self.mode == SparringMode::CounterHeavy {
            match self.input_phase {
                0 if self.age - self.pressed_at >= 3. && self.melee.attack.is_none() => {
                    input.primary_pressed = true;
                    input.primary_held = true;
                    self.pressed_at = self.age;
                    self.input_phase = 1;
                }
                1 => {
                    if self.age - self.pressed_at >= 0.22 {
                        input.primary_released = true;
                        self.input_phase = 2;
                    } else {
                        input.primary_held = true;
                    }
                }
                2 if self.melee.attack.is_none() => {
                    self.input_phase = 0;
                }
                _ => {}
            }
        } else {
            match self.input_phase {
                0 if self.age >= 1. && self.melee.attack.is_none() => {
                    input.primary_pressed = true;
                    input.primary_held = true;
                    self.pressed_at = self.age;
                    self.input_phase = 1;
                }
                1 => {
                    let hold = if self.attack_count % 2 == 0 {
                        0.05
                    } else {
                        0.22
                    };
                    if self.age - self.pressed_at >= hold {
                        input.primary_released = true;
                        self.input_phase = 2;
                    } else {
                        input.primary_held = true;
                    }
                }
                2 if self.melee.attack.is_none() => {
                    self.attack_count += 1;
                    self.input_phase = 0;
                    self.age = 0.;
                }
                _ => {}
            }
        }
        input
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Reaction {
    #[default]
    Idle,
    Flinch(u8),
    StunStart,
    Stunned,
    BlowStart,
    Airborne,
    GetUp,
    Down,
    DownDamage,
    StandUp,
    Dead,
}

pub(super) const PRACTICE_RESPAWN_SECONDS: f32 = 3.0;

#[derive(Component)]
pub struct PracticeTarget {
    pub(super) body: Player,
    pub(crate) fighter: Option<PracticeFighter>,
    pub(super) last_hit: Option<(u64, u8)>,
    pub(super) reset_serial: u32,
    pub(super) home: Vec3,
    pub stun: f32,
    pub(super) reaction: Reaction,
    pub(super) reaction_age: f32,
    pub(crate) reaction_serial: u32,
}

impl PracticeTarget {
    pub(super) fn new(position: Vec3, reset_serial: u32) -> Self {
        Self {
            body: Player::new(position, &Motion::default()),
            fighter: None,
            last_hit: None,
            reset_serial,
            home: position,
            stun: 0.0,
            reaction: Reaction::Idle,
            reaction_age: 0.0,
            reaction_serial: 0,
        }
    }

    pub fn health(&self) -> f32 {
        self.body.health
    }

    pub(super) fn solid(&self, reset_serial: u32) -> bool {
        self.body.health > 0.0 && self.reset_serial == reset_serial
    }

    pub(super) fn set_reaction(&mut self, reaction: Reaction) {
        self.body.hit_reaction = match reaction {
            Reaction::Idle | Reaction::Dead => None,
            other => Some(HitReaction {
                kind: match other {
                    Reaction::Flinch(d) => HitReactionKind::Damage(d),
                    Reaction::StunStart | Reaction::Stunned => HitReactionKind::Stun,
                    Reaction::Down => HitReactionKind::Down,
                    Reaction::DownDamage => HitReactionKind::DownDamage,
                    Reaction::StandUp => HitReactionKind::StandUp,
                    _ => HitReactionKind::Blow,
                },
                age: 0.,
                duration: if matches!(other, Reaction::StunStart | Reaction::Stunned) {
                    self.stun
                } else {
                    1.
                },
                moving: false,
                phase: match other {
                    Reaction::BlowStart => 1,
                    Reaction::Airborne => 2,
                    Reaction::GetUp => 3,
                    _ => 0,
                },
            }),
        };
        self.body.death_age = (reaction == Reaction::Dead).then_some(0.);
        self.reaction = reaction;
        self.reaction_age = 0.;
        self.reaction_serial = self.reaction_serial.wrapping_add(1);
    }

    fn sync_reaction(&mut self) {
        let reaction = if self.body.health <= 0. {
            Reaction::Dead
        } else {
            match self.body.hit_reaction {
                None => Reaction::Idle,
                Some(hit) => match hit.kind {
                    HitReactionKind::Damage(d) => Reaction::Flinch(d),
                    HitReactionKind::Stun if hit.age <= 0.7 => Reaction::StunStart,
                    HitReactionKind::Stun => Reaction::Stunned,
                    HitReactionKind::Blow => match hit.phase {
                        1 => Reaction::BlowStart,
                        2 => Reaction::Airborne,
                        _ => Reaction::GetUp,
                    },
                    HitReactionKind::Down => Reaction::Down,
                    HitReactionKind::DownDamage => Reaction::DownDamage,
                    HitReactionKind::StandUp => Reaction::StandUp,
                },
            }
        };
        if self.reaction != reaction {
            self.reaction_serial = self.reaction_serial.wrapping_add(1);
        }
        self.reaction = reaction;
        self.reaction_age = self
            .body
            .death_age
            .unwrap_or_else(|| self.body.hit_reaction.map_or(0., |hit| hit.age));
        self.stun = self
            .body
            .hit_reaction
            .filter(|hit| hit.kind == HitReactionKind::Stun)
            .map_or(0., |hit| (hit.duration - hit.age).max(0.));
    }

    pub(super) fn step(
        &mut self,
        content: &SimulationContent,
        world: &crate::controller::CollisionWorld,
        motion: &Motion,
        transform: &mut Transform,
        dt: f32,
    ) {
        self.body.feet = transform.translation;
        self.body.actor_facing = transform.rotation * Vec3::Z;
        let melee = if self.reaction == Reaction::Idle && self.body.health > 0. {
            self.fighter
                .as_mut()
                .map_or_else(MeleeInput::default, |fighter| fighter.input(dt))
        } else {
            MeleeInput::default()
        };
        if let Some(fighter) = &mut self.fighter {
            std::mem::swap(&mut self.body.melee, &mut fighter.melee);
        }
        let yaw = self.body.actor_facing.x.atan2(-self.body.actor_facing.z);
        self.body.step(
            content,
            world,
            motion,
            MovementInput {
                yaw,
                melee,
                ..default()
            },
            dt,
        );
        if let Some(fighter) = &mut self.fighter {
            std::mem::swap(&mut self.body.melee, &mut fighter.melee);
        }
        transform.translation = self.body.feet;
        transform.rotation = Quat::from_rotation_arc(Vec3::Z, self.body.actor_facing);
        self.sync_reaction();
    }

    pub(crate) fn reacting(&self) -> bool {
        self.reaction != Reaction::Idle
    }
    pub(crate) fn reaction_elapsed(&self) -> f32 {
        if self.reaction == Reaction::Stunned {
            (self.reaction_age - 0.7).max(0.)
        } else {
            self.reaction_age
        }
    }

    pub(crate) fn animation_action(&self) -> &'static str {
        if self.reaction == Reaction::Idle {
            if let Some(fighter) = &self.fighter {
                return match fighter.melee.attack {
                    Some(Attack::CounterCombo(_)) => {
                        if fighter.melee.phase == AttackPhase::Normal {
                            "counter_combo_1"
                        } else {
                            "counter_combo_1_recovery"
                        }
                    }
                    Some(Attack::CounterRetaliation) => {
                        if fighter.melee.phase == AttackPhase::Normal {
                            "counter_retaliation"
                        } else {
                            "counter_retaliation_recovery"
                        }
                    }
                    Some(Attack::CounterGuard) => {
                        if fighter.melee.phase == AttackPhase::Normal {
                            "counter_guard"
                        } else {
                            "counter_guard_recovery"
                        }
                    }
                    _ => "counter_idle_upper",
                };
            }
        }
        match self.reaction {
            Reaction::Idle => "idle",
            Reaction::Flinch(direction) if self.fighter.is_some() => [
                "counter_damage_0",
                "counter_damage_1",
                "counter_damage_2",
                "counter_damage_3",
            ][direction.min(3) as usize],
            Reaction::Flinch(_) | Reaction::BlowStart | Reaction::GetUp => {
                self.timed_reaction_action().unwrap().source_name()
            }
            Reaction::StunStart => "stun_start",
            Reaction::Stunned => "stun_loop",
            Reaction::Airborne => "blow_fall",
            Reaction::Down => "down",
            Reaction::DownDamage => "down_hit",
            Reaction::StandUp => "stand_up",
            Reaction::Dead => "death",
        }
    }

    pub(crate) fn prepare_sparring_attack(&mut self, heavy: bool) {
        let fighter = self
            .fighter
            .get_or_insert_with(|| PracticeFighter::new(SparringMode::Attack));
        fighter.reset();
        fighter.mode = SparringMode::Attack;
        fighter.attack_count = if heavy { 1 } else { 0 };
        self.stun = 0.;
        self.body = Player::new(self.body.feet, &Motion::default());
        self.set_reaction(Reaction::Idle);
    }
    pub(crate) fn prepare_passive(&mut self, position: Vec3) {
        self.fighter = None;
        self.body = Player::new(position, &Motion::default());
        self.stun = 0.;
        self.last_hit = None;
        self.home = position;
        self.set_reaction(Reaction::Idle);
    }
    pub(crate) fn animation_duration_ms(&self) -> f64 {
        match self.reaction {
            Reaction::StunStart => 700.,
            Reaction::Airborne => 200.,
            Reaction::GetUp => 400.,
            Reaction::Down => 1000.,
            Reaction::DownDamage => 500.,
            Reaction::Idle => self
                .fighter
                .as_ref()
                .map_or(0., |fighter| fighter.melee.animation_duration_ms()),
            _ => 0.,
        }
    }
    pub(super) fn die(&mut self) {
        self.body.die();
        if let Some(fighter) = &mut self.fighter {
            fighter.reset();
        }
        self.reaction_serial = self.reaction_serial.wrapping_add(1);
        self.sync_reaction();
    }

    pub(super) fn counter_stun(&mut self, seconds: f32) {
        self.body.receive_counter_stun(seconds);
        if let Some(fighter) = &mut self.fighter {
            fighter.reset();
        }
        self.reaction_serial = self.reaction_serial.wrapping_add(1);
        self.sync_reaction();
    }
    pub(super) fn receive(&mut self, content: &SimulationContent, spec: &HitSpec, facing: Vec3) {
        let serial = self.body.hit_serial;
        let death = self.body.death_serial;
        self.body.receive_melee_hit(content, spec, facing);
        if serial != self.body.hit_serial || death != self.body.death_serial {
            if let Some(fighter) = &mut self.fighter {
                fighter.reset();
            }
            self.reaction_serial = self.reaction_serial.wrapping_add(1);
        }
        self.sync_reaction();
    }

    pub(super) fn timed_reaction_action(
        &self,
    ) -> Option<crate::content::reactions::ReactionAction> {
        use crate::content::reactions::ReactionAction;
        match self.reaction {
            Reaction::Flinch(0) => Some(ReactionAction::Flinch0),
            Reaction::Flinch(1) => Some(ReactionAction::Flinch1),
            Reaction::Flinch(2) => Some(ReactionAction::Flinch2),
            Reaction::Flinch(_) => Some(ReactionAction::Flinch3),
            Reaction::BlowStart => Some(ReactionAction::BlowStart),
            Reaction::GetUp => Some(ReactionAction::GetUp),
            _ => None,
        }
    }

    pub(super) fn respawn_if_ready(&mut self, transform: &mut Transform) -> bool {
        if self.reaction != Reaction::Dead || self.reaction_age < PRACTICE_RESPAWN_SECONDS {
            return false;
        }
        if let Some(fighter) = &mut self.fighter {
            fighter.reset();
        }
        self.body = Player::new(self.home, &Motion::default());
        self.stun = 0.0;
        self.last_hit = None;
        transform.translation = self.home;
        self.set_reaction(Reaction::Idle);
        true
    }
}
