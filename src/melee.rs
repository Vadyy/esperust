pub use crate::combat_types::{Attack, AttackPhase, HitShape, HitSpec, Weapon};

impl Weapon {
    pub fn move_speed_rate(self) -> f32 {
        match self {
            Self::Unarmed => 1.0,
            Self::PlasmaSword | Self::CounterSword => 1.1,
            Self::Dagger => 1.2,
        }
    }
    pub fn attack_speed_rate(self) -> f32 {
        match self {
            Self::Unarmed | Self::PlasmaSword | Self::CounterSword => 1.0,
            Self::Dagger => 1.2,
        }
    }
}

impl Attack {
    pub fn is_dagger_cut(self) -> bool {
        matches!(self, Self::DaggerSlash(_) | Self::DaggerAir)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MeleeInput {
    pub primary_pressed: bool,
    pub primary_held: bool,
    pub primary_released: bool,
    pub secondary_pressed: bool,
    pub secondary_held: bool,
    pub secondary_released: bool,
    pub equip: Option<Weapon>,
    pub movement_requested: bool,
    pub dodging: bool,
    pub dispatch_blocked: bool,
    pub recognition_deferred: bool,
    pub secondary_down_sample: bool,
    pub primary_down_sample: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MeleeContext {
    pub grounded: bool,
    pub plasma_jump_attack: bool,
    pub guard_lower_busy: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MeleeOutput {
    pub cancel_movement: bool,
    pub clear_velocity: bool,
    pub suppress_ballistic: bool,
    pub terminal_velocity: Option<f32>,
    pub velocity_y: Option<f32>,
    pub velocity_world: Option<[f32; 3]>,
    pub movement_speed: Option<f32>,
    pub forward_speed: Option<f32>,
    pub lateral_speed: Option<f32>,
    pub movement_scale: f32,
    pub spend_sp: f32,
    pub suppress_movement_actions: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Melee {
    pub equipped: Weapon,
    pending_weapon: Option<Weapon>,
    pub attack: Option<Attack>,
    pub phase: AttackPhase,
    pub elapsed: f32,
    pub phase_elapsed: f32,
    pub serial: u64,
    pub full_body: bool,
    pub counter_stage: u8,
    pub aim_direction: [f32; 3],
    counter_attack_direction: [f32; 3],
    previous_elapsed: f32,
    primary_hold_age: f32,
    primary_pending: bool,
    next_plasma: u8,
    next_dagger: u8,
    air_dagger_variant: u8,
    stopped_by_hit: bool,
    hit_stop_success: bool,
    hit_stop_active: bool,
    hit_stop_start_crossed: bool,
    hit_stop_end_crossed: bool,
    animation_delay_seconds: f32,
    drop_started: bool,
    counter_jump_cooldown: f32,
    counter_followup: Option<Attack>,
    counter_drop_y: f32,
}

impl Melee {
    fn apply_pending_weapon(&mut self) {
        if self.attack.is_none() {
            if let Some(weapon) = self.pending_weapon.take() {
                self.equipped = weapon;
                self.reset();
            }
        }
    }

    pub fn strike_direction(&self) -> [f32; 3] {
        match self.attack {
            Some(Attack::CounterCombo(_) | Attack::CounterRetaliation | Attack::CounterJump) => {
                use bevy::math::Vec3;
                Vec3::from(self.aim_direction).with_y(0.)
                    .normalize_or(Vec3::from(self.counter_attack_direction)).to_array()
            }
            _ => self.aim_direction,
        }
    }
    pub fn interrupt_guard(&mut self) {
        if self.attack == Some(Attack::CounterGuard) {
            self.attack = None;
            self.counter_followup = None;
            self.serial = self.serial.wrapping_add(1);
            self.apply_pending_weapon();
        }
    }
    pub fn reset(&mut self) {
        let equipped = self.equipped;
        let serial = self.serial.wrapping_add(1);
        let counter_jump_cooldown = self.counter_jump_cooldown;
        *self = Self {
            equipped,
            serial,
            counter_jump_cooldown,
            ..Self::default()
        };
    }

    pub fn confirm_hit(&mut self) {
        if self.attack == Some(Attack::CounterRetaliation) && self.phase == AttackPhase::Normal {
            self.hit_stop_success = true;
        }
        if matches!(self.attack, Some(Attack::PlasmaDash | Attack::DaggerDash | Attack::CounterCombo(0))) {
            self.stopped_by_hit = true;
        }
    }

    pub fn interrupt_for_hit(&mut self) {
        let hold_age=self.primary_hold_age;
        let pending=self.primary_pending;
        let pending_weapon=self.pending_weapon;
        self.reset();
        self.primary_hold_age=hold_age;
        self.primary_pending=pending;
        self.pending_weapon=pending_weapon;
    }

    pub fn animation_phase_elapsed(&self) -> f32 {
        if self.attack == Some(Attack::CounterRetaliation) && self.phase == AttackPhase::Normal {
            (self.phase_elapsed - self.animation_delay_seconds).max(0.)
        } else { self.phase_elapsed }
    }

    pub fn finish_hit_frame(&mut self) {
        if self.attack != Some(Attack::CounterRetaliation) || self.phase != AttackPhase::Normal {
            self.hit_stop_active = false;
            return;
        }
        let previous_ms = (self.previous_elapsed as f64 * 1000. + 0.0001).floor();
        let elapsed_ms = (self.elapsed as f64 * 1000. + 0.0001).floor();
        if !self.hit_stop_start_crossed && previous_ms <= 350. && elapsed_ms > 350. {
            self.hit_stop_start_crossed = true;
            self.hit_stop_active = self.hit_stop_success;
        }
        if !self.hit_stop_end_crossed && previous_ms <= 550. && elapsed_ms > 550. {
            self.hit_stop_end_crossed = true;
            self.hit_stop_active = false;
        }
    }

    pub fn dagger_slash_index(&self) -> u8 {
        match self.attack {
            Some(Attack::DaggerSlash(index)) => index,
            _ => self.air_dagger_variant,
        }
    }

    fn start(&mut self, attack: Attack, _context: MeleeContext) {
        if matches!(attack, Attack::CounterCombo(_) | Attack::CounterJump | Attack::CounterRetaliation) {
            use bevy::math::Vec3;
            self.counter_attack_direction = Vec3::from(self.aim_direction).with_y(0.).normalize_or(Vec3::Z).to_array();
        }
        if attack == Attack::DaggerAir {
            self.air_dagger_variant = self.next_dagger;
            self.next_dagger = (self.next_dagger + 1) % 2;
        }
        self.attack = Some(attack);
        self.phase = AttackPhase::Normal;
        self.elapsed = 0.0;
        self.previous_elapsed = -f32::EPSILON;
        self.phase_elapsed = 0.0;
        self.serial = self.serial.wrapping_add(1);
        self.stopped_by_hit = false;
        self.hit_stop_success = false;
        self.hit_stop_active = false;
        self.hit_stop_start_crossed = false;
        self.hit_stop_end_crossed = false;
        self.animation_delay_seconds = 0.;
        self.drop_started = false;
        self.counter_stage = 0;
        self.counter_followup = None;
        if attack != Attack::CounterGuard { self.primary_pending = false; }
        self.full_body = !matches!(attack, Attack::PlasmaWeak(_) | Attack::DaggerSlash(_) | Attack::DaggerAir);
    }

    fn delay(&mut self) {
        self.phase = AttackPhase::Delay;
        self.phase_elapsed = 0.0;
        self.hit_stop_active = false;
        self.animation_delay_seconds = 0.;
        self.serial = self.serial.wrapping_add(1);
    }

    pub fn animation_duration_ms(&self) -> f64 {
        let normal = self.phase == AttackPhase::Normal;
        match self.attack {
            Some(Attack::PlasmaWeak(_)) => {
                if normal {
                    500.0
                } else {
                    0.0
                }
            }
            Some(Attack::PlasmaStrong) => {
                if normal {
                    1000.0
                } else {
                    0.0
                }
            }
            Some(Attack::PlasmaDash) => {
                if normal {
                    750.0
                } else {
                    1250.0
                }
            }
            Some(Attack::PlasmaDashStrong) => {
                if normal {
                    1000.0
                } else {
                    40.0
                }
            }
            Some(Attack::PlasmaJump) => {
                if normal {
                    400.0
                } else {
                    1000.0
                }
            }
            Some(Attack::DaggerSlash(_) | Attack::DaggerAir) => {
                if normal {
                    550.0
                } else {
                    0.0
                }
            }
            Some(Attack::DaggerDash) => {
                if normal {
                    1000.0
                } else {
                    1500.0
                }
            }
            Some(Attack::CounterCombo(index)) => if normal { [800.,600.,800.,1150.][index.min(3) as usize] } else { [250.,400.,300.,300.][index.min(3) as usize] },
            Some(Attack::CounterJump) => 600.,
            Some(Attack::CounterRetaliation) => if normal { 1000. } else { 500. },
            Some(Attack::CounterGuard) => if normal {0.} else {500.},
            Some(Attack::CounterDash) => 0.,
            None => 0.0,
        }
    }

    pub fn tick(
        &mut self,
        content: &crate::content::simulation::SimulationContent,
        input: MeleeInput,
        context: MeleeContext,
        dt: f32,
        _sp: f32,
    ) -> MeleeOutput {
        let dt = dt.max(0.0);
        self.counter_jump_cooldown -= dt;
        let mut output = MeleeOutput {
            cancel_movement: false,
            clear_velocity: false,
            suppress_ballistic: false,
            terminal_velocity: None,
            velocity_y: None,
            velocity_world: None,
            movement_speed: None,
            forward_speed: None,
            lateral_speed: None,
            movement_scale: self.equipped.move_speed_rate(),
            spend_sp: 0.0,
            suppress_movement_actions: false,
        };
        if let Some(weapon) = input.equip {
            if weapon != self.equipped {
                self.pending_weapon = Some(weapon);
            }
        }
        self.apply_pending_weapon();
        self.previous_elapsed = self.elapsed;
        let previous_attack = self.attack;
        let previous_phase = self.phase;
        let previous_phase_elapsed = self.phase_elapsed;
        if self.attack.is_some() {
            if self.attack == Some(Attack::CounterRetaliation)
                && self.phase == AttackPhase::Normal
                && self.hit_stop_active
            {
                self.animation_delay_seconds += dt * 0.9;
            }
            self.elapsed += dt;
            self.phase_elapsed += dt;
        }
        if self.attack == Some(Attack::DaggerDash)
            && self.elapsed >= 1.451
            && input.movement_requested
        {
            self.attack = None;
        }
        if input.primary_pressed {
            self.primary_hold_age = 0.0;
            self.primary_pending =
                matches!(self.equipped, Weapon::PlasmaSword | Weapon::CounterSword);
        }
        if input.primary_held && self.primary_pending {
            self.primary_hold_age += dt;
        }
        if !input.primary_held && !input.primary_released && !input.primary_pressed {
            self.primary_pending = false;
        }
        let recognition_deferred = input.recognition_deferred
            || (matches!(self.equipped, Weapon::CounterSword | Weapon::Dagger)
                && (input.primary_pressed || input.primary_down_sample));
        let short_event = !recognition_deferred
            && self.primary_pending
            && input.primary_released
            && self.primary_hold_age <= 0.2;
        let long_event = !recognition_deferred
            && self.primary_pending
            && input.primary_held
            && self.primary_hold_age >= 0.2;
        if input.primary_released || long_event {
            self.primary_pending = false;
        }
        let plasma_short = short_event && !input.dispatch_blocked;
        let plasma_long = long_event && !input.dispatch_blocked;

        if self.attack == Some(Attack::PlasmaDash) && (plasma_short || plasma_long) {
            self.start(Attack::PlasmaDashStrong, context);
            output.cancel_movement = true;
        }

        if self.equipped == Weapon::CounterSword {
            self.tick_counter(input, context, plasma_short, plasma_long, &mut output);
        }
        if let Some(attack) = self.attack {
            if matches!(attack, Attack::CounterGuard | Attack::CounterDash) {
            } else if self.phase == AttackPhase::Normal {
                match attack {
                    Attack::PlasmaJump => {
                        if self.drop_started {
                            self.delay();
                            output.clear_velocity = true;
                        } else if self.elapsed >= 0.4 {
                            self.drop_started = true;
                            output.velocity_y = Some(-500.0);
                        }
                    }
                    Attack::CounterJump if !self.drop_started => {
                    }
                    _ if if attack.is_dagger_cut() {
                        (self.phase_elapsed as f64 * 1000. + 0.0001).floor() > 550.
                    } else if attack == Attack::CounterJump {
                        (self.phase_elapsed as f64 * 1000. + 0.0001).floor() > 600.
                    } else {
                        self.phase_elapsed * 1000.0 >= self.animation_duration_ms() as f32
                    } =>
                    {
                        let delay = match attack {
                            Attack::PlasmaDash => 1.25,
                            Attack::PlasmaDashStrong => 0.04,
                            Attack::DaggerDash => 1.5,
                            Attack::CounterCombo(i) => [0.25, 0.4, 0.3, 0.3][i.min(3) as usize],
                            Attack::CounterJump => 0.6,
                            Attack::CounterRetaliation => 0.5,
                            _ => 0.0,
                        };
                        if attack.is_dagger_cut() {
                            self.delay();
                            output.clear_velocity = true;
                        } else if delay > 0.0 {
                            self.delay();
                            if matches!(
                                attack,
                                Attack::PlasmaDashStrong
                                    | Attack::CounterCombo(_)
                                    | Attack::CounterJump
                                    | Attack::CounterRetaliation
                            ) {
                                output.clear_velocity = true;
                            }
                        } else {
                            self.attack = None;
                        }
                    }
                    _ => {}
                }
            } else if if attack.is_dagger_cut() {
                (self.phase_elapsed as f64 * 1000. + 0.0001).floor() > 0.
            } else {
                self.phase_elapsed * 1000.0 >= self.animation_duration_ms() as f32
            } {
                self.attack = None;
            }
        }

        self.apply_pending_weapon();
        let dagger_exiting =
            previous_attack.is_some_and(Attack::is_dagger_cut) && self.attack.is_none();
        if self.attack.is_none()
            && !dagger_exiting
            && !input.dispatch_blocked
            && !recognition_deferred
        {
            let attack = match self.equipped {
                Weapon::CounterSword
                    if !input.secondary_pressed
                        && !input.secondary_down_sample
                        && input.secondary_held =>
                {
                    Some(Attack::CounterGuard)
                }
                Weapon::CounterSword
                    if (plasma_short || plasma_long)
                        && context.plasma_jump_attack
                        && self.counter_jump_cooldown < 0. =>
                {
                    Some(Attack::CounterJump)
                }
                Weapon::CounterSword if plasma_short && context.grounded => {
                    Some(Attack::CounterCombo(0))
                }
                Weapon::CounterSword if plasma_long => Some(Attack::CounterRetaliation),
                Weapon::PlasmaSword
                    if (input.secondary_pressed
                        || (!input.secondary_down_sample && input.secondary_held))
                        && (context.grounded || input.dodging) =>
                {
                    Some(Attack::PlasmaDash)
                }
                Weapon::Dagger
                    if (input.secondary_pressed
                        || (!input.secondary_down_sample && input.secondary_held))
                        && (context.grounded || input.dodging) =>
                {
                    Some(Attack::DaggerDash)
                }
                Weapon::PlasmaSword
                    if (plasma_long || plasma_short) && context.plasma_jump_attack =>
                {
                    Some(Attack::PlasmaJump)
                }
                Weapon::PlasmaSword if plasma_long => Some(Attack::PlasmaStrong),
                Weapon::PlasmaSword if plasma_short => {
                    let index = self.next_plasma;
                    self.next_plasma = (self.next_plasma + 1) % 3;
                    Some(Attack::PlasmaWeak(index))
                }
                Weapon::Dagger
                    if input.primary_held
                        && !input.secondary_down_sample
                        && !input.secondary_pressed =>
                {
                    if context.grounded {
                        let index = self.next_dagger;
                        self.next_dagger = (self.next_dagger + 1) % 2;
                        Some(Attack::DaggerSlash(index))
                    } else {
                        Some(Attack::DaggerAir)
                    }
                }
                _ => None,
            };
            if let Some(attack) = attack {
                self.start(attack, context);
                if attack == Attack::CounterJump {
                    self.counter_jump_cooldown = 1.5;
                }
                output.cancel_movement = true;
            }
        }

        output.movement_scale = self.equipped.move_speed_rate();
        if let Some(attack) = self.attack {
            if attack == Attack::PlasmaStrong {
                self.full_body = self.elapsed < 0.5;
            }
            let normal = self.phase == AttackPhase::Normal;
            output.movement_scale = self.equipped.attack_speed_rate()
                * match attack {
                    Attack::PlasmaWeak(_) | Attack::PlasmaStrong => 0.8,
                    Attack::DaggerSlash(_) | Attack::DaggerAir => 0.5,
                    Attack::CounterGuard => 1.0,
                    _ => 0.0,
                };
            output.suppress_movement_actions = true;
            if attack == Attack::CounterGuard {
                self.full_body = false;
                output.suppress_movement_actions = false;
            }
            if attack == Attack::CounterJump && normal {
                output.suppress_ballistic = !self.drop_started && self.elapsed > 0.;
                if !self.drop_started && (self.elapsed as f64 * 1000. + 0.0001).floor() > 400. {
                    self.drop_started = true;
                    output.suppress_ballistic = false;
                    use bevy::math::Vec3;
                    let velocity = Vec3::from(self.strike_direction()) * 60. - Vec3::Y * 40.;
                    output.velocity_world = Some(velocity.to_array());
                    self.counter_drop_y = velocity.y;
                }
                if self.drop_started {
                    output.terminal_velocity = Some(self.counter_drop_y);
                }
            }
            if attack == Attack::PlasmaJump {
                output.suppress_ballistic = normal && !self.drop_started;
                if self.drop_started {
                    output.terminal_velocity = Some(-500.0);
                }
            }
            if matches!(
                attack,
                Attack::PlasmaDash | Attack::DaggerDash | Attack::CounterCombo(_)
            ) {
                let outgoing_normal =
                    previous_attack == Some(attack) && previous_phase == AttackPhase::Normal;
                let (forward, lateral) = if !self.stopped_by_hit && (normal || outgoing_normal) {
                    let previous = if outgoing_normal {
                        previous_phase_elapsed
                    } else {
                        0.0
                    };
                    content.root_speed(attack, previous, previous + dt, dt)
                } else {
                    (0.0, 0.0)
                };
                output.forward_speed = Some(forward);
                output.lateral_speed = Some(lateral);
            }
        }
        if previous_attack.is_some_and(Attack::is_dagger_cut) {
            output.movement_scale = if previous_phase == AttackPhase::Normal {
                self.equipped.attack_speed_rate() * 0.5
            } else {
                0.
            };
            output.suppress_movement_actions = true;
        }
        output
    }

    pub fn guard_query(&self, angle_degrees: f32) -> Option<i32> {
        (self.attack == Some(Attack::CounterGuard)
            && self.phase == AttackPhase::Normal
            && angle_degrees.is_finite() && (0.0..=90.0).contains(&angle_degrees)).then_some(3000)
    }

    pub fn counter_query(&self, angle_degrees: f32) -> bool {
        self.attack == Some(Attack::CounterGuard) && self.phase == AttackPhase::Normal
            && self.counter_stage == 1 && angle_degrees.is_finite()
            && (0.0..60.0).contains(&angle_degrees)
    }

    fn tick_counter(&mut self, input: MeleeInput, context: MeleeContext, short: bool, long: bool, output: &mut MeleeOutput) {
        match self.attack {
            Some(attack @ (Attack::CounterCombo(0..=2) | Attack::CounterRetaliation | Attack::CounterJump)) => {
                let (end, immediate, recognized): (f32,f32,Option<Attack>) = match attack {
                    Attack::CounterCombo(index) => (0.650, 0.651,
                        if long { Some(Attack::CounterRetaliation) }
                        else if short { Some(Attack::CounterCombo(index + 1)) } else { None }),
                    Attack::CounterJump => (0.700, 0.701, (short || long).then_some(Attack::CounterRetaliation)),
                    _ => (0.800, 0.801, (short || long).then_some(Attack::CounterCombo(0))),
                };
                let elapsed_ms=(self.elapsed as f64*1000.+0.0001).floor() as u32;
                let end_ms=(end*1000.).round() as u32;
                let immediate_ms=(immediate*1000.).round() as u32;
                if elapsed_ms <= end_ms {
                    if self.counter_followup.is_none() { self.counter_followup = recognized; }
                } else {
                    let next = self.counter_followup.take().or(if elapsed_ms >= immediate_ms { recognized } else { None });
                    if let Some(next) = next.filter(|next| !matches!(next, Attack::CounterCombo(_)) || context.grounded) {
                        self.start(next, context);
                        output.cancel_movement = true;
                    }
                }
            }
            Some(Attack::CounterGuard) => {
                if self.phase == AttackPhase::Normal {
                    if input.secondary_released {
                        self.counter_stage = 2;
                        self.delay();
                    } else if self.counter_stage == 0 && (self.phase_elapsed as f64*1000.+0.0001).floor() > 200. {
                        self.counter_stage = 1;
                        self.phase_elapsed = 0.;
                    } else if self.counter_stage == 1 && (self.phase_elapsed as f64*1000.+0.0001).floor() > 1_000_000. {
                        self.counter_stage = 2; self.delay();
                    } else if self.counter_stage == 1 && !context.guard_lower_busy && (long || (short && context.grounded)) {
                        self.start(if long { Attack::CounterRetaliation } else { Attack::CounterCombo(0) }, context);
                        output.cancel_movement = true;
                    }
                } else if (self.phase_elapsed as f64*1000.+0.0001).floor() > 500. { self.attack = None; }
            }
            _ => {}
        }
    }

    pub fn hit_specs(
        &self,
        content: &crate::content::simulation::SimulationContent,
    ) -> Vec<HitSpec> {
        let Some(attack) = self.attack else {
            return Vec::new();
        };
        content
            .hit_times(attack)
            .iter()
            .enumerate()
            .filter(|(_, time)| self.previous_elapsed < **time && self.elapsed >= **time)
            .filter_map(|(index, _)| content.hit_spec(attack, index))
            .collect()
    }

    #[cfg(test)]
    pub fn hit_spec(
        &self,
        content: &crate::content::simulation::SimulationContent,
    ) -> Option<HitSpec> {
        let attack = self.attack?;
        content
            .hit_times(attack)
            .iter()
            .enumerate()
            .filter(|(_, time)| self.previous_elapsed < **time && self.elapsed >= **time)
            .filter_map(|(index, _)| content.hit_spec(attack, index))
            .find(|spec| !spec.critical)
    }
}

#[cfg(test)]
mod tests {
    use crate::content::simulation::parity_content;
    use super::*;
    #[test]
    fn counter_combo_camera_turn_updates_each_swing_and_recovery_without_restart() {
        for stage in 0..4 {
            let mut m=Melee::default();
            m.aim_direction=[0.,0.,-1.];
            m.start(Attack::CounterCombo(stage),ground());
            let serial=m.serial;
            for phase in [AttackPhase::Normal,AttackPhase::Delay] {
                m.phase=phase;
                for aim in [[0.6,0.8,0.],[0.,-0.8,0.6],[-0.6,0.8,0.]] {
                    m.aim_direction=aim;
                    m.tick(parity_content(), Default::default(),ground(),0.016,100.);
                    let actual=bevy::math::Vec3::from(m.strike_direction());
                    let expected=bevy::math::Vec3::from(aim).with_y(0.).normalize();
                    assert!(actual.distance(expected)<0.0001);
                    assert_eq!(m.attack,Some(Attack::CounterCombo(stage)));
                    assert_eq!(m.serial,serial,"camera motion must not restart the swing");
                }
            }
        }
    }
    #[test]
    fn counter_heavy_camera_turn_survives_hit_stop_and_recovery_without_restart() {
        let mut m=Melee::default();m.equipped=Weapon::CounterSword;
        m.aim_direction=[0.,0.,-1.];
        m.start(Attack::CounterRetaliation,ground());
        let serial=m.serial;
        m.confirm_hit();
        m.tick(parity_content(), Default::default(),ground(),0.351,100.);
        m.finish_hit_frame();
        assert!(m.hit_stop_active);
        let pose_age=m.animation_phase_elapsed();
        m.aim_direction=[0.6,0.8,0.];
        m.tick(parity_content(), Default::default(),ground(),0.05,100.);
        assert!((m.animation_phase_elapsed()-pose_age-0.005).abs()<0.0001);
        assert_eq!(m.strike_direction(),[1.,0.,0.]);
        assert_eq!(m.serial,serial);
        m.tick(parity_content(), Default::default(),ground(),0.6,100.);
        assert_eq!(m.phase,AttackPhase::Delay);
        let recovery_serial=m.serial;
        m.aim_direction=[0.,-0.8,-0.6];
        m.tick(parity_content(), Default::default(),ground(),0.016,100.);
        assert_eq!(m.strike_direction(),[0.,0.,-1.]);
        assert_eq!(m.serial,recovery_serial);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation));
    }
    #[test]
    fn counter_critical_rays_are_distinct_source_ordered_events_without_head_multiplier() {
        for (attack,damage,count) in [(Attack::CounterCombo(0),60.,3),(Attack::CounterCombo(1),60.,2),(Attack::CounterCombo(2),60.,2),(Attack::CounterCombo(3),90.,3),(Attack::CounterJump,90.,2),(Attack::CounterRetaliation,90.,2)] {
            let mut m=Melee::default(); m.start(attack,ground()); m.tick(parity_content(), Default::default(),ground(),0.7,100.);
            let specs=m.hit_specs(parity_content()); assert_eq!(specs.len(),count);
            assert!(specs[0].critical); assert!(specs[1..].iter().all(|spec|!spec.critical));
            assert_eq!(specs.iter().map(|spec|spec.damage).sum::<f32>(),damage);
            assert_eq!(specs.iter().map(|spec|spec.hit_index).collect::<Vec<_>>(),(0..count as u8).collect::<Vec<_>>());
            assert!(matches!(specs[0].shape,HitShape::Ray {length:4.,..}));
        }
    }
    #[test]
    fn counter_drop_uses_current_planar_facing_at_launch_and_does_not_reemit_velocity() {
        use bevy::math::Vec3;
        for pitch in [0.,std::f32::consts::FRAC_PI_6,-std::f32::consts::FRAC_PI_6] {
            let mut m=Melee::default(); m.aim_direction=[0.,pitch.sin(),pitch.cos()]; m.start(Attack::CounterJump,ground());
            m.aim_direction=[pitch.cos(),pitch.sin(),0.];
            let output=m.tick(parity_content(), Default::default(),ground(),0.401,100.);
            let velocity=Vec3::from(output.velocity_world.unwrap());
            assert!(velocity.distance(Vec3::new(60.,-40.,0.))<0.00001);
            assert_eq!(output.terminal_velocity,Some(velocity.y));
            m.aim_direction=[0.,0.,-1.];
            let later=m.tick(parity_content(), Default::default(),ground(),0.01,100.);
            assert_eq!(later.terminal_velocity,Some(velocity.y));
            assert_eq!(later.velocity_world,None);
            assert_eq!(m.strike_direction(),[0.,0.,-1.]);
        }
    }
    #[test]
    fn counter_jump_native_timers_preserve_equality_and_drop_branch_priority() {
        let mut m=Melee::default();m.start(Attack::CounterJump,ground());
        assert!(m.tick(parity_content(), Default::default(),ground(),0.4,100.).suppress_ballistic);
        assert_eq!(m.tick(parity_content(), Default::default(),ground(),0.001,100.).velocity_world,Some([0.,-40.,60.]));
        m.tick(parity_content(), Default::default(),ground(),0.199,100.);
        assert_eq!(m.phase,AttackPhase::Normal);
        assert!(m.tick(parity_content(), Default::default(),ground(),0.001,100.).clear_velocity);
        let mut m=Melee::default();m.start(Attack::CounterJump,ground());
        assert!(m.tick(parity_content(), Default::default(),ground(),0.8,100.).velocity_world.is_some());
        assert_eq!(m.phase,AttackPhase::Normal);
        assert!(m.tick(parity_content(), Default::default(),ground(),0.001,100.).clear_velocity);
    }
    #[test]
    fn counter_jump_cooldown_survives_local_reset_and_blocks_equality() {
        let mut m=Melee::default(); m.equipped=Weapon::CounterSword; m.counter_jump_cooldown=1.5;
        m.reset(); assert_eq!(m.counter_jump_cooldown,1.5);
        let air=MeleeContext {grounded:false,plasma_jump_attack:true, guard_lower_busy: false };
        m.tick(parity_content(), press(),air,1.5,100.);
        m.tick(parity_content(), MeleeInput {primary_released:true,..Default::default()},air,0.,100.);
        assert!(m.attack.is_none());
        m.tick(parity_content(), press(),air,0.0005,100.);
        m.tick(parity_content(), MeleeInput {primary_released:true,..Default::default()},air,0.0005,100.);
        assert_eq!(m.attack,Some(Attack::CounterJump));
    }
    #[test]
    fn weapon_selection_survives_held_guard_and_switches_after_recovery() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.tick(parity_content(), MeleeInput {secondary_held:true,..Default::default()},ground(),0.016,100.);
        m.tick(parity_content(), MeleeInput {secondary_held:true,equip:Some(Weapon::Dagger),..Default::default()},ground(),0.3,100.);
        for _ in 0..10 {
            m.tick(parity_content(), MeleeInput {secondary_held:true,..Default::default()},ground(),0.1,100.);
            assert_eq!(m.equipped,Weapon::CounterSword);
            assert_eq!(m.attack,Some(Attack::CounterGuard));
        }
        m.tick(parity_content(), MeleeInput {secondary_released:true,..Default::default()},ground(),0.016,100.);
        assert_eq!(m.phase,AttackPhase::Delay);
        assert_eq!(m.equipped,Weapon::CounterSword);
        m.tick(parity_content(), Default::default(),ground(),0.501,100.);
        assert_eq!(m.equipped,Weapon::Dagger);
        assert_eq!(m.attack,None);
        assert_eq!(m.pending_weapon,None);
    }
    #[test]
    fn weapon_selection_is_one_latest_different_slot_and_reset_clears_it() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.start(Attack::CounterGuard,ground());
        for weapon in [Weapon::Dagger,Weapon::PlasmaSword,Weapon::CounterSword] {
            m.tick(parity_content(), MeleeInput {secondary_held:true,equip:Some(weapon),..Default::default()},ground(),0.016,100.);
        }
        assert_eq!(m.pending_weapon,Some(Weapon::PlasmaSword));
        m.reset();
        m.tick(parity_content(), Default::default(),ground(),0.016,100.);
        assert_eq!(m.equipped,Weapon::CounterSword);
        assert_eq!(m.pending_weapon,None);
    }
    #[test]
    fn weapon_selection_is_shared_with_other_attacks_and_survives_hit_interrupt() {
        let mut m=Melee {equipped:Weapon::Dagger,..Default::default()};
        m.start(Attack::DaggerDash,ground());
        m.tick(parity_content(), MeleeInput {equip:Some(Weapon::PlasmaSword),..Default::default()},ground(),0.016,100.);
        assert_eq!(m.equipped,Weapon::Dagger);
        m.interrupt_for_hit();
        m.tick(parity_content(), Default::default(),ground(),0.016,100.);
        assert_eq!(m.equipped,Weapon::PlasmaSword);
        assert_eq!(m.attack,None);
    }
    #[test]
    fn counter_guard_preserves_distinct_native_protection_and_counter_angles() {
        let mut m = Melee { equipped: Weapon::CounterSword, ..Default::default() };
        m.start(Attack::CounterGuard, ground());
        assert_eq!(m.guard_query(90.),Some(3000));
        assert_eq!(m.guard_query(90.001),None);
        assert!(!m.counter_query(0.));
        m.tick(parity_content(), MeleeInput {secondary_held:true,..Default::default()},ground(),0.501,100.);
        assert!(m.counter_query(59.99));
        assert!(!m.counter_query(60.));
        m.tick(parity_content(), MeleeInput {secondary_released:true,..Default::default()},ground(),0.01,100.);
        assert_eq!(m.guard_query(0.),None);
        assert!(!m.counter_query(0.));
    }
    #[test]
    fn counter_uppercut_has_its_own_eight_hundred_ms_followup_window() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.start(Attack::CounterRetaliation,ground());
        m.tick(parity_content(), press(),ground(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_released:true,..Default::default()},ground(),0.05,100.);
        assert_eq!(m.counter_followup,Some(Attack::CounterCombo(0)));
        m.tick(parity_content(), Default::default(),ground(),0.649,100.);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation));
        m.tick(parity_content(), Default::default(),ground(),0.002,100.);
        assert_eq!(m.attack,Some(Attack::CounterCombo(0)));
    }
    #[test]
    fn counter_jump_buffers_short_or_held_into_airborne_heavy_then_grounded_combo() {
        let air=MeleeContext {plasma_jump_attack:true,..Default::default()};
        for held in [false,true] {
            let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
            m.start(Attack::CounterJump,air);
            m.tick(parity_content(), press(),air,0.05,100.);
            m.tick(parity_content(), if held {MeleeInput {primary_held:true,..Default::default()}} else {MeleeInput {primary_released:true,..Default::default()}},air,0.15,100.);
            assert_eq!(m.counter_followup,Some(Attack::CounterRetaliation));
            m.tick(parity_content(), Default::default(),air,0.2,100.);
            m.tick(parity_content(), Default::default(),air,0.2,100.);
            m.tick(parity_content(), Default::default(),air,0.1,100.);
            assert_eq!(m.attack,Some(Attack::CounterJump),"700ms remains buffered");
            m.tick(parity_content(), Default::default(),air,0.001,100.);
            assert_eq!(m.attack,Some(Attack::CounterRetaliation));
            m.tick(parity_content(), press(),air,0.05,100.);
            m.tick(parity_content(), MeleeInput {primary_released:true,..Default::default()},air,0.05,100.);
            m.tick(parity_content(), Default::default(),ground(),0.701,100.);
            assert_eq!(m.attack,Some(Attack::CounterCombo(0)));
        }
    }
    #[test]
    fn counter_guard_held_secondary_returns_after_heavy_without_a_new_edge() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        let right=MeleeInput {secondary_held:true,..Default::default()};
        m.tick(parity_content(), right,ground(),0.016,100.);
        m.tick(parity_content(), right,ground(),0.501,100.);
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,..right},ground(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,..right},ground(),0.101,100.);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation));
        m.tick(parity_content(), right,ground(),1.,100.);
        m.tick(parity_content(), right,ground(),0.5,100.);
        assert_eq!(m.attack,Some(Attack::CounterGuard));
        assert_eq!(m.counter_stage,0);
    }
    #[test]
    fn counter_right_new_down_consumes_its_sample_before_continuous_guard() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.tick(parity_content(), MeleeInput {secondary_pressed:true,secondary_held:true,secondary_down_sample:true,..Default::default()},ground(),0.016,100.);
        assert!(m.attack.is_none());
        m.tick(parity_content(), MeleeInput {secondary_held:true,secondary_down_sample:true,..Default::default()},ground(),0.016,100.);
        assert!(m.attack.is_none(),"several fixed ticks still belong to the same new-down sample");
        m.tick(parity_content(), MeleeInput {secondary_held:true,..Default::default()},ground(),0.016,100.);
        assert_eq!(m.attack,Some(Attack::CounterGuard));
    }
    #[test]
    fn counter_guard_startup_consumes_early_heavy_until_release_and_repress() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        let right=MeleeInput {secondary_held:true,..Default::default()};
        m.tick(parity_content(), right,ground(),0.016,100.);
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,..right},ground(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,..right},ground(),0.101,100.);
        assert!(!m.primary_pending);
        m.tick(parity_content(), MeleeInput {primary_held:true,..right},ground(),0.5,100.);
        assert_eq!(m.attack,Some(Attack::CounterGuard));assert_eq!(m.counter_stage,1);
        m.tick(parity_content(), MeleeInput {primary_released:true,..right},ground(),0.01,100.);
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,..right},ground(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,..right},ground(),0.101,100.);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation));
    }
    #[test]
    fn counter_dodge_scan_defers_held_primary_recognition_instead_of_consuming_it() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,recognition_deferred:true,..Default::default()},ground(),0.201,100.);
        assert!(m.attack.is_none());assert!(m.primary_pending);
        m.tick(parity_content(), MeleeInput {primary_held:true,..Default::default()},ground(),0.016,100.);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation));
    }
    #[test]
    fn counter_preinput_keeps_the_first_event_and_rechecks_grounded_child() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.start(Attack::CounterCombo(0),ground());
        let mut output=m.tick(parity_content(), Default::default(),ground(),0.1,100.);
        m.tick_counter(Default::default(),ground(),true,false,&mut output);
        m.tick_counter(Default::default(),ground(),false,true,&mut output);
        assert_eq!(m.counter_followup,Some(Attack::CounterCombo(1)));
        m.tick(parity_content(), Default::default(),MeleeContext {grounded:false,..Default::default()},0.551,100.);
        assert_eq!(m.attack,Some(Attack::CounterCombo(0)));
        assert!(m.counter_followup.is_none());
    }
    #[test]
    fn counter_combo_uses_explicit_preinput_and_fourth_stage_is_terminal() {
        let mut m = Melee { equipped: Weapon::CounterSword, ..Default::default() };
        m.tick(parity_content(), press(), ground(), 0.01, 100.);
        m.tick(parity_content(), MeleeInput { primary_released: true, ..Default::default() }, ground(), 0.05, 100.);
        assert_eq!(m.attack, Some(Attack::CounterCombo(0)));
        for index in 0..3 {
            m.tick(parity_content(), press(), ground(), 0.05, 100.);
            m.tick(parity_content(), MeleeInput { primary_released: true, ..Default::default() }, ground(), 0.05, 100.);
            assert_eq!(m.attack, Some(Attack::CounterCombo(index)));
            m.tick(parity_content(), Default::default(), ground(), 0.549, 100.);
            assert_eq!(m.attack, Some(Attack::CounterCombo(index)));
            m.tick(parity_content(), Default::default(), ground(), 0.002, 100.);
            assert_eq!(m.attack, Some(Attack::CounterCombo(index + 1)));
        }
        m.tick(parity_content(), press(), ground(), 0.1, 100.);
        m.tick(parity_content(), MeleeInput { primary_released: true, ..Default::default() }, ground(), 0.05, 100.);
        m.tick(parity_content(), Default::default(), ground(), 0.6, 100.);
        assert_eq!(m.attack, Some(Attack::CounterCombo(3)));
        assert!(m.counter_followup.is_none());
    }
    #[test]
    fn counter_guard_release_and_active_input_have_separate_gates() {
        let mut m = Melee { equipped: Weapon::CounterSword, ..Default::default() };
        m.tick(parity_content(), MeleeInput { secondary_held:true, ..Default::default() }, ground(), 0.01, 100.);
        m.tick(parity_content(), MeleeInput { secondary_held:true, ..Default::default() }, ground(), 0.2, 100.);
        assert_eq!(m.counter_stage,0);
        let out=m.tick(parity_content(), MeleeInput { secondary_held:true, ..Default::default() }, ground(), 0.002, 100.);
        assert_eq!(m.counter_stage,1);
        assert!((out.movement_scale - 1.0).abs() < 1e-6);
        m.tick(parity_content(), MeleeInput { secondary_released:true, ..Default::default() }, ground(), 0.01,100.);
        assert_eq!(m.phase,AttackPhase::Delay);
        m.tick(parity_content(), Default::default(),ground(),0.5,100.);
        assert_eq!(m.attack,Some(Attack::CounterGuard));
        m.tick(parity_content(), Default::default(),ground(),0.002,100.);
        assert!(m.attack.is_none());
    }
    #[test]
    fn counter_guard_moving_walker_can_chain_but_lower_jump_cannot() {
        for (busy,grounded,expected) in [(false,true,Attack::CounterCombo(0)),(true,true,Attack::CounterGuard),(false,false,Attack::CounterGuard)] {
            let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
            m.start(Attack::CounterGuard,ground());m.counter_stage=1;
            m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,movement_requested:true,..Default::default()},ground(),0.05,100.);
            m.tick(parity_content(), MeleeInput {primary_released:true,movement_requested:true,..Default::default()},MeleeContext {grounded,guard_lower_busy:busy,..Default::default()},0.01,100.);
            assert_eq!(m.attack,Some(expected));
        }
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.start(Attack::CounterGuard,ground());m.counter_stage=1;
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,..Default::default()},MeleeContext::default(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,..Default::default()},MeleeContext::default(),0.101,100.);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation),"retained walker permits airborne strong, whose child is not ground-only");
    }
    #[test]
    fn counter_sprint_gate_discards_recognized_events_but_keeps_unexpired_hold_age() {
        let mut m=Melee {equipped:Weapon::CounterSword,..Default::default()};
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,dispatch_blocked:true,..Default::default()},ground(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,dispatch_blocked:true,..Default::default()},ground(),0.11,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,..Default::default()},ground(),0.4,100.);
        assert!(m.attack.is_none(),"held recognition already expired inside gated input map");
        m.tick(parity_content(), MeleeInput {primary_released:true,..Default::default()},ground(),0.01,100.);
        m.tick(parity_content(), MeleeInput {primary_pressed:true,primary_held:true,dispatch_blocked:true,..Default::default()},ground(),0.1,100.);
        m.tick(parity_content(), MeleeInput {primary_held:true,..Default::default()},ground(),0.11,100.);
        assert_eq!(m.attack,Some(Attack::CounterRetaliation));
    }
    #[test]
    fn counter_jump_freezes_then_drops_until_its_separate_normal_timer() {
        let mut m = Melee { equipped: Weapon::CounterSword, ..Default::default() };
        let air=MeleeContext { grounded:false,plasma_jump_attack:true, guard_lower_busy: false };
        m.tick(parity_content(), press(),air,0.01,100.);
        let entered=m.tick(parity_content(), MeleeInput { primary_released:true,..Default::default() },air,0.01,100.);
        assert_eq!(m.attack,Some(Attack::CounterJump));
        assert!(!entered.suppress_ballistic,"entry preserves one outgoing ballistic pass");
        m.tick(parity_content(), Default::default(),air,0.399,100.);
        let drop=m.tick(parity_content(), Default::default(),air,0.002,100.);
        assert_eq!(drop.velocity_world,Some([0., -40., 60.]));
        assert_eq!(drop.terminal_velocity,Some(-40.));
        assert!(!drop.suppress_ballistic);
        let next=m.tick(parity_content(), Default::default(),air,0.05,100.);
        assert_eq!(m.phase,AttackPhase::Normal);
        assert_eq!(next.velocity_world,None);
        let recovery=m.tick(parity_content(), Default::default(),air,0.15,100.);
        assert_eq!(m.phase,AttackPhase::Delay);
        assert!(recovery.clear_velocity);
        let hit=m.hit_spec(parity_content()).unwrap();
        assert_eq!(hit.damage,45.);
        assert!(matches!(hit.shape,HitShape::Cylinder { radius:5.,height:6.,.. }));
    }
    #[test]
    fn every_crossed_hit_survives_a_coarse_physics_step() {
        for attack in [Attack::CounterCombo(0),Attack::CounterCombo(3),Attack::DaggerDash,Attack::PlasmaDash] {
            let mut m=Melee::default(); m.start(attack,ground());
            m.tick(parity_content(), Default::default(),ground(),0.7,100.);
            let hits=m.hit_specs(parity_content());
            let expected=3;
            assert_eq!(hits.len(),expected,"{attack:?}");
            assert!(hits.windows(2).all(|h|h[0].hit_index!=h[1].hit_index));
        }
    }
    fn ground() -> MeleeContext {
        MeleeContext {
            grounded: true,
            ..Default::default()
        }
    }
    fn press() -> MeleeInput {
        MeleeInput {
            primary_pressed: true,
            primary_held: true,
            ..Default::default()
        }
    }
    #[test]
    fn source_equipment_rates_and_reset() {
        let mut m = Melee {
            equipped: Weapon::Dagger,
            ..Default::default()
        };
        assert_eq!(
            m.tick(parity_content(), Default::default(), ground(), 0.016, 100.0)
                .movement_scale,
            1.2
        );
        m.reset();
        assert_eq!(m.equipped, Weapon::Dagger);
        assert_eq!(Weapon::PlasmaSword.move_speed_rate(), 1.1);
    }
    #[test]
    fn dagger_air_interrupt_preserves_ballistic_velocity() {
        let mut m = Melee {
            equipped: Weapon::Dagger,
            ..Default::default()
        };
        let out = m.tick(parity_content(), MeleeInput {primary_held:true,..Default::default()},
            MeleeContext {
                grounded: false,
                plasma_jump_attack: true, guard_lower_busy: false },
            0.016,
            100.0,
        );
        assert!(out.cancel_movement);
        assert_eq!(out.velocity_y, None);
        assert!(!out.clear_velocity);
        assert_eq!(m.attack, Some(Attack::DaggerAir));
        assert_eq!(out.spend_sp, 0.0);
    }
    #[test]
    fn dagger_cut_has_strict_expiry_separate_delay_and_no_same_tick_repeat() {
        let mut m=Melee {equipped:Weapon::Dagger,..Default::default()};
        let air=MeleeContext::default();
        let held=MeleeInput {primary_held:true,..Default::default()};
        assert!(!m.tick(parity_content(), press(),air,0.016,100.).cancel_movement);
        assert!(m.tick(parity_content(), held,air,0.016,100.).cancel_movement);
        let serial=m.serial;
        m.tick(parity_content(), held,air,0.55,100.);
        assert_eq!(m.phase,AttackPhase::Normal,"equal550ms is not expired");
        let end=m.tick(parity_content(), held,air,0.001,100.);
        assert_eq!(m.phase,AttackPhase::Delay);assert!(end.clear_velocity);
        assert_eq!(m.attack,Some(Attack::DaggerAir));
        assert_eq!(end.movement_scale,0.6,"outgoing normal still steers");
        let exit=m.tick(parity_content(), held,air,0.001,100.);
        assert!(m.attack.is_none());assert_eq!(exit.movement_scale,0.);
        assert!(!exit.cancel_movement);assert!(!exit.clear_velocity);
        assert!(m.tick(parity_content(), held,air,0.016,100.).cancel_movement);
        assert!(m.serial>serial);assert_eq!(m.dagger_slash_index(),1);
    }
    #[test]
    fn plasma_release_and_hold_follow_two_hundred_ms_recognizers() {
        let mut m = Melee {
            equipped: Weapon::PlasmaSword,
            ..Default::default()
        };
        let pending = m.tick(parity_content(), press(), ground(), 0.016, 100.0);
        assert!(m.attack.is_none());
        assert!(!pending.suppress_movement_actions);
        assert_eq!(pending.movement_scale, 1.1);
        m.tick(parity_content(), MeleeInput {
                primary_released: true,
                ..Default::default()
            },
            ground(),
            0.016,
            100.0,
        );
        assert_eq!(m.attack, Some(Attack::PlasmaWeak(0)));
        assert_eq!(m.animation_duration_ms(), 500.0);
        m.reset();
        m.tick(parity_content(), press(), ground(), 0.1, 100.0);
        let strong = m.tick(parity_content(), MeleeInput {
                primary_held: true,
                ..Default::default()
            },
            ground(),
            0.1,
            100.0,
        );
        assert_eq!(m.attack, Some(Attack::PlasmaStrong));
        assert_eq!(m.animation_duration_ms(), 1000.0);
        assert_eq!(strong.movement_scale, 0.8);
        assert!(m.full_body);
        for _ in 0..15 {
            m.tick(parity_content(), MeleeInput {
                    primary_held: true,
                    ..Default::default()
                },
                ground(),
                0.1,
                100.0,
            );
        }
        assert!(m.attack.is_none(), "held input8 fires once until release");
        m.tick(parity_content(), MeleeInput {
                primary_released: true,
                ..Default::default()
            },
            ground(),
            0.016,
            100.0,
        );
        assert!(
            m.attack.is_none(),
            "release after held attack does not emit weak7"
        );
        m.reset();
        m.tick(parity_content(), press(), ground(), 0.1, 100.0);
        m.tick(parity_content(), Default::default(), ground(), 0.1, 100.0);
        m.tick(parity_content(), MeleeInput {
                primary_released: true,
                ..Default::default()
            },
            ground(),
            0.016,
            100.0,
        );
        assert!(m.attack.is_none());
    }
    #[test]
    fn moving_plasma_strong_releases_lower_body_after_five_hundred_ms() {
        for context in [ground(), MeleeContext::default()] {
            let mut m = Melee {
                equipped: Weapon::PlasmaSword,
                ..Default::default()
            };
            m.start(Attack::PlasmaStrong, context);
            let serial = m.serial;
            assert!(m.full_body);
            m.tick(parity_content(), Default::default(), context, 0.499, 100.0);
            assert!(m.full_body);
            m.tick(parity_content(), Default::default(), context, 0.002, 100.0);
            assert!(!m.full_body);
            assert_eq!(m.attack, Some(Attack::PlasmaStrong));
            assert_eq!(m.phase, AttackPhase::Normal);
            assert_eq!(
                m.serial, serial,
                "lower-part transition preserves upper clock"
            );
            assert!((m.elapsed - 0.501).abs() < 0.00001);
            assert_eq!(m.animation_duration_ms(), 1000.0);
        }
        let mut m = Melee {
            equipped: Weapon::PlasmaSword,
            ..Default::default()
        };
        m.start(Attack::PlasmaWeak(0), ground());
        assert!(!m.full_body, "weak43 has zero lower-part delay");
        m.start(Attack::PlasmaDashStrong, ground());
        m.tick(parity_content(), Default::default(), ground(), 0.6, 100.0);
        assert!(
            m.full_body,
            "generic strong46 has no moving-attack transition"
        );
    }
    #[test]
    fn plasma_dash_tap_and_hold_interrupt_both_phases_into_generic_strong() {
        for phase in [AttackPhase::Normal, AttackPhase::Delay] {
            for held in [false, true] {
                let mut m = Melee {
                    equipped: Weapon::PlasmaSword,
                    ..Default::default()
                };
                m.start(Attack::PlasmaDash, ground());
                if phase == AttackPhase::Delay {
                    m.tick(parity_content(), Default::default(), ground(), 0.8, 100.0);
                }
                assert_eq!(m.phase, phase);
                m.tick(parity_content(), press(), ground(), 0.05, 100.0);
                assert_eq!(m.attack, Some(Attack::PlasmaDash));
                let input = if held {
                    MeleeInput {
                        primary_held: true,
                        ..Default::default()
                    }
                } else {
                    MeleeInput {
                        primary_released: true,
                        ..Default::default()
                    }
                };
                let out = m.tick(parity_content(), input, ground(), if held { 0.16 } else { 0.016 }, 100.0);
                assert_eq!(m.attack, Some(Attack::PlasmaDashStrong));
                assert_eq!(m.phase, AttackPhase::Normal);
                assert_eq!(m.elapsed, 0.0);
                assert!(m.full_body);
                assert!(out.cancel_movement);
                assert!(out.suppress_movement_actions);
                assert_eq!(out.movement_scale, 0.0);
                assert_eq!(out.forward_speed, None);
                assert_eq!(out.lateral_speed, None);
                assert!(!out.suppress_ballistic);
                assert!(!out.clear_velocity);
                assert_eq!(out.velocity_y, None);
                assert_eq!(out.terminal_velocity, None);
                assert_eq!(out.spend_sp, 0.0);
            }
        }
    }
    #[test]
    fn plasma_dash_strong_has_one_heavy_hit_and_timed_velocity_clear() {
        let mut m = Melee {
            equipped: Weapon::PlasmaSword,
            ..Default::default()
        };
        m.start(Attack::PlasmaDashStrong, ground());
        assert_eq!(m.animation_duration_ms(), 1000.0);
        let mut hits = Vec::new();
        for _ in 0..50 {
            let out = m.tick(parity_content(), Default::default(), ground(), 0.01, 100.0);
            assert!(!out.clear_velocity);
            assert!(!out.suppress_ballistic);
            if let Some(hit) = m.hit_spec(parity_content()) {
                hits.push((hit.hit_index, hit.damage, hit.push, hit.lift, hit.attribute));
            }
        }
        assert_eq!(
            hits,
            vec![(
                0,
                34.0,
                30.0,
                20.0,
                crate::combat_types::AttackAttributeId::PlasmaStandStrong
            )]
        );
        let delay = m.tick(parity_content(), Default::default(), ground(), 0.51, 100.0);
        assert_eq!(m.attack, Some(Attack::PlasmaDashStrong));
        assert_eq!(m.phase, AttackPhase::Delay);
        assert_eq!(m.animation_duration_ms(), 40.0);
        assert!(delay.clear_velocity);
        let recovering = m.tick(parity_content(), press(), ground(), 0.03, 100.0);
        assert_eq!(m.attack, Some(Attack::PlasmaDashStrong));
        assert!(!recovering.clear_velocity);
        m.tick(parity_content(), MeleeInput {
                primary_released: true,
                ..Default::default()
            },
            ground(),
            0.011,
            100.0,
        );
        assert_eq!(m.attack, Some(Attack::PlasmaWeak(0)));
    }
    #[test]
    fn grounded_right_click_only_and_hit_stops_source_root_motion() {
        let mut m = Melee {
            equipped: Weapon::Dagger,
            ..Default::default()
        };
        let right = MeleeInput {
            secondary_pressed: true,
            ..Default::default()
        };
        m.tick(parity_content(), right, MeleeContext::default(), 0.016, 100.0);
        assert!(m.attack.is_none());
        m.tick(parity_content(), right, ground(), 0.016, 100.0);
        let moving = m.tick(parity_content(), Default::default(), ground(), 0.1, 100.0);
        assert!(moving.forward_speed.unwrap() > 1.0);
        m.confirm_hit();
        let stopped = m.tick(parity_content(), Default::default(), ground(), 0.016, 100.0);
        assert_eq!(stopped.forward_speed, Some(0.0));
        assert_eq!(m.attack, Some(Attack::DaggerDash));
    }
    #[test]
    fn dagger_dash_uses_three_distinct_source_hit_events() {
        let mut m = Melee {
            equipped: Weapon::Dagger,
            ..Default::default()
        };
        m.tick(parity_content(), MeleeInput {
                secondary_pressed: true,
                ..Default::default()
            },
            ground(),
            0.016,
            100.0,
        );
        let mut hits = Vec::new();
        for _ in 0..70 {
            m.tick(parity_content(), Default::default(), ground(), 0.016, 100.0);
            if let Some(h) = m.hit_spec(parity_content()) {
                hits.push((h.hit_index, h.damage));
            }
        }
        assert_eq!(hits, vec![(0, 50.0), (1, 25.0), (2, 62.5)]);
    }
    #[test]
    fn jump_drop_waits_four_hundred_ms_then_delays_without_landing() {
        let mut m = Melee {
            equipped: Weapon::PlasmaSword,
            ..Default::default()
        };
        let air = MeleeContext {
            grounded: false,
            plasma_jump_attack: true, guard_lower_busy: false };
        m.tick(parity_content(), MeleeInput {
                primary_pressed: true,
                primary_released: true,
                ..Default::default()
            },
            air,
            0.016,
            100.0,
        );
        for _ in 0..24 {
            assert_eq!(
                m.tick(parity_content(), Default::default(), air, 0.016, 100.0).velocity_y,
                None
            );
        }
        assert_eq!(
            m.tick(parity_content(), Default::default(), air, 0.016, 100.0).velocity_y,
            Some(-500.0)
        );
        let delay = m.tick(parity_content(), Default::default(), air, 0.016, 100.0);
        assert_eq!(m.phase, AttackPhase::Delay);
        assert_eq!(m.phase_elapsed, 0.0);
        assert!(delay.clear_velocity);
        assert_eq!(delay.terminal_velocity, Some(-500.0));
    }
    #[test]
    fn plasma_press_during_cut_can_release_after_cut_without_being_dropped() {
        let mut m = Melee { equipped: Weapon::PlasmaSword, ..Default::default() };
        let tap = MeleeInput { primary_pressed: true, primary_released: true, ..Default::default() };
        m.tick(parity_content(), tap, ground(), 0.016, 100.);
        m.tick(parity_content(), Default::default(), ground(), 0.4, 100.);
        m.tick(parity_content(), MeleeInput { primary_pressed: true, primary_held: true, ..Default::default() }, ground(), 0.08, 100.);
        assert_eq!(m.attack, Some(Attack::PlasmaWeak(0)));
        m.tick(parity_content(), MeleeInput { primary_released: true, ..Default::default() }, ground(), 0.03, 100.);
        assert_eq!(m.attack, Some(Attack::PlasmaWeak(1)));
        m.tick(parity_content(), tap, ground(), 0.1, 100.);
        m.tick(parity_content(), Default::default(), ground(), 0.41, 100.);
        assert_eq!(m.attack, None);
    }

    #[test]
    fn plasma_hold_recognizer_consumes_once_even_during_an_unbound_cut() {
        let mut m = Melee { equipped: Weapon::PlasmaSword, ..Default::default() };
        m.tick(parity_content(), MeleeInput { primary_pressed: true, primary_released: true, ..Default::default() }, ground(), 0.016, 100.);
        m.tick(parity_content(), MeleeInput { primary_pressed: true, primary_held: true, ..Default::default() }, ground(), 0.1, 100.);
        for _ in 0..10 {
            m.tick(parity_content(), MeleeInput { primary_held: true, ..Default::default() }, ground(), 0.1, 100.);
        }
        assert_eq!(m.attack, None);
    }

    #[test]
    fn dagger_secondary_has_source_1000ms_active_and_late_movement_cancel() {
        let mut m = Melee { equipped: Weapon::Dagger, ..Default::default() };
        m.tick(parity_content(), MeleeInput { secondary_pressed: true, ..Default::default() }, ground(), 0.016, 100.);
        assert_eq!(m.animation_duration_ms(), 1000.);
        m.tick(parity_content(), MeleeInput { movement_requested: true, ..Default::default() }, ground(), 0.999, 100.);
        assert_eq!(m.phase, AttackPhase::Normal);
        m.tick(parity_content(), Default::default(), ground(), 0.0011, 100.);
        assert_eq!(m.phase, AttackPhase::Delay);
        m.tick(parity_content(), MeleeInput { movement_requested: true, ..Default::default() }, ground(), 0.450, 100.);
        assert_eq!(m.attack, Some(Attack::DaggerDash));
        m.tick(parity_content(), MeleeInput { movement_requested: true, ..Default::default() }, ground(), 0.001, 100.);
        assert_eq!(m.attack, None);
    }

    #[test]
    fn root_track_displacement_is_independent_of_physics_rate() {
        for hz in [60.0_f32, 62.5, 120.0, 240.0] {
            let dt = 1.0 / hz;
            let mut z = 0.0;
            let mut age = 0.0_f32;
            while age < 1.0 {
                let next = (age + dt).min(1.0);
                z += parity_content().root_speed(Attack::DaggerDash, age, next, next - age).0 * (next - age);
                age = next;
            }
            assert!((z - 10.724391).abs() < 0.0001, "{hz}: {z}");
        }
    }
}


#[cfg(test)]
mod attack_definition_tests {
    use crate::content::simulation::parity_content;
    use super::*;
    use crate::combat_types::AttackAttributeId as Id;

    fn crossed(attack: Attack, previous: f32, elapsed: f32) -> Vec<HitSpec> {
        Melee {
            attack: Some(attack),
            previous_elapsed: previous,
            elapsed,
            ..Default::default()
        }
        .hit_specs(parity_content())
    }

    #[test]
    fn counter_crossings_keep_source_indices_and_strict_lower_inclusive_upper_boundary() {
        let attack = Attack::CounterCombo(0);
        assert!(crossed(attack, 0., 0.1_f32.next_down()).is_empty());
        let early = crossed(attack, 0., 0.1);
        assert_eq!(early.iter().map(|s| s.hit_index).collect::<Vec<_>>(), [1]);
        assert!(!early[0].critical);
        assert!(crossed(attack, 0.1, 0.35_f32.next_down()).is_empty());
        let late = crossed(attack, 0.1, 0.35);
        assert_eq!(late.iter().map(|s| s.hit_index).collect::<Vec<_>>(), [0, 2]);
        assert!(late[0].critical);
        let coarse = crossed(attack, 0., 1.);
        assert_eq!(
            coarse.iter().map(|s| s.hit_index).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(crossed(attack, 0.35, 0.35_f32.next_up()).is_empty());
        assert!(crossed(Attack::CounterGuard, 0., 1.).is_empty());
        assert!(crossed(Attack::CounterDash, 0., 1.).is_empty());
    }

    #[test]
    fn combo_fallback_keeps_distinct_schedule_damage_and_critical_matching() {
        for stage in [3, 4, u8::MAX] {
            let hits = crossed(Attack::CounterCombo(stage), 0., 1.);
            assert_eq!(
                hits.iter().map(|s| s.hit_index).collect::<Vec<_>>(),
                [0, 1, 2]
            );
            assert_eq!(hits[0].damage, if stage == 3 { 45. } else { 30. });
            assert_eq!(hits[0].attribute, Id::CounterCritical);
            assert_eq!(hits[1].damage, 22.5);
            assert_eq!(hits[1].attribute, Id::CounterAttack4);
            assert_eq!(hits[1].range, 5.);
            assert_eq!(
                crossed(Attack::CounterCombo(stage), 0., 0.05)[0].hit_index,
                1
            );
        }
    }

    #[test]
    fn multi_hit_attributes_and_dagger_damage_keep_their_event_indices() {
        let dash = crossed(Attack::PlasmaDash, 0., 1.);
        assert_eq!(
            dash.iter().map(|s| s.attribute).collect::<Vec<_>>(),
            [
                Id::PlasmaAttack2Weak,
                Id::PlasmaAttack2Weak,
                Id::PlasmaAttack2
            ]
        );
        let dagger = crossed(Attack::DaggerDash, 0., 1.);
        assert_eq!(
            dagger.iter().map(|s| s.damage).collect::<Vec<_>>(),
            [50., 25., 62.5]
        );
        assert!(
            dagger
                .iter()
                .all(|s| s.attribute == Id::DaggerAttack1 && !s.critical)
        );
        for stage in [0, 1, 2, 3, 4, u8::MAX] {
            assert_eq!(
                crossed(Attack::PlasmaWeak(stage), 0., 1.),
                crossed(Attack::PlasmaWeak(0), 0., 1.)
            );
            assert_eq!(
                crossed(Attack::DaggerSlash(stage), 0., 1.),
                crossed(Attack::DaggerAir, 0., 1.)
            );
        }
    }

    #[test]
    fn critical_companions_keep_ray_origins_and_clear_ordinary_reaction_fields() {
        for attack in [
            Attack::CounterCombo(3),
            Attack::CounterJump,
            Attack::CounterRetaliation,
        ] {
            let hits = crossed(attack, 0., 1.);
            let ray = hits[0];
            assert!(ray.critical);
            assert_eq!(
                (ray.damage, ray.range, ray.push, ray.lift, ray.stun),
                (45., 4., 0., 0., 0.)
            );
            assert_eq!(
                ray.shape,
                HitShape::Ray {
                    origin: [
                        0.,
                        2.5,
                        if attack == Attack::CounterJump {
                            0.
                        } else {
                            1.
                        }
                    ],
                    direction: [0., 0., 1.],
                    length: 4.
                }
            );
            assert_eq!(
                ray.attribute,
                if attack == Attack::CounterRetaliation {
                    Id::CounterCounterCritical
                } else {
                    Id::CounterCritical
                }
            );
            assert!(!hits[1].critical);
        }
    }
}
