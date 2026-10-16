use super::{ContentError, Field, read_json_at};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

const FILE: &str = "melee/root-motion.json";
const SELECTED: [&str; 6] = ["B0000", "R0000", "R0001", "R0002", "R0003", "SN001"];

#[derive(Deserialize)]
struct ClipWire {
    #[serde(default)]
    has_source_track: Field<bool>,
    #[serde(default)]
    duration_ticks: Field<f64>,
    #[serde(default)]
    keys: Field<Vec<Field<[f64; 4]>>>,
}
#[derive(Deserialize)]
struct SourceWire {
    clips: HashMap<String, ClipWire>,
}

#[derive(Debug)]
pub(crate) struct RootMotionClip {
    duration: f32,
    keys: Vec<[f32; 4]>,
}
impl RootMotionClip {
    fn validate(wire: ClipWire, root: &Path, name: &str) -> Result<Self, ContentError> {
        let invalid =
            |field: &str, reason: &str| ContentError::invalid(root, FILE, name, field, reason);
        let has_track = wire
            .has_source_track
            .required(root, FILE, name, "has_source_track")?;
        let keys = wire.keys.required(root, FILE, name, "keys")?;
        if !has_track && keys.is_empty() {
            return Ok(Self {
                duration: 0.,
                keys: Vec::new(),
            });
        }
        let ticks = wire
            .duration_ticks
            .required(root, FILE, name, "duration_ticks")?;
        let duration = ticks as f32 / 4800.;
        if !ticks.is_finite() || ticks <= 0. || !duration.is_finite() || duration <= 0. {
            return Err(invalid(
                "duration_ticks",
                "must produce a finite positive duration",
            ));
        }
        let mut validated: Vec<[f32; 4]> = Vec::with_capacity(keys.len());
        for (i, key) in keys.into_iter().enumerate() {
            let field = format!("keys[{i}]");
            let key = key
                .required(root, FILE, name, &field)?
                .map(|value| value as f32);
            if key.iter().any(|value| !value.is_finite()) {
                return Err(invalid(&field, "components must be finite f32 values"));
            }
            if key[0] < 0.
                || validated
                    .last()
                    .is_some_and(|previous| previous[0] > key[0])
            {
                return Err(invalid(
                    &field,
                    "timestamps must be nonnegative and ordered",
                ));
            }
            validated.push(key);
        }
        Ok(Self {
            duration,
            keys: validated,
        })
    }

    fn sample(&self, time: f32, target_duration: f32) -> (f32, f32) {
        let Some(mut a) = self.keys.first() else {
            return (0., 0.);
        };
        let time = (time / target_duration).clamp(0., 1.) * self.duration;
        for b in &self.keys[1..] {
            let end = b[0];
            if end >= time {
                let start = a[0];
                let t = ((time - start) / (end - start)).clamp(0., 1.);
                let interpolate = |axis: usize| {
                    let x = a[axis];
                    x + (b[axis] - x) * t
                };
                return (interpolate(3), interpolate(1));
            }
            a = b;
        }
        (a[3], a[1])
    }
    pub(crate) fn speed(
        &self,
        previous: f32,
        current: f32,
        dt: f32,
        target_duration: f32,
        multiplier: f32,
    ) -> (f32, f32) {
        if dt <= 0. {
            return (0., 0.);
        }
        let a = self.sample(previous, target_duration);
        let b = self.sample(current, target_duration);
        ((b.0 - a.0) * multiplier / dt, (b.1 - a.1) * multiplier / dt)
    }
}

#[derive(Debug)]
pub(crate) struct RootMotion {
    clips: HashMap<String, RootMotionClip>,
}
impl RootMotion {
    pub(crate) fn clip(&self, name: &str) -> &RootMotionClip {
        self.clips
            .get(name)
            .expect("selected root clip validated at load")
    }
}
pub(crate) fn load(root: &Path) -> Result<RootMotion, ContentError> {
    from_source(read_json_at(root, FILE)?, root)
}

#[cfg(test)]
pub(super) fn synthetic(value: serde_json::Value) -> Result<RootMotion, ContentError> {
    let root = Path::new("synthetic");
    let source = serde_json::from_value(value).map_err(|error| {
        ContentError::invalid(root, FILE, "fixture", "schema", &error.to_string())
    })?;
    from_source(source, root)
}

fn from_source(source: SourceWire, root: &Path) -> Result<RootMotion, ContentError> {
    for name in SELECTED {
        if !source.clips.contains_key(name) {
            return Err(ContentError::invalid(
                root,
                FILE,
                name,
                "clip",
                "missing required selected clip",
            ));
        }
    }
    let clips = source
        .clips
        .into_iter()
        .map(|(name, wire)| RootMotionClip::validate(wire, root, &name).map(|clip| (name, clip)))
        .collect::<Result<_, _>>()?;
    Ok(RootMotion { clips })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampling_keeps_columns_clamps_endpoints_and_nonpositive_dt() {
        let clip = RootMotionClip {
            duration: 1.,
            keys: vec![[0., 2., 99., 1.], [0.5, 4., -99., 5.], [1., 8., 0., 7.]],
        };
        assert_eq!(clip.sample(-1., 2.), (1., 2.));
        assert_eq!(clip.sample(1., 2.), (5., 4.));
        assert_eq!(clip.sample(9., 2.), (7., 8.));
        assert_eq!(clip.speed(0., 1., 1., 2., 1.7), (4. * 1.7, 2. * 1.7));
        assert_eq!(clip.speed(0., 1., 0., 2., 1.7), (0., 0.));
        assert_eq!(clip.speed(0., 1., -1., 2., 1.7), (0., 0.));
    }
    #[test]
    fn equal_timestamps_keep_source_order_and_legacy_zero_span_nan() {
        let wire: ClipWire = serde_json::from_str(
            r#"{"has_source_track":true,"duration_ticks":4800,
            "keys":[[0,0,0,0],[0.5,1,0,2],[0.5,3,0,4],[1,5,0,6]]}"#,
        )
        .unwrap();
        let clip = RootMotionClip::validate(wire, Path::new("synthetic"), "equal").unwrap();
        assert_eq!(clip.sample(0.5, 1.), (2., 1.));
        let after = clip.sample(0.75, 1.);
        assert_eq!(after, (5., 4.));
        let clip = RootMotionClip {
            duration: 1.,
            keys: vec![[0., 1., 0., 2.], [0., 3., 0., 4.]],
        };
        let zero = clip.sample(0., 1.);
        assert!(zero.0.is_nan() && zero.1.is_nan());
    }
}
