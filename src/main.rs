#![cfg_attr(windows, windows_subsystem = "windows")]

mod asset_import;
mod branding;
mod logging;
mod animation;
mod animation_core;
mod audio;
mod chat;
mod combat;
mod combat_geometry;
mod combat_types;
mod content;
mod controller;
mod counter_combat_verification;
mod counter_followup_verification;
mod counter_verification;
mod critical_feedback;
mod dagger_effect_check;
mod effect_core;
mod effects;
mod dynamic_mesh;
mod emotes;
mod gameplay;
mod hit_rules;
mod hud;
mod ui_text;
mod loading;
mod materials;
mod melee;
mod melee_verification;
mod minimap;
mod music;
mod parity;
mod particle_clock;
mod particle_geometry;
mod performance;
mod practice_animation;
mod scene;
mod simulation;
mod verification;
mod weapons;

use bevy::{
    app::AppExit,
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::PresentMode,
};
use std::path::PathBuf;

const DEFAULT_PHYSICS_HZ: f64 = 60.0;

#[derive(Resource, Clone)]
pub struct Options {
    pub smoke: bool,
    pub profile: bool,
    pub profile_render: bool,
    pub loading_check: bool,
    pub chat_smoke: bool,
    pub movement_smoke: bool,
    pub animation_smoke: bool,
    pub hud_smoke: bool,
    pub melee_smoke: bool,
    pub practice_smoke: bool,
    pub counter_smoke: bool,
    pub counter_combat_check: bool,
    pub counter_followup_check: bool,
    pub dagger_effect_check: bool,
    pub emotes_check: bool,
    pub unarmed: bool,
    pub skill: gameplay::Skill,
    pub ui_scale: Option<f32>,
    pub ui_aspect: hud::UiAspect,
    pub resolution: Option<[u32; 2]>,
    pub overview: bool,
    pub duration: Option<f32>,
    pub capture: Option<PathBuf>,
    pub map: scene::MapId,
    pub physics_hz: f64,
    pub telemetry: Option<PathBuf>,
    pub spawn: Option<Vec3>,
    pub yaw: Option<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            smoke: false,
            profile: false,
            profile_render: false,
            loading_check: false,
            chat_smoke: false,
            movement_smoke: false,
            animation_smoke: false,
            hud_smoke: false,
            melee_smoke: false,
            practice_smoke: false,
            counter_smoke: false,
            counter_combat_check: false,
            counter_followup_check: false,
            dagger_effect_check: false,
            emotes_check: false,
            unarmed: false,
            skill: gameplay::Skill::default(),
            ui_scale: None,
            ui_aspect: hud::UiAspect::default(),
            resolution: None,
            overview: false,
            duration: None,
            capture: None,
            map: scene::MapId::default(),
            physics_hz: DEFAULT_PHYSICS_HZ,
            telemetry: None,
            spawn: None,
            yaw: None,
        }
    }
}

fn main() {
    let original_cwd = std::env::current_dir().expect("Cannot read working folder");
    let root = asset_import::runtime_root();
    let root = if root.is_absolute() { root } else { original_cwd.join(root) };
    let _log_guard = logging::initialize(&root).unwrap_or_else(|error| {
        logging::report_error(&format!("Cannot open logs/esperust.log: {error}"));
        std::process::exit(1);
    });
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--help" || a == "-h") {
        match asset_import::ensure(&root, &args, &original_cwd) {
            Ok(true) => return,
            Ok(false) => {},
            Err(message) => { logging::report_error(&format!("{message}\n\nDetails: {}", root.join("logs/importer-console.log").display())); std::process::exit(1); }
        }
    }
    std::env::set_current_dir(&root).expect("Cannot enter ESPER data folder");
    if args.iter().any(|a| a == "--validate") {
        scene::validate_assets().expect("Map asset validation failed");
        return;
    }
    let mut options = Options::default();
    for (i, arg) in args.iter().enumerate().skip(1) {
        match arg.as_str() {
            "--emotes-check" => {
                options.emotes_check = true;
                options.duration = Some(25.);
            }
            "--profile" => options.profile = true,
            "--profile-render" => {
                options.profile = true;
                options.profile_render = true;
            }
            "--loading-check" => {
                options.loading_check = true;
                options.duration = Some(8.);
            }
            "--smoke-test" => {
                options.smoke = true;
                options.duration = Some(18.0);
            }
            "--chat-smoke-test" => {
                options.chat_smoke = true;
                options.duration = Some(12.0);
            }
            "--movement-smoke-test" => {
                options.movement_smoke = true;
                options.duration = Some(23.0);
            }
            "--animation-smoke-test" => {
                options.animation_smoke = true;
                options.duration = Some(8.0);
            }
            "--hud-smoke-test" => {
                options.hud_smoke = true;
                options.duration = Some(14.0);
            }
            "--melee-smoke-test" => {
                options.melee_smoke = true;
                options.duration = Some(36.5);
            }
            "--counter-combat-check" => {
                options.counter_combat_check = true;
                options.duration = Some(35.);
            }
            "--counter-followup-check" => {
                options.counter_followup_check = true;
                options.duration = Some(30.);
            }
            "--counter-smoke-test" => {
                options.counter_smoke = true;
                options.duration = Some(50.0);
            }
            "--dagger-effect-check" => {
                options.dagger_effect_check = true;
                options.duration = Some(8.5);
            }
            "--practice-smoke-test" => {
                options.practice_smoke = true;
                options.duration = Some(20.0);
            }
            "--unarmed" => options.unarmed = true,
            "--skill" => {
                options.skill = match args.get(i + 1).map(String::as_str) {
                    Some("sp") => gameplay::Skill::SpMastery,
                    Some("none") => gameplay::Skill::None,
                    _ => panic!("--skill needs sp or none"),
                };
            }
            "--overview" => options.overview = true,
            "--physics-hz" => {
                options.physics_hz =
                    parse_physics_hz(args.get(i + 1).expect("--physics-hz needs a frequency"))
                        .unwrap_or_else(|error| panic!("{error}"));
            }
            "--telemetry" => {
                options.telemetry = Some(PathBuf::from(
                    args.get(i + 1).expect("--telemetry needs a JSONL path"),
                ));
            }
            "--spawn" => {
                options.spawn = Some(
                    parse_spawn(args.get(i + 1).expect("--spawn needs x,y,z in metres"))
                        .unwrap_or_else(|error| panic!("{error}")),
                );
            }
            "--yaw" => {
                options.yaw = Some(
                    parse_yaw(args.get(i + 1).expect("--yaw needs radians"))
                        .unwrap_or_else(|error| panic!("{error}")),
                );
            }
            "--resolution" => {
                options.resolution = Some(
                    parse_resolution(args.get(i + 1).expect("--resolution needs a size"))
                        .unwrap_or_else(|error| panic!("{error}")),
                );
            }
            "--ui-aspect" => {
                options.ui_aspect = args
                    .get(i + 1)
                    .expect("--ui-aspect needs 4:3, 16:9 or 16:10")
                    .parse()
                    .expect("Invalid UI aspect; use 4:3, 16:9 or 16:10");
            }
            "--ui-scale" => {
                let scale: f32 = args
                    .get(i + 1)
                    .expect("--ui-scale needs a factor")
                    .parse()
                    .expect("Invalid UI scale");
                assert!(
                    scale.is_finite() && (0.25..=4.).contains(&scale),
                    "UI scale must be between 0.25 and 4"
                );
                options.ui_scale = Some(scale);
            }
            "--map" => {
                options.map = args
                    .get(i + 1)
                    .expect("--map needs a map id")
                    .parse()
                    .unwrap_or_else(|error| panic!("{error}"));
            }
            "--duration" => {
                options.duration = Some(
                    args.get(i + 1)
                        .expect("--duration needs seconds")
                        .parse()
                        .expect("Invalid seconds"),
                )
            }
            "--capture" => {
                options.capture = Some(PathBuf::from(
                    args.get(i + 1).expect("--capture needs a path"),
                ))
            }
            "--help" | "-h" => {
                println!(
                    "ESPERUST\n  --resources PATH       Import from a client folder or _resources (no executable needed)\n  --index PATH           Matching resource.s4hd if stored elsewhere\n  --import-only          Prepare assets and exit\n  --profile              Write frame intervals and CPU spans to logs/performance.json\n  --profile-render       Also collect render pass CPU/GPU diagnostics (extra overhead)\n  --emotes-check         Verify nine emotes, original UI/voices and /hug\n  --loading-check        Capture startup and all three map loading screens, then exit\n  --validate             Validate all original maps and collision without a display\n  --map MAP              circle-2, station-2, or neden-1\n  --smoke-test           Scripted movement, screenshots, then exit\n  --chat-smoke-test      Keyboard, chat, and map-switch integration checks\n  --hud-smoke-test       Source HUD, gauge, and mode-header integration checks\n  --movement-smoke-test  Keyboard and ground/air movement integration checks\n  --animation-smoke-test Rapid A/D turns with geometry regression checks\n  --melee-smoke-test     Plasma sword/dagger attacks, effects and damage\n  --counter-smoke-test   Counter sword input, chained attacks and guard presentation\n  --counter-combat-check Counter guard, attacker stun, crush and critical feedback\n  --counter-followup-check Held guard dodges/heavy and forward jump combo continuation\n  --dagger-effect-check  Capture only the dagger right-click sweep at seven timings\n  --practice-smoke-test  Dummy animations, hurt sounds, contact and cleanup checks\n  --unarmed              Unequip weapons for original movement comparisons\n  --overview             Elevated overview camera\n  --ui-scale FACTOR      Extra UI size multiplier (default 1; e.g. 1.25)\n  --ui-aspect ASPECT     Original UI aspect: 4:3, 16:9 or 16:10 (default 16:9)\n  --resolution SIZE      Window size: 1080, 1440, or WIDTHxHEIGHT\n  --skill sp|none       Passive skill (default sp: Skill Mastery, +40 SP)\n  --physics-hz HZ        Fixed physics frequency, 1..1000 (default 60)\n  --telemetry PATH       Record every completed physics tick as JSONL\n  --spawn X,Y,Z          Initial feet position in metres; normal respawns unchanged\n  --yaw RADIANS          Initial camera yaw (0 faces -Z, positive turns toward +X)\n  --duration SECONDS     Exit after a bounded run\n  --capture PATH         Save a screenshot after assets settle\nControls: click to capture mouse, WASD, double-W sprint, A/D held + Space dodge, Space jump/wall jump, Ctrl crouch, 1 plasma sword, 2 dagger, 3 counter sword, left/right mouse attacks, hold left for plasma heavy, R respawn, E emotes then 1–9, 0 cancel, Enter chat, Esc close emotes/release mouse, F3 diagnostics, F12 screenshot.\nChat: /help (all commands), /hug, /map neden-1, /map station-2, /map circle-2, /dummy, /spar, /spar guard, /spar cs-heavy, /god, /smooth (240 Hz), /smooth HZ, /smooth off (original 60 Hz)"
                );
                return;
            }
            _ => {}
        }
    }
    if options.smoke && options.capture.is_none() {
        options.capture = Some(root.join(format!("logs/{}-smoke.png", options.map.id())));
    }
    if options.chat_smoke && options.capture.is_none() {
        options.capture = Some(root.join("logs/chat-smoke.png"));
    }
    if options.movement_smoke && options.capture.is_none() {
        options.capture = Some(root.join("logs/movement-smoke.png"));
    }
    std::fs::create_dir_all("logs").expect("Cannot create prototype log folder");
    let window_title = String::from("ESPERUST");
    let [window_width, window_height] = options.resolution.unwrap_or([1280, 720]);
    let physics_hz = options.physics_hz;
    let telemetry = options
        .telemetry
        .as_deref()
        .map(|path| parity::Telemetry::open(path).expect("Cannot open parity telemetry output"));
    let mut app = App::new();
    app.insert_resource(options)
        .insert_resource(Time::<Fixed>::from_hz(physics_hz))
        .insert_resource(ClearColor(Color::srgb(0.16, 0.20, 0.26)))
        .add_plugins(
            DefaultPlugins
                .set(bevy::audio::AudioPlugin {
                    global_volume: bevy::audio::GlobalVolume::new(bevy::audio::Volume::Linear(0.25)),
                    ..default()
                })
                .set(bevy::log::LogPlugin { fmt_layer: logging::fmt_layer, ..default() })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: window_title,
                        resolution: (window_width, window_height).into(),
                        present_mode: PresentMode::AutoNoVsync,
                        #[cfg(target_os = "linux")]
                        transparent: true,
                        #[cfg(target_os = "linux")]
                        composite_alpha_mode: bevy::window::CompositeAlphaMode::PreMultiplied,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: root.join("assets").to_string_lossy().into_owned(),
                    ..default()
                }),
        )
        .add_systems(Update, branding::set_window_icon)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(simulation::SimulationPlugin)
        .add_plugins(loading::LoadingPlugin)
        .add_plugins(materials::DualTexturePlugin)
        .add_plugins(music::MusicPlugin)
        .add_plugins(audio::MovementAudioPlugin)
        .add_plugins(emotes::EmotesPlugin)
        .add_plugins(effects::MovementEffectsPlugin)
        .add_plugins(weapons::WeaponsPlugin)
        .add_plugins(combat::CombatPlugin)
        .add_plugins(critical_feedback::CriticalFeedbackPlugin)
        .add_plugins(chat::ChatPlugin)
        .add_plugins(hud::HudPlugin)
        .add_plugins(minimap::MinimapPlugin)
        .init_resource::<gameplay::Controls>()
        .init_resource::<gameplay::MovementInput>()
        .init_resource::<gameplay::CameraShake>()
        .add_systems(
            crate::loading::PrepareWorld,
            (content::preload, scene::setup, gameplay::setup).chain(),
        )
        .add_systems(crate::loading::PrepareMap, scene::load_requested_map)
        .add_systems(
            PreUpdate,
            (gameplay::read_input.after(chat::chat_input)).run_if(crate::loading::ready),
        )
        .add_systems(
            FixedUpdate,
            gameplay::prepare_input
                .in_set(simulation::SimulationSet::Prepare)
                .run_if(crate::loading::ready),
        )
        .add_systems(
            FixedUpdate,
            gameplay::simulate
                .in_set(simulation::SimulationSet::Move)
                .run_if(crate::loading::ready),
        )
        .add_systems(
            Update,
            ((
                scene::switch_maps,
                gameplay::capture_camera_shake,
                gameplay::present,
                animation::animate,
                scene::animate_map,
                gameplay::hud,
                capture_and_exit,
            )
                .chain())
            .run_if(crate::loading::ready),
        );
    if let Some(telemetry) = telemetry {
        app.insert_resource(telemetry)
            .add_systems(Last, parity::flush_on_exit);
    }
    dynamic_mesh::install(&mut app);
    verification::install(&mut app);
    app.run();
}

fn parse_physics_hz(value: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|hz| hz.is_finite() && (1.0..=1000.0).contains(hz))
        .ok_or_else(|| "Physics frequency must be a finite number between 1 and 1000 Hz".into())
}

fn parse_spawn(value: &str) -> Result<Vec3, String> {
    let coordinates: Vec<f32> = value
        .split(',')
        .map(|coordinate| coordinate.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|_| "Spawn must contain exactly three finite coordinates: X,Y,Z".to_owned())?;
    if coordinates.len() != 3 || coordinates.iter().any(|value| !value.is_finite()) {
        return Err("Spawn must contain exactly three finite coordinates: X,Y,Z".into());
    }
    Ok(Vec3::new(coordinates[0], coordinates[1], coordinates[2]))
}

fn parse_yaw(value: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .ok()
        .filter(|yaw| yaw.is_finite())
        .ok_or_else(|| "Yaw must be a finite angle in radians".into())
}

fn parse_resolution(value: &str) -> Result<[u32; 2], String> {
    let size = match value.to_ascii_lowercase().as_str() {
        "720" | "720p" => Some([1280, 720]),
        "1080" | "1080p" => Some([1920, 1080]),
        "1440" | "1440p" => Some([2560, 1440]),
        "2160" | "2160p" | "4k" => Some([3840, 2160]),
        value => value
            .split_once('x')
            .and_then(|(width, height)| Some([width.parse().ok()?, height.parse().ok()?])),
    };
    size.filter(|size| size.iter().all(|dimension| (1..=16384).contains(dimension)))
        .ok_or_else(|| "Resolution must be 720, 1080, 1440, 2160, or WIDTHxHEIGHT".into())
}

fn capture_and_exit(
    mut commands: Commands,
    options: Res<Options>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut saved: Local<bool>,
    mut exit: MessageWriter<AppExit>,
    player: Res<gameplay::Player>,
    hud: Res<hud::MatchHud>,
    selection: Res<scene::MapSelection>,
    audio: Res<audio::MovementAudioStats>,
    effects: Res<effects::EffectStats>,
    music_sinks: Query<&AudioSink, With<music::MapMusic>>,
    window: Single<&Window>,
    ui_scale: Res<UiScale>,
) {
    let capture_time = if options.chat_smoke { 10.5 } else { 5.0 };
    if (!*saved && time.elapsed_secs() > capture_time && options.capture.is_some())
        || keys.just_pressed(KeyCode::F12)
    {
        let path = options.capture.clone().unwrap_or_else(|| {
            PathBuf::from(format!(
                "logs/{}-{}.png",
                selection.active.id(),
                time.elapsed_secs() as u32
            ))
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        *saved = true;
    }
    if options.duration.is_some_and(|d| time.elapsed_secs() >= d) {
        let report = serde_json::json!({
            "map":selection.active.label(), "fixed_hz":options.physics_hz, "elapsed_seconds":time.elapsed_secs(),
            "position_m":player.feet.to_array(), "grounded":player.grounded,
            "distance_m":player.distance, "jumps":player.jumps,
            "dodges":player.dodges, "wall_jumps":player.wall_jumps,
            "air_dodges":player.air_dodges, "air_sprints":player.air_sprints,
            "sprint_distance_m":player.sprint_distance, "sp":player.sp,
            "max_sp":player.max_sp(), "skill":format!("{:?}", player.equipped_skill()),
            "health":player.health, "max_health":player.max_health,
            "window":{"physical_width":window.physical_width(),
                      "physical_height":window.physical_height(),
                      "display_scale":window.scale_factor(), "ui_scale":ui_scale.0},
            "hud":{"mode":format!("{:?}", hud::HudMode::for_map(selection.active)),
                   "remaining_seconds":hud.remaining_seconds,
                   "chaser_round_seconds":hud.chaser_round_seconds},
            "respawns":player.respawns, "finite":player.feet.is_finite(),
            "audio": {"footsteps":audio.footsteps, "jumps":audio.jumps,
                      "dodges":audio.dodges, "wall_jumps":audio.wall_jumps, "sprints":audio.sprints, "landings":audio.landings,
                      "playing_music_sinks":music_sinks.iter().filter(|sink| !sink.empty()).count()},
            "effects": &*effects,
            "parity":"original geometry/textures/constants; collision/camera/action transitions provisional"
        });
        let _ = std::fs::write(
            "logs/last-run.json",
            serde_json::to_string_pretty(&report).unwrap(),
        );
        info!("Bounded run complete: {}", report);
        if options.movement_smoke {
            let _ = std::fs::write(
                "logs/movement-run.json",
                serde_json::to_string_pretty(&report).unwrap(),
            );
        }
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod option_tests {
    use super::*;

    #[test]
    fn physics_frequency_keeps_default_and_accepts_supported_rates() {
        assert_eq!(Options::default().physics_hz, 60.0);
        for hz in [
            "1", "30", "60", "62.5", "120", "180", "240", "1000", "179.82",
        ] {
            assert_eq!(parse_physics_hz(hz).unwrap(), hz.parse::<f64>().unwrap());
        }
    }

    #[test]
    fn physics_frequency_rejects_invalid_and_out_of_range_values() {
        for hz in [
            "", "abc", "0", "-180", "0.9", "1000.1", "NaN", "inf", "-inf",
        ] {
            assert!(parse_physics_hz(hz).is_err(), "accepted {hz}");
        }
    }

    #[test]
    fn paired_start_requires_finite_coordinates_and_yaw() {
        assert_eq!(
            parse_spawn("1.5, -2, 3").unwrap(),
            Vec3::new(1.5, -2.0, 3.0)
        );
        for spawn in ["", "1,2", "1,2,3,4", "1,NaN,3", "1,2,inf"] {
            assert!(parse_spawn(spawn).is_err(), "accepted {spawn}");
        }
        assert_eq!(parse_yaw("-3.14").unwrap(), -3.14);
        for yaw in ["", "abc", "NaN", "inf", "-inf"] {
            assert!(parse_yaw(yaw).is_err(), "accepted {yaw}");
        }
    }
}
