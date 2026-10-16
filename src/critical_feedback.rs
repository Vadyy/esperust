use crate::{Options, combat::CombatHit};
use bevy::{audio::Volume, prelude::*};
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

pub struct CriticalFeedbackPlugin;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CriticalFeedbackSet;
impl Plugin for CriticalFeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FeedbackClock>()
            .init_resource::<CriticalFeedbackStats>()
            .add_systems(crate::loading::PrepareWorld, setup)
            .add_systems(
                Update,
                ((consume_hits, animate).chain().in_set(CriticalFeedbackSet))
                    .run_if(crate::loading::ready),
            );
    }
}

#[derive(Deserialize)]
struct Manifest {
    ui_feedback: HashMap<String, FeedbackSource>,
    audio: HashMap<String, AudioEntry>,
}
#[derive(Deserialize)]
struct AudioEntry {
    path: String,
}
#[derive(Resource, Deserialize)]
struct FeedbackSource {
    sequence: Sequence,
    models: HashMap<String, Model>,
    audio_key: String,
    debounce_ms: u64,
}
#[derive(Deserialize)]
struct Sequence {
    duration_ms: u32,
    components: Vec<Loader>,
}
#[derive(Deserialize)]
struct Loader {
    start_ms: u32,
    end_ms: u32,
    fade_in_ms: u32,
    fade_out_ms: u32,
}
impl Loader {
    fn opacity(&self, age_ms: f32) -> f32 {
        if age_ms < self.start_ms as f32 {
            return 0.;
        }
        if age_ms <= self.end_ms as f32 {
            if self.fade_in_ms == 0 {
                1.
            } else {
                ((age_ms - self.start_ms as f32) / self.fade_in_ms as f32).min(1.)
            }
        } else if self.fade_out_ms == 0 {
            0.
        } else {
            (1. - (age_ms - self.end_ms as f32) / self.fade_out_ms as f32).max(0.)
        }
    }
}
#[derive(Deserialize)]
struct Model {
    ticks_per_ms: f32,
    units: String,
    meshes: Vec<Quad>,
}
#[derive(Deserialize)]
struct Quad {
    texture: String,
    shader_flags: u32,
    uvs: Vec<[f32; 2]>,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Frame {
    tick: f32,
    alpha: f32,
    positions: Vec<[f32; 3]>,
}
struct Sample {
    min: Vec2,
    max: Vec2,
    alpha: f32,
}
impl Quad {
    fn sample(&self, tick: f32) -> Sample {
        let index = self
            .frames
            .partition_point(|f| f.tick < tick)
            .min(self.frames.len() - 1);
        let a = &self.frames[index.saturating_sub(1)];
        let b = &self.frames[index];
        let weight = if b.tick > a.tick {
            ((tick - a.tick) / (b.tick - a.tick)).clamp(0., 1.)
        } else {
            0.
        };
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for (a, b) in a.positions.iter().zip(&b.positions) {
            let p = Vec2::new(a[0], a[1]).lerp(Vec2::new(b[0], b[1]), weight);
            min = min.min(p);
            max = max.max(p);
        }
        Sample {
            min,
            max,
            alpha: a.alpha + (b.alpha - a.alpha) * weight,
        }
    }
    fn validate(&self) {
        assert_eq!(self.uvs, [[0., 1.], [1., 1.], [0., 0.], [1., 0.]]);
        assert_eq!(
            self.shader_flags & (2 | 32),
            2,
            "Critical artwork uses alpha blend, not additive"
        );
        assert!(!self.frames.is_empty());
        for f in &self.frames {
            assert_eq!(f.positions.len(), 4);
            let [a, b, c, d] = f.positions.as_slice() else {
                unreachable!()
            };
            assert!((a[0] - c[0]).abs() < 0.001 && (b[0] - d[0]).abs() < 0.001);
            assert!((a[1] - b[1]).abs() < 0.001 && (c[1] - d[1]).abs() < 0.001);
        }
    }
}
#[derive(Resource)]
struct CriticalSound(Handle<AudioSource>);
#[derive(Component)]
pub(crate) struct CriticalFeedbackLayer(usize);
#[derive(Resource, Default)]
struct FeedbackClock {
    last_accepted: Option<Duration>,
    started: Option<Duration>,
}
impl FeedbackClock {
    fn accept(&mut self, now: Duration, debounce_ms: u64) -> bool {
        if self
            .last_accepted
            .is_some_and(|last| now.saturating_sub(last) < Duration::from_millis(debounce_ms))
        {
            return false;
        }
        self.last_accepted = Some(now);
        self.started = Some(now);
        true
    }
    fn age(&mut self, now: Duration, lifetime_ms: u32) -> Option<f32> {
        let age = (now.saturating_sub(self.started?).as_secs_f64() * 1000.) as f32;
        if age >= lifetime_ms as f32 {
            self.started = None;
            None
        } else {
            Some(age)
        }
    }
}
#[derive(Resource, Default)]
pub(crate) struct CriticalFeedbackStats {
    pub accepted: u32,
    pub suppressed: u32,
    pub sounds_played: u32,
    pub age_ms: Option<f32>,
    pub layer_opacity: [f32; 2],
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read("assets/melee/melee.json")
            .expect("Critical feedback requires imported melee manifest"),
    )
    .expect("Invalid critical feedback manifest");
    let mut feedback = manifest.ui_feedback;
    let source = feedback
        .remove("critical")
        .expect("Missing original critical feedback");
    assert_eq!(source.sequence.components.len(), 1);
    let model = &source.models["inter_critical_damage"];
    assert_eq!(model.units, "source_screen_units");
    assert_eq!(model.meshes.len(), 2);
    let canvas = commands
        .spawn((
            Name::new("Original critical feedback logical canvas"),
            crate::hud::DesignCanvas,
            crate::hud::source_node([0., 0., 1024., 768.]),
            UiTransform::default(),
        ))
        .id();
    for (i, quad) in model.meshes.iter().enumerate() {
        quad.validate();
        let layer = commands
            .spawn((
                Name::new(format!("Original critical feedback layer {i}")),
                CriticalFeedbackLayer(i),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ImageNode::new(assets.load(quad.texture.clone())),
                Visibility::Hidden,
                GlobalZIndex(8 + i as i32),
            ))
            .id();
        commands.entity(canvas).add_child(layer);
    }
    commands.insert_resource(CriticalSound(
        assets.load(manifest.audio[&source.audio_key].path.clone()),
    ));
    commands.insert_resource(source);
}
fn consume_hits(
    mut commands: Commands,
    time: Res<Time<Real>>,
    source: Res<FeedbackSource>,
    sound: Res<CriticalSound>,
    mut clock: ResMut<FeedbackClock>,
    mut stats: ResMut<CriticalFeedbackStats>,
    mut hits: MessageReader<CombatHit>,
) {
    for _ in hits.read().filter(|hit| hit.critical && hit.local_attacker) {
        if !clock.accept(time.elapsed(), source.debounce_ms) {
            stats.suppressed += 1;
            continue;
        }
        stats.accepted += 1;
        stats.sounds_played += 1;
        commands.spawn((
            AudioPlayer::new(sound.0.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.9)),
        ));
    }
}
fn screen_rect(sample: &Sample, viewport: Vec2, scale: f32) -> [f32; 4] {
    let root_x = (viewport.x - 1024.) * 0.5;
    [
        viewport.x * 0.5 / scale + root_x + sample.min.x,
        viewport.y * 0.5 / scale - sample.max.y,
        sample.max.x - sample.min.x,
        sample.max.y - sample.min.y,
    ]
}
fn animate(
    time: Res<Time<Real>>,
    source: Res<FeedbackSource>,
    options: Res<Options>,
    mut clock: ResMut<FeedbackClock>,
    mut stats: ResMut<CriticalFeedbackStats>,
    mut layers: Query<(
        &CriticalFeedbackLayer,
        &mut Node,
        &mut ImageNode,
        &mut Visibility,
    )>,
) {
    let age = clock.age(time.elapsed(), source.sequence.duration_ms);
    stats.age_ms = age;
    stats.layer_opacity = [0.; 2];
    let viewport = options.ui_aspect.canvas();
    let scale = options.ui_scale.unwrap_or(1.);
    let model = &source.models["inter_critical_damage"];
    for (layer, mut node, mut image, mut visibility) in &mut layers {
        let Some(age) = age else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let sample = model.meshes[layer.0].sample(age * model.ticks_per_ms);
        let opacity = sample.alpha * source.sequence.components[0].opacity(age);
        let rect = screen_rect(&sample, viewport, scale);
        node.left = px(rect[0]);
        node.top = px(rect[1]);
        node.width = px(rect[2]);
        node.height = px(rect[3]);
        image.color = Color::srgba(1., 1., 1., opacity);
        *visibility = if opacity > 0. {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        stats.layer_opacity[layer.0] = opacity;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> FeedbackSource {
        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read("assets/melee/melee.json").unwrap()).unwrap();
        manifest
            .ui_feedback
            .into_iter()
            .find(|(key, _)| key == "critical")
            .unwrap()
            .1
    }
    #[test]
    fn original_critical_quads_keep_native_projection_uv_and_fade_tail() {
        let source = source();
        let model = &source.models["inter_critical_damage"];
        for q in &model.meshes {
            q.validate();
        }
        let sample = model.meshes[0].sample(480.);
        let rect = screen_rect(&sample, Vec2::new(1024., 768.), 1.);
        assert!((rect[0] - 748.8816).abs() < 0.001);
        assert!((rect[1] - 348.482).abs() < 0.001);
        assert!((rect[2] - 256.).abs() < 0.001 && (rect[3] - 64.).abs() < 0.001);
        let wide = screen_rect(&sample, Vec2::new(1365., 768.), 1.);
        assert!((wide[0] - rect[0] - 341.).abs() < 0.001);
        assert_eq!(model.meshes[0].sample(0.).alpha, 0.);
        assert_eq!(model.meshes[0].sample(480.).alpha, 1.);
        assert_eq!(model.meshes[1].sample(1200.).alpha, 0.);
        let loader = &source.sequence.components[0];
        assert_eq!(
            (
                loader.end_ms,
                loader.fade_out_ms,
                source.sequence.duration_ms
            ),
            (1500, 500, 2000)
        );
        assert_eq!(loader.opacity(1500.), 1.);
        assert_eq!(loader.opacity(1750.), 0.5);
        assert_eq!(loader.opacity(2000.), 0.);
    }
    #[test]
    fn critical_projection_applies_native_scale_once_inside_the_shared_canvas() {
        let sample = Sample {
            min: Vec2::new(100., -30.),
            max: Vec2::new(356., 34.),
            alpha: 1.,
        };
        let canvas = crate::hud::UiAspect::SixteenNine.canvas();
        let rect = screen_rect(&sample, canvas, 1.25);
        assert!((rect[0] - (1365. / 2.5 + 170.5 + 100.)).abs() < 0.001);
        assert_eq!([rect[2], rect[3]], [256., 64.]);
        assert!((rect[2] * 1.25 * 1920. / 1365. - 450.1099).abs() < 0.001);
    }
    #[test]
    fn native_debounce_suppresses_restart_but_replaces_old_sequence_after_250ms() {
        let mut clock = FeedbackClock::default();
        assert!(clock.accept(Duration::ZERO, 250));
        assert!(!clock.accept(Duration::from_millis(249), 250));
        assert_eq!(clock.age(Duration::from_millis(249), 2000), Some(249.));
        assert!(clock.accept(Duration::from_millis(250), 250));
        assert_eq!(clock.age(Duration::from_millis(250), 2000), Some(0.));
        assert_eq!(clock.age(Duration::from_millis(2249), 2000), Some(1999.));
        assert_eq!(clock.age(Duration::from_millis(2250), 2000), None);
        assert!(clock.started.is_none());
    }
    #[test]
    fn only_local_attacker_critical_messages_emit_ui_and_sound() {
        let mut app = App::new();
        app.add_message::<CombatHit>()
            .insert_resource(Time::<Real>::default())
            .insert_resource(source())
            .insert_resource(CriticalSound(Handle::default()))
            .init_resource::<FeedbackClock>()
            .init_resource::<CriticalFeedbackStats>()
            .add_systems(Update, consume_hits);
        let hit = CombatHit {
            position: Vec3::ZERO,
            attack: crate::melee::Attack::CounterCombo(0),
            hit_index: 0,
            attribute: crate::combat_types::AttackAttributeId::CounterCritical,
            critical: true,
            local_attacker: true,
            lift: 0.,
            stun: 0.,
        };
        app.world_mut().write_message(CombatHit {
            critical: false,
            ..hit
        });
        app.world_mut().write_message(CombatHit {
            local_attacker: false,
            ..hit
        });
        app.world_mut().write_message(hit);
        app.world_mut().write_message(hit);
        app.update();
        let stats = app.world().resource::<CriticalFeedbackStats>();
        assert_eq!(
            (stats.accepted, stats.suppressed, stats.sounds_played),
            (1, 1, 1)
        );
        assert_eq!(
            app.world_mut()
                .query::<&AudioPlayer>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
