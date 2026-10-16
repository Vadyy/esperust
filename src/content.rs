pub(crate) mod attacks;
pub(crate) mod emotes;
pub(crate) mod attributes;
pub(crate) mod reactions;
pub(crate) mod root_motion;
pub(crate) mod simulation;

use serde::de::DeserializeOwned;
use std::{
    fmt,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct ContentError {
    path: PathBuf,
    cause: Box<dyn std::error::Error + Send + Sync>,
}

impl ContentError {
    fn invalid(root: &Path, relative: &str, entry: &str, field: &str, reason: &str) -> Self {
        Self {
            path: root.join(relative),
            cause: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{entry}: invalid {field}: {reason}"),
            )),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Field<T> {
    Value(T),
    Invalid(serde::de::IgnoredAny),
    Missing,
}
impl<T> Default for Field<T> {
    fn default() -> Self {
        Self::Missing
    }
}
impl<T> Field<T> {
    fn required(
        self,
        root: &Path,
        file: &str,
        entry: &str,
        field: &str,
    ) -> Result<T, ContentError> {
        match self {
            Self::Value(value) => Ok(value),
            _ => Err(ContentError::invalid(
                root,
                file,
                entry,
                field,
                "missing or malformed value",
            )),
        }
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cannot load imported content {}: {}",
            self.path.display(),
            self.cause
        )
    }
}

impl std::error::Error for ContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

fn read_json_at<T: DeserializeOwned>(root: &Path, relative: &str) -> Result<T, ContentError> {
    let path = root.join(relative);
    let file = File::open(&path).map_err(|cause| ContentError {
        path: path.clone(),
        cause: Box::new(cause),
    })?;
    serde_json::from_reader(BufReader::new(file)).map_err(|cause| ContentError {
        path,
        cause: Box::new(cause),
    })
}

#[cfg(test)]
pub(crate) fn required_json<T: DeserializeOwned>(relative: &str) -> T {
    read_json_at(Path::new("assets"), relative).unwrap_or_else(|error| panic!("{error}"))
}

pub(crate) fn preload(mut commands: bevy::prelude::Commands) {
    let (simulation, presentation) =
        simulation::load_simulation(Path::new("assets")).unwrap_or_else(|error| panic!("{error}"));
    commands.insert_resource(simulation);
    commands.insert_resource(presentation);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "esper-content-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct SyntheticRules {
        rate: u32,
    }

    #[test]
    fn content_is_loaded_from_the_supplied_runtime_root() {
        let fixture = Fixture::new();
        std::fs::create_dir(fixture.0.join("rules")).unwrap();
        std::fs::write(fixture.0.join("rules/test.json"), r#"{"rate":240}"#).unwrap();
        let rules: SyntheticRules = read_json_at(&fixture.0, "rules/test.json").unwrap();
        assert_eq!(rules, SyntheticRules { rate: 240 });
    }

    #[test]
    fn missing_and_invalid_content_report_the_file_instead_of_defaulting() {
        let fixture = Fixture::new();
        let missing = read_json_at::<SyntheticRules>(&fixture.0, "missing.json").unwrap_err();
        assert!(missing.to_string().contains("missing.json"));
        for text in ["{", r#"{"rate":"wrong type"}"#] {
            std::fs::write(fixture.0.join("invalid.json"), text).unwrap();
            let invalid = read_json_at::<SyntheticRules>(&fixture.0, "invalid.json").unwrap_err();
            assert!(invalid.to_string().contains("invalid.json"));
            assert!(invalid.to_string().contains("line 1"));
        }
    }
    impl Fixture {
        fn write(&self, relative: &str, value: &serde_json::Value) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
        }
    }

    pub(crate) fn synthetic_reactions() -> serde_json::Value {
        use serde_json::json;
        let actions: serde_json::Map<String, serde_json::Value> = reactions::ReactionAction::ALL
            .into_iter()
            .map(|action| {
                (
                    action.source_name().to_owned(),
                    json!({
                        "clip": "synthetic", "arguments": [0, 0, 0, false, 1.25, true]
                    }),
                )
            })
            .collect();
        json!({"source_ticks_per_ms": 4.8, "actions": actions,
            "clips": {"synthetic": {"duration_ticks": 1234.5}}})
    }
    pub(crate) fn synthetic_roots() -> serde_json::Value {
        use serde_json::json;
        let clips: serde_json::Map<String, serde_json::Value> =
            ["B0000", "R0000", "R0001", "R0002", "R0003", "SN001"]
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        json!({"has_source_track": true,
                "duration_ticks": 4800., "keys": [[0., 1., 0., 2.], [1., 3., 0., 6.]]}),
                    )
                })
                .collect();
        json!({"clips": clips})
    }
    pub(crate) fn synthetic_attributes() -> serde_json::Value {
        use serde_json::json;
        let names = [
            "ATTACKATTRIB_PLASMA_SWORD_CRITICAL",
            "ATTACKATTRIB_PLASMA_SWORD_STAND_WEAK",
            "ATTACKATTRIB_PLASMA_SWORD_STAND_STRONG",
            "ATTACKATTRIB_PLASMA_SWORD_ATTACK2_WEAK",
            "ATTACKATTRIB_PLASMA_SWORD_ATTACK2",
            "ATTACKATTRIB_PLASMA_SWORD_JUMP_CRITICAL",
            "ATTACKATTRIB_PLASMA_SWORD_JUMP",
            "ATTACKATTRIB_COUNTER_SWORD_COUNTER_CRITICAL",
            "ATTACKATTRIB_COUNTER_SWORD_COUNTER_ATTACK",
            "ATTACKATTRIB_COUNTER_SWORD_CRITICAL",
            "ATTACKATTRIB_COUNTER_SWORD_ATTACK1",
            "ATTACKATTRIB_COUNTER_SWORD_ATTACK2",
            "ATTACKATTRIB_COUNTER_SWORD_ATTACK3",
            "ATTACKATTRIB_COUNTER_SWORD_ATTACK4",
            "ATTACKATTRIB_COUNTER_SWORD_JUMPDASH",
            "ATTACKATTRIB_SPYDAGGER_ATTACK1",
            "ATTACKATTRIB_SPYDAGGER_ATTACK2",
        ];
        let calls: Vec<_> = names
            .into_iter()
            .map(|name| {
                json!({"method": "SetDamageData",
            "target": format!("AttackAttribIndexer.GetAttackAttrib({name})"),
            "arguments": [125.5]})
            })
            .collect();
        let builders: serde_json::Map<String, serde_json::Value> = [
            "StandardCounterSwordCounterAttack",
            "StandardSpyDaggerAttack1",
        ]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                json!({"calls": [
                    {"method": "SetCameraShakeEnable", "arguments": [true, false, 0.]},
                    {"method": "SetCameraShakeWhenSuccess", "arguments": [true]},
                    {"method": "SetCameraShakeSetFactor1", "arguments": [123.5, 47.25, 6.125]},
                    {"method": "SetCameraShakeSetFactor2", "arguments": [2.5, 9., 123.25, 987.75]}
                ]}),
            )
        })
        .collect();
        json!({"effects": {"attribute_calls": calls}, "weapon_builders": builders})
    }
    fn assert_context(error: ContentError, file: &str, entry: &str, field: &str) {
        let message = error.to_string();
        for expected in [file, entry, field] {
            assert!(
                message.contains(expected),
                "{expected} missing from {message}"
            );
        }
    }

    #[test]
    fn reaction_loaders_use_selected_actions_and_preserve_f64_conversion() {
        let fixture = Fixture::new();
        let mut source = synthetic_reactions();
        source["actions"]["counter_idle_upper"] = serde_json::json!({"arguments": "ignored"});
        fixture.write("practice/practice.json", &source);
        let timings = reactions::load(&fixture.0).unwrap();
        for action in reactions::ReactionAction::ALL {
            assert_eq!(
                timings.duration(action),
                (1234.5_f64 / 4.8 / 1000. / 1.25) as f32
            );
        }
    }
    #[test]
    fn reaction_loader_reports_missing_actions_clips_and_invalid_numeric_fields() {
        let fixture = Fixture::new();
        for (pointer, value, entry, field) in [
            (
                "/source_ticks_per_ms",
                serde_json::json!(0),
                "reactions",
                "source_ticks_per_ms",
            ),
            (
                "/source_ticks_per_ms",
                serde_json::json!("bad"),
                "reactions",
                "source_ticks_per_ms",
            ),
            (
                "/actions/blow_start/clip",
                serde_json::json!("missing"),
                "blow_start",
                "clip",
            ),
            (
                "/actions/blow_start/arguments/4",
                serde_json::json!(-1),
                "blow_start",
                "arguments[4]",
            ),
            (
                "/clips/synthetic/duration_ticks",
                serde_json::json!(0),
                "flinch_0",
                "duration_ticks",
            ),
        ] {
            let mut source = synthetic_reactions();
            *source.pointer_mut(pointer).unwrap() = value;
            fixture.write("practice/practice.json", &source);
            assert_context(
                reactions::load(&fixture.0).unwrap_err(),
                "practice/practice.json",
                entry,
                field,
            );
        }
        let mut source = synthetic_reactions();
        source["actions"].as_object_mut().unwrap().remove("get_up");
        fixture.write("practice/practice.json", &source);
        assert_context(
            reactions::load(&fixture.0).unwrap_err(),
            "practice/practice.json",
            "get_up",
            "action",
        );
    }
    #[test]
    fn root_loader_distinguishes_optional_empty_tracks_and_missing_selected_clips() {
        let fixture = Fixture::new();
        let mut source = synthetic_roots();
        source["clips"]["SN001"] = serde_json::json!({"has_source_track": false, "keys": []});
        fixture.write("melee/root-motion.json", &source);
        let roots = root_motion::load(&fixture.0).unwrap();
        assert_eq!(roots.clip("SN001").speed(0., 1., 1., 1., 2.), (0., 0.));
        assert_eq!(roots.clip("B0000").speed(0., 1., 1., 1., 2.), (8., 4.));
        source["clips"].as_object_mut().unwrap().remove("SN001");
        fixture.write("melee/root-motion.json", &source);
        assert_context(
            root_motion::load(&fixture.0).unwrap_err(),
            "root-motion.json",
            "SN001",
            "clip",
        );
    }
    #[test]
    fn root_loader_rejects_missing_malformed_unordered_and_nonfinite_keys() {
        let fixture = Fixture::new();
        for (field, value) in [
            ("keys", serde_json::json!(null)),
            ("keys", serde_json::json!([[0., 1., 2.]])),
            (
                "keys",
                serde_json::json!([[1., 0., 0., 0.], [0., 0., 0., 0.]]),
            ),
            ("keys", serde_json::json!([[0., 1e100, 0., 0.]])),
            ("duration_ticks", serde_json::json!(0)),
            ("duration_ticks", serde_json::json!("bad")),
        ] {
            let mut source = synthetic_roots();
            source["clips"]["B0000"][field] = value;
            fixture.write("melee/root-motion.json", &source);
            assert_context(
                root_motion::load(&fixture.0).unwrap_err(),
                "root-motion.json",
                "B0000",
                field,
            );
        }
    }
    #[test]
    fn attributes_preserve_optional_defaults_last_writes_and_unrelated_methods() {
        let fixture = Fixture::new();
        let mut source = synthetic_attributes();
        let key = "ATTACKATTRIB_COUNTER_SWORD_ATTACK1";
        let calls = source["effects"]["attribute_calls"].as_array_mut().unwrap();
        for (method, args) in [
            (
                "SetCounterAttack",
                serde_json::json!(["COUNTERATTACK_ENABLE"]),
            ),
            (
                "SetGuardData",
                serde_json::json!([57.5, "GUARDCRUSH_ENABLE"]),
            ),
            (
                "SetDamageData",
                serde_json::json!([777.25, "DAMAGEMOVING_ENABLE", {"unrelated": true}]),
            ),
            ("SetDamageData", serde_json::json!([251.75])),
            ("SetGuardData", serde_json::json!([31.25])),
            ("SetCounterAttack", serde_json::json!([])),
            (
                "UnknownRegistration",
                serde_json::json!({"ignored": "any metadata"}),
            ),
        ] {
            calls.push(serde_json::json!({"method": method,
                "target": format!("AttackAttribIndexer.GetAttackAttrib({key})"), "arguments": args}));
        }
        fixture.write("melee/hit-rules.json", &source);
        let rules = attributes::load(&fixture.0).unwrap();
        assert_eq!(
            rules.profile(crate::combat_types::AttackAttributeId::CounterAttack1),
            attributes::AttackProfile {
                guard_time: 31.25_f32 * 0.001,
                damage_time: 251.75_f32 * 0.001,
                ..Default::default()
            }
        );
    }
    #[test]
    fn attributes_reject_missing_selected_profiles_and_malformed_selected_arguments() {
        let fixture = Fixture::new();
        let key = "ATTACKATTRIB_PLASMA_SWORD_CRITICAL";
        let mut source = synthetic_attributes();
        source["effects"]["attribute_calls"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        fixture.write("melee/hit-rules.json", &source);
        assert_context(
            attributes::load(&fixture.0).unwrap_err(),
            "hit-rules.json",
            key,
            "attribute",
        );
        for args in [
            serde_json::json!(["bad"]),
            serde_json::json!([1e100]),
            serde_json::json!([10., true]),
            serde_json::json!([10., "UNKNOWN"]),
        ] {
            let mut source = synthetic_attributes();
            source["effects"]["attribute_calls"][0]["arguments"] = args;
            fixture.write("melee/hit-rules.json", &source);
            assert_context(
                attributes::load(&fixture.0).unwrap_err(),
                "hit-rules.json",
                key,
                "arguments[",
            );
        }
    }
    #[test]
    fn selected_attribute_calls_reject_malformed_targets_even_after_valid_calls() {
        let fixture = Fixture::new();
        let mut source = synthetic_attributes();
        source["effects"]["attribute_calls"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"method": "SetDamageData", "target": 42, "arguments": [99.]}));
        fixture.write("melee/hit-rules.json", &source);
        assert_context(
            attributes::load(&fixture.0).unwrap_err(),
            "hit-rules.json",
            "attribute_calls[17]",
            "target",
        );
    }
    #[test]
    fn camera_builders_preserve_first_match_and_cast_before_scaling() {
        let fixture = Fixture::new();
        let mut source = synthetic_attributes();
        let builder = "StandardCounterSwordCounterAttack";
        source["weapon_builders"][builder]["calls"].as_array_mut().unwrap().push(
            serde_json::json!({"method": "SetCameraShakeSetFactor1", "arguments": [999., 999., 999.]}));
        fixture.write("melee/hit-rules.json", &source);
        let shake = attributes::load(&fixture.0)
            .unwrap()
            .shake(builder)
            .unwrap();
        assert_eq!(shake.full_seconds, 123.5_f32 * 0.001);
        assert_eq!(shake.fade_seconds, 47.25_f32 * 0.001);
        assert_eq!(shake.amplitude, 6.125_f32 * 0.01);
        assert_eq!(shake.near, 123.25_f32 * 0.01);
        assert_eq!(shake.far, 987.75_f32 * 0.01);
        assert!(!shake.replace);
    }
    #[test]
    fn camera_builders_report_missing_methods_and_invalid_selected_fields() {
        let fixture = Fixture::new();
        let builder = "StandardCounterSwordCounterAttack";
        for (pointer, value, field) in [
            (
                "/weapon_builders/StandardCounterSwordCounterAttack/calls/0/arguments/1",
                serde_json::json!("bad"),
                "arguments[1]",
            ),
            (
                "/weapon_builders/StandardCounterSwordCounterAttack/calls/2/arguments/0",
                serde_json::json!(1e100),
                "arguments[0]",
            ),
            (
                "/weapon_builders/StandardCounterSwordCounterAttack/calls/0/arguments/2",
                serde_json::json!(1),
                "eligibility",
            ),
        ] {
            let mut source = synthetic_attributes();
            *source.pointer_mut(pointer).unwrap() = value;
            fixture.write("melee/hit-rules.json", &source);
            assert_context(
                attributes::load(&fixture.0).unwrap_err(),
                "hit-rules.json",
                builder,
                field,
            );
        }
        let mut source = synthetic_attributes();
        source["weapon_builders"][builder]["calls"]
            .as_array_mut()
            .unwrap()
            .pop();
        fixture.write("melee/hit-rules.json", &source);
        assert_context(
            attributes::load(&fixture.0).unwrap_err(),
            "hit-rules.json",
            builder,
            "SetCameraShakeSetFactor2",
        );
    }
    #[test]
    fn catalog_roots_are_isolated_and_selected_cross_references_fail_contextually() {
        let first = Fixture::new();
        let second = Fixture::new();
        for fixture in [&first, &second] {
            fixture.write("emotes/emotes.json", &serde_json::json!({
                "menu": { "text": "Synthetic", "rect": [0.,0.,250.,300.],
                    "font_size": 11., "line_height": 20., "color": [238,129,192,255] }, "entries":
                (0..10).map(|index| serde_json::json!({
                    "source_id": emotes::SOURCE_IDS[index], "label": "Synthetic",
                    "action": format!("emote_{index}"), "duration": 1., "use_weapon": false,
                    "sounds": { "male": { "path": "synthetic.ogg" } }
                })).collect::<Vec<_>>() }));
            fixture.write("practice/practice.json", &synthetic_reactions());
            fixture.write("melee/root-motion.json", &synthetic_roots());
            fixture.write("melee/hit-rules.json", &synthetic_attributes());
        }
        let mut changed = synthetic_reactions();
        changed["clips"]["synthetic"]["duration_ticks"] = serde_json::json!(2469.);
        second.write("practice/practice.json", &changed);
        let (a, _) = simulation::load_simulation(&first.0).unwrap();
        let (b, _) = simulation::load_simulation(&second.0).unwrap();
        let action = reactions::ReactionAction::BlowStart;
        assert_eq!(a.duration(action) * 2., b.duration(action));
        std::fs::remove_dir_all(&first.0).unwrap();
        assert!(a.duration(action) > 0., "snapshot does not reopen its root");
        changed["actions"]["blow_start"]["clip"] = serde_json::json!("missing");
        second.write("practice/practice.json", &changed);
        assert_context(
            simulation::load_simulation(&second.0).unwrap_err(),
            "practice.json",
            "blow_start",
            "clip",
        );
        second.write("practice/practice.json", &synthetic_reactions());
        let mut changed = synthetic_roots();
        changed["clips"].as_object_mut().unwrap().remove("SN001");
        second.write("melee/root-motion.json", &changed);
        assert_context(
            simulation::load_simulation(&second.0).unwrap_err(),
            "root-motion.json",
            "SN001",
            "clip",
        );
        second.write("melee/root-motion.json", &synthetic_roots());
        let mut changed = synthetic_attributes();
        changed["effects"]["attribute_calls"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        second.write("melee/hit-rules.json", &changed);
        assert_context(
            simulation::load_simulation(&second.0).unwrap_err(),
            "hit-rules.json",
            "PLASMA_SWORD_CRITICAL",
            "attribute",
        );
    }

    #[test]
    fn catalog_rejects_negative_durations_and_invalid_camera_bounds() {
        let fixture = Fixture::new();
        fixture.write("practice/practice.json", &synthetic_reactions());
        fixture.write("melee/root-motion.json", &synthetic_roots());
        for pointer in [
            "/effects/attribute_calls/0/arguments/0",
            "/weapon_builders/StandardCounterSwordCounterAttack/calls/2/arguments/1",
            "/weapon_builders/StandardCounterSwordCounterAttack/calls/3/arguments/3",
        ] {
            let mut attributes = synthetic_attributes();
            *attributes.pointer_mut(pointer).unwrap() = serde_json::json!(-1.);
            fixture.write("melee/hit-rules.json", &attributes);
            assert!(
                simulation::load_simulation(&fixture.0)
                    .unwrap_err()
                    .to_string()
                    .contains("hit-rules.json")
            );
        }
    }
}
