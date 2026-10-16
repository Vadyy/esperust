use crate::scene::MapId;
use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
    text::{LineHeight, TextLayoutInfo},
    ui::{CalculatedClip, UiGlobalTransform},
    window::{CursorGrabMode, CursorOptions, Ime},
};
use std::collections::VecDeque;

const MESSAGE_WIDTH: f32 = 600.0;
const MESSAGE_HEIGHT: f32 = 180.0;
const MESSAGE_ROW_HEIGHT: f32 = 20.0;
const MESSAGE_ROWS: usize = 9;
const INPUT_WIDTH: f32 = 260.0;
const CARET_WIDTH: f32 = 2.0;
const INPUT_MAX_CHARACTERS: usize = 2048;
const INPUT_MAX_BYTES: usize = 64;

pub struct ChatPlugin;
impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatState>()
            .add_message::<MapCommand>()
            .add_systems(crate::loading::PrepareWorld, setup)
            .add_systems(PreUpdate, (chat_input.after(bevy::input::InputSystems)).run_if(crate::loading::ready))
            .add_systems(PreUpdate, (toggle_god_mode.after(chat_input)).run_if(crate::loading::ready))
            .add_systems(PreUpdate, (set_physics_frequency.after(chat_input)).run_if(crate::loading::ready))
            .add_systems(Update, (chat_ui).run_if(crate::loading::ready))
            .add_systems(
                PostUpdate,
                chat_clipping.after(bevy::ui::update::update_clipping_system),
            );
    }
}

#[derive(Message, Debug)]
pub struct MapCommand {
    pub map: String,
}

#[derive(Resource)]
pub struct ChatState {
    pub active: bool,
    pub blocks_gameplay: bool,
    pub practice_toggle_requested: bool,
    pub(crate) hug_requested: bool,
    god_toggle_requested: bool,
    physics_hz_requested: Option<SmoothCommand>,
    pub(crate) practice_sparring_requested: Option<crate::combat::SparringMode>,
    text: String,
    cursor: usize,
    selected_all: bool,
    preedit: String,
    history: VecDeque<String>,
    history_index: Option<usize>,
    history_draft: String,
    messages: VecDeque<ChatLine>,
    restore_cursor: Option<(CursorGrabMode, bool)>,
    caret_age: f32,
}
impl Default for ChatState {
    fn default() -> Self {
        Self {
            active: false,
            blocks_gameplay: false,
            practice_toggle_requested: false,
            hug_requested: false,
            god_toggle_requested: false,
            physics_hz_requested: None,
            practice_sparring_requested: None,
            text: String::new(),
            cursor: 0,
            selected_all: false,
            preedit: String::new(),
            history: VecDeque::new(),
            history_index: None,
            history_draft: String::new(),
            messages: VecDeque::new(),
            restore_cursor: None,
            caret_age: 0.0,
        }
    }
}
struct ChatLine {
    text: String,
    system: bool,
    age: f32,
}
impl ChatState {
    pub fn system(&mut self, message: impl Into<String>) {
        self.line(message.into(), true);
    }
    fn line(&mut self, text: String, system: bool) {
        self.messages.push_back(ChatLine {
            text,
            system,
            age: 0.0,
        });
        if self.messages.len() > 100 {
            self.messages.pop_front();
        }
    }
    fn insert(&mut self, value: &str) {
        if self.selected_all {
            self.text.clear();
            self.cursor = 0;
            self.selected_all = false;
        }
        let remaining = INPUT_MAX_CHARACTERS.saturating_sub(self.text.chars().count());
        let available_bytes = INPUT_MAX_BYTES.saturating_sub(self.text.len());
        let mut used_bytes = 0;
        let value: String = value
            .chars()
            .filter(|c| !c.is_control())
            .take(remaining)
            .take_while(|c| {
                used_bytes += c.len_utf8();
                used_bytes <= available_bytes
            })
            .collect();
        self.text.insert_str(self.cursor, &value);
        self.cursor += value.len();
        self.caret_age = 0.0;
    }
    fn clear_selection(&mut self) -> bool {
        if self.selected_all {
            self.text.clear();
            self.cursor = 0;
            self.selected_all = false;
            true
        } else {
            false
        }
    }
    fn history_move(&mut self, older: bool) {
        if self.history.is_empty() || (self.history_index.is_none() && !older) {
            return;
        }
        if self.history_index.is_none() && older {
            self.history_draft.clone_from(&self.text);
        }
        self.history_index = match (self.history_index, older) {
            (None, true) => Some(self.history.len() - 1),
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < self.history.len() => Some(i + 1),
            _ => None,
        };
        self.text = self
            .history_index
            .map(|i| self.history[i].clone())
            .unwrap_or_else(|| self.history_draft.clone());
        self.cursor = self.text.len();
        self.selected_all = false;
    }
    fn submit(&mut self, maps: &mut MessageWriter<MapCommand>) {
        let text = self.text.trim().to_owned();
        if !text.is_empty() {
            if self.history.back() != Some(&text) {
                self.history.push_back(text.clone());
                if self.history.len() > 100 {
                    self.history.pop_front();
                }
            }
            if text.eq_ignore_ascii_case("/help") {
                for line in [
                    "/help - Show all chat commands.",
                    "/hug - Play the hug emote.",
                    "/map circle-2 | station-2 | neden-1 - Change map.",
                    "/dummy - Toggle the practice dummy.",
                    "/spar - Spar with an attacking opponent.",
                    "/spar guard - Spar with a guarding opponent.",
                    "/spar cs-heavy - Counter sword heavy every 3 seconds.",
                    "/god - Toggle invulnerability.",
                    "/smooth [Hz] - Enable (default 240, range 1-1000); /smooth off - 60 Hz.",
                ] {
                    self.system(line);
                }
            } else if text.eq_ignore_ascii_case("/hug") {
                self.hug_requested = true;
            } else if text.eq_ignore_ascii_case("/dummy") {
                self.practice_toggle_requested = true;
            } else if text.eq_ignore_ascii_case("/god") {
                self.god_toggle_requested = true;
            } else if text.split_whitespace().next().is_some_and(|command| command.eq_ignore_ascii_case("/smooth")) {
                match smooth_command(&text) {
                    Ok(hz) => self.physics_hz_requested = Some(hz),
                    Err(message) => self.system(message),
                }
            } else if text.split_whitespace().next().is_some_and(|command| command.eq_ignore_ascii_case("/spar")) {
                let words: Vec<_> = text.split_whitespace().collect();
                self.practice_sparring_requested = match words.as_slice() {
                    [_] => Some(crate::combat::SparringMode::Attack),
                    [_, mode] if mode.eq_ignore_ascii_case("guard") => Some(crate::combat::SparringMode::Guard),
                    [_, mode] if mode.eq_ignore_ascii_case("cs-heavy") => Some(crate::combat::SparringMode::CounterHeavy),
                    _ => {
                        self.system("Sparring: /spar, /spar guard, /spar cs-heavy (heavy every 3 seconds)");
                        None
                    }
                };
            } else if text.starts_with('/') {
                match map_command(&text) {
                    Ok(map) => {
                        maps.write(MapCommand {
                            map: map.id().into(),
                        });
                    }
                    Err(message) => self.system(message),
                }
            } else {
                self.line(format!("Vady : {text}"), false);
            }
        }
        self.text.clear();
        self.cursor = 0;
        self.selected_all = false;
        self.history_index = None;
        self.history_draft.clear();
    }
}

#[derive(Clone, Copy)]
enum SmoothCommand {
    Enable(f64),
    Off,
}

fn smooth_command(text: &str) -> Result<SmoothCommand, String> {
    let words: Vec<_> = text.split_whitespace().collect();
    match words.as_slice() {
        [command] if command.eq_ignore_ascii_case("/smooth") => Ok(SmoothCommand::Enable(240.0)),
        [command, mode] if command.eq_ignore_ascii_case("/smooth") && mode.eq_ignore_ascii_case("off") => Ok(SmoothCommand::Off),
        [command, hz] if command.eq_ignore_ascii_case("/smooth") => crate::parse_physics_hz(hz).map(SmoothCommand::Enable),
        _ => Err("Usage: /smooth [Hz] (default 240; 1–1000 Hz), or /smooth off for original 60 Hz physics.".into()),
    }
}

fn set_physics_frequency(
    mut chat: ResMut<ChatState>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut options: ResMut<crate::Options>,
    mut motion: ResMut<crate::gameplay::Motion>,
) {
    let Some(command) = chat.physics_hz_requested.take() else { return; };
    let (hz, smooth) = match command {
        SmoothCommand::Enable(hz) => (hz, true),
        SmoothCommand::Off => (crate::DEFAULT_PHYSICS_HZ, false),
    };
    fixed_time.set_timestep_hz(hz);
    options.physics_hz = hz;
    motion.smooth_physics = smooth;
    chat.system(if smooth {
        format!("Smooth physics ON: {hz} Hz.")
    } else {
        "Smooth physics OFF: original 60 Hz behavior.".into()
    });
}

fn toggle_god_mode(mut chat: ResMut<ChatState>, mut player: ResMut<crate::gameplay::Player>) {
    if !std::mem::take(&mut chat.god_toggle_requested) { return; }
    player.god_mode = !player.god_mode;
    if player.god_mode {
        player.health = player.max_health;
        player.sp = player.max_sp();
        player.death_age = None;
    }
    chat.system(if player.god_mode {
        "God Mode enabled."
    } else {
        "God mode OFF."
    });
}

fn map_command(text: &str) -> Result<MapId, String> {
    let mut words = text.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some(command), Some(map), None) if command.eq_ignore_ascii_case("/map") => map.parse(),
        _ => Err("Maps: /map circle-2, /map station-2, /map neden-1".into()),
    }
}

fn close_chat(state: &mut ChatState, window: &mut Window, cursor: &mut CursorOptions) {
    state.active = false;
    state.blocks_gameplay = true;
    state.preedit.clear();
    window.ime_enabled = false;
    if let Some((grab_mode, visible)) = state.restore_cursor.take() {
        cursor.grab_mode = grab_mode;
        cursor.visible = visible;
    }
}

pub fn chat_input(
    mut state: ResMut<ChatState>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut ime: MessageReader<Ime>,
    keys: Res<ButtonInput<KeyCode>>,
    mut maps: MessageWriter<MapCommand>,
    mut primary: Single<(&mut Window, &mut CursorOptions)>,
) {
    let (window, cursor_options) = &mut *primary;
    state.blocks_gameplay = state.active;
    if !window.focused && state.active {
        close_chat(&mut state, window, cursor_options);
    }
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        if matches!(event.key_code, KeyCode::Enter | KeyCode::NumpadEnter) && !event.repeat {
            if state.active {
                state.submit(&mut maps);
                close_chat(&mut state, window, cursor_options);
            } else if window.focused {
                state.active = true;
                state.blocks_gameplay = true;
                state.caret_age = 0.0;
                state.restore_cursor = Some((cursor_options.grab_mode, cursor_options.visible));
                cursor_options.grab_mode = CursorGrabMode::None;
                cursor_options.visible = true;
                window.ime_enabled = true;
            }
            continue;
        }
        if !state.active {
            continue;
        }
        state.caret_age = 0.0;
        match event.key_code {
            KeyCode::Escape => {
                state.text.clear();
                state.cursor = 0;
                state.selected_all = false;
                state.history_index = None;
                close_chat(&mut state, window, cursor_options);
            }
            KeyCode::Backspace => {
                if !state.clear_selection() && state.cursor > 0 {
                    let previous = state.text[..state.cursor]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    let end = state.cursor;
                    state.text.replace_range(previous..end, "");
                    state.cursor = previous;
                }
            }
            KeyCode::Delete => {
                if !state.clear_selection() && state.cursor < state.text.len() {
                    let start = state.cursor;
                    let end = start + state.text[start..].chars().next().unwrap().len_utf8();
                    state.text.replace_range(start..end, "");
                }
            }
            KeyCode::ArrowLeft => {
                state.cursor = state.text[..state.cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                state.selected_all = false;
            }
            KeyCode::ArrowRight => {
                let advance = state.text[state.cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0);
                state.cursor += advance;
                state.selected_all = false;
            }
            KeyCode::Home => {
                state.cursor = 0;
                state.selected_all = false;
            }
            KeyCode::End => {
                state.cursor = state.text.len();
                state.selected_all = false;
            }
            KeyCode::ArrowUp => state.history_move(true),
            KeyCode::ArrowDown => state.history_move(false),
            KeyCode::KeyA if control => {
                state.selected_all = true;
                state.cursor = state.text.len();
            }
            _ if !control => {
                if let Some(text) = &event.text {
                    state.insert(text);
                }
            }
            _ => {}
        }
    }
    for event in ime.read() {
        if !state.active {
            continue;
        }
        match event {
            Ime::Commit { value, .. } => {
                state.insert(value);
                state.preedit.clear();
            }
            Ime::Preedit { value, .. } => state.preedit.clone_from(value),
            _ => {}
        }
    }
}

#[derive(Component)]
struct ChatCanvas;
#[derive(Component)]
struct Composer;
#[derive(Component)]
struct InputText;
#[derive(Component)]
struct PrefixMeasure;
#[derive(Component)]
struct Caret;
#[derive(Component)]
struct MessageRow(usize);

fn text_style(font: &Handle<Font>) -> TextFont {
    TextFont {
        font: font.clone(),
        font_size: 11.0,
        ..default()
    }
}
fn absolute(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(left),
        top: px(top),
        width: px(width),
        height: px(height),
        ..default()
    }
}
fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    let font = assets.load("ui/verdana.ttf");
    let edge = assets.load("ui/chat-edge.png");
    let center = assets.load("ui/chat-center.png");
    let language = assets.load("ui/chat-language.png");
    let canvas = commands
        .spawn((
            Name::new("Original chat logical canvas"),
            ChatCanvas,
            crate::hud::DesignCanvas,
            absolute(0., 0., 1024., 768.),
            UiTransform::default(),
            GlobalZIndex(20),
        ))
        .id();
    let root = commands
        .spawn((
            Name::new("Original EU1162 in-game chat"),
            Node {
                position_type: PositionType::Absolute,
                left: px(9),
                bottom: px(142),
                width: px(MESSAGE_WIDTH),
                height: px(215),
                ..default()
            },
            GlobalZIndex(20),
        ))
        .id();
    commands.entity(canvas).add_child(root);
    let composer = commands
        .spawn((
            Composer,
            Node {
                overflow: Overflow::clip(),
                ..absolute(1., 181., 372., 34.)
            },
            Visibility::Hidden,
        ))
        .id();
    for (left, width, texture, flip) in [
        (0., 32., edge.clone(), false),
        (32., 308., center, false),
        (340., 32., edge.clone(), true),
    ] {
        let image = commands
            .spawn((
                ImageNode::new(texture).with_color(Color::srgba(1., 1., 1., 0.6)),
                absolute(left, 0., width, 34.),
            ))
            .id();
        if flip {
            commands.entity(image).insert(
                ImageNode::new(edge.clone())
                    .with_flip_x()
                    .with_color(Color::srgba(1., 1., 1., 0.6)),
            );
        }
        commands.entity(composer).add_child(image);
    }
    let language = commands
        .spawn((ImageNode::new(language), absolute(10., 8., 42., 18.)))
        .id();
    let target = commands
        .spawn((
            Text::new("Total :"),
            text_style(&font),
            TextColor(Color::WHITE),
            TextLayout::new_with_justify(Justify::Right).with_no_wrap(),
            LineHeight::Px(20.),
            absolute(54., 7., 46., 20.),
        ))
        .id();
    let input_clip = commands
        .spawn(Node {
            overflow: Overflow::clip(),
            ..absolute(102., 7., INPUT_WIDTH, 20.)
        })
        .id();
    let input = commands
        .spawn((
            InputText,
            Text::new(""),
            text_style(&font),
            TextColor(Color::WHITE),
            TextLayout::new_with_no_wrap(),
            LineHeight::Px(20.),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                ..default()
            },
        ))
        .id();
    let measure = commands
        .spawn((
            PrefixMeasure,
            Text::new(""),
            text_style(&font),
            TextColor(Color::NONE),
            TextLayout::new_with_no_wrap(),
            LineHeight::Px(20.),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                ..default()
            },
        ))
        .id();
    let caret = commands
        .spawn((
            Caret,
            BackgroundColor(Color::WHITE),
            absolute(0., 3., CARET_WIDTH, 14.),
        ))
        .id();
    commands
        .entity(input_clip)
        .add_children(&[input, measure, caret]);
    commands
        .entity(composer)
        .add_children(&[language, target, input_clip]);
    commands.entity(root).add_child(composer);
    let message_list = commands
        .spawn((
            Name::new("Original message_ListBox clipping viewport"),
            Node {
                overflow: Overflow::clip(),
                ..absolute(0., 0., MESSAGE_WIDTH, MESSAGE_HEIGHT)
            },
        ))
        .id();
    commands.entity(root).add_child(message_list);
    for row in 0..MESSAGE_ROWS {
        let message = commands
            .spawn((
                MessageRow(row),
                Text::new(""),
                text_style(&font),
                TextColor(Color::WHITE),
                TextLayout::new_with_no_wrap(),
                LineHeight::Px(MESSAGE_ROW_HEIGHT),
                TextShadow {
                    offset: Vec2::splat(1.),
                    color: Color::srgba(0., 0., 0., 0.9),
                },
                Node {
                    overflow: Overflow::clip(),
                    ..absolute(
                        0.,
                        MESSAGE_HEIGHT - (MESSAGE_ROWS - row) as f32 * MESSAGE_ROW_HEIGHT,
                        MESSAGE_WIDTH,
                        MESSAGE_ROW_HEIGHT,
                    )
                },
            ))
            .id();
        commands.entity(message_list).add_child(message);
    }
}

fn chat_clipping(
    mut commands: Commands,
    roots: Query<Entity, With<ChatCanvas>>,
    mut nodes: Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&Children>,
        Option<&mut CalculatedClip>,
    )>,
) {
    for root in &roots {
        update_chat_clip(root, None, &mut nodes, &mut commands);
    }
}

fn transformed_clip(size: Vec2, transform: &UiGlobalTransform) -> Rect {
    let half = size * 0.5;
    let mut minimum = Vec2::splat(f32::INFINITY);
    let mut maximum = Vec2::splat(f32::NEG_INFINITY);
    for corner in [
        Vec2::new(-half.x, -half.y),
        Vec2::new(half.x, -half.y),
        Vec2::new(half.x, half.y),
        Vec2::new(-half.x, half.y),
    ] {
        let point = transform.transform_point2(corner);
        minimum = minimum.min(point);
        maximum = maximum.max(point);
    }
    Rect::from_corners(minimum, maximum)
}

fn update_chat_clip(
    entity: Entity,
    inherited: Option<Rect>,
    nodes: &mut Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&Children>,
        Option<&mut CalculatedClip>,
    )>,
    commands: &mut Commands,
) {
    let Ok((node, computed, transform, children, existing_clip)) = nodes.get_mut(entity) else {
        return;
    };
    let inherited = if node.display == Display::None {
        Some(Rect::default())
    } else {
        inherited
    };
    if let Some(clip) = inherited {
        if let Some(mut existing) = existing_clip {
            if existing.clip != clip {
                existing.clip = clip;
            }
        } else {
            commands.entity(entity).insert(CalculatedClip { clip });
        }
    }
    let children_clip = if node.overflow.is_visible() {
        inherited
    } else {
        let mut clip = transformed_clip(computed.size(), transform);
        if node.overflow.x == OverflowAxis::Visible {
            clip.min.x = f32::NEG_INFINITY;
            clip.max.x = f32::INFINITY;
        }
        if node.overflow.y == OverflowAxis::Visible {
            clip.min.y = f32::NEG_INFINITY;
            clip.max.y = f32::INFINITY;
        }
        Some(inherited.map_or(clip, |parent| parent.intersect(clip)))
    };
    let children: Vec<Entity> = children
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        update_chat_clip(child, children_clip, nodes, commands);
    }
}

fn chat_ui(
    time: Res<Time>,
    mut state: ResMut<ChatState>,
    mut composer: Query<&mut Visibility, With<Composer>>,
    mut text: Query<(&mut Text, &mut Node, &mut TextColor), With<InputText>>,
    mut prefix: Query<
        (&mut Text, &mut Node, &TextLayoutInfo),
        (With<PrefixMeasure>, Without<InputText>),
    >,
    mut caret: Query<
        (&mut Node, &mut Visibility),
        (
            With<Caret>,
            Without<Composer>,
            Without<InputText>,
            Without<PrefixMeasure>,
        ),
    >,
    mut rows: Query<
        (&MessageRow, &mut Text, &mut TextColor, &mut TextShadow, &mut Visibility),
        (Without<InputText>, Without<PrefixMeasure>, Without<Composer>, Without<Caret>),
    >,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("chat_ui"));
    let delta = time.delta_secs();
    state.caret_age += delta;
    for line in &mut state.messages {
        line.age += delta;
    }
    for mut visibility in &mut composer {
        visibility.set_if_neq(if state.active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let mut measured = 0.0;
    for (mut value, mut node, layout) in &mut prefix {
        let before = &state.text[..state.cursor];
        if value.0 != before {
            value.0 = before.to_owned();
        }
        measured = layout.size.x;
        let left = px(-((measured - (INPUT_WIDTH - CARET_WIDTH)).max(0.)));
        if node.left != left { node.left = left; }
    }
    let scroll = (measured - (INPUT_WIDTH - CARET_WIDTH)).max(0.);
    for (mut value, mut node, mut color) in &mut text {
        let mut display = state.text.clone();
        display.insert_str(state.cursor, &state.preedit);
        if value.0 != display {
            value.0 = display;
        }
        if node.left != px(-scroll) { node.left = px(-scroll); }
        let desired = if state.selected_all {
            Color::srgb(0.3, 0.8, 1.)
        } else {
            Color::WHITE
        };
        if color.0 != desired { color.0 = desired; }
    }
    for (mut node, mut visibility) in &mut caret {
        if node.left != px(measured - scroll) { node.left = px(measured - scroll); }
        visibility.set_if_neq(if state.active && state.caret_age % 1.0 < 0.5 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let first = state.messages.len().saturating_sub(MESSAGE_ROWS);
    let padding = MESSAGE_ROWS.saturating_sub(state.messages.len());
    for (row, mut text, mut color, mut shadow, mut visibility) in &mut rows {
        let line = row
            .0
            .checked_sub(padding)
            .and_then(|i| state.messages.get(first + i));
        if let Some(line) = line {
            if text.0 != line.text {
                text.0.clone_from(&line.text);
            }
            let alpha = if state.active {
                1.0
            } else {
                ((16.0 - line.age) / 1.0).clamp(0., 1.)
            };
            visibility.set_if_neq(if alpha > 0. {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            let desired_shadow = Color::srgba(0., 0., 0., 0.9 * alpha);
            if shadow.color != desired_shadow { shadow.color = desired_shadow; }
            let desired = if line.system {
                Color::srgba(1., 0.9, 0.35, alpha)
            } else {
                Color::srgba(1., 1., 1., alpha)
            };
            if color.0 != desired { color.0 = desired; }
        } else {
            if !text.0.is_empty() { text.0.clear(); }
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_chat_hides_text_and_shadow_and_reopening_restores_history() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<ChatState>()
            .add_systems(Update, chat_ui);
        let row = app.world_mut().spawn((
            MessageRow(MESSAGE_ROWS - 1), Text::new(""),
            TextColor(Color::WHITE), TextShadow::default(), Visibility::Inherited,
        )).id();
        app.world_mut().resource_mut::<ChatState>().system("Retained message");
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(15.5));
        app.update();
        assert_eq!(app.world().get::<TextColor>(row).unwrap().0.alpha(), 0.5);
        assert!((app.world().get::<TextShadow>(row).unwrap().color.alpha() - 0.45).abs() < 0.001);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
        app.update();
        assert_eq!(*app.world().get::<Visibility>(row).unwrap(), Visibility::Hidden);
        assert_eq!(app.world().get::<TextShadow>(row).unwrap().color.alpha(), 0.);
        app.world_mut().resource_mut::<ChatState>().active = true;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(row).unwrap(), Visibility::Inherited);
        assert_eq!(app.world().get::<Text>(row).unwrap().0, "Retained message");
        assert_eq!(app.world().get::<TextColor>(row).unwrap().0.alpha(), 1.);
        assert_eq!(app.world().get::<TextShadow>(row).unwrap().color.alpha(), 0.9);
        app.world_mut().resource_mut::<ChatState>().active = false;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(row).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn smooth_submission_changes_the_live_clock_and_reported_rate() {
        fn type_command(app: &mut App, window: Entity, command: &str) {
            use bevy::input::keyboard::Key;
            let mut keyboard = app.world_mut().resource_mut::<Messages<KeyboardInput>>();
            for (key_code, logical_key, text) in [
                (KeyCode::Enter, Key::Enter, None),
                (KeyCode::KeyR, Key::Character(command.into()), Some(command.into())),
                (KeyCode::Enter, Key::Enter, None),
            ] {
                keyboard.write(KeyboardInput {
                    key_code,
                    logical_key,
                    state: ButtonState::Pressed,
                    text,
                    repeat: false,
                    window,
                });
            }
            app.update();
        }
        let mut app = App::new();
        app.init_resource::<ChatState>()
            .init_resource::<crate::Options>()
            .init_resource::<crate::gameplay::Motion>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Time::<Fixed>::from_hz(crate::DEFAULT_PHYSICS_HZ))
            .add_message::<KeyboardInput>()
            .add_message::<Ime>()
            .add_message::<MapCommand>()
            .add_systems(PreUpdate, (chat_input, set_physics_frequency).chain());
        let window = app.world_mut().spawn((Window { focused: true, ..default() }, CursorOptions::default())).id();
        assert!(!app.world().resource::<crate::gameplay::Motion>().smooth_physics);
        for (command, hz, smooth) in [
            ("/smooth", 240.0, true),
            (" /SMOOTH 179.82 ", 179.82, true),
            ("/smooth 30", 30.0, true),
            ("/smooth 60", 60.0, true),
            ("/smooth OFF", 60.0, false),
        ] {
            type_command(&mut app, window, command);
            assert_eq!(app.world().resource::<crate::Options>().physics_hz, hz);
            assert_eq!(app.world().resource::<crate::gameplay::Motion>().smooth_physics, smooth);
            let timestep = app.world().resource::<Time<Fixed>>().timestep().as_secs_f64();
            assert!((timestep - 1.0 / hz).abs() < 1e-9);
            let chat = app.world().resource::<ChatState>();
            assert!(chat.physics_hz_requested.is_none());
            assert_eq!(chat.messages.back().unwrap().text, if smooth {
                format!("Smooth physics ON: {hz} Hz.")
            } else {
                "Smooth physics OFF: original 60 Hz behavior.".into()
            });
        }
        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        for command in ["/smooth 0", "/smooth NaN", "/smooth inf", "/smooth -1", "/smooth 1001", "/smooth 120 extra"] {
            type_command(&mut app, window, command);
            assert_eq!(app.world().resource::<Time<Fixed>>().timestep(), timestep);
            assert_eq!(app.world().resource::<crate::Options>().physics_hz, 60.0);
            assert!(!app.world().resource::<crate::gameplay::Motion>().smooth_physics);
            assert!(app.world().resource::<ChatState>().messages.back().unwrap().system);
        }
    }

    #[test]
    fn transformed_composer_clip_includes_the_entire_frame() {
        let source_min = Vec2::new(10., 592.);
        let source_size = Vec2::new(372., 34.);
        for viewport in [
            Vec2::new(1280., 720.),
            Vec2::new(720., 1280.),
            Vec2::new(1024., 768.),
        ] {
            let projection = viewport / Vec2::new(1024., 768.);
            let ui_scale = projection.y;
            let transform =
                UiGlobalTransform::from(bevy::math::Affine2::from_scale_angle_translation(
                    Vec2::new(projection.x / ui_scale, 1.),
                    0.,
                    (source_min + source_size * 0.5) * projection,
                ));
            let clip = transformed_clip(source_size * ui_scale, &transform);
            assert!(clip.min.abs_diff_eq(source_min * projection, 0.001));
            assert!(
                clip.max
                    .abs_diff_eq((source_min + source_size) * projection, 0.001)
            );
        }
    }
    #[test]
    fn commands_require_one_supported_map() {
        assert_eq!(map_command("/map neden-1").unwrap(), MapId::Neden1);
        assert_eq!(map_command(" /MAP station-2 ").unwrap(), MapId::Station2);
        assert!(map_command("/map missing").is_err());
        assert!(map_command("/map neden-1 unexpected").is_err());
    }
    #[test]
    fn editing_respects_unicode_boundaries_and_native_limit() {
        let mut state = ChatState::default();
        state.insert("Привет");
        state.cursor = "При".len();
        state.insert("x");
        assert_eq!(state.text, "Приxвет");
        state.selected_all = true;
        state.insert(&"界".repeat(450));
        assert_eq!(state.text.chars().count(), 21);
        assert!(state.text.len() <= INPUT_MAX_BYTES);
        assert_eq!(state.cursor, state.text.len());
    }
}
