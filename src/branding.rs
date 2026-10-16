use bevy::{ecs::system::NonSendMarker, prelude::*, window::PrimaryWindow, winit::WINIT_WINDOWS};

pub(crate) fn set_window_icon(
    _main_thread: NonSendMarker,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut applied: Local<bool>,
) {
    if *applied {
        return;
    }
    let Ok(entity) = primary.single() else { return };
    WINIT_WINDOWS.with_borrow(|windows| {
        if let Some(window) = windows.get_window(entity) {
            let icon = winit::window::Icon::from_rgba(
                include_bytes!("../branding/esperust.rgba").to_vec(), 256, 256,
            ).expect("Invalid bundled window icon");
            window.set_window_icon(Some(icon));
            *applied = true;
        }
    });
}
