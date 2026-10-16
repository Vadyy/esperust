use bevy::{app::AppExit, prelude::*};
use std::collections::BTreeMap;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const WARMUP: Duration = Duration::from_secs(3);

#[derive(Resource, Default)]
pub(crate) struct SystemTimings {
    enabled: AtomicBool,
    samples: Mutex<BTreeMap<&'static str, Vec<f64>>>,
}
impl SystemTimings {
    pub(crate) fn span(&self, name: &'static str) -> Option<Span<'_>> {
        self.enabled.load(Ordering::Relaxed).then(|| Span {
            owner: self,
            name,
            start: Instant::now(),
        })
    }
}
pub(crate) struct Span<'a> {
    owner: &'a SystemTimings,
    name: &'static str,
    start: Instant,
}
impl Drop for Span<'_> {
    fn drop(&mut self) {
        let micros = self.start.elapsed().as_secs_f64() * 1e6;
        self.owner
            .samples
            .lock()
            .unwrap()
            .entry(self.name)
            .or_default()
            .push(micros);
    }
}

#[derive(Resource)]
struct Measurements {
    started: Instant,
    ready: Option<Instant>,
    frame: Option<Instant>,
    previous_frame: Option<Instant>,
    interval_us: Vec<f64>,
    render: BTreeMap<String, (bevy::platform::time::Instant, Vec<f64>)>,
    fixed: Option<Instant>,
    frame_us: Vec<f64>,
    fixed_us: Vec<f64>,
    saved: bool,
}
impl Default for Measurements {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            ready: None,
            frame: None,
            previous_frame: None,
            interval_us: Vec::new(),
            render: BTreeMap::new(),
            fixed: None,
            frame_us: Vec::new(),
            fixed_us: Vec::new(),
            saved: false,
        }
    }
}
impl Measurements {
    fn warmed(&self) -> bool {
        self.ready.is_some_and(|ready| ready.elapsed() >= WARMUP)
    }
}

pub(crate) fn install(app: &mut App) {
    use crate::simulation::SimulationSet;
    if app.world().resource::<crate::Options>().profile_render {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
    app.init_resource::<Measurements>()
        .init_resource::<SystemTimings>()
        .add_systems(First, begin_frame.run_if(crate::loading::ready))
        .add_systems(
            FixedUpdate,
            begin_fixed
                .before(SimulationSet::Prepare)
                .run_if(crate::loading::ready),
        )
        .add_systems(
            FixedUpdate,
            end_fixed
                .after(SimulationSet::CapturePresentation)
                .run_if(crate::loading::ready),
        )
        .add_systems(Last, end_frame);
}

fn begin_frame(mut samples: ResMut<Measurements>, systems: Res<SystemTimings>) {
    samples.ready.get_or_insert_with(Instant::now);
    let warmed = samples.warmed();
    systems.enabled.store(warmed, Ordering::Relaxed);
    let now = Instant::now();
    if warmed {
        if let Some(previous) = samples.previous_frame {
            samples
                .interval_us
                .push(now.duration_since(previous).as_secs_f64() * 1e6);
        }
    }
    samples.previous_frame = warmed.then_some(now);
    samples.frame = warmed.then_some(now);
}
fn begin_fixed(mut samples: ResMut<Measurements>) {
    samples.fixed = samples.warmed().then(Instant::now);
}
fn end_fixed(mut samples: ResMut<Measurements>) {
    if let Some(start) = samples.fixed.take() {
        samples.fixed_us.push(start.elapsed().as_secs_f64() * 1e6);
    }
}

fn summary(values: &[f64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::json!({"samples":0,"median_us":null,"p95_us":null,"p99_us":null});
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((p * sorted.len() as f64).ceil() as usize).saturating_sub(1)];
    serde_json::json!({"samples":sorted.len(), "median_us":percentile(0.5), "p95_us":percentile(0.95), "p99_us":percentile(0.99)})
}

fn end_frame(
    mut samples: ResMut<Measurements>,
    systems: Res<SystemTimings>,
    mut exits: MessageReader<AppExit>,
    options: Res<crate::Options>,
    windows: Query<&Window>,
    player: Option<Res<crate::gameplay::Player>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
) {
    if let Some(start) = samples.frame.take() {
        samples.frame_us.push(start.elapsed().as_secs_f64() * 1e6);
    }
    if options.profile_render {
        let warmed = samples.warmed();
        for diagnostic in diagnostics
            .iter()
            .filter(|d| d.path().as_str().starts_with("render/") && d.suffix == "ms")
        {
            if let Some(measurement) = diagnostic.measurement() {
                let entry = samples
                    .render
                    .entry(diagnostic.path().to_string())
                    .or_insert_with(|| (measurement.time, Vec::new()));
                if entry.0 != measurement.time {
                    entry.0 = measurement.time;
                    if warmed {
                        entry.1.push(measurement.value * 1000.);
                    }
                }
            }
        }
    }
    if exits.read().next().is_none() || samples.saved {
        return;
    }
    let window = windows.single().ok();
    let report = serde_json::json!({
        "schema":2,
        "scope":"First/Last main-world marker spans; excludes render app, GPU execution, presentation and input-to-photon latency",
        "scenario": if options.smoke { "fixed smoke movement v2 (complete commands)" } else { "user selected scenario" },
        "physics_hz":options.physics_hz, "map":options.map.id(),
        "requested_resolution":options.resolution,
        "physical_resolution":window.map(|w| [w.physical_width(),w.physical_height()]),
        "warmup_seconds_after_ready":WARMUP.as_secs_f64(),
        "startup_to_first_ready_frame_seconds":samples.ready.map(|ready| ready.duration_since(samples.started).as_secs_f64()),
        "build_debug_assertions":cfg!(debug_assertions),
        "adapter":adapter.as_ref().map(|a| format!("{:?}", &***a)),
        "render_diagnostics_enabled":options.profile_render,
        "render_pass_spans":samples.render.iter().map(|(name, (_, values))| (name, summary(values))).collect::<BTreeMap<_, _>>(),
        "render_note":"Opt-in render pass CPU/GPU diagnostics, deduplicated by delivery timestamp. Delayed/subsampled delivery, nested spans overlap; not end-to-end latency. Missing GPU entries mean unavailable.",
        "fixed_simulation_phases":summary(&samples.fixed_us),
        "main_world_frame":summary(&samples.frame_us),
        "frame_interval":summary(&samples.interval_us),
        "frame_interval_scope":"First-to-First wall time, including between-update render/presentation waits; not GPU time or display presentation timestamps",
        "throughput_fps": if samples.interval_us.is_empty() { None } else { Some(1e6 * samples.interval_us.len() as f64 / samples.interval_us.iter().sum::<f64>()) },
        "system_cpu_spans":systems.samples.lock().unwrap().iter().map(|(name, values)| (*name, summary(values))).collect::<BTreeMap<_, _>>(),
        "system_span_note":"Inclusive wall-clock system bodies; nested spans overlap, scheduling/lock waits before entry excluded. Profiling adds timer/recording overhead.",
        "frame_interval_raw_us":samples.interval_us,
        "fixed_raw_us":samples.fixed_us, "frame_raw_us":samples.frame_us,
        "allocations":null,
        "allocation_note":"No allocation instrumentation installed; shared-buffer accounting is recorded separately in C3",
        "final_actor":player.map(|p| serde_json::json!({"position":p.feet.to_array(),"jumps":p.jumps,"dodges":p.dodges,"distance_m":p.distance})),
        "percentiles":"nearest rank",
    });
    std::fs::write(
        "logs/performance.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .expect("Write opt-in performance report");
    samples.saved = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentiles_preserve_outliers_and_empty_samples_are_unavailable() {
        assert_eq!(summary(&[])["median_us"], serde_json::Value::Null);
        let mut values: Vec<_> = (1..=100).map(|v| v as f64).collect();
        values.reverse();
        let result = summary(&values);
        assert_eq!(result["samples"], 100);
        assert_eq!(result["median_us"], 50.);
        assert_eq!(result["p95_us"], 95.);
        assert_eq!(result["p99_us"], 99.);
        assert_eq!(summary(&[17.])["p99_us"], 17.);
    }
    #[test]
    fn profiling_is_absent_from_ordinary_startup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::Options::default());
        crate::verification::install(&mut app);
        assert!(!app.world().contains_resource::<Measurements>());
        assert!(!app.world().contains_resource::<SystemTimings>());
        app.update();
    }
    #[test]
    fn system_spans_only_record_after_warmup_is_enabled() {
        let timings = SystemTimings::default();
        assert!(timings.span("fixture").is_none());
        assert!(timings.samples.lock().unwrap().is_empty());
        timings.enabled.store(true, Ordering::Relaxed);
        drop(timings.span("fixture"));
        assert_eq!(timings.samples.lock().unwrap()["fixture"].len(), 1);
    }
}
