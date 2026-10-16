use super::{ContentError, Field, read_json_at};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

const FILE: &str = "practice/practice.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReactionAction {
    Flinch0,
    Flinch1,
    Flinch2,
    Flinch3,
    BlowStart,
    GetUp,
}
impl ReactionAction {
    pub(crate) const ALL: [Self; 6] = [
        Self::Flinch0,
        Self::Flinch1,
        Self::Flinch2,
        Self::Flinch3,
        Self::BlowStart,
        Self::GetUp,
    ];
    pub(crate) fn source_name(self) -> &'static str {
        match self {
            Self::Flinch0 => "flinch_0",
            Self::Flinch1 => "flinch_1",
            Self::Flinch2 => "flinch_2",
            Self::Flinch3 => "flinch_3",
            Self::BlowStart => "blow_start",
            Self::GetUp => "get_up",
        }
    }
}

#[derive(Deserialize)]
struct ActionWire {
    #[serde(default)]
    clip: Field<String>,
    #[serde(default)]
    arguments: Field<(f64, u32, u32, bool, f64, bool)>,
}
#[derive(Deserialize)]
struct ClipWire {
    #[serde(default)]
    duration_ticks: Field<f64>,
}
#[derive(Deserialize)]
struct SourceWire {
    #[serde(default)]
    source_ticks_per_ms: Field<f64>,
    actions: HashMap<String, ActionWire>,
    clips: HashMap<String, ClipWire>,
}

#[derive(Debug)]
pub(crate) struct ReactionTimings([f32; 6]);
impl ReactionTimings {
    pub(crate) fn duration(&self, action: ReactionAction) -> f32 {
        self.0[action as usize]
    }
}

pub(crate) fn load(root: &Path) -> Result<ReactionTimings, ContentError> {
    from_source(read_json_at(root, FILE)?, root)
}

#[cfg(test)]
pub(super) fn synthetic(value: serde_json::Value) -> Result<ReactionTimings, ContentError> {
    let root = Path::new("synthetic");
    let source = serde_json::from_value(value).map_err(|error| {
        ContentError::invalid(root, FILE, "fixture", "schema", &error.to_string())
    })?;
    from_source(source, root)
}

fn from_source(source: SourceWire, root: &Path) -> Result<ReactionTimings, ContentError> {
    let rate =
        source
            .source_ticks_per_ms
            .required(root, FILE, "reactions", "source_ticks_per_ms")?;
    if !rate.is_finite() || rate <= 0. {
        return Err(ContentError::invalid(
            root,
            FILE,
            "reactions",
            "source_ticks_per_ms",
            "must be finite and positive",
        ));
    }
    let mut durations = [0.; 6];
    for (index, action) in ReactionAction::ALL.into_iter().enumerate() {
        let name = action.source_name();
        let missing = |field| {
            ContentError::invalid(root, FILE, name, field, "missing selected action or clip")
        };
        let wire = source.actions.get(name).ok_or_else(|| missing("action"))?;
        let Field::Value(clip_name) = &wire.clip else {
            return Err(missing("clip"));
        };
        let Field::Value(arguments) = &wire.arguments else {
            return Err(missing("arguments"));
        };
        let speed = arguments.4;
        if !speed.is_finite() || speed <= 0. {
            return Err(ContentError::invalid(
                root,
                FILE,
                name,
                "arguments[4]",
                "rate must be finite and positive",
            ));
        }
        let clip = source.clips.get(clip_name).ok_or_else(|| missing("clip"))?;
        let Field::Value(ticks) = clip.duration_ticks else {
            return Err(missing("duration_ticks"));
        };
        let duration = (ticks / rate / 1000. / speed) as f32;
        if !ticks.is_finite() || ticks <= 0. || !duration.is_finite() || duration <= 0. {
            return Err(ContentError::invalid(
                root,
                FILE,
                name,
                "duration_ticks",
                "must produce a finite positive duration",
            ));
        }
        durations[index] = duration;
    }
    Ok(ReactionTimings(durations))
}
