use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// A database object we can diff.
pub trait Diffable {
    fn labels(&self) -> &HashMap<String, String>;
    fn annotations(&self) -> &HashMap<String, String>;
    fn finalizers(&self) -> &Vec<String>;
    fn data(&self) -> &Value;
}

/// A macro that implements `Diffable` for structs that have all required fields.
#[macro_export]
macro_rules! diffable {
    ($t:ty) => {
        impl $crate::models::diff::Diffable for $t {
            fn labels(&self) -> &HashMap<String, String, RandomState> {
                &self.labels
            }

            fn annotations(&self) -> &HashMap<String, String, RandomState> {
                &self.annotations
            }

            fn finalizers(&self) -> &Vec<String> {
                &self.finalizers
            }

            fn data(&self) -> &Value {
                &self.data
            }
        }
    };
}

/// Detect changes detection between current and new state.
pub fn diff_paths<D>(current: &D, new: &D) -> Vec<String>
where
    D: Diffable,
{
    let mut result = Vec::new();

    if current.annotations() != new.annotations()
        || current.labels() != new.labels()
        || current.finalizers() != new.finalizers()
    {
        result.push(".metadata".to_string());
    }

    diff_data(&current.data(), &new.data(), &mut result);

    result
}

fn diff_data(current: &Value, new: &Value, paths: &mut Vec<String>) {
    diff_section(
        current.as_object().unwrap_or(&Map::new()),
        new.as_object().unwrap_or(&Map::new()),
        paths,
    );
}

fn diff_section(current: &Map<String, Value>, new: &Map<String, Value>, paths: &mut Vec<String>) {
    let mut checked = HashSet::new();

    for (k, v) in current {
        checked.insert(k);

        let other = new.get(k).unwrap_or(&Value::Null);
        diff_maps_data(v, other, paths, &format!(".{}", k))
    }

    for (k, v) in new {
        if checked.contains(k) {
            continue;
        }

        let other = current.get(k).unwrap_or(&Value::Null);
        diff_maps_data(other, v, paths, &format!(".{}", k))
    }
}

/// check last level maps
fn diff_maps(
    current: &Map<String, Value>,
    new: &Map<String, Value>,
    paths: &mut Vec<String>,
    prefix: &str,
) {
    let mut checked = HashSet::new();

    for (k, v) in current {
        checked.insert(k.to_string());
        if let Some(other) = new.get(k) {
            if v != other {
                // got changed
                paths.push(format!("{}.{}", prefix, k));
            }
        } else {
            // got removed
            paths.push(format!("{}.{}", prefix, k));
        }
    }

    for k in new.keys() {
        if checked.contains(k) {
            continue;
        }
        // got added
        paths.push(format!("{}.{}", prefix, k));
    }
}

fn diff_maps_data(current: &Value, new: &Value, paths: &mut Vec<String>, prefix: &str) {
    match (current, new) {
        (Value::Object(current), Value::Object(new)) => {
            diff_maps(current, new, paths, prefix);
        }
        (Value::Object(current), _) => {
            paths.extend(current.keys().map(|k| format!("{}.{}", prefix, k)));
        }
        (_, Value::Object(new)) => {
            paths.extend(new.keys().map(|k| format!("{}.{}", prefix, k)));
        }
        _ => {}
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_diff_1() {
        let mut paths = Vec::new();
        diff_data(
            &json!({
                "spec": {
                    "keep": "same",
                    "remove": "me",
                    "change": 1,
                    "complex": {"foo": "bar"},
                },
                "status": {
                    "foo": "bar",
                    "bar": "foo",
                }
            }),
            &json!({
                "spec":{
                    "keep": "same",
                    "add": "me",
                    "change": 2,
                    "complex": {"foo": "baz"},
                },
                "other": {
                    "bar": "foo",
                    "baz": "baz",
                }
            }),
            &mut paths,
        );

        paths.sort_unstable();

        let mut expected = vec![
            ".spec.remove",
            ".spec.add",
            ".spec.change",
            ".spec.complex",
            ".status.foo",
            ".status.bar",
            ".other.bar",
            ".other.baz",
        ];
        expected.sort_unstable();

        assert_eq!(paths, expected,);
    }
}
