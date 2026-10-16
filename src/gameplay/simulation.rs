use super::{
    HitReaction, HitReactionKind, LowerJump, Motion, MovementAction, MovementInput, Player,
    WallJumpPhase,
};
use crate::{
    controller::CollisionWorld,
    melee::{Melee, MeleeContext, MeleeInput},
};
use bevy::prelude::*;

impl Player {
    pub(crate) fn melee_lower_jump(&self) -> Option<LowerJump> {
        match self.melee.attack {
            Some(crate::melee::Attack::CounterGuard) => self.guard_jump,
            Some(attack) if attack.is_dagger_cut() => self.dagger_jump,
            _ => None,
        }
    }
    pub(crate) fn guard_walk_mode(&self) -> u8 {
        Self::guard_movement_mode(self.movement)
    }
    fn guard_movement_mode(input: Vec2) -> u8 {
        if input.length_squared() < 0.01 {
            0
        } else {
            ((input.x.atan2(input.y) + std::f32::consts::TAU + std::f32::consts::PI / 8.)
                / (std::f32::consts::PI / 4.))
                .floor() as u8
                % 8
                + 1
        }
    }
    pub(crate) fn normal_jump_active(&self) -> bool {
        self.normal_jump_active
    }

    pub(crate) fn velocity(&self) -> Vec3 {
        self.velocity
    }

    pub fn new(feet: Vec3, motion: &Motion) -> Self {
        Self::with_skill(feet, motion, super::Skill::None)
    }

    pub fn equipped_skill(&self) -> super::Skill {
        self.equipped_skill
    }

    pub fn max_sp(&self) -> f32 {
        self.sp_capacity
    }

    pub fn with_skill(feet: Vec3, motion: &Motion, skill: super::Skill) -> Self {
        let sp_capacity = motion.max_sp + skill.sp_bonus();
        Self {
            emote: None,
            emote_serial: 0,
            feet,
            previous: feet,
            velocity: Vec3::ZERO,
            grounded: false,
            distance: 0.0,
            jumps: 0,
            respawns: 0,
            movement: Vec2::ZERO,
            vertical_speed: 0.0,
            landing_age: 10.0,
            fall_landing: false,
            action: MovementAction::None,
            action_age: 0.0,
            action_serial: 0,
            pose_reset_serial: 0,
            sp: sp_capacity,
            equipped_skill: skill,
            health: 100.0,
            max_health: 100.0,
            god_mode: false,
            death_age: None,
            death_serial: 0,
            dodges: 0,
            air_dodges: 0,
            air_sprints: 0,
            wall_jumps: 0,
            wall_jump_failures: 0,
            sprint_distance: 0.0,
            action_direction: Vec3::ZERO,
            walljump_launched: false,
            walljump_used: false,
            walljump_landing: false,
            walljump_phase: WallJumpPhase::None,
            walljump_phase_age: 0.0,
            wall_normal: Vec3::ZERO,
            wall_contact: None,
            actor_facing: Vec3::NEG_Z,
            sp_recovery_age: 0.0,
            sp_capacity,
            last_dodge_age: f32::INFINITY,
            normal_jump_active: false,
            jump_started_moving: false,
            guard_jump: None,
            dagger_jump: None,
            guard_recovery_lower_override: false,
            guard_recovery_mode: 0,
            walljump_velocity: Vec3::ZERO,
            ballistic_horizontal: Vec3::ZERO,
            counter_ballistic_suppressed: false,
            counter_terminal_velocity: None,
            melee: Melee::default(),
            hit_reaction: None,
            hit_serial: 0,
        }
    }

    fn set_action(&mut self, action: MovementAction) {
        if self.action != action {
            if self.action == MovementAction::WallJump {
                self.velocity = Vec3::ZERO;
                self.walljump_velocity = Vec3::ZERO;
                self.ballistic_horizontal = Vec3::ZERO;
                self.walljump_phase = WallJumpPhase::None;
                self.walljump_phase_age = 0.0;
            }
            self.action = action;
            self.action_age = 0.0;
            self.action_serial = self.action_serial.wrapping_add(1);
        }
    }

    pub(super) fn reset_motion(&mut self, feet: Vec3) {
        self.emote = None;
        self.counter_ballistic_suppressed = false;
        self.counter_terminal_velocity = None;
        self.feet = feet;
        self.previous = feet;
        self.velocity = Vec3::ZERO;
        self.ballistic_horizontal = Vec3::ZERO;
        self.grounded = false;
        self.movement = Vec2::ZERO;
        self.vertical_speed = 0.0;
        self.landing_age = 10.0;
        self.fall_landing = false;
        self.action = MovementAction::None;
        self.action_age = 0.0;
        self.action_serial = self.action_serial.wrapping_add(1);
        self.pose_reset_serial = self.pose_reset_serial.wrapping_add(1);
        self.action_direction = Vec3::ZERO;
        self.walljump_launched = false;
        self.walljump_used = false;
        self.walljump_landing = false;
        self.walljump_phase = WallJumpPhase::None;
        self.walljump_phase_age = 0.0;
        self.wall_normal = Vec3::ZERO;
        self.wall_contact = None;
        self.actor_facing = Vec3::NEG_Z;
        self.sp = self.sp_capacity;
        self.health = self.max_health;
        self.death_age = None;
        self.sp_recovery_age = 0.0;
        self.last_dodge_age = f32::INFINITY;
        self.normal_jump_active = false;
        self.jump_started_moving = false;
        self.guard_jump = None;
        self.dagger_jump = None;
        self.guard_recovery_lower_override = false;
        self.walljump_velocity = Vec3::ZERO;
        self.melee.reset();
        self.hit_reaction = None;
        self.hit_serial = self.hit_serial.wrapping_add(1);
    }

    fn spend_sp(&mut self, amount: f32) {
        if self.god_mode {
            return;
        }
        self.sp = (self.sp - amount).max(0.0);
        self.sp_recovery_age = 0.0;
    }

    pub(crate) fn receive_counter_stun(&mut self, seconds: f32) {
        if self.health <= 0. {
            return;
        }
        self.emote = None;
        self.set_action(MovementAction::None);
        self.melee.interrupt_for_hit();
        self.hit_serial = self.hit_serial.wrapping_add(1);
        self.hit_reaction = Some(HitReaction {
            kind: HitReactionKind::Stun,
            age: 0.,
            duration: seconds,
            moving: false,
            phase: 0,
        });
    }
    pub(crate) fn receive_melee_hit(
        &mut self,
        content: &crate::content::simulation::SimulationContent,
        spec: &crate::melee::HitSpec,
        facing: Vec3,
    ) {
        if self.health <= 0. {
            return;
        }
        self.emote = None;
        let down_hit = self.hit_reaction.is_some_and(|reaction| {
            matches!(
                reaction.kind,
                HitReactionKind::Down | HitReactionKind::DownDamage
            )
        });
        let reduction = if self
            .hit_reaction
            .is_some_and(|reaction| reaction.kind == HitReactionKind::Down)
        {
            0.5
        } else {
            1.
        };
        if !self.god_mode {
            self.health = (self.health - spec.damage * reduction).max(0.);
        }
        if self.health == 0. {
            self.die();
            return;
        }
        let profile = content.profile(spec.attribute);
        if spec.stun > 0. {
            self.receive_counter_stun(spec.stun);
            return;
        }
        if profile.damage_time > 0. || spec.lift > 0. {
            self.set_action(MovementAction::None);
            self.melee.interrupt_for_hit();
            let incoming = -facing.normalize_or_zero();
            let right = self.actor_facing.cross(Vec3::Y);
            let front = self.actor_facing.dot(incoming);
            let side = right.dot(incoming);
            let direction = if front.abs() >= side.abs() {
                if front >= 0. { 0 } else { 1 }
            } else if side >= 0. {
                2
            } else {
                3
            };
            self.hit_serial = self.hit_serial.wrapping_add(1);
            let blow = spec.lift > 0.;
            let duration = if blow {
                ((content.duration(crate::content::reactions::ReactionAction::BlowStart) * 1000.)
                    .floor()
                    * 0.75)
                    .floor()
                    * 0.001
            } else if down_hit {
                0.5
            } else {
                profile.damage_time
            };
            self.hit_reaction = Some(HitReaction {
                kind: if blow {
                    HitReactionKind::Blow
                } else if down_hit {
                    HitReactionKind::DownDamage
                } else {
                    HitReactionKind::Damage(direction)
                },
                age: 0.,
                duration,
                moving: !down_hit && profile.damage_moving,
                phase: if blow { 1 } else { 0 },
            });
            if down_hit && !blow {
                self.ballistic_horizontal = facing.with_y(0.).normalize_or_zero() * 2.5;
                self.velocity = self.ballistic_horizontal.with_y(self.velocity.y);
            }
        }
        if spec.push > 0. || spec.lift > 0. {
            self.velocity = facing.with_y(0.).normalize_or_zero() * spec.push + Vec3::Y * spec.lift;
            self.ballistic_horizontal = self.velocity.with_y(0.);
            if spec.lift > 0. {
                if self.ballistic_horizontal.length_squared() > 0. {
                    self.actor_facing = -self.ballistic_horizontal.normalize();
                }
                self.grounded = false;
                self.normal_jump_active = false;
                self.walljump_landing = false;
                self.fall_landing = false;
            }
        }
    }

    pub(crate) fn die(&mut self) {
        if self.god_mode {
            return;
        }
        if self.death_age.is_some() {
            return;
        }
        self.health = 0.;
        self.emote = None;
        self.hit_reaction = None;
        self.death_age = Some(0.);
        self.death_serial = self.death_serial.wrapping_add(1);
        self.set_action(MovementAction::None);
        self.melee.reset();
        self.counter_ballistic_suppressed = false;
        self.counter_terminal_velocity = None;
        self.velocity.x = 0.;
        self.velocity.z = 0.;
        self.ballistic_horizontal = Vec3::ZERO;
        self.movement = Vec2::ZERO;
    }

    pub(crate) fn constrain_actor_contact(&mut self, feet: Vec3, normal: Vec3) {
        self.feet = feet;
        let normal = normal.normalize_or_zero();
        for velocity in [
            &mut self.velocity,
            &mut self.walljump_velocity,
            &mut self.ballistic_horizontal,
        ] {
            let inward = velocity.dot(normal);
            if inward < 0.0 {
                *velocity -= normal * inward;
            }
        }
        self.vertical_speed = self.velocity.y;
    }

    pub fn step(
        &mut self,
        content: &crate::content::simulation::SimulationContent,
        world: &CollisionWorld,
        motion: &Motion,
        mut command: MovementInput,
        dt: f32,
    ) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }
        let reference_dt = 1.0 / crate::DEFAULT_PHYSICS_HZ as f32;
        if motion.smooth_physics && dt > reference_dt {
            let steps = (dt / reference_dt).ceil() as u32;
            let tick_start = self.feet;
            command.melee.primary_down_sample |= command.melee.primary_pressed;
            command.melee.secondary_down_sample |= command.melee.secondary_pressed;
            command.melee.recognition_deferred |= command.dodge;
            for _ in 0..steps {
                self.step_inner(content, world, motion, command, dt / steps as f32);
                command.jump = false;
                command.emote = None;
                command.dodge = false;
                command.melee.primary_pressed = false;
                command.melee.primary_released = false;
                command.melee.secondary_pressed = false;
                command.melee.secondary_released = false;
                command.melee.equip = None;
            }
            self.previous = tick_start;
        } else {
            self.step_inner(content, world, motion, command, dt);
        }
    }

    fn step_inner(
        &mut self,
        content: &crate::content::simulation::SimulationContent,
        world: &CollisionWorld,
        motion: &Motion,
        mut command: MovementInput,
        dt: f32,
    ) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }
        self.previous = self.feet;
        self.step_emote(command, command.emote.and_then(|index| content.emote_duration(index)), dt);
        if let Some(reaction) = &mut self.hit_reaction {
            reaction.age += dt;
            if !matches!(
                reaction.kind,
                HitReactionKind::Blow
                    | HitReactionKind::Down
                    | HitReactionKind::DownDamage
                    | HitReactionKind::StandUp
            ) && reaction.age >= reaction.duration
            {
                self.hit_reaction = None;
            }
        }
        if let Some(reaction) = self.hit_reaction.filter(|reaction| {
            matches!(
                reaction.kind,
                HitReactionKind::Down | HitReactionKind::DownDamage | HitReactionKind::StandUp
            )
        }) {
            let elapsed_ms = (reaction.age as f64 * 1000. + 0.0001).floor() as u32;
            let duration_ms = if reaction.kind == HitReactionKind::DownDamage {
                500
            } else {
                1000
            };
            if elapsed_ms > duration_ms {
                if reaction.kind == HitReactionKind::DownDamage {
                    self.velocity = Vec3::Y * self.velocity.y;
                    self.ballistic_horizontal = Vec3::ZERO;
                    self.hit_reaction = Some(HitReaction {
                        kind: HitReactionKind::Down,
                        age: 0.,
                        duration: 1.,
                        moving: false,
                        phase: 0,
                    });
                    self.hit_serial = self.hit_serial.wrapping_add(1);
                } else if reaction.kind == HitReactionKind::Down {
                    self.hit_reaction = Some(HitReaction {
                        kind: HitReactionKind::StandUp,
                        age: 0.,
                        duration: 1.,
                        moving: false,
                        phase: 0,
                    });
                    self.hit_serial = self.hit_serial.wrapping_add(1);
                } else {
                    self.hit_reaction = None;
                }
            }
        }
        if let Some(reaction) = self.hit_reaction {
            match reaction.kind {
                HitReactionKind::Blow | HitReactionKind::Down | HitReactionKind::StandUp => {
                    let stand_up =
                        matches!(reaction.kind, HitReactionKind::Blow | HitReactionKind::Down)
                            && !command.dodge
                            && (command.jump
                                || (command.melee.primary_held
                                    && !command.melee.primary_down_sample)
                                || (command.melee.secondary_held
                                    && !command.melee.secondary_down_sample));
                    command.melee.equip = None;
                    command.melee.dispatch_blocked = true;
                    command.jump = false;
                    command.sprint = false;
                    command.crouch = false;
                    if stand_up {
                        self.hit_reaction = Some(HitReaction {
                            kind: HitReactionKind::StandUp,
                            age: 0.,
                            duration: 1.,
                            moving: false,
                            phase: 0,
                        });
                        self.hit_serial = self.hit_serial.wrapping_add(1);
                    }
                }
                _ => {
                    command.melee.equip = None;
                    command.melee.dispatch_blocked = true;
                    command.jump = false;
                    command.dodge = false;
                    command.sprint = false;
                    if !reaction.moving {
                        command.movement = Vec2::ZERO;
                    }
                }
            }
        }
        if self.health <= 0. {
            self.die();
            *self.death_age.as_mut().unwrap() += dt;
            command = MovementInput {
                yaw: self.actor_facing.x.atan2(-self.actor_facing.z),
                ..default()
            };
        }
        self.landing_age += dt;
        self.action_age += dt;
        self.walljump_phase_age += dt;
        self.sp_recovery_age += dt;
        self.last_dodge_age += dt;
        if self.action == MovementAction::WallJump && !self.walljump_phase.accepts_actions() {
            command.melee = MeleeInput::default();
        }
        let dagger_was_active = self
            .melee
            .attack
            .is_some_and(crate::melee::Attack::is_dagger_cut);
        let dagger_was_normal =
            dagger_was_active && self.melee.phase == crate::melee::AttackPhase::Normal;
        let counter_jump_was_active = self.melee.attack == Some(crate::melee::Attack::CounterJump);
        let guard_was_active = self.melee.attack == Some(crate::melee::Attack::CounterGuard);
        let guard_was_recovering =
            guard_was_active && self.melee.phase == crate::melee::AttackPhase::Delay;
        if guard_was_active {
            if let Some(jump) = &mut self.guard_jump {
                if jump.advance(self.grounded, self.velocity.y, dt) {
                    self.guard_jump = None;
                }
            }
        } else {
            self.guard_jump = None;
        }
        let guard_chord_launch =
            guard_was_active && command.dodge && self.guard_jump.is_none() && self.grounded;
        if guard_was_active && command.dodge && self.guard_jump.is_none() {
            self.guard_jump = Some(LowerJump::enter(if self.grounded {
                1.
            } else {
                self.velocity.y
            }));
        }
        self.melee.aim_direction = [
            command.yaw.sin() * command.pitch.cos(),
            command.pitch.sin(),
            -command.yaw.cos() * command.pitch.cos(),
        ];
        let mut melee = self.melee.tick(
            content,
            MeleeInput {
                movement_requested: command.movement.length_squared() > 0.01,
                dodging: self.action.is_dodge(),
                dispatch_blocked: command.melee.dispatch_blocked
                    || (self.action == MovementAction::Sprint && self.action_age <= 0.5),
                recognition_deferred: command.dodge || command.melee.recognition_deferred,
                ..command.melee
            },
            MeleeContext {
                grounded: self.grounded,
                plasma_jump_attack: !self.grounded
                    && matches!(self.action, MovementAction::None | MovementAction::WallJump),
                guard_lower_busy: self.guard_jump.is_some() || (guard_was_active && command.jump),
            },
            dt,
            self.sp,
        );
        let dagger_active = self
            .melee
            .attack
            .is_some_and(crate::melee::Attack::is_dagger_cut);
        let dagger_entering = dagger_active && melee.cancel_movement;
        if dagger_entering {
            melee.movement_scale = self.melee.equipped.move_speed_rate();
            melee.suppress_movement_actions = false;
        }
        let counter_ballistic_deferred =
            counter_jump_was_active || self.melee.attack == Some(crate::melee::Attack::CounterJump);
        let suppress_ballistic = if counter_ballistic_deferred {
            self.counter_ballistic_suppressed
        } else {
            melee.suppress_ballistic
        };
        if guard_was_active || self.melee.attack == Some(crate::melee::Attack::CounterGuard) {
            let aim = Vec3::from(self.melee.aim_direction).normalize_or(self.actor_facing);
            let height = motion.collision_axis_scales[1] * 2.;
            let start = self.feet + Vec3::Y * (height * 0.75);
            if 2. > motion.collision_axis_scales[0] * 0.5 {
                if let Some(distance) = world.raycast(start, aim, 2.) {
                    self.feet = world
                        .move_ellipsoid(
                            self.feet,
                            -aim * (2. - distance + 0.0003),
                            Vec3::from(motion.collision_axis_scales),
                            motion.movement_skin_fraction,
                        )
                        .position;
                }
            }
        }
        if melee.cancel_movement && !dagger_entering {
            self.set_action(MovementAction::None);
            self.normal_jump_active = false;
            self.fall_landing = false;
            self.landing_age = motion.land_delay.max(motion.walljump_landing_time);
        }
        if melee.clear_velocity && !counter_ballistic_deferred && !dagger_was_active {
            self.velocity = Vec3::ZERO;
            self.walljump_velocity = Vec3::ZERO;
            self.ballistic_horizontal = Vec3::ZERO;
        }
        if let Some(velocity) = melee.velocity_world.filter(|_| !counter_ballistic_deferred) {
            self.velocity = Vec3::from(velocity);
            self.ballistic_horizontal = self.velocity.with_y(0.);
            self.grounded = false;
        }
        if let Some(velocity_y) = melee.velocity_y {
            self.velocity.y = velocity_y;
        }
        if melee.spend_sp > 0.0 {
            self.spend_sp(melee.spend_sp);
        }
        if melee.suppress_movement_actions {
            command.jump = false;
            command.dodge = false;
            command.sprint = false;
        }
        let guarding = self.melee.attack == Some(crate::melee::Attack::CounterGuard);
        if guarding {
            command.sprint = false;
            if (!guard_was_active || command.jump) && !self.grounded && self.guard_jump.is_none() {
                self.guard_jump = Some(LowerJump::enter(self.velocity.y));
            }
        }
        let input = command.movement.normalize_or_zero();
        let dodge_side = if command.dodge_side.abs() > 0.01 {
            command.dodge_side.signum()
        } else if input.x.abs() > 0.01 {
            input.x.signum()
        } else {
            0.0
        };
        let mut jump = command.jump;
        self.movement = input;
        let forward = Vec3::new(command.yaw.sin(), 0.0, -command.yaw.cos());
        let right = Vec3::new(command.yaw.cos(), 0.0, command.yaw.sin());
        if self.action != MovementAction::WallJump
            && !self.hit_reaction.is_some_and(|reaction| {
                matches!(
                    reaction.kind,
                    HitReactionKind::Blow
                        | HitReactionKind::Down
                        | HitReactionKind::DownDamage
                        | HitReactionKind::StandUp
                )
            })
        {
            self.actor_facing = Vec3::from(self.melee.strike_direction())
                .with_y(0.)
                .normalize_or(forward);
        } else if let Some(reaction) = self
            .hit_reaction
            .filter(|reaction| reaction.kind == HitReactionKind::StandUp)
        {
            let yaw = self.actor_facing.x.atan2(-self.actor_facing.z);
            self.actor_facing = Quat::from_rotation_y(-yaw).slerp(
                Quat::from_rotation_y(-command.yaw),
                reaction.age.clamp(0., 1.),
            ) * Vec3::NEG_Z;
        }

        if self.action.is_dodge()
            && self.action_age + 0.000_001 >= motion.dodge_time + motion.dodge_delay
        {
            self.set_action(MovementAction::None);
        }
        if self.action == MovementAction::WallJump {
            if self.walljump_phase == WallJumpPhase::Landing
                && self.walljump_phase_age + 0.000_001 >= motion.walljump_landing_time
            {
                self.set_action(MovementAction::None);
                self.landing_age = self.landing_age.max(motion.walljump_landing_time);
            } else if self.walljump_phase.launches()
                && self.walljump_phase_age + 0.000_001 >= motion.walljump_time
            {
                self.walljump_phase = if self.walljump_phase == WallJumpPhase::LaunchA {
                    WallJumpPhase::AirA
                } else {
                    WallJumpPhase::AirB
                };
                self.walljump_phase_age = 0.0;
            }
        }
        let cancel_dodge = self.action.is_dodge() && (command.jump || command.dodge);
        if cancel_dodge {
            self.set_action(MovementAction::None);
            self.normal_jump_active = true;
            jump = true;
        }
        let free_action = matches!(self.action, MovementAction::None | MovementAction::Sprint);
        let interruptible_walljump =
            self.action == MovementAction::WallJump && self.walljump_phase.accepts_actions();
        if command.dodge
            && !cancel_dodge
            && (free_action || interruptible_walljump)
            && !command.crouch
            && dodge_side.abs() > 0.01
            && self.sp >= motion.dodge_cost
            && self.last_dodge_age + 0.000_001 >= motion.dodge_cooldown
        {
            self.set_action(if dodge_side < 0.0 {
                MovementAction::DodgeLeft
            } else {
                MovementAction::DodgeRight
            });
            self.hit_reaction = None;
            self.actor_facing = forward;
            self.melee.interrupt_guard();
            self.guard_jump = None;
            self.action_direction = right * dodge_side;
            self.normal_jump_active = false;
            self.spend_sp(motion.dodge_cost);
            self.last_dodge_age = 0.0;
            self.dodges += 1;
            if !self.grounded {
                self.air_dodges += 1;
            }
            jump = false;
        } else if command.dodge
            && !cancel_dodge
            && !guard_was_active
            && dodge_side.abs() > 0.01
            && (free_action || interruptible_walljump)
            && (self.grounded
                || !self.normal_jump_active
                || self.action == MovementAction::Sprint
                || interruptible_walljump)
        {
            jump = true;
            self.hit_reaction = None;
            self.actor_facing = forward;
            self.normal_jump_active = true;
            if interruptible_walljump {
                self.set_action(MovementAction::None);
                self.normal_jump_active = true;
            }
        }
        let repeat_walljump = self.action == MovementAction::WallJump
            && self.walljump_phase.launches()
            && self.walljump_phase_age > 0.0;
        if jump && (free_action || repeat_walljump) && !self.grounded && !guarding {
            if let Some(normal) = self
                .wall_contact
                .and_then(|contact| {
                    contact.walljump_normal(self.feet, Vec3::from(motion.collision_axis_scales))
                })
                .filter(|normal| {
                    let facing = if repeat_walljump {
                        self.actor_facing
                    } else {
                        forward
                    };
                    let approach = -facing.dot(*normal);
                    if repeat_walljump {
                        approach.abs() > motion.walljump_min_approach_dot
                    } else {
                        approach > motion.walljump_min_approach_dot
                    }
                })
            {
                self.set_action(MovementAction::WallJump);
                self.wall_normal = normal;
                self.wall_contact = None;
                let mut facing = if repeat_walljump {
                    self.actor_facing
                } else {
                    forward
                };
                if facing.dot(normal) > 0.0 {
                    facing = -facing;
                }
                let reflected = (facing - normal * (2.0 * facing.dot(normal))).normalize_or_zero();
                self.action_direction =
                    reflected * (motion.walljump_force * motion.walljump_horizontal_multiplier);
                self.action_direction.y = motion.walljump_force;
                self.walljump_launched = false;
                self.walljump_used = true;
                self.walljump_phase = WallJumpPhase::Windup;
                self.walljump_phase_age = 0.0;
                self.action_age = 0.0;
                self.normal_jump_active = false;
                self.walljump_velocity = Vec3::ZERO;
                self.velocity = Vec3::ZERO;
                self.ballistic_horizontal = Vec3::ZERO;
                self.actor_facing = -normal;
            }
        }

        let mut planar;
        if self.action.is_dodge() {
            if self.action_age < motion.dodge_time {
                let side = if self.action == MovementAction::DodgeLeft {
                    -1.0
                } else {
                    1.0
                };
                self.action_direction = right * side;
            }
            let recovery = (self.action_age - motion.dodge_time).max(0.0);
            let speed = (motion.dodge_speed - motion.dodge_deceleration * recovery).max(0.0);
            planar = self.action_direction * speed;
            self.movement = if self.action == MovementAction::DodgeLeft {
                -Vec2::X
            } else {
                Vec2::X
            };
        } else if self.action == MovementAction::WallJump {
            if self.walljump_phase == WallJumpPhase::Windup
                && self.walljump_phase_age + 0.000_001 >= motion.walljump_windup
            {
                if self.sp >= motion.walljump_cost {
                    self.velocity = self.action_direction;
                    self.walljump_velocity = self.action_direction;
                    self.spend_sp(motion.walljump_cost);
                    self.wall_jumps += 1;
                    self.walljump_launched = true;
                    self.grounded = false;
                    self.walljump_phase = if forward.dot(self.actor_facing).abs() >= 0.75 {
                        WallJumpPhase::LaunchA
                    } else {
                        WallJumpPhase::LaunchB
                    };
                    self.actor_facing =
                        -Vec3::new(self.walljump_velocity.x, 0.0, self.walljump_velocity.z)
                            .normalize_or_zero();
                } else {
                    self.walljump_phase = WallJumpPhase::Failed;
                    self.wall_jump_failures = self.wall_jump_failures.wrapping_add(1);
                }
                self.walljump_phase_age = 0.0;
            }
            planar = if matches!(
                self.walljump_phase,
                WallJumpPhase::AirA | WallJumpPhase::AirB | WallJumpPhase::Failed
            ) {
                (forward * input.y + right * input.x) * (motion.base_speed * motion.air_control)
            } else {
                Vec3::ZERO
            };
            if matches!(
                self.walljump_phase,
                WallJumpPhase::AirA | WallJumpPhase::AirB | WallJumpPhase::Failed
            ) {
                let current_yaw = self.actor_facing.x.atan2(-self.actor_facing.z);
                let yaw = Quat::from_rotation_y(-current_yaw).slerp(
                    Quat::from_rotation_y(-command.yaw),
                    (self.walljump_phase_age / 0.5).clamp(0.0, 1.0),
                );
                self.actor_facing = yaw * Vec3::NEG_Z;
            }
        } else {
            let angle = input.x.abs().atan2(input.y);
            let landing_phase = self.landing_age
                < if self.fall_landing {
                    motion.fall_landing_time
                } else if self.walljump_landing {
                    motion.walljump_landing_time
                } else {
                    motion.land_delay
                };
            let direction_multiplier = if (guarding && (self.guard_jump.is_some() || jump))
                || (!guarding && (!self.grounded || landing_phase))
                || angle < std::f32::consts::PI * 0.125
            {
                1.0
            } else if angle >= std::f32::consts::PI * 0.875 {
                motion.back_multiplier
            } else {
                motion.side_multiplier
            };
            planar =
                (forward * input.y + right * input.x) * (motion.base_speed * direction_multiplier);
            let sprint = command.sprint
                && !jump
                && input.y > 0.0
                && !command.crouch
                && self.sp
                    >= if self.action == MovementAction::Sprint {
                        motion.sprint_sp_per_second * dt
                    } else {
                        motion.sprint_required_sp
                    };
            if sprint {
                if self.action != MovementAction::Sprint {
                    if !self.grounded {
                        self.air_sprints += 1;
                    }
                    self.velocity = Vec3::Y * self.velocity.y.min(0.0);
                    self.ballistic_horizontal = Vec3::ZERO;
                    self.normal_jump_active = false;
                    self.fall_landing = false;
                    self.landing_age = motion.land_delay.max(motion.walljump_landing_time);
                }
                self.set_action(MovementAction::Sprint);
                self.spend_sp(motion.sprint_sp_per_second * dt);
                planar = (forward * input.y + right * input.x)
                    * (motion.base_speed * motion.sprint_multiplier);
            } else if self.action == MovementAction::Sprint {
                self.set_action(MovementAction::None);
                if jump {
                    self.normal_jump_active = true;
                }
            }
            if command.crouch && !guarding {
                planar *= motion.crouch_multiplier;
            }
            let land_delay = if self.walljump_landing {
                motion.walljump_landing_time
            } else {
                motion.land_delay
            };
            if self.action != MovementAction::Sprint
                && !guarding
                && self.grounded
                && !self.fall_landing
                && self.landing_age < land_delay
            {
                planar *= motion.land_multiplier;
            }
            if !self.grounded && !guarding {
                planar *= motion.air_control;
            }
            if jump && self.grounded && (!guarding || self.guard_jump.is_none()) {
                self.jump_started_moving = !guarding && input.length_squared() > 0.01;
                if guarding {
                    self.velocity = Vec3::Y * motion.jump_speed;
                    self.ballistic_horizontal = Vec3::ZERO;
                    self.guard_jump = Some(LowerJump::enter(motion.jump_speed));
                } else {
                    self.velocity.y = motion.jump_speed;
                }
                self.grounded = false;
                self.walljump_landing = false;
                self.fall_landing = false;
                self.normal_jump_active = true;
                self.jumps += 1;
                self.set_action(MovementAction::None);
            }
        }
        if dagger_was_active {
            planar = (forward * input.y + right * input.x) * motion.base_speed;
        }
        if guarding {
            let mode = if let Some(jump) = self.guard_jump {
                9 + jump.phase + 4 * u8::from(jump.phase != 3 && jump.duration_ms() == 0.)
            } else {
                Self::guard_movement_mode(input)
            };
            if !guard_was_recovering && self.melee.phase == crate::melee::AttackPhase::Delay {
                self.guard_recovery_mode = mode;
                self.guard_recovery_lower_override = false;
            } else if guard_was_recovering && mode != self.guard_recovery_mode {
                self.guard_recovery_lower_override = true;
            }
        }
        if self.hit_reaction.is_some_and(|reaction| {
            matches!(
                reaction.kind,
                HitReactionKind::Blow
                    | HitReactionKind::Down
                    | HitReactionKind::DownDamage
                    | HitReactionKind::StandUp
            )
        }) {
            planar = Vec3::ZERO;
        }
        if let Some(speed) = melee.forward_speed {
            let facing = self.actor_facing;
            planar = facing * speed
                + Vec3::new(-facing.z, 0., facing.x) * melee.lateral_speed.unwrap_or(0.0);
            self.movement = Vec2::ZERO;
        } else if !self.action.is_dodge() {
            planar *= melee.movement_scale;
            if let Some(speed) = melee.movement_speed {
                planar = planar.normalize_or_zero() * speed;
            }
        }
        let dagger_lower_planar = if dagger_was_normal {
            let rate = if self.dagger_jump.is_some() {
                motion.air_control
            } else {
                match Self::guard_movement_mode(input) {
                    0 | 1 => 1.,
                    5 => motion.back_multiplier,
                    _ => motion.side_multiplier,
                }
            };
            (forward * input.y + right * input.x)
                * (motion.base_speed * self.melee.equipped.attack_speed_rate() * 0.5 * rate)
        } else {
            Vec3::ZERO
        };
        let wall_windup =
            self.action == MovementAction::WallJump && self.walljump_phase == WallJumpPhase::Windup;
        let terminal_velocity = if counter_ballistic_deferred {
            self.counter_terminal_velocity
        } else {
            melee.terminal_velocity
        }
        .unwrap_or(motion.terminal_velocity);
        let gravity_active = !self.grounded && !wall_windup && !suppress_ballistic;
        let vertical_before_gravity = self.velocity.y;
        if gravity_active {
            self.velocity.y = (self.velocity.y + motion.gravity * dt).max(terminal_velocity);
        }
        if self.action == MovementAction::WallJump {
            self.walljump_velocity.y = self.velocity.y;
        }
        let ballistic = if suppress_ballistic {
            Vec3::ZERO
        } else if self.action == MovementAction::WallJump && !self.grounded {
            Vec3::new(
                self.walljump_velocity.x,
                self.velocity.y,
                self.walljump_velocity.z,
            )
        } else if !self.grounded {
            self.ballistic_horizontal.with_y(self.velocity.y)
        } else {
            Vec3::ZERO
        };
        self.velocity.x = planar.x + dagger_lower_planar.x + ballistic.x;
        self.velocity.z = planar.z + dagger_lower_planar.z + ballistic.z;
        let axes = Vec3::from(motion.collision_axis_scales);
        let reference_dt = 1.0 / crate::DEFAULT_PHYSICS_HZ as f32;
        let free_skin = if motion.smooth_physics {
            motion.movement_skin_fraction * dt / reference_dt
        } else {
            motion.movement_skin_fraction
        };
        let move_channel = |feet, delta: Vec3| {
            let delta = if delta.y >= 0.0 || delta.x != 0.0 || delta.z != 0.0 {
                delta.clamp_length_max(1.8)
            } else {
                delta
            };
            world.move_ellipsoid_with_free_skin(
                feet,
                delta,
                axes,
                motion.movement_skin_fraction,
                free_skin,
            )
        };
        let horizontal = move_channel(self.feet, planar * dt);
        let lower_horizontal = if dagger_lower_planar != Vec3::ZERO {
            move_channel(horizontal.position, dagger_lower_planar * dt)
        } else {
            horizontal
        };
        let mut displacement = if self.grounded {
            Vec3::Y * terminal_velocity * dt
        } else {
            ballistic * dt
        };
        if motion.smooth_physics && gravity_active {
            let accelerating_time = if motion.gravity < 0.0 {
                ((terminal_velocity - vertical_before_gravity) / motion.gravity).clamp(0.0, dt)
            } else {
                dt
            };
            displacement.y = vertical_before_gravity * accelerating_time
                + 0.5 * motion.gravity * accelerating_time * (accelerating_time + reference_dt)
                + self.velocity.y * (dt - accelerating_time);
        }
        let mut result = move_channel(lower_horizontal.position, displacement);
        if displacement != Vec3::ZERO || planar != Vec3::ZERO || dagger_lower_planar != Vec3::ZERO {
            self.wall_contact = result
                .contact
                .or(lower_horizontal.contact)
                .or(horizontal.contact);
        }
        result.wall_normal = result
            .wall_normal
            .or(lower_horizontal.wall_normal)
            .or(horizontal.wall_normal);
        result.ceiling_hit |= horizontal.ceiling_hit || lower_horizontal.ceiling_hit;
        let traveled = result.position - self.feet;
        if self.velocity.y > 0.0 && result.ceiling_hit {
            self.velocity.y = 0.0;
        }
        let was_grounded = self.grounded;
        self.grounded = result.grounded && self.velocity.y <= 0.0 && !wall_windup;
        if !was_grounded && self.grounded {
            if matches!(
                self.action,
                MovementAction::Sprint | MovementAction::DodgeLeft | MovementAction::DodgeRight
            ) {
                self.landing_age = motion.land_delay.max(motion.walljump_landing_time);
                self.walljump_landing = false;
                self.fall_landing = false;
            } else {
                self.landing_age = 0.0;
                self.walljump_landing = self.action == MovementAction::WallJump;
                self.fall_landing = !self.walljump_landing && !self.normal_jump_active;
            }
            self.walljump_used = false;
            if self.action == MovementAction::WallJump {
                self.walljump_phase = WallJumpPhase::Landing;
                self.walljump_phase_age = 0.0;
                self.walljump_velocity = Vec3::ZERO;
                self.velocity = Vec3::ZERO;
                self.ballistic_horizontal = Vec3::ZERO;
            }
            self.normal_jump_active = false;
        }
        if was_grounded && !self.grounded && !wall_windup {
            self.velocity.y = motion.terminal_velocity * 0.1;
            self.ballistic_horizontal = Vec3::ZERO;
            self.normal_jump_active = false;
        }
        if self.grounded {
            self.velocity.y = 0.0;
            self.ballistic_horizontal = Vec3::ZERO;
        }
        let horizontal = Vec2::new(traveled.x, traveled.z).length();
        self.distance += horizontal;
        if self.action == MovementAction::Sprint {
            self.sprint_distance += horizontal;
        }
        self.feet = result.position;
        if let Some(mut reaction) = self
            .hit_reaction
            .filter(|reaction| reaction.kind == HitReactionKind::Blow)
        {
            let elapsed_ms = (reaction.age as f64 * 1000. + 0.0001).floor() as u32;
            let duration_ms = (reaction.duration as f64 * 1000.).round() as u32;
            match reaction.phase {
                1 if elapsed_ms > duration_ms => {
                    reaction.phase = 2;
                    reaction.age = 0.;
                }
                2 if self.grounded => {
                    reaction.phase = 3;
                    reaction.age = 0.;
                    reaction.duration = 1.;
                    self.velocity = Vec3::Y * -self.velocity.y;
                    self.ballistic_horizontal = Vec3::ZERO;
                }
                3 if elapsed_ms > duration_ms => {
                    reaction = HitReaction {
                        kind: HitReactionKind::Down,
                        age: 0.,
                        duration: 1.,
                        moving: false,
                        phase: 0,
                    };
                    self.hit_serial = self.hit_serial.wrapping_add(1);
                }
                _ => {}
            }
            if self.hit_reaction.is_some() {
                self.hit_reaction = Some(reaction);
            }
        }
        if guard_chord_launch {
            self.velocity = Vec3::Y * motion.jump_speed;
            self.ballistic_horizontal = Vec3::ZERO;
            self.grounded = false;
            self.normal_jump_active = true;
            self.jump_started_moving = false;
            self.walljump_landing = false;
            self.fall_landing = false;
            self.jumps += 1;
        }
        if dagger_was_normal {
            if let Some(jump) = &mut self.dagger_jump {
                if jump.advance(self.grounded, self.velocity.y, dt) {
                    self.dagger_jump = None;
                }
            }
        }
        if dagger_was_active && melee.clear_velocity {
            self.velocity = Vec3::ZERO;
            self.walljump_velocity = Vec3::ZERO;
            self.ballistic_horizontal = Vec3::ZERO;
        }
        if dagger_entering {
            self.set_action(MovementAction::None);
            self.normal_jump_active = false;
            self.fall_landing = false;
            self.landing_age = motion.land_delay.max(motion.walljump_landing_time);
            self.dagger_jump = (!self.grounded).then(|| LowerJump::enter(self.velocity.y));
        } else if dagger_was_active && !dagger_active {
            self.dagger_jump = None;
            self.normal_jump_active = false;
            self.fall_landing = false;
            self.walljump_landing = false;
            self.landing_age = motion.land_delay.max(motion.walljump_landing_time);
        } else if !dagger_active {
            self.dagger_jump = None;
        }
        if counter_ballistic_deferred {
            if melee.clear_velocity {
                self.velocity = Vec3::ZERO;
                self.walljump_velocity = Vec3::ZERO;
                self.ballistic_horizontal = Vec3::ZERO;
            }
            if let Some(velocity) = melee.velocity_world {
                self.velocity = Vec3::from(velocity);
                self.ballistic_horizontal = self.velocity.with_y(0.);
                self.grounded = false;
            }
        }
        self.counter_ballistic_suppressed = self.melee.attack
            == Some(crate::melee::Attack::CounterJump)
            && melee.suppress_ballistic;
        self.counter_terminal_velocity =
            if self.melee.attack == Some(crate::melee::Attack::CounterJump) {
                melee.terminal_velocity
            } else {
                None
            };
        self.vertical_speed = self.velocity.y;
        if self.action != MovementAction::Sprint && self.sp_recovery_age > motion.sp_recovery_delay
        {
            self.sp = (self.sp + motion.sp_recovery_per_second * 0.01 * self.sp_capacity * dt)
                .min(self.sp_capacity);
        }
    }
}

#[cfg(test)]
#[path = "simulation_tests.rs"]
mod tests;
