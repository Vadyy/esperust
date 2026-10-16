use super::{Controls, Player};
use crate::scene::Level;
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

#[derive(Component)]
pub struct Status;
#[derive(Component)]
pub struct FpsCounter;
#[derive(Component)]
pub struct Help;
#[derive(Component)]
pub struct MapTitle;

pub fn hud(
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time>,
    mut next_fps_refresh: Local<f64>,
    fixed_time: Res<Time<Fixed>>,
    controls: Res<Controls>,
    player: Res<Player>,
    level: Res<Level>,
    selection: Res<crate::scene::MapSelection>,
    mut fps_text: Query<
        &mut Text,
        (
            With<FpsCounter>,
            Without<Status>,
            Without<Help>,
            Without<MapTitle>,
        ),
    >,
    mut status: Single<(&mut Text, &mut Visibility), With<Status>>,
    mut help: Single<(&mut Text, &mut Visibility), (With<Help>, Without<Status>)>,
    mut title: Single<
        (&mut Text, &mut Visibility),
        (With<MapTitle>, Without<Status>, Without<Help>),
    >,
    profile: Option<Res<crate::performance::SystemTimings>>,
) {
    let _span = profile.as_deref().and_then(|p| p.span("diagnostics"));
    if time.elapsed_secs_f64() >= *next_fps_refresh {
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.);
        let value = format!("{fps:.0}");
        for mut text in &mut fps_text {
            text.0.clone_from(&value);
        }
        *next_fps_refresh = time.elapsed_secs_f64() + 0.5;
    }
    let visibility = if controls.diagnostics {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    status.1.set_if_neq(visibility);
    title.1.set_if_neq(visibility);
    help.1.set_if_neq(if controls.diagnostics {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !controls.diagnostics {
        return;
    }
    if title.0.0 != selection.active.label() {
        title.0.0 = selection.active.label().into();
    }
    let value = format!(
        "{:.1} Hz | SP {:.0} | {:?} | {:?} {:?} | {} | {:5.1}, {:5.1}, {:5.1} | {} triangles",
        1.0 / fixed_time.timestep().as_secs_f64(),
        player.sp,
        player.action,
        player.melee.equipped,
        player.melee.attack,
        if player.grounded {
            "grounded"
        } else {
            "airborne"
        },
        player.feet.x,
        player.feet.y,
        player.feet.z,
        level.triangle_count
    );
    if status.0.0 != value {
        status.0.0 = value;
    }
    let value = if controls.captured {
        "E emotes · 1–9 select · /hug in chat · F3 diagnostics · F12 screenshot"
    } else {
        "Click to capture mouse · F3 diagnostics · F12 screenshot"
    };
    if help.0.0 != value {
        help.0.0 = value.into();
    }
}
