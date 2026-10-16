use super::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
pub(super) fn install(app: &mut App) {
    app.add_systems(
        Update,
        smoke_capture.after(update).run_if(crate::loading::ready),
    );
}
fn smoke_capture(
    mut commands: Commands,
    options: Res<crate::Options>,
    time: Res<Time>,
    stats: Res<EffectStats>,
    mut captured: Local<[bool; 5]>,
) {
    if !options.movement_smoke {
        return;
    }
    for (index, (at, name)) in [
        (4.72, "jump"),
        (7.15, "dodge"),
        (8.40, "sprint"),
        (10.30, "wall-jump"),
    ]
    .iter()
    .enumerate()
    {
        if !captured[index] && time.elapsed_secs() >= *at {
            captured[index] = true;
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("logs/effect-{name}-smoke.png")));
        }
    }
    if !captured[4] && time.elapsed_secs() >= 20.5 {
        assert!(
            stats.jump_sequences > 0
                && stats.dodge_sequences > 0
                && stats.sprint_sequences > 0
                && stats.wall_jump_sequences > 0
        );
        assert!(stats.particles_emitted > 50 && stats.peak_particles > 10);
        let _ = std::fs::write(
            "logs/effects-smoke.json",
            serde_json::to_string_pretty(&*stats).unwrap(),
        );
        captured[4] = true;
        info!(
            "Movement effect smoke: {} particles emitted, peak {}",
            stats.particles_emitted, stats.peak_particles
        );
    }
}
