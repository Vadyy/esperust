mod config;
mod diagnostics;
mod emotes;
mod input;
mod presentation;
mod runtime;
mod setup;
mod skills;
mod simulation;

pub use config::Motion;
pub use diagnostics::{FpsCounter, Help, MapTitle, Status, hud};
pub use input::read_input;
pub use presentation::{Avatar, View, present};
pub(crate) use presentation::{CameraShake, CameraShakeRequest, capture_camera_shake};
pub use runtime::{prepare_input, reset_for_level, simulate};
pub use setup::setup;
pub use skills::Skill;

use crate::{
    controller::MapContact,
    melee::{Melee, MeleeInput},
};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct Controls {
    movement: Vec2,
    jump: bool,
    respawn: bool,
    crouch: bool,
    dodge: bool,
    dodge_side: f32,
    sprint: bool,
    last_forward_press: Option<f32>,
    captured: bool,
    yaw: f32,
    pitch: f32,
    pub diagnostics: bool,
    melee: MeleeInput,
    emote: Option<usize>,
    cancel_emote: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MovementAction {
    #[default]
    None,
    Sprint,
    DodgeLeft,
    DodgeRight,
    WallJump,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallJumpPhase {
    #[default]
    None,
    Windup,
    LaunchA,
    LaunchB,
    AirA,
    AirB,
    Landing,
    Failed,
}

impl WallJumpPhase {
    fn launches(self) -> bool {
        matches!(self, Self::LaunchA | Self::LaunchB)
    }

    fn accepts_actions(self) -> bool {
        matches!(
            self,
            Self::LaunchA | Self::LaunchB | Self::AirA | Self::AirB | Self::Failed
        )
    }
}

impl MovementAction {
    fn is_dodge(self) -> bool {
        matches!(self, Self::DodgeLeft | Self::DodgeRight)
    }
}

#[derive(Resource, Clone, Copy, Default)]
pub struct MovementInput {
    pub movement: Vec2,
    pub jump: bool,
    pub dodge: bool,
    pub dodge_side: f32,
    pub sprint: bool,
    pub crouch: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub melee: MeleeInput,
    pub emote: Option<usize>,
    pub cancel_emote: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EmotePlayback {
    pub index: usize,
    pub age: f32,
    pub duration: f32,
    pub yaw: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitReactionKind {
    Damage(u8),
    Stun,
    Blow,
    Down,
    DownDamage,
    StandUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LowerJump {
    pub phase: u8,
    pub age: f32,
}
impl LowerJump {
    pub fn enter(velocity_y: f32) -> Self {
        Self {
            phase: if velocity_y > 0. { 1 } else { 2 },
            age: 0.,
        }
    }
    fn advance(&mut self, grounded: bool, velocity_y: f32, dt: f32) -> bool {
        self.age += dt;
        if grounded && self.phase != 3 {
            self.phase = 3;
            self.age = 0.;
        } else if self.phase == 1 && velocity_y <= 0. {
            self.phase = 2;
        }
        self.phase == 3 && (self.age as f64 * 1000. + 0.0001).floor() > 300.
    }
    pub fn action(self) -> &'static str {
        match self.phase {
            1 if self.duration_ms() > 0. => "lower_jump_rise",
            2 if self.duration_ms() > 0. => "lower_jump_fall",
            1 | 2 => "lower_jump_fall_natural",
            _ => "lower_jump_land",
        }
    }
    pub fn duration_ms(self) -> f64 {
        match self.phase {
            1 | 2 if (self.age as f64 * 1000. + 0.0001).floor() <= 700. => 700.,
            3 => 300.,
            _ => 0.,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct HitReaction {
    pub kind: HitReactionKind,
    pub age: f32,
    pub duration: f32,
    pub moving: bool,
    pub phase: u8,
}

#[derive(Resource)]
pub struct Player {
    pub(crate) emote: Option<EmotePlayback>,
    pub(crate) emote_serial: u64,
    pub feet: Vec3,
    previous: Vec3,
    velocity: Vec3,
    pub grounded: bool,
    pub distance: f32,
    pub jumps: u32,
    pub respawns: u32,
    pub movement: Vec2,
    pub vertical_speed: f32,
    pub landing_age: f32,
    pub fall_landing: bool,
    pub action: MovementAction,
    pub action_age: f32,
    pub action_serial: u32,
    pub pose_reset_serial: u32,
    pub sp: f32,
    equipped_skill: Skill,
    pub health: f32,
    pub max_health: f32,
    pub god_mode: bool,
    pub death_age: Option<f32>,
    pub death_serial: u32,
    pub dodges: u32,
    pub air_dodges: u32,
    pub air_sprints: u32,
    pub wall_jumps: u32,
    pub wall_jump_failures: u32,
    pub sprint_distance: f32,
    action_direction: Vec3,
    walljump_launched: bool,
    walljump_used: bool,
    pub walljump_landing: bool,
    pub walljump_phase: WallJumpPhase,
    pub walljump_phase_age: f32,
    pub wall_normal: Vec3,
    wall_contact: Option<MapContact>,
    pub actor_facing: Vec3,
    sp_recovery_age: f32,
    sp_capacity: f32,
    last_dodge_age: f32,
    normal_jump_active: bool,
    pub jump_started_moving: bool,
    pub(crate) guard_jump: Option<LowerJump>,
    pub(crate) dagger_jump: Option<LowerJump>,
    pub(crate) guard_recovery_lower_override: bool,
    guard_recovery_mode: u8,
    walljump_velocity: Vec3,
    ballistic_horizontal: Vec3,
    counter_ballistic_suppressed: bool,
    counter_terminal_velocity: Option<f32>,
    pub melee: Melee,
    pub(crate) hit_reaction: Option<HitReaction>,
    pub(crate) hit_serial: u64,
}
