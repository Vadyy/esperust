use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SimulationSet {
    Prepare,
    Move,
    CaptureAttacks,
    ResolveCombat,
    CapturePresentation,
}

pub(crate) struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                SimulationSet::Prepare,
                SimulationSet::Move,
                SimulationSet::CaptureAttacks,
                SimulationSet::ResolveCombat,
                SimulationSet::CapturePresentation,
            )
                .chain(),
        );
    }
}
