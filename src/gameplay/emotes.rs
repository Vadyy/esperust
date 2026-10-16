use super::{EmotePlayback, MovementAction, MovementInput, Player};

impl Player {
    pub(crate) fn can_emote(&self) -> bool {
        self.health > 0.
            && self.grounded
            && self.hit_reaction.is_none()
            && self.action == MovementAction::None
            && self.melee.attack.is_none()
            && self.movement.length_squared() < 0.01
    }

    pub(super) fn step_emote(&mut self, input: MovementInput, duration: Option<f32>, dt: f32) {
        let interrupted = input.cancel_emote
            || input.movement.length_squared() > 0.01
            || input.jump
            || input.dodge
            || input.sprint
            || input.crouch
            || input.melee.primary_pressed
            || input.melee.primary_held
            || input.melee.secondary_pressed
            || input.melee.secondary_held
            || input.melee.equip.is_some()
            || !self.can_emote();
        if interrupted {
            self.emote = None;
            return;
        }
        if let (Some(index), Some(duration)) = (input.emote, duration)
            && duration.is_finite()
            && duration > 0.
        {
            self.emote = Some(EmotePlayback {
                index,
                age: 0.,
                duration,
                yaw: input.yaw,
            });
            self.emote_serial = self.emote_serial.wrapping_add(1);
        } else if let Some(emote) = &mut self.emote {
            emote.age += dt;
            if emote.age >= emote.duration {
                self.emote = None;
            }
        }
    }

    pub(crate) fn queue_hug(controls: &mut super::Controls) {
        controls.emote = Some(crate::content::emotes::HUG);
        controls.cancel_emote = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gameplay::Motion,
        melee::{MeleeInput, Weapon},
    };
    use bevy::prelude::*;

    fn actor() -> Player {
        let mut p = Player::new(Vec3::ZERO, &Motion::default());
        p.grounded = true;
        p
    }
    fn start(p: &mut Player) {
        p.step_emote(
            MovementInput {
                emote: Some(0),
                ..default()
            },
            Some(1.),
            0.1,
        );
        assert!(p.emote.is_some());
    }
    #[test]
    fn emote_finishes_at_natural_duration_and_repeats_restart_once() {
        let mut p = actor();
        start(&mut p);
        p.step_emote(default(), None, 0.75);
        assert_eq!(p.emote.unwrap().age, 0.75);
        start(&mut p);
        assert_eq!(p.emote_serial, 2);
        assert_eq!(p.emote.unwrap().age, 0.);
        p.step_emote(default(), None, 1.);
        assert!(p.emote.is_none());
    }
    #[test]
    fn emotes_cancel_for_gameplay_and_focus_or_chat_commands() {
        for input in [
            MovementInput {
                movement: Vec2::Y,
                ..default()
            },
            MovementInput {
                jump: true,
                ..default()
            },
            MovementInput {
                dodge: true,
                ..default()
            },
            MovementInput {
                sprint: true,
                ..default()
            },
            MovementInput {
                crouch: true,
                ..default()
            },
            MovementInput {
                cancel_emote: true,
                ..default()
            },
            MovementInput {
                melee: MeleeInput {
                    primary_held: true,
                    ..default()
                },
                ..default()
            },
            MovementInput {
                melee: MeleeInput {
                    secondary_pressed: true,
                    ..default()
                },
                ..default()
            },
            MovementInput {
                melee: MeleeInput {
                    equip: Some(Weapon::Dagger),
                    ..default()
                },
                ..default()
            },
        ] {
            let mut p = actor();
            start(&mut p);
            p.step_emote(input, None, 0.1);
            assert!(p.emote.is_none());
        }
    }
    #[test]
    fn emotes_reject_airborne_dead_attacking_and_invalid_requests() {
        for mode in 0..5 {
            let mut p = actor();
            let duration = match mode {
                0 => {
                    p.grounded = false;
                    Some(1.)
                }
                1 => {
                    p.health = 0.;
                    Some(1.)
                }
                2 => {
                    p.melee.attack = Some(crate::melee::Attack::PlasmaStrong);
                    Some(1.)
                }
                3 => None,
                _ => Some(f32::NAN),
            };
            p.step_emote(
                MovementInput {
                    emote: Some(0),
                    ..default()
                },
                duration,
                0.1,
            );
            assert!(p.emote.is_none());
            assert_eq!(p.emote_serial, 0);
        }
    }
    #[test]
    fn reset_death_and_stun_clear_emotes_immediately() {
        let mut p = actor();
        start(&mut p);
        p.reset_motion(Vec3::Y);
        assert!(p.emote.is_none());
        p.grounded = true;
        start(&mut p);
        p.receive_counter_stun(1.);
        assert!(p.emote.is_none());
        p.hit_reaction = None;
        start(&mut p);
        p.die();
        assert!(p.emote.is_none());
    }
}
