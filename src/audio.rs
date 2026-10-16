use crate::{
    animation::{self, Animation},
    gameplay::{MovementAction, Player},
    scene::{self, MapId, MapSelection},
};
use bevy::{audio::Volume, prelude::*};

const SOUND_ROOT: &str = "original/resources/sound/";
const FOOT_VOLUME: f32 = 0.5;
const SFX_VOLUME: f32 = 0.9;

pub struct MovementAudioPlugin;

impl Plugin for MovementAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementAudio>()
            .init_resource::<MovementAudioStats>()
            .add_systems(crate::loading::PrepareWorld, load_sounds)
            .add_systems(Update, (movement_audio.after(animation::animate)).run_if(crate::loading::ready));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Surface {
    #[default]
    Unknown,
    Ground,
    Rock,
    Steel,
    Wooden,
    Water,
    Glass,
}

impl Surface {
    const ALL: [Self; 6] = [
        Self::Ground,
        Self::Rock,
        Self::Steel,
        Self::Wooden,
        Self::Water,
        Self::Glass,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Ground => "ground",
            Self::Rock => "rock",
            Self::Steel => "steel",
            Self::Wooden => "wooden",
            Self::Water => "water",
            Self::Glass => "glass",
        }
    }

    fn from_mesh_name(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        if name.contains("/oct_land_ground") {
            Self::Ground
        } else if name.contains("/oct_land_stone") {
            Self::Rock
        } else if name.contains("/oct_land_steel") {
            Self::Steel
        } else if name.contains("/oct_land_wood") {
            Self::Wooden
        } else if name.contains("/oct_land_water") {
            Self::Water
        } else if name.contains("/oct_land_glass") {
            Self::Glass
        } else {
            Self::Unknown
        }
    }

    fn index(self) -> Option<usize> {
        Self::ALL.iter().position(|&surface| surface == self)
    }
}

struct SoundSet {
    steps: [Handle<AudioSource>; 4],
    landing: Handle<AudioSource>,
}

struct FloorTriangle {
    points: [Vec3; 3],
    surface: Surface,
}

#[derive(Resource, Default)]
struct MovementAudio {
    surfaces: Vec<SoundSet>,
    jump: Handle<AudioSource>,
    dodge: Handle<AudioSource>,
    wall_jump: Handle<AudioSource>,
    wall_jump_failure: Handle<AudioSource>,
    sprint_voice: Handle<AudioSource>,
    floor: Vec<FloorTriangle>,
    map: Option<MapId>,
    previous: Option<Snapshot>,
    foot_phase: Option<u32>,
    random: u32,
}

#[derive(Clone, Copy)]
struct Snapshot {
    jumps: u32,
    dodges: u32,
    wall_jumps: u32,
    wall_jump_failures: u32,
    grounded: bool,
    reset: u32,
    action: MovementAction,
    action_serial: u32,
}

impl Snapshot {
    fn from_player(player: &Player) -> Self {
        Self {
            jumps: player.jumps,
            dodges: player.dodges,
            wall_jumps: player.wall_jumps,
            wall_jump_failures: player.wall_jump_failures,
            grounded: player.grounded,
            reset: player.pose_reset_serial,
            action: player.action,
            action_serial: player.action_serial,
        }
    }
}

#[derive(Resource, Default)]
pub struct MovementAudioStats {
    pub footsteps: u32,
    pub jumps: u32,
    pub dodges: u32,
    pub wall_jumps: u32,
    pub wall_jump_failures: u32,
    pub sprints: u32,
    pub landings: u32,
}

fn load_sounds(server: Res<AssetServer>, mut audio: ResMut<MovementAudio>) {
    audio.jump = server.load(format!("{SOUND_ROOT}jump_up.ogg"));
    audio.dodge = server.load(format!("{SOUND_ROOT}dodge.ogg"));
    audio.wall_jump = server.load(format!("{SOUND_ROOT}dash.ogg"));
    audio.wall_jump_failure = server.load(format!("{SOUND_ROOT}warning.ogg"));
    audio.sprint_voice = server.load(format!("{SOUND_ROOT}voice_man_fastrun.ogg"));
    audio.surfaces = Surface::ALL
        .into_iter()
        .map(|surface| SoundSet {
            steps: std::array::from_fn(|i| {
                server.load(format!("{SOUND_ROOT}run_{}_{}.ogg", surface.name(), i + 1))
            }),
            landing: server.load(format!("{SOUND_ROOT}land_{}.ogg", surface.name())),
        })
        .collect();
    audio.random = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x6d2b79f5, |time| time.subsec_nanos().max(1));
}

fn footstep_phase(milliseconds: u32) -> u32 {
    milliseconds.wrapping_add(200) % 333
}

impl MovementAudio {
    fn load_floor(&mut self, id: MapId) {
        self.floor.clear();
        if let Ok(map) = scene::read_map_for(id) {
            for mesh in map.meshes.into_iter().filter(|mesh| mesh.collision) {
                let surface = Surface::from_mesh_name(&mesh.name);
                for indices in mesh.indices.chunks_exact(3) {
                    let points =
                        std::array::from_fn(|i| Vec3::from(mesh.positions[indices[i] as usize]));
                    let normal = (points[1] - points[0])
                        .cross(points[2] - points[0])
                        .normalize_or_zero();
                    if normal.y.abs() >= 0.5 {
                        self.floor.push(FloorTriangle { points, surface });
                    }
                }
            }
        }
        self.map = Some(id);
    }

    fn surface_at(&self, feet: Vec3) -> Surface {
        let mut best_distance = 0.8;
        let mut best = Surface::Unknown;
        for triangle in &self.floor {
            if let Some(y) = floor_height(triangle.points, feet.x, feet.z) {
                let distance = feet.y + 0.15 - y;
                if distance >= 0.0 && distance < best_distance {
                    best_distance = distance;
                    best = triangle.surface;
                }
            }
        }
        best
    }

    fn variant(&mut self) -> usize {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        (self.random & 3) as usize
    }
}

fn floor_height(points: [Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = points;
    let v0 = Vec2::new(b.x - a.x, b.z - a.z);
    let v1 = Vec2::new(c.x - a.x, c.z - a.z);
    let p = Vec2::new(x - a.x, z - a.z);
    let determinant = v0.perp_dot(v1);
    if determinant.abs() < 0.000001 {
        return None;
    }
    let u = p.perp_dot(v1) / determinant;
    let v = v0.perp_dot(p) / determinant;
    (u >= -0.001 && v >= -0.001 && u + v <= 1.001)
        .then_some(a.y + (b.y - a.y) * u + (c.y - a.y) * v)
}

fn play(commands: &mut Commands, source: Handle<AudioSource>, volume: f32) {
    debug!(
        "Movement sound {:?}, linear gain {}",
        source.path(),
        volume * SFX_VOLUME
    );
    commands.spawn((
        AudioPlayer::new(source),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume * SFX_VOLUME)),
    ));
}

fn movement_audio(
    mut commands: Commands,
    player: Res<Player>,
    selection: Res<MapSelection>,
    animation: Option<Res<Animation>>,
    mut audio: ResMut<MovementAudio>,
    mut stats: ResMut<MovementAudioStats>,
) {
    let current = Snapshot::from_player(&player);
    let phase = animation
        .and_then(|animation| animation.footstep_cursor_ms())
        .map(footstep_phase);
    if audio.map != Some(selection.active) {
        audio.load_floor(selection.active);
        audio.previous = Some(current);
        audio.foot_phase = phase;
        return;
    }
    let Some(previous) = audio.previous.replace(current) else {
        return;
    };
    if previous.reset != current.reset {
        audio.foot_phase = phase;
        return;
    }
    if current.jumps != previous.jumps {
        play(&mut commands, audio.jump.clone(), 1.0);
        stats.jumps += 1;
    }
    if current.dodges != previous.dodges {
        play(&mut commands, audio.dodge.clone(), 1.0);
        stats.dodges += 1;
    }
    if current.wall_jumps != previous.wall_jumps {
        play(&mut commands, audio.wall_jump.clone(), 1.0);
        stats.wall_jumps += 1;
    }
    if current.wall_jump_failures != previous.wall_jump_failures {
        play(&mut commands, audio.wall_jump_failure.clone(), 1.0);
        stats.wall_jump_failures += 1;
    }
    if current.action == MovementAction::Sprint && current.action_serial != previous.action_serial {
        play(&mut commands, audio.sprint_voice.clone(), 1.0);
        stats.sprints += 1;
    }
    let surface = audio.surface_at(player.feet);
    if !previous.grounded && current.grounded {
        if let Some(index) = surface.index() {
            play(&mut commands, audio.surfaces[index].landing.clone(), 1.0);
            stats.landings += 1;
        }
    }
    if current.grounded
        && phase
            .zip(audio.foot_phase)
            .is_some_and(|(now, last)| now < last)
    {
        if let Some(index) = surface.index() {
            let variant = audio.variant();
            play(
                &mut commands,
                audio.surfaces[index].steps[variant].clone(),
                FOOT_VOLUME,
            );
            stats.footsteps += 1;
        }
    }
    audio.foot_phase = phase;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::audio::Decodable;

    #[test]
    fn original_movement_vorbis_assets_all_decode() {
        let manifest: serde_json::Value =
            crate::content::required_json("movement-audio.json");
        let files = manifest["assets"].as_array().unwrap();
        assert_eq!(files.len(), 35);
        assert_eq!(manifest["sfx_volume_percent"], 90);
        assert_eq!(manifest["foot_volume"], 0.5);
        for file in files {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets")
                .join(file["path"].as_str().unwrap());
            let source = AudioSource {
                bytes: std::fs::read(&path).unwrap().into(),
            };
            assert!(
                source.decoder().take(1024).count() > 0,
                "{}",
                path.display()
            );
        }
    }

    #[test]
    fn native_footstep_clock_wraps_at_133_then_every_333_animation_milliseconds() {
        let steps: Vec<_> = (1..1000)
            .filter(|&ms| footstep_phase(ms) < footstep_phase(ms - 1))
            .collect();
        assert_eq!(steps, vec![133, 466, 799]);
        assert_eq!(footstep_phase(0), footstep_phase(666));
    }

    #[test]
    fn surface_selection_uses_original_collision_names_and_nearest_floor() {
        assert_eq!(
            Surface::from_mesh_name("ds1_neden01.scn/oct_land_stone/0"),
            Surface::Rock
        );
        assert_eq!(
            Surface::from_mesh_name("ds5_station.scn/oct_land_steel/0"),
            Surface::Steel
        );
        assert_eq!(
            Surface::from_mesh_name("ds5_station.scn/oct_Box2408/0"),
            Surface::Unknown
        );
        let floor = |y, surface| FloorTriangle {
            points: [
                Vec3::new(-1., y, -1.),
                Vec3::new(1., y, -1.),
                Vec3::new(0., y, 1.),
            ],
            surface,
        };
        let audio = MovementAudio {
            floor: vec![floor(0., Surface::Ground), floor(0.4, Surface::Steel)],
            ..default()
        };
        assert_eq!(audio.surface_at(Vec3::new(0., 0.4, 0.)), Surface::Steel);
        assert_eq!(audio.surface_at(Vec3::new(0., 0., 0.)), Surface::Ground);
        assert_eq!(audio.surface_at(Vec3::new(5., 0., 0.)), Surface::Unknown);
    }

    #[test]
    fn movement_sound_edges_do_not_repeat_or_replay_across_respawn() {
        let mut app = App::new();
        let mut player = Player::new(Vec3::ZERO, &crate::gameplay::Motion::default());
        player.grounded = true;
        let previous = Snapshot::from_player(&player);
        app.insert_resource(player)
            .insert_resource(MapSelection::default())
            .insert_resource(MovementAudio {
                map: Some(MapId::default()),
                previous: Some(previous),
                surfaces: Surface::ALL
                    .into_iter()
                    .map(|_| SoundSet {
                        steps: std::array::from_fn(|_| Handle::default()),
                        landing: Handle::default(),
                    })
                    .collect(),
                floor: vec![FloorTriangle {
                    points: [
                        Vec3::new(-1., 0., -1.),
                        Vec3::new(1., 0., -1.),
                        Vec3::new(0., 0., 1.),
                    ],
                    surface: Surface::Ground,
                }],
                ..default()
            })
            .init_resource::<MovementAudioStats>()
            .add_systems(Update, movement_audio);
        {
            let mut player = app.world_mut().resource_mut::<Player>();
            player.jumps += 1;
            player.grounded = false;
        }
        app.update();
        app.update();
        assert_eq!(app.world().resource::<MovementAudioStats>().jumps, 1);
        app.world_mut().resource_mut::<Player>().grounded = true;
        app.update();
        assert_eq!(app.world().resource::<MovementAudioStats>().landings, 1);
        app.world_mut().resource_mut::<Player>().dodges += 1;
        app.update();
        app.update();
        assert_eq!(app.world().resource::<MovementAudioStats>().dodges, 1);
        app.world_mut().resource_mut::<Player>().wall_jumps += 1;
        app.update();
        app.update();
        assert_eq!(app.world().resource::<MovementAudioStats>().wall_jumps, 1);
        {
            let mut player = app.world_mut().resource_mut::<Player>();
            player.pose_reset_serial += 1;
            player.jumps += 1;
        }
        app.update();
        assert_eq!(app.world().resource::<MovementAudioStats>().jumps, 1);
        let settings: Vec<_> = app
            .world_mut()
            .query::<&PlaybackSettings>()
            .iter(app.world())
            .collect();
        assert_eq!(settings.len(), 4);
        assert!(
            settings
                .iter()
                .all(|settings| matches!(settings.mode, bevy::audio::PlaybackMode::Despawn))
        );
        assert!(
            settings
                .iter()
                .all(|settings| settings.volume == Volume::Linear(SFX_VOLUME))
        );
    }
}
