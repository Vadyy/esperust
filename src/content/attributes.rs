use super::{ContentError, Field, read_json_at};
use crate::combat_types::AttackAttributeId;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

const FILE: &str = "melee/hit-rules.json";
const SELECTED_BUILDERS: [&str; 2] = [
    "StandardCounterSwordCounterAttack",
    "StandardSpyDaggerAttack1",
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AttackProfile {
    pub counterable: bool,
    pub guard_crush: bool,
    pub guard_time: f32,
    pub damage_time: f32,
    pub damage_moving: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CameraShakeProfile {
    pub full_seconds: f32,
    pub fade_seconds: f32,
    pub amplitude: f32,
    pub up_factor: f32,
    pub right_factor: f32,
    pub near: f32,
    pub far: f32,
    pub replace: bool,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Argument {
    Number(f64),
    Symbol(String),
    Bool(bool),
    Other(serde::de::IgnoredAny),
}
#[derive(Deserialize)]
struct CallWire {
    #[serde(default)]
    method: Field<String>,
    #[serde(default)]
    target: Field<String>,
    #[serde(default)]
    arguments: Field<Vec<Argument>>,
}
#[derive(Deserialize)]
struct EffectsWire {
    attribute_calls: Vec<CallWire>,
}
#[derive(Deserialize)]
struct BuilderWire {
    calls: Vec<CallWire>,
}
#[derive(Deserialize)]
struct SourceWire {
    effects: EffectsWire,
    weapon_builders: HashMap<String, BuilderWire>,
}

enum Registration {
    Counter(bool),
    Guard { seconds: f32, crush: bool },
    Damage { seconds: f32, moving: bool },
    Unrelated,
}
struct Arguments<'a> {
    values: &'a [Argument],
    root: &'a Path,
    entry: String,
}
impl<'a> Arguments<'a> {
    fn new(call: &'a CallWire, root: &'a Path, entry: String) -> Result<Self, ContentError> {
        let Field::Value(values) = &call.arguments else {
            return Err(ContentError::invalid(
                root,
                FILE,
                &entry,
                "arguments",
                "expected an argument array",
            ));
        };
        Ok(Self {
            values,
            root,
            entry,
        })
    }
    fn invalid(&self, index: usize, reason: &str) -> ContentError {
        ContentError::invalid(
            self.root,
            FILE,
            &self.entry,
            &format!("arguments[{index}]"),
            reason,
        )
    }
    fn number(&self, index: usize) -> Result<f64, ContentError> {
        match self.values.get(index) {
            Some(Argument::Number(value)) if value.is_finite() && (*value as f32).is_finite() => {
                Ok(*value)
            }
            _ => Err(self.invalid(index, "expected finite number representable as f32")),
        }
    }
    fn boolean(&self, index: usize) -> Result<bool, ContentError> {
        match self.values.get(index) {
            Some(Argument::Bool(value)) => Ok(*value),
            _ => Err(self.invalid(index, "expected boolean")),
        }
    }
    fn flag(&self, index: usize, enabled: &str, disabled: &str) -> Result<bool, ContentError> {
        match self.values.get(index) {
            None => Ok(false),
            Some(Argument::Symbol(symbol)) if symbol == enabled => Ok(true),
            Some(Argument::Symbol(symbol)) if symbol == disabled => Ok(false),
            _ => Err(self.invalid(
                index,
                "expected known enable/disable symbol or absent optional flag",
            )),
        }
    }
    fn registration(
        call: &'a CallWire,
        root: &'a Path,
        entry: String,
        method: &str,
    ) -> Result<Registration, ContentError> {
        if !matches!(
            method,
            "SetCounterAttack" | "SetGuardData" | "SetDamageData"
        ) {
            return Ok(Registration::Unrelated);
        }
        let args = Self::new(call, root, entry)?;
        Ok(match method {
            "SetCounterAttack" => Registration::Counter(args.flag(
                0,
                "COUNTERATTACK_ENABLE",
                "COUNTERATTACK_DISABLE",
            )?),
            "SetGuardData" => Registration::Guard {
                seconds: args.number(0)? as f32 * 0.001,
                crush: args.flag(1, "GUARDCRUSH_ENABLE", "GUARDCRUSH_DISABLE")?,
            },
            _ => Registration::Damage {
                seconds: args.number(0)? as f32 * 0.001,
                moving: args.flag(1, "DAMAGEMOVING_ENABLE", "DAMAGEMOVING_DISABLE")?,
            },
        })
    }
}
#[derive(Debug)]
pub(crate) struct Attributes {
    profiles: HashMap<AttackAttributeId, AttackProfile>,
    shakes: HashMap<String, CameraShakeProfile>,
}
impl Attributes {
    pub(crate) fn profile(&self, id: AttackAttributeId) -> AttackProfile {
        self.profiles[&id]
    }
    pub(crate) fn shake(&self, name: &str) -> Option<CameraShakeProfile> {
        self.shakes.get(name).copied()
    }
}
pub(crate) fn load(root: &Path) -> Result<Attributes, ContentError> {
    from_source(read_json_at(root, FILE)?, root)
}

#[cfg(test)]
pub(super) fn synthetic(value: serde_json::Value) -> Result<Attributes, ContentError> {
    let root = Path::new("synthetic");
    let source = serde_json::from_value(value).map_err(|error| {
        ContentError::invalid(root, FILE, "fixture", "schema", &error.to_string())
    })?;
    from_source(source, root)
}

fn from_source(source: SourceWire, root: &Path) -> Result<Attributes, ContentError> {
    let mut profiles: HashMap<String, AttackProfile> = HashMap::new();
    for (i, call) in source.effects.attribute_calls.into_iter().enumerate() {
        let entry = format!("attribute_calls[{i}]");
        let Field::Value(target) = &call.target else {
            if matches!(&call.method, Field::Value(method) if matches!(method.as_str(), "SetCounterAttack" | "SetGuardData" | "SetDamageData"))
            {
                return Err(ContentError::invalid(
                    root,
                    FILE,
                    &entry,
                    "target",
                    "expected attribute target string",
                ));
            }
            continue;
        };
        let Some(key) = target
            .strip_prefix("AttackAttribIndexer.GetAttackAttrib(")
            .and_then(|s| s.strip_suffix(')'))
        else {
            continue;
        };
        let entry = format!("{key} ({entry})");
        let Field::Value(method) = &call.method else {
            return Err(ContentError::invalid(
                root,
                FILE,
                &entry,
                "method",
                "expected method name",
            ));
        };
        let profile = profiles.entry(key.to_owned()).or_default();
        match Arguments::registration(&call, root, entry, method)? {
            Registration::Counter(value) => profile.counterable = value,
            Registration::Guard { seconds, crush } => {
                profile.guard_time = seconds;
                profile.guard_crush = crush;
            }
            Registration::Damage { seconds, moving } => {
                profile.damage_time = seconds;
                profile.damage_moving = moving;
            }
            Registration::Unrelated => {}
        }
    }
    for id in AttackAttributeId::ALL {
        let key = id.source_name();
        if !profiles.contains_key(key) {
            return Err(ContentError::invalid(
                root,
                FILE,
                key,
                "attribute",
                "missing required selected attribute",
            ));
        }
    }
    let mut shakes = HashMap::new();
    for name in SELECTED_BUILDERS {
        let builder = source.weapon_builders.get(name).ok_or_else(|| {
            ContentError::invalid(
                root,
                FILE,
                name,
                "builder",
                "missing selected camera-shake builder",
            )
        })?;
        let arguments = |method: &str| {
            let call = builder
                .calls
                .iter()
                .find(|call| matches!(&call.method, Field::Value(value) if value == method))
                .ok_or_else(|| {
                    ContentError::invalid(
                        root,
                        FILE,
                        name,
                        method,
                        "missing selected builder method",
                    )
                })?;
            Arguments::new(call, root, format!("{name}.{method}"))
        };
        let enable = arguments("SetCameraShakeEnable")?;
        let success = arguments("SetCameraShakeWhenSuccess")?;
        if !enable.boolean(0)? || enable.number(2)? != 0. || !success.boolean(0)? {
            return Err(ContentError::invalid(
                root,
                FILE,
                name,
                "camera-shake eligibility",
                "expected enabled, success-only shake with zero offset",
            ));
        }
        let first = arguments("SetCameraShakeSetFactor1")?;
        let second = arguments("SetCameraShakeSetFactor2")?;
        shakes.insert(
            name.to_owned(),
            CameraShakeProfile {
                full_seconds: first.number(0)? as f32 * 0.001,
                fade_seconds: first.number(1)? as f32 * 0.001,
                amplitude: first.number(2)? as f32 * 0.01,
                up_factor: second.number(0)? as f32,
                right_factor: second.number(1)? as f32,
                near: second.number(2)? as f32 * 0.01,
                far: second.number(3)? as f32 * 0.01,
                replace: enable.boolean(1)?,
            },
        );
    }
    Ok(Attributes {
        profiles: profiles
            .into_iter()
            .filter_map(|(name, profile)| {
                AttackAttributeId::from_source_name(&name).map(|id| (id, profile))
            })
            .collect(),
        shakes,
    })
}
