use bevy::{
    asset::{AssetId, LoadState, RenderAssetUsages, UntypedHandle},
    camera::{
        ClearColorConfig,
        visibility::{NoFrustumCulling, VisibilitySystems},
    },
    ecs::{schedule::ScheduleLabel, system::RunSystemOnce},
    mesh::Indices,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_phase::ViewSortedRenderPhases,
        render_resource::{
            CachedPipelineState, PipelineCache, PipelineDescriptor, SpecializedRenderPipelines,
        },
        texture::GpuImage,
        view::{
            screenshot::{Screenshot, save_to_disk},
            window::ExtractedWindows,
        },
    },
    shader::PipelineCacheError,
    ui_render::{TransparentUi, UiBatch, UiPipeline, UiPipelineKey},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PrepareWorld;
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PrepareMap;
#[derive(Component)]
struct LoadingRoot;
#[derive(Component)]
struct LoadingCamera;
#[derive(Component)]
struct LoadingBar;
#[derive(Component)]
struct LoadingWallpaper;
#[derive(Component)]
pub(crate) struct Warmup;
#[derive(Default)]
struct RenderStatus {
    frame: u64,
    window_size: UVec2,
    total: usize,
    pending: usize,
    errors: Vec<String>,
    images: HashSet<AssetId<Image>>,
    presented_images: HashSet<AssetId<Image>>,
}
#[derive(Resource, Clone, Default)]
struct RenderReadiness(Arc<Mutex<RenderStatus>>, Arc<AtomicU8>);

#[derive(Default)]
struct LoadingViewport {
    size: UVec2,
    last_frame: u64,
    stable_frames: u8,
    shown_frame: Option<u64>,
    painted_frame: Option<u64>,
}
impl LoadingViewport {
    fn observe(&mut self, frame: u64, size: UVec2, matches: bool) -> bool {
        if !matches || size != self.size {
            self.stable_frames = 0;
            self.shown_frame = None;
            self.painted_frame = None;
            self.size = size;
        }
        if frame != self.last_frame {
            self.last_frame = frame;
            if matches {
                self.stable_frames = self.stable_frames.saturating_add(1);
            }
        }
        matches && self.stable_frames >= 2
    }
}
#[derive(Resource, Default)]
pub(crate) struct Loading {
    complete: bool,
    built: bool,
    needs_screen: bool,
    pub(crate) pending_map: Option<crate::scene::MapId>,
    handles: Vec<UntypedHandle>,
    screen: Vec<Handle<Image>>,
    warm_meshes: Vec<Handle<Mesh>>,
    screen_frame: u64,
    observed_frame: u64,
    last_total: usize,
    quiet_frames: u32,
    started: f64,
    error: Option<String>,
    check_step: usize,
    progress: f32,
    viewport: LoadingViewport,
    launcher_ready: bool,
}
pub(crate) fn ready(loading: Option<Res<Loading>>) -> bool {
    loading.is_none_or(|loading| loading.complete)
}
impl Loading {
    pub(crate) fn request_map(&mut self, map: crate::scene::MapId) {
        self.complete = false;
        self.built = false;
        self.needs_screen = true;
        self.pending_map = Some(map);
        self.quiet_frames = 0;
        self.progress = 0.;
        self.error = None;
    }
}
pub(crate) struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        let status = RenderReadiness::default();
        app.insert_resource(status.clone())
            .init_resource::<Loading>()
            .init_resource::<crate::animation_core::storage::AnimationStorage>()
            .init_schedule(PrepareWorld)
            .init_schedule(PrepareMap)
            .add_systems(Startup, setup)
            .add_systems(Update, advance.before(crate::scene::switch_maps))
            .add_systems(PostUpdate, update_bar.before(bevy::ui::UiSystems::Layout))
            .add_systems(
                PostUpdate,
                present_when_sized
                    .after(bevy::ui::UiSystems::Layout)
                    .before(VisibilitySystems::VisibilityPropagate),
            );
        app.sub_app_mut(RenderApp)
            .insert_resource(status)
            .add_systems(
                Render,
                prewarm_loading_ui
                    .in_set(RenderSystems::Queue)
                    .before(bevy::ui_render::queue_uinodes),
            )
            .add_systems(Render, report_render.after(RenderSystems::Render));
    }
}
fn prewarm_loading_ui(
    pipeline: Res<UiPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<UiPipeline>>,
    cache: Res<PipelineCache>,
    mut queued: Local<bool>,
) {
    if !*queued {
        pipelines.specialize(
            &cache,
            &pipeline,
            UiPipelineKey {
                hdr: false,
                anti_alias: true,
            },
        );
        *queued = true;
    }
}

fn report_render(
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    status: Res<RenderReadiness>,
    windows: Res<ExtractedWindows>,
    ui_phases: Res<ViewSortedRenderPhases<TransparentUi>>,
    batches: Query<&UiBatch>,
) {
    let mode = status.1.load(Ordering::Relaxed);
    if mode == 0 {
        return;
    }
    let mut state = status.0.lock().unwrap();
    let total = cache.pipelines().filter(|pipeline| !matches!(&pipeline.descriptor,
        PipelineDescriptor::RenderPipelineDescriptor(descriptor) if descriptor.label.as_deref() == Some("screenshot-to-screen"))).count();
    state.window_size = windows
        .primary
        .and_then(|entity| windows.get(&entity))
        .map_or(UVec2::ZERO, |window| {
            UVec2::new(window.physical_width, window.physical_height)
        });
    state.frame += 1;
    state.total = total;
    state.pending = cache.waiting_pipelines().count();
    state.errors = cache
        .pipelines()
        .filter_map(|pipeline| match &pipeline.state {
            CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            ) => None,
            CachedPipelineState::Err(error) => Some(error.to_string()),
            _ => None,
        })
        .collect();
    if mode == 1 {
        state.presented_images = ui_phases
            .values()
            .flat_map(|phase| phase.items.iter())
            .filter(|item| {
                !item.batch_range.is_empty() && cache.get_render_pipeline(item.pipeline).is_some()
            })
            .filter_map(|item| batches.get(item.entity.0).ok().map(|batch| batch.image))
            .collect();
        state.images = images.iter().map(|(id, _)| id).collect();
    }
}
fn selected_wallpaper(map: Option<crate::scene::MapId>) -> Option<String> {
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read("assets/ui/loading/selection.json").ok()?).ok()?;
    if let Some(map) = map {
        return value["maps"][map.id()].as_str().map(str::to_owned);
    }
    let choices = value["wallpapers"].as_array()?;
    if choices.is_empty() {
        return None;
    }
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;
    choices[seed % choices.len()].as_str().map(str::to_owned)
}
fn setup(
    mut commands: Commands,
    mut time: ResMut<Time<Virtual>>,
    real: Res<Time<Real>>,
    assets: Res<AssetServer>,
    mut loading: ResMut<Loading>,
    render: Res<RenderReadiness>,
) {
    render.1.store(1, Ordering::Relaxed);
    time.pause();
    loading.started = real.elapsed_secs_f64();
    loading.screen_frame = render.0.lock().unwrap().frame;
    loading.screen.clear();
    loading.viewport = LoadingViewport::default();
    loading.needs_screen = false;
    let camera = commands
        .spawn((
            LoadingCamera,
            Camera2d,
            Camera {
                order: 1000,
                clear_color: if loading.pending_map.is_none() {
                    ClearColorConfig::Custom(Color::NONE)
                } else {
                    ClearColorConfig::None
                },
                ..default()
            },
        ))
        .id();
    let root = commands
        .spawn((
            LoadingRoot,
            Visibility::Hidden,
            UiTargetCamera(camera),
            GlobalZIndex(1000),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::BLACK),
        ))
        .id();
    if let Some(path) = selected_wallpaper(loading.pending_map) {
        info!("Loading wallpaper: {path}");
        let image = assets.load(path);
        loading.screen.push(image.clone());
        let art = commands
            .spawn((
                LoadingWallpaper,
                ImageNode::new(image).with_mode(bevy::ui::widget::NodeImageMode::Stretch),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
            ))
            .id();
        commands.entity(root).add_child(art);
    }
    let map = loading.pending_map;
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read("assets/ui/loading/selection.json").expect("Import loading UI"),
    )
    .expect("Loading selection");
    let spec = if let Some(map) = map {
        &manifest["bars"][map.id()]
    } else {
        &manifest["bars"]["title"]
    };
    let rect: [f32; 4] =
        serde_json::from_value(spec["rect"].clone()).expect("Missing source loading bar rectangle");
    let canvas: [f32; 2] =
        serde_json::from_value(spec["canvas"].clone()).expect("Missing source loading canvas");
    let track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(rect[0] / canvas[0] * 100.),
                top: percent(rect[1] / canvas[1] * 100.),
                width: percent(rect[2] / canvas[0] * 100.),
                height: percent(rect[3] / canvas[1] * 100.),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    commands.entity(root).add_child(track);
    let fill_path = spec["fill"]
        .as_str()
        .expect("Missing source loading bar material");
    let image = assets.load(fill_path.to_owned());
    loading.screen.push(image.clone());
    let bar = commands
        .spawn((
            LoadingBar,
            ImageNode::new(image),
            Node {
                width: percent(0.),
                height: percent(100.),
                ..default()
            },
        ))
        .id();
    commands.entity(track).add_child(bar);
}
fn present_when_sized(
    windows: Query<&Window>,
    cameras: Query<&Camera, With<LoadingCamera>>,
    mut roots: Query<(&ComputedNode, &mut Visibility), With<LoadingRoot>>,
    wallpapers: Query<&ComputedNode, With<LoadingWallpaper>>,
    render: Res<RenderReadiness>,
    mut loading: ResMut<Loading>,
) {
    if loading.complete {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    if !loading.launcher_ready {
        loading.launcher_ready = match std::env::var_os("ESPER_WINDOW_READY_FILE") {
            None => true,
            Some(path) => std::fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|value| {
                    value["logical_size"].as_array().is_none_or(|size| {
                        size.len() == 2
                            && (window.width() - size[0].as_f64().unwrap_or(0.) as f32).abs() < 1.
                            && (window.height() - size[1].as_f64().unwrap_or(0.) as f32).abs() < 1.
                    })
                }),
        };
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    let Ok((root, mut visibility)) = roots.single_mut() else {
        return;
    };
    let Ok(wallpaper) = wallpapers.single() else {
        return;
    };
    let size = UVec2::new(window.physical_width(), window.physical_height());
    let status = render.0.lock().unwrap();
    let matches = loading.launcher_ready
        && size.min_element() > 0
        && status.window_size == size
        && camera.physical_viewport_size() == Some(size)
        && (root.size() - size.as_vec2()).abs().max_element() <= 1.
        && (wallpaper.size() - size.as_vec2()).abs().max_element() <= 1.
        && loading
            .screen
            .iter()
            .all(|image| status.images.contains(&image.id()));
    if loading.viewport.observe(status.frame, size, matches) {
        if loading.viewport.shown_frame.is_none() {
            loading.viewport.shown_frame = Some(status.frame);
            info!(
                "Loading wallpaper ready at {}x{} after resize/layout synchronization",
                size.x, size.y
            );
        }
        *visibility = Visibility::Visible;
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn update_bar(loading: Res<Loading>, mut bars: Query<&mut Node, With<LoadingBar>>) {
    for mut node in &mut bars {
        node.width = percent(loading.progress * 100.);
    }
}
fn set_progress(world: &mut World, progress: f32) {
    let mut loading = world.resource_mut::<Loading>();
    loading.progress = loading.progress.max(progress.clamp(0., 1.));
}

fn collect_paths(value: &serde_json::Value, paths: &mut HashSet<String>) {
    match value {
        serde_json::Value::String(path) => {
            let supported = ["png", "dds", "ogg", "ttf"].contains(
                &std::path::Path::new(path)
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or(""),
            );
            if supported && std::path::Path::new("assets").join(path).is_file() {
                paths.insert(path.clone());
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_paths(value, paths);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect_paths(value, paths);
            }
        }
        _ => {}
    }
}
fn preload(world: &mut World) {
    let mut paths = HashSet::new();
    let map = world.resource::<crate::scene::MapSelection>().active;
    for file in [
        map.asset_path(),
        "assets/player.json",
        "assets/movement-effects.json",
        "assets/movement-audio.json",
        "assets/melee/melee.json",
        "assets/melee/reactions.json",
        "assets/practice/practice.json",
        "assets/ui/hud/hud-source.json",
        "assets/ui/weapon-switch.json",
        "assets/ui/minimap/minimap.json",
    ] {
        if let Ok(bytes) = std::fs::read(file) {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                collect_paths(&value, &mut paths);
            }
        }
    }
    for entry in std::fs::read_dir("assets/ui/hud").unwrap().flatten() {
        if entry.path().extension().is_some_and(|ext| ext == "png") {
            paths.insert(format!("ui/hud/{}", entry.file_name().to_string_lossy()));
        }
    }
    let music: serde_json::Value =
        serde_json::from_str(&crate::music::catalog_json()).unwrap();
    for entry in music["maps"].as_array().unwrap() {
        if entry["id"].as_str() == Some(map.id()) {
            collect_paths(&entry["tracks"], &mut paths);
        }
    }
    paths.insert("ui/verdana.ttf".into());
    let server = world.resource::<AssetServer>();
    let mut handles = Vec::new();
    for path in paths {
        let existing = server.get_handles_untyped(path.clone());
        if !existing.is_empty() {
            handles.extend(existing);
            continue;
        }
        handles.push(
            match std::path::Path::new(&path)
                .extension()
                .and_then(|x| x.to_str())
            {
                Some("ogg") => server.load::<AudioSource>(path).untyped(),
                Some("ttf") => server.load::<Font>(path).untyped(),
                _ => server.load::<Image>(path).untyped(),
            },
        );
    }
    world.resource_mut::<Loading>().handles = handles;
}
fn warm_mesh(source: &Mesh) -> Mesh {
    let mut mesh = Mesh::new(
        source.primitive_topology(),
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    for (attribute, _) in source.attributes() {
        if attribute.id == Mesh::ATTRIBUTE_POSITION.id {
            mesh.insert_attribute(
                *attribute,
                vec![[-0.01, -0.01, 0.], [0.01, -0.01, 0.], [0., 0.01, 0.]],
            );
        } else if attribute.id == Mesh::ATTRIBUTE_NORMAL.id {
            mesh.insert_attribute(*attribute, vec![[0., 0., 1.]; 3]);
        } else if attribute.id == Mesh::ATTRIBUTE_UV_0.id || attribute.id == Mesh::ATTRIBUTE_UV_1.id
        {
            mesh.insert_attribute(*attribute, vec![[0., 0.]; 3]);
        } else if attribute.id == Mesh::ATTRIBUTE_COLOR.id {
            mesh.insert_attribute(*attribute, vec![[1.; 4]; 3]);
        } else {
            panic!("Uncovered warmup mesh attribute: {}", attribute.name);
        }
    }
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}
fn warm_material<M: Material>(world: &mut World) {
    let pairs: Vec<_> = world
        .query::<(&Mesh3d, &MeshMaterial3d<M>)>()
        .iter(world)
        .map(|(mesh, material)| (mesh.0.clone(), material.0.clone()))
        .collect();
    let camera = *world
        .query_filtered::<&Transform, With<crate::gameplay::View>>()
        .single(world)
        .unwrap();
    let mut unique = HashSet::new();
    let mut layouts = HashMap::new();
    for (source, material) in pairs {
        let Some(mesh) = world.resource::<Assets<Mesh>>().get(&source) else {
            continue;
        };
        let Ok(attributes) = mesh.try_attributes() else {
            continue;
        };
        let key: Vec<_> = attributes.map(|(attribute, _)| attribute.id).collect();
        if !unique.insert((key.clone(), material.id())) {
            continue;
        }
        let handle = if let Some(handle) = layouts.get(&key) {
            Handle::<Mesh>::clone(handle)
        } else {
            let mesh = warm_mesh(mesh);
            let handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
            layouts.insert(key, handle.clone());
            world
                .resource_mut::<Loading>()
                .warm_meshes
                .push(handle.clone());
            handle
        };
        world.spawn((
            Warmup,
            Mesh3d(handle),
            MeshMaterial3d(material),
            NoFrustumCulling,
            Transform {
                translation: camera.translation + camera.forward() * 0.2,
                rotation: camera.rotation,
                ..default()
            },
        ));
    }
}
fn advance(world: &mut World) {
    if world.resource::<Loading>().complete {
        if world
            .resource::<RenderReadiness>()
            .1
            .load(Ordering::Relaxed)
            == 0
        {
            return;
        }
        let total = world.resource::<RenderReadiness>().0.lock().unwrap().total;
        let mut loading = world.resource_mut::<Loading>();
        if total > loading.last_total {
            warn!(
                "Gameplay requested {} additional pipelines after warmup ({} -> {})",
                total - loading.last_total,
                loading.last_total,
                total
            );
            loading.last_total = total;
        }
        return;
    }
    if world.resource::<Loading>().needs_screen {
        world.run_system_once(setup).expect("Loading screen setup");
        return;
    }
    let (frame, total, pending, errors, uploaded) = {
        let status = world.resource::<RenderReadiness>().0.lock().unwrap();
        (
            status.frame,
            status.total,
            status.pending,
            status.errors.clone(),
            status.images.clone(),
        )
    };
    if !world.resource::<Loading>().built {
        let now = world.resource::<Time<Real>>().elapsed_secs_f64();
        let server = world.resource::<AssetServer>();
        let loading = world.resource::<Loading>();
        let failure = errors
            .first()
            .cloned()
            .or_else(|| {
                loading.screen.iter().find_map(|image| {
                    if let Some(LoadState::Failed(error)) = server.get_load_state(image.id()) {
                        Some(error.to_string())
                    } else {
                        None
                    }
                })
            })
            .or_else(|| {
                (now - loading.started > 120.).then(|| "Loading screen assets timed out".into())
            });
        if let Some(error) = failure {
            if loading.error.is_none() {
                error!("{error}");
                world.resource_mut::<Loading>().error = Some(error);
            }
            return;
        }
        if world
            .resource::<Loading>()
            .viewport
            .shown_frame
            .is_none_or(|shown| frame <= shown)
        {
            return;
        }
        let screen_ready = loading
            .screen
            .iter()
            .all(|image| uploaded.contains(&image.id()));
        let wallpaper_presented = loading.screen.first().is_some_and(|wallpaper| {
            world
                .resource::<RenderReadiness>()
                .0
                .lock()
                .unwrap()
                .presented_images
                .contains(&wallpaper.id())
        });
        if frame < loading.screen_frame + 3 || pending > 0 || !screen_ready || !wallpaper_presented
        {
            return;
        }
        let first_paint = world.resource::<Loading>().viewport.painted_frame;
        if first_paint.is_none() {
            world.resource_mut::<Loading>().viewport.painted_frame = Some(frame);
            return;
        }
        if first_paint.is_some_and(|painted| frame <= painted) {
            return;
        }
        info!(
            "Loading wallpaper presented; starting asset preparation at {:.3} seconds",
            now - loading.started
        );
        if world.resource::<crate::Options>().loading_check {
            let label = loading.pending_map.map_or("title", |map| map.id());
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("logs/loading-{label}.png")));
        }
        if world.resource::<Loading>().pending_map.is_some() {
            world.run_schedule(PrepareMap);
        } else {
            world.run_schedule(PrepareWorld);
        }
        let camera = world
            .query_filtered::<Entity, With<crate::gameplay::View>>()
            .single(world)
            .unwrap();
        world.entity_mut(camera).insert(IsDefaultUiCamera);
        set_progress(world, 0.1);
        preload(world);
        crate::minimap::warmup(world);
        warm_material::<StandardMaterial>(world);
        warm_material::<crate::materials::DualTexture>(world);
        warm_material::<crate::materials::TrailHaze>(world);
        let mut loading = world.resource_mut::<Loading>();
        loading.built = true;
        loading.observed_frame = frame;
        info!(
            "Loading: prepared {} assets and {} warmup mesh layouts",
            loading.handles.len(),
            loading.warm_meshes.len()
        );
        return;
    }
    let mut loaded = 0;
    let mut gpu_ready = true;
    let mut failure = errors
        .first()
        .map(|error| format!("Shader compilation failed: {error}"));
    {
        let loading = world.resource::<Loading>();
        let server = world.resource::<AssetServer>();
        for handle in &loading.handles {
            match server.get_load_state(handle.id()) {
                Some(LoadState::Failed(error)) => {
                    failure = Some(format!(
                        "Cannot load {}: {error}",
                        server
                            .get_path(handle.id())
                            .map_or_else(|| format!("{:?}", handle.id()), |path| path.to_string())
                    ))
                }
                Some(LoadState::Loaded) if server.is_loaded_with_dependencies(handle.id()) => {
                    loaded += 1
                }
                _ => {}
            }
            if handle.type_id() == std::any::TypeId::of::<Image>()
                && !uploaded.contains(&handle.id().typed::<Image>())
            {
                gpu_ready = false;
            }
        }
    }
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let mut loading = world.resource_mut::<Loading>();
    if now - loading.started > 120. {
        failure.get_or_insert_with(|| {
            "Loading timed out. See logs for the unfinished asset or shader.".into()
        });
    }
    if let Some(failure) = failure {
        if loading.error.is_none() {
            error!("{failure}");
            loading.error = Some(failure);
        }
    }
    if loading.error.is_some() {
        return;
    }
    let count = loading.handles.len();
    let settled = loaded == count && gpu_ready && pending == 0 && total > 0;
    if frame != loading.observed_frame {
        loading.quiet_frames = if settled && total == loading.last_total {
            loading.quiet_frames + 1
        } else {
            0
        };
        loading.observed_frame = frame;
        loading.last_total = total;
    }
    let finished = loading.quiet_frames >= 9;
    let progress = if loaded < count {
        10 + 60 * loaded / count.max(1)
    } else {
        75 + 25 * loading.quiet_frames.min(8) as usize / 8
    };
    drop(loading);
    set_progress(world, progress as f32 / 100.);
    if world.resource::<crate::Options>().loading_check
        && world.resource::<Loading>().quiet_frames == 2
    {
        let label = world
            .resource::<Loading>()
            .pending_map
            .map_or("title", |map| map.id());
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("logs/loading-{label}-progress.png")));
    }
    if finished {
        let loading = world.resource::<Loading>();
        let report = serde_json::json!({"screen":loading.pending_map.map_or("title", |map| map.id()),
            "seconds":now-loading.started, "assets":count, "pipelines":total, "pending_pipelines":pending,
            "stable_render_frames":loading.quiet_frames, "warm_mesh_layouts":loading.warm_meshes.len()});
        info!("Loading complete: {report}");
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("logs/loading-report.jsonl")
        {
            let _ = writeln!(file, "{report}");
        }
        let entities: Vec<_> = world
            .query_filtered::<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>, With<Warmup>)>>()
            .iter(world)
            .collect();
        for entity in entities {
            world.despawn(entity);
        }
        let mut loading = world.resource_mut::<Loading>();
        loading.warm_meshes.clear();
        loading.screen.clear();
        loading.pending_map = None;
        loading.complete = true;
        world.resource_mut::<Time<Virtual>>().unpause();
        let options = world.resource::<crate::Options>();
        let monitor = options.loading_check
            || options.melee_smoke
            || options.counter_smoke
            || options.movement_smoke;
        world
            .resource::<RenderReadiness>()
            .1
            .store(if monitor { 2 } else { 0 }, Ordering::Relaxed);
        world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
        world.resource_mut::<ButtonInput<MouseButton>>().reset_all();
    }
}

fn check_map_switches(
    options: Res<crate::Options>,
    time: Res<Time>,
    mut loading: ResMut<Loading>,
    mut requests: MessageWriter<crate::chat::MapCommand>,
) {
    if !options.loading_check || !loading.complete {
        return;
    }
    let maps = ["station-2", "neden-1", "circle-2"];
    if loading.check_step < maps.len()
        && time.elapsed_secs() >= 2. * (loading.check_step + 1) as f32
    {
        requests.write(crate::chat::MapCommand {
            map: maps[loading.check_step].into(),
        });
        loading.check_step += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_effect_geometry_warms_the_same_shader_vertex_layout() {
        let mut effect = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        effect.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
        effect.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
        effect.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
        effect.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
        effect.insert_indices(Indices::U32(Vec::new()));
        let warm = warm_mesh(&effect);
        assert_eq!(warm.count_vertices(), 3);
        assert_eq!(warm.indices().unwrap().len(), 3);
        let layout = |mesh: &Mesh| {
            mesh.attributes()
                .map(|(attribute, _)| (attribute.id, attribute.format))
                .collect::<Vec<_>>()
        };
        assert_eq!(layout(&effect), layout(&warm));
        assert_eq!(
            effect.count_vertices(),
            0,
            "warmup must not mutate live effect buffers"
        );
    }
}

pub(crate) fn install_verification(app: &mut App) {
    app.add_systems(
        Update,
        check_map_switches
            .after(advance)
            .before(crate::scene::switch_maps),
    );
}
