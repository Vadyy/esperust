use super::*;

#[derive(Resource)]
pub(super) struct WeaponHud {
    pub(super) observed: Option<Weapon>,
    pub(super) age: Option<f32>,
    pub(super) reset: u32,
}
#[derive(Component)]
pub(super) struct WeaponFrame {
    pub(super) slot: u8,
    pub(super) on: Handle<Image>,
    pub(super) off: Handle<Image>,
}
#[derive(Component)]
pub(super) struct WeaponSlotIcon;
#[derive(Component)]
pub(super) struct WeaponHudRoot;
#[derive(Deserialize)]
pub(super) struct SwitchLayout {
    pub(super) slots: Vec<SwitchSlot>,
}
#[derive(Deserialize)]
pub(super) struct SwitchSlot {
    pub(super) number: u8,
    pub(super) rect: [f32; 4],
    pub(super) icon_rect: [f32; 4],
    pub(super) on: String,
    pub(super) off: String,
}

pub(super) fn setup_weapon_ui(commands: &mut Commands, assets: &AssetServer, manifest: &Manifest) {
    use crate::hud::{Anchor, SourceRect, source_node};
    let layout: SwitchLayout = serde_json::from_slice(
        &std::fs::read("assets/ui/weapon-switch.json").expect("Export original weapon slots"),
    )
    .unwrap();
    let root = commands
        .spawn((
            Name::new("Original three weapon slots"),
            WeaponHudRoot,
            crate::hud::DesignCanvas,
            UiTransform::default(),
            source_node([0., 0., 1024., 768.]),
            Visibility::Hidden,
        ))
        .id();
    for slot in layout.slots {
        let off = assets.load(slot.off);
        let frame = commands
            .spawn((
                Name::new(format!("Original weapon slot {}", slot.number)),
                WeaponFrame {
                    slot: slot.number,
                    on: assets.load(slot.on),
                    off: off.clone(),
                },
                ImageNode::new(off),
                source_node(slot.rect),
                SourceRect {
                    rect: slot.rect,
                    anchor: Anchor::BottomCenter,
                },
            ))
            .id();
        commands.entity(root).add_child(frame);
        if let Some(key) = match slot.number {
            1 => Some("plasma"),
            2 => Some("dagger"),
            3 => Some("counter"),
            _ => None,
        } {
            let icon = commands
                .spawn((
                    WeaponSlotIcon,
                    ImageNode::new(assets.load(manifest.ui[key].path.clone())),
                    ZIndex(1),
                    source_node(slot.icon_rect),
                    SourceRect {
                        rect: slot.icon_rect,
                        anchor: Anchor::BottomCenter,
                    },
                ))
                .id();
            commands.entity(root).add_child(icon);
        }
    }
    commands.insert_resource(WeaponHud {
        observed: None,
        age: None,
        reset: 0,
    });
}

pub(super) fn switch_opacity(age: f32) -> f32 {
    if age < 0.2 {
        (age / 0.2).max(0.)
    } else if age < 1.0 {
        1.
    } else {
        (1. - (age - 1.) / 0.2).clamp(0., 1.)
    }
}

pub(super) fn update_weapon_ui(
    hud: Option<ResMut<WeaponHud>>,
    player: Res<Player>,
    options: Res<crate::Options>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<crate::chat::ChatState>,
    emotes: Res<crate::emotes::EmoteMenu>,
    mut roots: Query<&mut Visibility, With<WeaponHudRoot>>,
    mut frames: Query<(&WeaponFrame, &mut ImageNode), Without<WeaponSlotIcon>>,
    mut icons: Query<&mut ImageNode, (With<WeaponSlotIcon>, Without<WeaponFrame>)>,
) {
    let Some(mut hud) = hud else {
        return;
    };
    if hud.reset != player.pose_reset_serial {
        hud.reset = player.pose_reset_serial;
        hud.age = None;
    }
    let changed = hud
        .observed
        .is_some_and(|weapon| weapon != player.melee.equipped);
    hud.observed = Some(player.melee.equipped);
    let pressed = !chat.blocks_gameplay && !emotes.consumed_numbers
        && [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
            .iter()
            .any(|key| keys.just_pressed(*key));
    if changed || pressed {
        hud.age = Some(
            hud.age
                .filter(|age| *age < 0.2)
                .unwrap_or(if hud.age.is_some() { 0.2 } else { 0. }),
        );
    }
    if let Some(age) = hud.age.as_mut() {
        *age += time.delta_secs();
    }
    let alpha = hud.age.map_or(0., switch_opacity);
    if hud.age.is_some_and(|age| age >= 1.2) {
        hud.age = None;
    }
    for mut visible in &mut roots {
        *visible = if alpha > 0.
            && player.health > 0.
            && player.melee.equipped != Weapon::Unarmed
            && !options.overview
        {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    let selected = match player.melee.equipped {
        Weapon::PlasmaSword => 1,
        Weapon::Dagger => 2,
        Weapon::CounterSword => 3,
        Weapon::Unarmed => 0,
    };
    for (frame, mut image) in &mut frames {
        image.image = if frame.slot == selected {
            frame.on.clone()
        } else {
            frame.off.clone()
        };
        image.color.set_alpha(alpha);
    }
    for mut image in &mut icons {
        image.color.set_alpha(alpha);
    }
}
