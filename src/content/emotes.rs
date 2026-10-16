use super::{ContentError, read_json_at};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

pub(crate) const MENU_COUNT: usize = 9;
pub(crate) const HUG: usize = 9;
pub(crate) const SOURCE_IDS: [u32; 10] = [3, 0, 1, 19, 23, 5, 12, 13, 14, 18];

#[derive(Debug, Deserialize)]
pub(crate) struct MenuStyle {
    pub text: String,
    pub rect: [f32; 4],
    pub font_size: f32,
    pub line_height: f32,
    pub color: [u8; 4],
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sound {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Definition {
    pub source_id: u32,
    pub label: String,
    pub action: String,
    pub duration: f32,
    pub use_weapon: bool,
    pub sounds: HashMap<String, Sound>,
}

#[derive(Debug, Deserialize, bevy::prelude::Resource)]
pub(crate) struct EmoteCatalog {
    pub entries: Vec<Definition>,
    pub menu: MenuStyle,
}

pub(crate) fn load(root: &Path) -> Result<EmoteCatalog, ContentError> {
    let catalog: EmoteCatalog = read_json_at(root, "emotes/emotes.json")?;
    if catalog.entries.len() != MENU_COUNT + 1
        || catalog.entries.iter().enumerate().any(|(index, e)| {
            e.source_id != SOURCE_IDS[index]
                || !e.duration.is_finite()
                || e.duration <= 0.
                || e.label.is_empty()
                || e.action.is_empty()
                || e.use_weapon
                || !e.sounds.contains_key("male")
        })
    {
        return Err(ContentError::invalid(
            root,
            "emotes/emotes.json",
            "catalog",
            "entries",
            "requires nine ordered menu actions and Hug, with positive durations and male voices",
        ));
    }
    Ok(catalog)
}
