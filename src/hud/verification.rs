use super::*;
use crate::gameplay::Motion;
use bevy::{
    input::InputSystems,
    render::view::screenshot::{Screenshot, save_to_disk},
};
pub(super) fn install(app: &mut App) {
    app.init_resource::<HudSmoke>()
        .add_systems(
            PreUpdate,
            drive_smoke
                .before(InputSystems)
                .run_if(crate::loading::ready),
        )
        .add_systems(
            Update,
            inspect_smoke.after(update).run_if(crate::loading::ready),
        );
}
#[derive(Resource, Default)]
struct HudSmoke {
    stage: usize,
    captures: usize,
    checks: Vec<String>,
    inspected_frames: usize,
    previous_clock: Option<(HudMode, u32)>,
    clock_decreases: usize,
    minimum_sp: Option<f32>,
    previous_sp: Option<f32>,
    sp_recovered: bool,
    movement_reported: bool,
}

fn drive_smoke(world: &mut World) {
    if !world.resource::<Options>().hud_smoke {
        return;
    }
    let elapsed = world.resource::<Time>().elapsed_secs();
    let stage = world.resource::<HudSmoke>().stage;
    let schedule = [
        1.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
    ];
    if stage >= schedule.len() || elapsed < schedule[stage] {
        return;
    }
    match stage {
        0 | 5 | 8 => {
            let size = match stage {
                0 => (1024, 768),
                5 => (1280, 720),
                _ => (1920, 1080),
            };
            let mut window = world.query::<&mut Window>().single_mut(world).unwrap();
            window.resolution.set(size.0 as f32, size.1 as f32);
            window.focused = true;
            if stage != 0 {
                let map = if stage == 5 { "neden-1" } else { "circle-2" };
                world
                    .resource_mut::<Messages<crate::chat::MapCommand>>()
                    .write(crate::chat::MapCommand { map: map.into() });
            }
        }
        2 => {
            world
                .resource_mut::<Messages<crate::chat::MapCommand>>()
                .write(crate::chat::MapCommand {
                    map: "station-2".into(),
                });
        }
        3 | 6 | 9 => {
            let mut player = world.resource_mut::<Player>();
            player.health = 67.;
            player.sp = 50.;
            world.resource_mut::<Motion>().sp_recovery_per_second = 0.;
            let mut state = world.resource_mut::<MatchHud>();
            state.alpha_score = 12;
            state.beta_score = 8;
            state.first_place_score = 123;
            state.personal_score = 7;
            state.phase_label = if stage == 3 {
                "First Half".into()
            } else {
                String::new()
            };
        }
        1 | 4 | 7 | 10 => {
            let name = match stage {
                1 => "idle",
                4 => "td",
                7 => "dm",
                _ => "chaser",
            };
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("logs/hud-{name}.png")));
            world.resource_mut::<HudSmoke>().captures += 1;
        }
        11 => {
            let smoke = world.resource::<HudSmoke>();
            assert!(smoke.inspected_frames > 30);
            assert_eq!(smoke.captures, 4);
            assert!(smoke.clock_decreases >= 3, "Live clocks must count down");
            let report = serde_json::json!({"passed":true, "captures":smoke.captures,
                "inspected_frames":smoke.inspected_frames, "checks":smoke.checks,
                "clock_decreases":smoke.clock_decreases,
                "limits":"Scores and HP/SP snapshot values are injected; clocks are live local countdowns. Chaser uses a provisional 120-second practice round, not implemented match rules."});
            std::fs::write(
                "logs/hud-smoke.json",
                serde_json::to_string_pretty(&report).unwrap(),
            )
            .unwrap();
            info!("HUD smoke passed: {report}");
        }
        _ => {}
    }
    world.resource_mut::<HudSmoke>().stage += 1;
}

fn inspect_smoke(
    options: Res<Options>,
    time: Res<Time>,
    selection: Res<MapSelection>,
    player: Res<Player>,
    state: Res<MatchHud>,
    images: Res<HudImages>,
    bars: Query<(&HudBar, &Node, &ImageNode, &SourceRect)>,
    teams: Query<&Visibility, With<TeamOnly>>,
    modes: Query<(&ModeOnly, &Visibility)>,
    numbers: Query<(&Digit, &ImageNode, &Visibility)>,
    names: Query<&Name>,
    mut smoke: ResMut<HudSmoke>,
) {
    if !options.hud_smoke && !options.movement_smoke {
        return;
    }
    let mode = HudMode::for_map(selection.active);
    assert!(
        names
            .iter()
            .all(|name| !matches!(name.as_str(), "EU1162 label chaser_name" | "EU1162 label chaser_total_score"))
    );
    for visible in &teams {
        assert_eq!(*visible == Visibility::Hidden, mode == HudMode::Chaser);
    }
    for (only, visible) in &modes {
        assert_eq!(*visible == Visibility::Hidden, only.0 != mode);
    }
    for (bar, node, image, source) in &bars {
        let expected = match bar.kind {
            BarKind::Health => fraction(player.health, player.max_health),
            BarKind::Stamina => fraction(player.sp, player.max_sp()),
        };
        assert!((bar.fraction - expected).abs() < 0.0001);
        assert_eq!(node.width, px((bar.width * expected).trunc()));
        assert_eq!(image.rect.unwrap().min, Vec2::ZERO);
        assert!((image.rect.unwrap().width() - bar.texture_size[0] * expected).abs() < 0.0001);
        if bar.kind == BarKind::Stamina {
            let anchored_left =
                source.rect[0] + (options.ui_aspect.canvas().x / options.ui_scale.unwrap_or(1.) - 1024.) * 0.5;
            assert_eq!(
                node.left,
                px(anchored_left),
                "Depletion must preserve the gauge anchor"
            );
        }
    }
    for (digit, image, _) in &numbers {
        let value = match digit.number {
            Number::Minutes => Some(state.remaining_seconds / 60),
            Number::Seconds => Some(state.remaining_seconds % 60),
            Number::ChaserTime => Some(state.chaser_round_seconds),
            _ => None,
        };
        if let Some(value) = value {
            let n = value / 10u32.pow(digit.place) % 10;
            assert_eq!(image.image, images.0[&format!("{}-{n}", digit.material)]);
        }
    }
    let seconds = if mode == HudMode::Chaser {
        state.chaser_round_seconds
    } else {
        state.remaining_seconds
    };
    if let Some((previous_mode, previous)) = smoke.previous_clock {
        if previous_mode == mode {
            assert!(seconds <= previous);
            if seconds < previous {
                smoke.clock_decreases += 1;
            }
        }
    }
    smoke.previous_clock = Some((mode, seconds));
    smoke.minimum_sp = Some(smoke.minimum_sp.unwrap_or(player.sp).min(player.sp));
    if let Some(previous) = smoke.previous_sp {
        if player.sp > previous + 0.0001 && player.sp < player.max_sp() - 0.01 {
            smoke.sp_recovered = true;
        }
    }
    smoke.previous_sp = Some(player.sp);
    let label = format!(
        "{mode:?}: correct header visibility, stationary HP/SP fill crops, live timer sprites and no Chaser username"
    );
    if !smoke.checks.contains(&label) {
        smoke.checks.push(label);
    }
    smoke.inspected_frames += 1;
    if options.movement_smoke && time.elapsed_secs() >= 22.0 && !smoke.movement_reported {
        assert!(smoke.minimum_sp.unwrap() < player.max_sp() - 5.);
        assert!(
            smoke.sp_recovered,
            "SP must recover after actual movement actions"
        );
        let report = serde_json::json!({
            "passed":true, "inspected_frames":smoke.inspected_frames,
            "minimum_sp":smoke.minimum_sp, "sp_recovered":smoke.sp_recovered,
            "clock_decreases":smoke.clock_decreases,
            "checks":["HUD follows actual movement SP every rendered frame", "SP artwork stays fixed while its right edge depletes", "UV crop starts at zero", "Live local clock continues while chat blocks movement"],
        });
        std::fs::write(
            "logs/hud-movement-smoke.json",
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        smoke.movement_reported = true;
    }
}
