use bevy::prelude::*;

pub(crate) fn styled_text(
    commands: &mut Commands,
    parent: Entity,
    text: &str,
    font: TextFont,
    layout: TextLayout,
    color: Color,
    style: u32,
    edge: Color,
) -> Vec<Entity> {
    let offsets: &[Vec2] = match style {
        1 => &[Vec2::ONE],
        2 => &[
            Vec2::new(-1., -1.), Vec2::new(-1., 0.), Vec2::new(-1., 1.),
            Vec2::new(0., -1.), Vec2::new(0., 1.),
            Vec2::new(1., -1.), Vec2::new(1., 0.), Vec2::new(1., 1.),
        ],
        _ => &[],
    };
    let mut entities = Vec::new();
    for (offset, pass_color) in offsets.iter().copied().map(|offset| (offset, edge))
        .chain(std::iter::once((Vec2::ZERO, color)))
    {
        let entity = commands.spawn((
            Text::new(text), font.clone(), layout.clone(), TextColor(pass_color),
            Node {
                grid_row: GridPlacement::start(1),
                grid_column: GridPlacement::start(1),
                ..default()
            },
            UiTransform::from_translation(bevy::ui::Val2::px(offset.x, offset.y)),
        )).id();
        commands.entity(parent).add_child(entity);
        entities.push(entity);
    }
    entities
}
