use crate::{
    Options,
    gameplay::Player,
    scene::{MapId, MapSelection},
};
use bevy::{prelude::*, ui::widget::NodeImageMode};
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

#[cfg(test)]
use crate::gameplay::Motion;

pub struct HudPlugin;
impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchHud>()
            .init_resource::<MatchClock>()
            .add_systems(crate::loading::PrepareWorld, setup.after(crate::gameplay::setup))
            .add_systems(
                Update,
                ((scale_ui, tick_clock, update)
                    .chain()
                    .after(crate::scene::switch_maps))
                .run_if(crate::loading::ready),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudMode {
    Deathmatch,
    Touchdown,
    Chaser,
}
impl HudMode {
    pub fn for_map(map: MapId) -> Self {
        match map {
            MapId::Neden1 => Self::Deathmatch,
            MapId::Station2 => Self::Touchdown,
            MapId::Circle2 => Self::Chaser,
        }
    }
}

#[derive(Resource, Default)]
pub struct MatchHud {
    pub alpha_score: u32,
    pub beta_score: u32,
    pub remaining_seconds: u32,
    pub chaser_round_seconds: u32,
    pub first_place_score: u32,
    pub personal_score: u32,
    pub phase_label: String,
}

const MATCH_SECONDS: u32 = 20 * 60;
const LOCAL_CHASER_ROUND_SECONDS: u32 = 120;

#[derive(Resource, Default)]
struct MatchClock {
    map: Option<MapId>,
    started_at: Duration,
}

impl MatchClock {
    fn read(&mut self, map: MapId, now: Duration) -> (u32, u32, bool) {
        let reset = self.map != Some(map);
        if reset {
            self.map = Some(map);
            self.started_at = now;
        }
        let elapsed = now.saturating_sub(self.started_at).as_secs();
        let duration = if HudMode::for_map(map) == HudMode::Touchdown {
            MATCH_SECONDS / 2
        } else {
            MATCH_SECONDS
        };
        let remaining = u64::from(duration).saturating_sub(elapsed) as u32;
        let round = u64::from(LOCAL_CHASER_ROUND_SECONDS)
            .saturating_sub(elapsed)
            .min(u64::from(remaining)) as u32;
        (remaining, round, reset)
    }
}

fn tick_clock(
    time: Res<Time<Real>>,
    selection: Res<MapSelection>,
    mut clock: ResMut<MatchClock>,
    mut state: ResMut<MatchHud>,
) {
    let (remaining, round, reset) = clock.read(selection.active, time.elapsed());
    state.remaining_seconds = remaining;
    state.chaser_round_seconds = if HudMode::for_map(selection.active) == HudMode::Chaser {
        round
    } else {
        0
    };
    if reset {
        state.phase_label = if HudMode::for_map(selection.active) == HudMode::Touchdown {
            "First Half".into()
        } else {
            String::new()
        };
    }
}

#[derive(Deserialize)]
struct SourceSprite {
    file: String,
    size: [f32; 2],
    canvas_rect: Option<[f32; 4]>,
}
#[derive(Deserialize)]
struct SourceNode {
    offset: String,
    canvas_rect: [f32; 4],
}
#[derive(Deserialize)]
struct SourceLabel {
    canvas_rect: [f32; 4],
    english_text: String,
    english_font: HashMap<String, String>,
    color: [f32; 4],
    alignment_enum: u32,
    font_style_enum: u32,
    shadow_color: [f32; 4],
}
#[derive(Deserialize)]
struct SourceGauge {
    fill_direction: u32,
    fill_margin_x: u32,
    fill_margin_y: u32,
    #[serde(rename = "reverse_uv")]
    _reverse_uv: bool,
}
#[derive(Resource, Deserialize)]
struct HudSource {
    base_canvas: [f32; 2],
    sprites: HashMap<String, SourceSprite>,
    labels: HashMap<String, SourceLabel>,
    gauges: HashMap<String, SourceGauge>,
    layouts: serde_json::Value,
    nodes: Vec<SourceNode>,
}
impl HudSource {
    fn rect(&self, offset: &str) -> [f32; 4] {
        self.nodes
            .iter()
            .find(|n| n.offset == offset)
            .unwrap_or_else(|| panic!("Missing original HUD node {offset}"))
            .canvas_rect
    }
    fn layout_rect(&self, section: &str, name: &str) -> [f32; 4] {
        serde_json::from_value(self.layouts[section][name].clone())
            .unwrap_or_else(|_| panic!("Missing original HUD layout {section}/{name}"))
    }
}

#[derive(Resource)]
struct HudImages(HashMap<String, Handle<Image>>);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiAspect {
    FourThree,
    #[default]
    SixteenNine,
    SixteenTen,
}
impl UiAspect {
    pub fn canvas(self) -> Vec2 {
        Vec2::new(
            match self {
                Self::FourThree => 1024.,
                Self::SixteenNine => 1365.,
                Self::SixteenTen => 1229.,
            },
            768.,
        )
    }
}
impl std::str::FromStr for UiAspect {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "4:3" | "4x3" => Ok(Self::FourThree),
            "16:9" | "16x9" => Ok(Self::SixteenNine),
            "16:10" | "16x10" => Ok(Self::SixteenTen),
            _ => Err("Expected 4:3, 16:9 or 16:10"),
        }
    }
}

#[derive(Component)]
pub struct DesignCanvas;
#[derive(Component, Clone, Copy)]
pub(crate) enum Anchor {
    BottomLeft,
    BottomRight,
    BottomCenter,
    TopLeft,
    TopCenter,
    Center,
}
#[derive(Component, Clone, Copy)]
pub(crate) struct SourceRect {
    pub rect: [f32; 4],
    pub anchor: Anchor,
}
#[derive(Component)]
struct ModeOnly(HudMode);
#[derive(Component)]
struct TeamOnly;
#[derive(Component)]
pub struct HudBar {
    pub kind: BarKind,
    pub fraction: f32,
    width: f32,
    texture_size: [f32; 2],
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    Health,
    Stamina,
}
#[derive(Clone, Copy)]
enum Number {
    Health,
    MaximumHealth,
    Alpha,
    Beta,
    Minutes,
    Seconds,
    ChaserTime,
    FirstPlace,
    Personal,
}
#[derive(Component)]
struct Digit {
    number: Number,
    place: u32,
    padded: bool,
    material: &'static str,
    group_digits: Option<u32>,
}
#[derive(Component)]
enum RuntimeLabel {
    Phase,
}

pub(crate) fn source_node(rect: [f32; 4]) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(rect[0]),
        top: px(rect[1]),
        width: px(rect[2]),
        height: px(rect[3]),
        ..default()
    }
}

fn sprite(
    commands: &mut Commands,
    assets: &AssetServer,
    source: &HudSource,
    parent: Entity,
    name: &str,
    anchor: Anchor,
) -> Entity {
    let spec = &source.sprites[name];
    let rect = spec
        .canvas_rect
        .expect("HUD sprite needs its source rectangle");
    let entity = commands
        .spawn((
            Name::new(format!("EU1162 {name}")),
            ImageNode::new(assets.load(spec.file.clone())).with_mode(NodeImageMode::Stretch),
            source_node(rect),
            SourceRect { rect, anchor },
        ))
        .id();
    commands.entity(parent).add_child(entity);
    entity
}

fn digits(
    commands: &mut Commands,
    assets: &AssetServer,
    parent: Entity,
    rect: [f32; 4],
    anchor: Anchor,
    number: Number,
    count: u32,
    material: &'static str,
    padded: bool,
    group_digits: Option<u32>,
) {
    let width = rect[2] / count as f32;
    for index in 0..count {
        let digit_rect = [rect[0] + index as f32 * width, rect[1], width, rect[3]];
        let entity = commands
            .spawn((
                Name::new("EU1162 numeric sprite"),
                ImageNode::new(assets.load(format!("ui/hud/{material}-0.png")))
                    .with_mode(NodeImageMode::Stretch),
                source_node(digit_rect),
                SourceRect {
                    rect: digit_rect,
                    anchor,
                },
                Digit {
                    number,
                    place: count - index - 1,
                    material,
                    padded,
                    group_digits,
                },
            ))
            .id();
        commands.entity(parent).add_child(entity);
    }
}

fn label(
    commands: &mut Commands,
    assets: &AssetServer,
    source: &HudSource,
    parent: Entity,
    name: &str,
    anchor: Anchor,
) -> Entity {
    let spec = &source.labels[name];
    let rect = spec.canvas_rect;
    let [r, g, b, a] = spec.color;
    let justify = match spec.alignment_enum {
        0 => Justify::Center,
        2 => Justify::Right,
        _ => Justify::Left,
    };
    let container = commands
        .spawn((source_node(rect), SourceRect { rect, anchor }))
        .id();
    commands.entity(container).insert(Node {
        display: Display::Grid,
        justify_items: match justify {
            Justify::Center => JustifyItems::Center,
            Justify::Right => JustifyItems::End,
            _ => JustifyItems::Start,
        },
        align_items: AlignItems::Center,
        ..source_node(rect)
    });
    let [sr, sg, sb, sa] = spec.shadow_color;
    let passes = crate::ui_text::styled_text(
        commands, container, &spec.english_text,
        TextFont {
            font: assets.load("ui/verdana.ttf"),
            font_size: spec.english_font["size"].parse().expect("Original font size"),
            ..default()
        },
        TextLayout::new_with_justify(justify).with_no_wrap(),
        Color::srgba(r, g, b, a), spec.font_style_enum,
        Color::srgba(sr, sg, sb, sa),
    );
    let entity = *passes.last().expect("Foreground text pass");
    commands.entity(entity).insert(Name::new(format!("EU1162 label {name}")));
    commands.entity(parent).add_child(container);
    entity
}

fn group(commands: &mut Commands, parent: Entity, name: &str) -> Entity {
    let entity = commands
        .spawn((
            Name::new(name.to_owned()),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(entity);
    entity
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, options: Res<Options>, player: Res<Player>) {
    let source: HudSource = serde_json::from_slice(
        &std::fs::read("assets/ui/hud/hud-source.json")
            .expect("Run tools/asset_hud.py to import the original HUD"),
    )
    .expect("Invalid original HUD layout manifest");
    assert_eq!(source.base_canvas, [1024., 768.]);
    let images = HudImages(
        source
            .sprites
            .iter()
            .map(|(name, spec)| (name.clone(), assets.load(spec.file.clone())))
            .collect(),
    );
    let root = commands
        .spawn((
            Name::new("Original EU1162 gameplay HUD"),
            DesignCanvas,
            UiTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: px(source.base_canvas[0]),
                height: px(source.base_canvas[1]),
                ..default()
            },
            GlobalZIndex(10),
        ))
        .id();
    for name in ["hp-background", "sp-background"] {
        sprite(
            &mut commands,
            &assets,
            &source,
            root,
            name,
            Anchor::BottomCenter,
        );
    }
    for (name, kind) in [("hp-fill", BarKind::Health), ("sp-fill", BarKind::Stamina)] {
        let entity = sprite(
            &mut commands,
            &assets,
            &source,
            root,
            name,
            Anchor::BottomCenter,
        );
        let spec = &source.sprites[name];
        let gauge = &source.gauges[if kind == BarKind::Health { "hp" } else { "sp" }];
        assert_eq!(
            (
                gauge.fill_direction,
                gauge.fill_margin_x,
                gauge.fill_margin_y
            ),
            (2, 0, 0)
        );
        commands.entity(entity).insert(HudBar {
            kind,
            fraction: 1.,
            width: spec.canvas_rect.unwrap()[2],
            texture_size: spec.size,
        });
    }
    for (name, anchor) in [
        ("skill-frame", Anchor::BottomLeft),
        ("ammo-frame", Anchor::BottomRight),
        ("infinity-main", Anchor::BottomRight),
        ("infinity-reserve", Anchor::BottomRight),
        ("ammo-slash", Anchor::BottomRight),
        ("hp-slash", Anchor::BottomCenter),
    ] {
        sprite(&mut commands, &assets, &source, root, name, anchor);
    }
    if let Some(name) = player.equipped_skill().hud_sprite() {
        sprite(&mut commands, &assets, &source, root, name, Anchor::BottomLeft);
    }
    digits(
        &mut commands,
        &assets,
        root,
        source.layout_rect("hp_digits", "current"),
        Anchor::BottomCenter,
        Number::Health,
        3,
        "digit-white",
        false,
        None,
    );
    digits(
        &mut commands,
        &assets,
        root,
        source.layout_rect("hp_digits", "maximum"),
        Anchor::BottomCenter,
        Number::MaximumHealth,
        3,
        "digit-white",
        false,
        None,
    );
    if !options.overview {
        sprite(
            &mut commands,
            &assets,
            &source,
            root,
            "crosshair",
            Anchor::Center,
        );
    }

    let team = group(&mut commands, root, "Original TD/DM header");
    commands.entity(team).insert(TeamOnly);
    sprite(
        &mut commands,
        &assets,
        &source,
        team,
        "score-frame",
        Anchor::TopCenter,
    );
    let phase = label(
        &mut commands,
        &assets,
        &source,
        team,
        "team_state",
        Anchor::TopCenter,
    );
    commands.entity(phase).insert(RuntimeLabel::Phase);
    for (number, material, offsets) in [
        (
            Number::Alpha,
            "digit-green",
            ["0x2f0e3", "0x2f00e", "0x2ef39"],
        ),
        (Number::Beta, "digit-red", ["0x2ee64", "0x2ed8f", "0x2ecba"]),
    ] {
        for (i, offset) in offsets.into_iter().enumerate() {
            digits(
                &mut commands,
                &assets,
                team,
                source.rect(offset),
                Anchor::TopCenter,
                number,
                i as u32 + 1,
                material,
                true,
                Some(i as u32 + 1),
            );
        }
    }
    let timer: Vec<[f32; 4]> =
        serde_json::from_value(source.layouts["team_score"]["timer"].clone()).unwrap();
    for (i, &rect) in timer.iter().enumerate() {
        if i == 2 {
            let entity = commands
                .spawn((
                    ImageNode::new(assets.load("ui/hud/timer-colon.png"))
                        .with_mode(NodeImageMode::Stretch),
                    source_node(rect),
                    SourceRect {
                        rect,
                        anchor: Anchor::TopCenter,
                    },
                ))
                .id();
            commands.entity(team).add_child(entity);
        } else {
            let (number, place) = if i < 2 {
                (Number::Minutes, 1 - i as u32)
            } else {
                (Number::Seconds, 4 - i as u32)
            };
            let entity = commands
                .spawn((
                    ImageNode::new(assets.load("ui/hud/timer-0.png"))
                        .with_mode(NodeImageMode::Stretch),
                    source_node(rect),
                    SourceRect {
                        rect,
                        anchor: Anchor::TopCenter,
                    },
                    Digit {
                        number,
                        place,
                        padded: true,
                        material: "timer",
                        group_digits: None,
                    },
                ))
                .id();
            commands.entity(team).add_child(entity);
        }
    }

    let chaser = group(&mut commands, root, "Original Chaser header");
    commands.entity(chaser).insert(ModeOnly(HudMode::Chaser));
    let chaser_timer: Vec<[f32; 4]> =
        serde_json::from_value(source.layouts["chaser"]["timer_digits"].clone()).unwrap();
    for (i, rect) in chaser_timer.into_iter().enumerate() {
        let entity = commands
            .spawn((
                Name::new("EU1162 Chaser clock digit"),
                ImageNode::new(assets.load("ui/hud/chaser-digit-0.png"))
                    .with_mode(NodeImageMode::Stretch),
                source_node(rect),
                SourceRect {
                    rect,
                    anchor: Anchor::TopCenter,
                },
                Digit {
                    number: Number::ChaserTime,
                    place: 2 - i as u32,
                    padded: false,
                    material: "chaser-digit",
                    group_digits: None,
                },
            ))
            .id();
        commands.entity(chaser).add_child(entity);
    }
    for name in ["chaser_first_place", "chaser_my_score"] {
        label(
            &mut commands,
            &assets,
            &source,
            chaser,
            name,
            Anchor::TopLeft,
        );
    }
    for (number, material, offsets) in [
        (
            Number::FirstPlace,
            "digit-red",
            ["0x2742e", "0x27549", "0x27358"],
        ),
        (
            Number::Personal,
            "digit-green",
            ["0x2712e", "0x27058", "0x26f82"],
        ),
    ] {
        for (i, offset) in offsets.into_iter().enumerate() {
            digits(
                &mut commands,
                &assets,
                chaser,
                source.rect(offset),
                Anchor::TopLeft,
                number,
                i as u32 + 1,
                material,
                true,
                Some(i as u32 + 1),
            );
        }
    }
    commands.insert_resource(source);
    commands.insert_resource(images);
}

fn scale_ui(
    window: Single<&Window>,
    options: Res<Options>,
    mut scale: ResMut<UiScale>,
    mut rects: Query<(&SourceRect, &mut Node), Without<DesignCanvas>>,
    mut canvases: Query<(&mut Node, &mut UiTransform), (With<DesignCanvas>, Without<SourceRect>)>,
    mut previous_layout: Local<Option<(Vec2, Vec2, f32)>>,
) {
    let factor = options.ui_scale.unwrap_or(1.);
    let logical = options.ui_aspect.canvas();
    let canvas = logical / factor;
    let projection = Vec2::new(window.width(), window.height()) / logical;
    if *previous_layout == Some((canvas, projection, factor)) {
        return;
    }
    *previous_layout = Some((canvas, projection, factor));
    let raster_scale = factor * projection.y;
    if (scale.0 - raster_scale).abs() > f32::EPSILON {
        scale.0 = raster_scale;
    }
    let aspect_scale = projection / projection.y;
    for (mut node, mut transform) in &mut canvases {
        node.width = px(canvas.x);
        node.height = px(canvas.y);
        *transform = UiTransform {
            scale: aspect_scale,
            translation: bevy::ui::Val2::px(
                (aspect_scale.x - 1.) * canvas.x * 0.5,
                (aspect_scale.y - 1.) * canvas.y * 0.5,
            ),
            ..default()
        };
    }
    let width = canvas.x;
    let height = canvas.y;
    for (source, mut node) in &mut rects {
        let [x, y, _, _] = source.rect;
        node.left = px(x + match source.anchor {
            Anchor::BottomRight => width - 1024.,
            Anchor::BottomCenter | Anchor::TopCenter | Anchor::Center => (width - 1024.) * 0.5,
            _ => 0.,
        });
        node.top = px(y + match source.anchor {
            Anchor::BottomLeft | Anchor::BottomRight | Anchor::BottomCenter => height - 768.,
            Anchor::Center => (height - 768.) * 0.5,
            _ => 0.,
        });
    }
}

pub fn fraction(value: f32, maximum: f32) -> f32 {
    if !value.is_finite() || !maximum.is_finite() || maximum <= 0. {
        0.
    } else {
        (value / maximum).clamp(0., 1.)
    }
}

fn update(
    player: Res<Player>,
    selection: Res<MapSelection>,
    state: Res<MatchHud>,
    images: Res<HudImages>,
    mut bars: Query<(&mut HudBar, &mut Node, &mut ImageNode)>,
    mut numbers: Query<(&Digit, &mut ImageNode, &mut Visibility), Without<HudBar>>,
    mut modes: Query<(&ModeOnly, &mut Visibility), (Without<Digit>, Without<TeamOnly>)>,
    mut team: Query<&mut Visibility, (With<TeamOnly>, Without<Digit>, Without<ModeOnly>)>,
    mut labels: Query<(&RuntimeLabel, &mut Text)>,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("hud"));
    let mode = HudMode::for_map(selection.active);
    for (mut bar, mut node, mut image) in &mut bars {
        let value = match bar.kind {
            BarKind::Health => fraction(player.health, player.max_health),
            BarKind::Stamina => fraction(player.sp, player.max_sp()),
        };
        let width = px((bar.width * value).trunc());
        let rect = Some(Rect::from_corners(
            Vec2::ZERO,
            Vec2::new(bar.texture_size[0] * value, bar.texture_size[1]),
        ));
        if bar.fraction != value {
            bar.fraction = value;
        }
        if node.width != width {
            node.width = width;
        }
        if image.rect != rect {
            image.rect = rect;
        }
    }
    for (only, mut visible) in &mut modes {
        let desired = if only.0 == mode {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visible != desired {
            *visible = desired;
        }
    }
    for mut visible in &mut team {
        let desired = if mode == HudMode::Chaser {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visible != desired {
            *visible = desired;
        }
    }
    for (digit, mut image, mut visible) in &mut numbers {
        let value = match digit.number {
            Number::Health => player.health.max(0.) as u32,
            Number::MaximumHealth => player.max_health.max(0.) as u32,
            Number::Alpha => state.alpha_score,
            Number::Beta => state.beta_score,
            Number::Minutes => (state.remaining_seconds / 60).min(99),
            Number::Seconds => state.remaining_seconds % 60,
            Number::ChaserTime => {
                if (1..=500).contains(&state.chaser_round_seconds) {
                    state.chaser_round_seconds
                } else {
                    0
                }
            }
            Number::FirstPlace => state.first_place_score,
            Number::Personal => state.personal_score,
        }
        .min(999);
        let places = if value >= 100 {
            3
        } else if value >= 10 {
            2
        } else {
            1
        };
        let show = digit.group_digits.is_none_or(|n| n == places)
            && (digit.padded || digit.place == 0 || value >= 10u32.pow(digit.place));
        let desired = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visible != desired {
            *visible = desired;
        }
        let n = value / 10u32.pow(digit.place) % 10;
        let handle = &images.0[&format!("{}-{n}", digit.material)];
        if image.image != *handle {
            image.image = handle.clone();
        }
    }
    for (kind, mut text) in &mut labels {
        let value = match kind {
            RuntimeLabel::Phase => &state.phase_label,
        };
        if text.0 != *value {
            text.0.clone_from(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::simulation::parity_content;

    #[test]
    fn original_canvas_projection_scales_source_art_and_keeps_screen_anchors() {
        let source: HudSource = crate::content::required_json("ui/hud/hud-source.json");
        let mut app = App::new();
        app.init_resource::<UiScale>()
            .init_resource::<Options>()
            .add_systems(Update, scale_ui);
        assert_eq!(
            app.world().resource::<Options>().ui_aspect,
            UiAspect::SixteenNine
        );
        let window = app.world_mut().spawn(Window::default()).id();
        let canvas = app
            .world_mut()
            .spawn((DesignCanvas, Node::default(), UiTransform::default()))
            .id();
        let spawn = |app: &mut App, name: &str, anchor| {
            let rect = source.sprites[name].canvas_rect.unwrap();
            app.world_mut()
                .spawn((source_node(rect), SourceRect { rect, anchor }))
                .id()
        };
        let hp = spawn(&mut app, "hp-background", Anchor::BottomCenter);
        let ammo = spawn(&mut app, "ammo-frame", Anchor::BottomRight);
        let reticle = spawn(&mut app, "crosshair", Anchor::Center);
        for (width, height, aspect, hp_size) in [
            (1024., 768., UiAspect::FourThree, Vec2::new(490., 63.)),
            (
                1280.,
                720.,
                UiAspect::SixteenNine,
                Vec2::new(459.48718, 59.0625),
            ),
            (
                1920.,
                1080.,
                UiAspect::SixteenNine,
                Vec2::new(689.23077, 88.59375),
            ),
            (
                2560.,
                1440.,
                UiAspect::SixteenNine,
                Vec2::new(918.97436, 118.125),
            ),
            (
                1920.,
                1200.,
                UiAspect::SixteenTen,
                Vec2::new(765.5004, 98.4375),
            ),
        ] {
            app.world_mut().resource_mut::<Options>().ui_aspect = aspect;
            app.world_mut()
                .get_mut::<Window>(window)
                .unwrap()
                .resolution
                .set(width, height);
            app.update();
            let logical = aspect.canvas();
            let raster_scale = app.world().resource::<UiScale>().0;
            assert_eq!(raster_scale, height / logical.y);
            let transform = app.world().get::<UiTransform>(canvas).unwrap();
            let bar = app.world().get::<Node>(hp).unwrap();
            assert_eq!((bar.width, bar.height), (px(490), px(63)));
            assert!((Vec2::new(490., 63.) * raster_scale * transform.scale).abs_diff_eq(hp_size, 0.001));
            assert_eq!(
                (bar.left, bar.top),
                (px(logical.x * 0.5 - 243.), px(logical.y - 80.))
            );
            assert_eq!(
                app.world().get::<Node>(ammo).unwrap().left,
                px(logical.x - 156.)
            );
            let aim = app.world().get::<Node>(reticle).unwrap();
            assert_eq!(
                (aim.left, aim.top),
                (px(logical.x * 0.5 - 32.), px(logical.y * 0.5 - 32.))
            );
            let mut affine = transform.compute_affine(raster_scale, logical * raster_scale, Vec2::new(width, height));
            affine.translation += logical * raster_scale * 0.5;
            assert!(
                affine
                    .transform_point2(-logical * raster_scale * 0.5)
                    .abs_diff_eq(Vec2::ZERO, 0.001)
            );
            assert!(
                affine
                    .transform_point2(logical * raster_scale * 0.5)
                    .abs_diff_eq(Vec2::new(width, height), 0.001)
            );
        }
        app.world_mut().resource_mut::<Options>().ui_scale = Some(1.25);
        app.update();
        assert_eq!(app.world().resource::<UiScale>().0, 1.25 * 1200. / 768.);
        assert_eq!(
            app.world().get::<Node>(canvas).unwrap().width,
            px(1229. / 1.25)
        );
        let transform = app.world().get::<UiTransform>(canvas).unwrap();
        assert!((490. * app.world().resource::<UiScale>().0 * transform.scale.x - 956.8755).abs() < 0.001);
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set_scale_factor_override(Some(2.));
        app.update();
        let window = app.world().get::<Window>(window).unwrap();
        let transform = app.world().get::<UiTransform>(canvas).unwrap();
        assert!((490. * app.world().resource::<UiScale>().0 * transform.scale.x * window.scale_factor() - 956.8755).abs() < 0.001);
    }

    #[test]
    fn original_aspect_option_accepts_only_the_three_native_modes() {
        assert_eq!(
            "4:3".parse::<UiAspect>().unwrap().canvas(),
            Vec2::new(1024., 768.)
        );
        assert_eq!(
            "16x9".parse::<UiAspect>().unwrap().canvas(),
            Vec2::new(1365., 768.)
        );
        assert_eq!(
            "16:10".parse::<UiAspect>().unwrap().canvas(),
            Vec2::new(1229., 768.)
        );
        assert!("21:9".parse::<UiAspect>().is_err());
    }

    #[test]
    fn real_time_clock_runs_while_chat_and_virtual_time_are_paused() {
        let mut app = App::new();
        let mut virtual_time = Time::<Virtual>::default();
        virtual_time.pause();
        let mut chat = crate::chat::ChatState::default();
        chat.active = true;
        app.insert_resource(Time::<Real>::default())
            .insert_resource(virtual_time)
            .insert_resource(chat)
            .insert_resource(MapSelection {
                active: MapId::Neden1,
            })
            .init_resource::<MatchClock>()
            .init_resource::<MatchHud>()
            .add_systems(Update, tick_clock);
        app.update();
        assert_eq!(app.world().resource::<MatchHud>().remaining_seconds, 1200);
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(1250));
        app.update();
        assert_eq!(app.world().resource::<MatchHud>().remaining_seconds, 1199);
        assert!(app.world().resource::<crate::chat::ChatState>().active);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());

        app.world_mut().resource_mut::<MapSelection>().active = MapId::Station2;
        app.update();
        let state = app.world().resource::<MatchHud>();
        assert_eq!(state.remaining_seconds, 600);
        assert_eq!(state.phase_label, "First Half");
        assert_eq!(state.chaser_round_seconds, 0);
        app.world_mut().resource_mut::<MapSelection>().active = MapId::Circle2;
        app.update();
        assert_eq!(app.world().resource::<MatchHud>().chaser_round_seconds, 120);
        assert!(app.world().resource::<MatchHud>().phase_label.is_empty());
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(5000));
        app.update();
        let state = app.world().resource::<MatchHud>();
        assert_eq!(
            (state.remaining_seconds, state.chaser_round_seconds),
            (0, 0)
        );
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(5000));
        app.update();
        assert_eq!(app.world().resource::<MatchHud>().chaser_round_seconds, 0);
    }

    #[test]
    fn movement_sp_drains_and_recovers_in_the_stationary_hud_crop() {
        use crate::{controller::CollisionWorld, gameplay::MovementInput};
        let motion = Motion::default();
        let floor = CollisionWorld::new(vec![
            [
                Vec3::new(-100., 0., -100.),
                Vec3::new(100., 0., 100.),
                Vec3::new(100., 0., -100.),
            ],
            [
                Vec3::new(-100., 0., -100.),
                Vec3::new(-100., 0., 100.),
                Vec3::new(100., 0., 100.),
            ],
        ]);
        let mut player = Player::new(Vec3::new(0., 0.001, 0.), &motion);
        player.step(
            parity_content(),
            &floor,
            &motion,
            MovementInput::default(),
            1. / 240.,
        );
        let source: HudSource = crate::content::required_json("ui/hud/hud-source.json");
        let spec = &source.sprites["sp-fill"];
        let rect = spec.canvas_rect.unwrap();
        let mut app = App::new();
        app.insert_resource(player)
            .insert_resource(Motion::default())
            .insert_resource(MapSelection::default())
            .insert_resource(MatchHud::default())
            .insert_resource(HudImages(HashMap::new()))
            .add_systems(Update, update);
        let entity = app
            .world_mut()
            .spawn((
                HudBar {
                    kind: BarKind::Stamina,
                    fraction: 1.,
                    width: rect[2],
                    texture_size: spec.size,
                },
                source_node(rect),
                ImageNode::default(),
            ))
            .id();
        for _ in 0..480 {
            app.world_mut().resource_mut::<Player>().step(
                parity_content(),
                &floor,
                &motion,
                MovementInput {
                    movement: Vec2::Y,
                    sprint: true,
                    ..default()
                },
                1. / 240.,
            );
        }
        app.update();
        let player = app.world().resource::<Player>();
        assert!((player.sp - 80.).abs() < 0.01);
        let node = app.world().get::<Node>(entity).unwrap();
        let image = app.world().get::<ImageNode>(entity).unwrap();
        assert_eq!(node.left, px(rect[0]));
        assert_eq!(node.width, px((rect[2] * 0.8).trunc()));
        assert_eq!(image.rect.unwrap().min, Vec2::ZERO);
        assert!((image.rect.unwrap().max.x - spec.size[0] * 0.8).abs() < 0.02);
        for _ in 0..(3 * 240) {
            app.world_mut().resource_mut::<Player>().step(
                parity_content(),
                &floor,
                &motion,
                MovementInput::default(),
                1. / 240.,
            );
        }
        app.update();
        assert_eq!(app.world().resource::<Player>().sp, motion.max_sp);
        let node = app.world().get::<Node>(entity).unwrap();
        assert_eq!((node.left, node.width), (px(rect[0]), px(rect[2])));
        assert_eq!(
            app.world()
                .get::<ImageNode>(entity)
                .unwrap()
                .rect
                .unwrap()
                .min,
            Vec2::ZERO
        );
    }

    #[test]
    fn invalid_or_empty_capacity_never_produces_invalid_uvs() {
        assert_eq!(fraction(50., 100.), 0.5);
        assert_eq!(fraction(120., 100.), 1.);
        for (value, capacity) in [
            (1., 0.),
            (-10., 100.),
            (f32::NAN, 100.),
            (10., f32::INFINITY),
        ] {
            assert_eq!(fraction(value, capacity), 0.);
        }
    }
}

mod verification;
pub(crate) fn install_verification(app: &mut App) {
    verification::install(app);
}
