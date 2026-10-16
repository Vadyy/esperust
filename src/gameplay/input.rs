use super::{Controls, Motion};
use crate::melee::{MeleeInput, Weapon};
use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

impl Controls {
    pub(crate) fn practice_camera(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch;
    }

    pub(crate) fn practice_movement(&mut self, movement: Vec2, yaw: f32) {
        self.movement = movement;
        self.yaw = yaw;
        self.jump = false;
        self.crouch = false;
        self.dodge = false;
        self.sprint = false;
        self.melee = MeleeInput::default();
    }

    pub(super) fn clear_actions(&mut self) {
        self.dodge = false;
        self.dodge_side = 0.0;
        self.sprint = false;
        self.last_forward_press = None;
        self.melee = MeleeInput::default();
    }

    pub(super) fn forward_key(&mut self, pressed: bool, held: bool, now: f32, window: f32) {
        if pressed {
            self.sprint = self
                .last_forward_press
                .is_some_and(|previous| now >= previous && now - previous <= window);
            if !self.sprint {
                self.last_forward_press = Some(now);
            }
        }
        if !held {
            self.sprint = false;
        }
    }

    pub(super) fn movement_keys(&mut self, keys: &ButtonInput<KeyCode>, now: f32, motion: &Motion) {
        self.movement = Vec2::new(
            (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32,
            (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32,
        );
        self.crouch = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        if keys.just_pressed(KeyCode::Space) {
            if self.movement.x.abs() > 0.01 {
                self.dodge = true;
                self.dodge_side = self.movement.x.signum();
                self.jump = false;
            } else {
                self.jump = true;
            }
        }
        self.forward_key(
            keys.just_pressed(KeyCode::KeyW),
            keys.pressed(KeyCode::KeyW),
            now,
            motion.sprint_double_tap_window,
        );
        if self.crouch || self.dodge {
            self.sprint = false;
        }
    }
}

pub fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    mut cursor: Single<&mut CursorOptions>,
    mut controls: ResMut<Controls>,
    motion: Res<Motion>,
    window: Single<&Window>,
    mut chat: ResMut<crate::chat::ChatState>,
    mut menu: ResMut<crate::emotes::EmoteMenu>,
    player: Res<super::Player>,
    time: Res<Time>,
) {
    let menu_input = menu.sample(
        &keys,
        !chat.blocks_gameplay && window.focused && controls.captured && player.health > 0.,
        player.pose_reset_serial,
    );
    if chat.blocks_gameplay {
        controls.movement = Vec2::ZERO;
        controls.jump = false;
        controls.crouch = false;
        controls.respawn = false;
        controls.clear_actions();
        controls.emote = None;
        controls.cancel_emote = true;
        if std::mem::take(&mut chat.hug_requested) && window.focused {
            super::Player::queue_hug(&mut controls);
        }
        if !window.focused {
            controls.captured = false;
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
        return;
    }
    if buttons.just_pressed(MouseButton::Left) {
        controls.captured = true;
    }
    if (keys.just_pressed(KeyCode::Escape) && !menu_input.consumed_escape) || !window.focused {
        controls.captured = false;
    }
    controls.cancel_emote |= keys.just_pressed(KeyCode::Escape) || !window.focused;
    if let Some(index) = menu_input.selected {
        if player.can_emote() {
            controls.emote = Some(index);
        } else {
            chat.system("Stand still on the ground to use an emote.");
        }
    }
    cursor.visible = !controls.captured;
    cursor.grab_mode = if controls.captured {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if controls.captured {
        controls.yaw += mouse.delta.x * motion.mouse_sensitivity;
        controls.pitch =
            (controls.pitch - mouse.delta.y * motion.mouse_sensitivity).clamp(-1.15, 1.15);
        controls.movement_keys(&keys, time.elapsed_secs(), &motion);
        controls.melee.recognition_deferred = controls.dodge;
        controls.melee.primary_pressed |= buttons.just_pressed(MouseButton::Left);
        controls.melee.primary_down_sample = controls.melee.primary_pressed;
        controls.melee.primary_held = buttons.pressed(MouseButton::Left);
        controls.melee.primary_released |= buttons.just_released(MouseButton::Left);
        controls.melee.secondary_pressed |= buttons.just_pressed(MouseButton::Right);
        controls.melee.secondary_down_sample = controls.melee.secondary_pressed;
        controls.melee.secondary_held = buttons.pressed(MouseButton::Right);
        controls.melee.secondary_released |= buttons.just_released(MouseButton::Right);
        if !menu.consumed_numbers && keys.just_pressed(KeyCode::Digit1) {
            controls.melee.equip = Some(Weapon::PlasmaSword);
        } else if !menu.consumed_numbers && keys.just_pressed(KeyCode::Digit2) {
            controls.melee.equip = Some(Weapon::Dagger);
        } else if !menu.consumed_numbers && keys.just_pressed(KeyCode::Digit3) {
            controls.melee.equip = Some(Weapon::CounterSword);
        }
    } else {
        controls.movement = Vec2::ZERO;
        controls.jump = false;
        controls.crouch = false;
        controls.clear_actions();
    }
    controls.respawn |= keys.just_pressed(KeyCode::KeyR);
    if keys.just_pressed(KeyCode::F3) {
        controls.diagnostics = !controls.diagnostics;
    }
}
