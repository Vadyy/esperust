use crate::{content::emotes::MENU_COUNT, gameplay::Player};

mod verification;
use bevy::{audio::Volume, prelude::*};
pub(crate) use verification::install as install_verification;

pub(crate) struct EmotesPlugin;
impl Plugin for EmotesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmoteMenu>()
            .init_resource::<EmoteAudio>()
            .add_systems(crate::loading::PrepareWorld, setup)
            .add_systems(Update, (menu_ui, play_voice).run_if(crate::loading::ready));
    }
}

#[derive(Resource, Default)]
pub(crate) struct EmoteMenu {
    pub open: bool,
    pub consumed_numbers: bool,
    reset: u32,
}

#[derive(Default)]
pub(crate) struct MenuInput {
    pub selected: Option<usize>,
    pub consumed_escape: bool,
}

impl EmoteMenu {
    pub(crate) fn sample(
        &mut self,
        keys: &ButtonInput<KeyCode>,
        enabled: bool,
        reset: u32,
    ) -> MenuInput {
        self.consumed_numbers = false;
        if !enabled || self.reset != reset {
            self.open = false;
            self.reset = reset;
            return MenuInput::default();
        }
        let consumed_escape = self.open && keys.just_pressed(KeyCode::Escape);
        if consumed_escape || (self.open && keys.just_pressed(KeyCode::Digit0)) {
            self.open = false;
        } else if keys.just_pressed(KeyCode::KeyE) {
            self.open = !self.open;
        }
        let mut selected = None;
        if self.open {
            self.consumed_numbers = true;
            selected = [
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::Digit5,
                KeyCode::Digit6,
                KeyCode::Digit7,
                KeyCode::Digit8,
                KeyCode::Digit9,
            ]
            .iter()
            .position(|key| keys.just_pressed(*key));
            if selected.is_some() {
                self.open = false;
            }
        }
        MenuInput {
            selected,
            consumed_escape,
        }
    }
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct EmoteVoice;
#[derive(Resource, Default)]
pub(crate) struct EmoteAudio {
    sounds: Vec<Handle<AudioSource>>,
    observed: Option<(u64, u32)>,
    pub voices_played: u32,
}

fn setup(mut commands: Commands, server: Res<AssetServer>) {
    let catalog = crate::content::emotes::load(std::path::Path::new("assets"))
        .unwrap_or_else(|error| panic!("{error}"));
    let sounds = catalog
        .entries
        .iter()
        .map(|e| server.load(e.sounds["male"].path.clone()))
        .collect();
    let canvas = commands
        .spawn((
            crate::hud::DesignCanvas,
            UiTransform::default(),
            crate::hud::source_node([0., 0., 1024., 768.]),
            GlobalZIndex(20),
        ))
        .id();
    let rect = catalog.menu.rect;
    let [r, g, b, a] = catalog.menu.color;
    let panel = commands
        .spawn((
            MenuRoot,
            Name::new("Original quickchat emote list"),
            Text::new(catalog.menu.text.clone()),
            TextFont {
                font: server.load("ui/verdana.ttf"),
                font_size: catalog.menu.font_size,
                ..default()
            },
            TextColor(Color::srgba_u8(r, g, b, a)),
            TextLayout::new_with_no_wrap(),
            bevy::text::LineHeight::Px(catalog.menu.line_height),
            TextShadow {
                offset: Vec2::splat(1.),
                color: Color::BLACK,
            },
            crate::hud::source_node(rect),
            crate::hud::SourceRect {
                rect,
                anchor: crate::hud::Anchor::TopLeft,
            },
            Visibility::Hidden,
        ))
        .id();
    commands.entity(canvas).add_child(panel);
    commands.insert_resource(catalog);
    commands.insert_resource(EmoteAudio {
        sounds,
        ..default()
    });
}

fn menu_ui(
    menu: Res<EmoteMenu>,
    player: Res<Player>,
    options: Res<crate::Options>,
    mut panels: Query<&mut Visibility, With<MenuRoot>>,
) {
    for mut visibility in &mut panels {
        *visibility = if menu.open && player.health > 0. && !options.overview {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn play_voice(
    mut commands: Commands,
    player: Res<Player>,
    mut audio: ResMut<EmoteAudio>,
    voices: Query<Entity, With<EmoteVoice>>,
) {
    let current = player
        .emote
        .map(|_| (player.emote_serial, player.pose_reset_serial));
    if current == audio.observed {
        return;
    }
    for entity in &voices {
        commands.entity(entity).despawn();
    }
    audio.observed = current;
    if let Some(emote) = player.emote {
        commands.spawn((
            EmoteVoice,
            AudioPlayer::new(audio.sounds[emote.index].clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.9)),
        ));
        audio.voices_played += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picker_consumes_number_even_on_selection_frame_and_has_only_nine_slots() {
        let mut menu = EmoteMenu::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyE);
        menu.sample(&keys, true, 0);
        assert!(menu.open);
        keys.clear();
        keys.release(KeyCode::KeyE);
        keys.press(KeyCode::Digit9);
        let input = menu.sample(&keys, true, 0);
        assert_eq!(input.selected, Some(8));
        assert!(!menu.open && menu.consumed_numbers);
        keys.clear();
        menu.sample(&keys, true, 0);
        assert!(!menu.consumed_numbers);
    }
    #[test]
    fn chat_focus_reset_and_escape_close_picker() {
        for mode in 0..4 {
            let mut menu = EmoteMenu {
                open: true,
                ..default()
            };
            let mut keys = ButtonInput::default();
            if mode == 3 {
                keys.press(KeyCode::Escape);
            }
            let input = menu.sample(&keys, mode >= 2, u32::from(mode == 2));
            assert!(!menu.open);
            assert_eq!(input.consumed_escape, mode == 3);
        }
    }
    #[test]
    fn zero_cancels_without_selecting_an_emote() {
        let mut menu = EmoteMenu {
            open: true,
            ..default()
        };
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Digit0);
        assert!(menu.sample(&keys, true, 0).selected.is_none());
        assert!(!menu.open);
    }
}
