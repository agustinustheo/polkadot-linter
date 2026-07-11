use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
};

const SEC013_NAME: &str = "unbounded-storage-collections";

pub fn analyze_rustdoc_json(path: &Path, config: &Config) -> Result<Vec<Diagnostic>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("could not read rustdoc JSON {}: {err}", path.display()))?;
    analyze_rustdoc_json_str(&content, None, config)
}

pub fn analyze_rustdoc_json_str(
    content: &str,
    source_root: Option<&Path>,
    config: &Config,
) -> Result<Vec<Diagnostic>, String> {
    if !config.rule_enabled("SEC013") {
        return Ok(Vec::new());
    }

    let root: Value =
        serde_json::from_str(content).map_err(|err| format!("invalid rustdoc JSON: {err}"))?;
    let Some(index) = root.get("index").and_then(Value::as_object) else {
        return Err("rustdoc JSON did not contain an object at `index`".to_string());
    };

    let mut diagnostics = Vec::new();
    for item in index.values() {
        let Some(type_alias) = item.pointer("/inner/type_alias/type") else {
            continue;
        };
        let Some((storage_kind, value_type)) = storage_value_type(type_alias) else {
            continue;
        };
        if item_has_unbounded_attr(item) || docs_mark_capacity_limited(item) {
            continue;
        }
        if !type_contains_unbounded_collection(value_type, false) {
            continue;
        }

        let (file, line, column) = span_location(item, source_root);
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("<unnamed storage>");
        diagnostics.push(Diagnostic {
            rule_id: "SEC013".to_string(),
            rule_name: SEC013_NAME.to_string(),
            category: RuleCategory::Semantic,
            severity: config.rule_severity("SEC013", Severity::Warning),
            file,
            line,
            column,
            end_line: None,
            message: format!(
                "`{name}` stores an unbounded collection in `{storage_kind}`"
            ),
            explanation: "rustdoc JSON resolved this storage alias to a FRAME storage type whose \
                value argument contains an unbounded collection such as `Vec` or `BTreeMap`. \
                Unbounded storage values can grow without a local type-level limit and make every \
                access pay state-size-dependent decode cost."
                .to_string(),
            suggestion: Some(
                "Use a bounded collection wrapper or document and explicitly mark the storage as unbounded"
                    .to_string(),
            ),
        });
    }

    diagnostics.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    Ok(diagnostics)
}

fn storage_value_type(type_value: &Value) -> Option<(&'static str, &Value)> {
    let path = type_value.pointer("/resolved_path/path")?.as_str()?;
    let kind = storage_kind(path)?;
    let args = type_args(type_value)?;
    let value_index = match kind {
        "StorageValue" => 1,
        "StorageMap" | "CountedStorageMap" => 3,
        "StorageDoubleMap" => 5,
        "StorageNMap" => 2,
        _ => return None,
    };
    args.get(value_index)?
        .get("type")
        .map(|value| (kind, value))
}

fn storage_kind(path: &str) -> Option<&'static str> {
    match path.rsplit("::").next().unwrap_or(path) {
        "StorageValue" => Some("StorageValue"),
        "StorageMap" => Some("StorageMap"),
        "CountedStorageMap" => Some("CountedStorageMap"),
        "StorageDoubleMap" => Some("StorageDoubleMap"),
        "StorageNMap" => Some("StorageNMap"),
        _ => None,
    }
}

fn type_args(type_value: &Value) -> Option<&Vec<Value>> {
    type_value
        .pointer("/resolved_path/args/angle_bracketed/args")
        .and_then(Value::as_array)
}

fn type_contains_unbounded_collection(type_value: &Value, inside_bounded: bool) -> bool {
    if let Some(path) = type_value
        .pointer("/resolved_path/path")
        .and_then(Value::as_str)
    {
        let name = path.rsplit("::").next().unwrap_or(path);
        let is_bounded = inside_bounded || is_bounded_collection(name);
        if !is_bounded && is_unbounded_collection(name) {
            return true;
        }
        return type_args(type_value).is_some_and(|args| {
            args.iter().any(|arg| {
                arg.get("type")
                    .is_some_and(|ty| type_contains_unbounded_collection(ty, is_bounded))
            })
        });
    }

    match type_value {
        Value::Array(values) => values
            .iter()
            .any(|value| type_contains_unbounded_collection(value, inside_bounded)),
        Value::Object(map) => map
            .values()
            .any(|value| type_contains_unbounded_collection(value, inside_bounded)),
        _ => false,
    }
}

fn is_unbounded_collection(name: &str) -> bool {
    matches!(
        name,
        "Vec" | "BTreeMap" | "BTreeSet" | "BinaryHeap" | "LinkedList" | "VecDeque"
    )
}

fn is_bounded_collection(name: &str) -> bool {
    matches!(
        name,
        "BoundedVec" | "WeakBoundedVec" | "BoundedBTreeMap" | "BoundedBTreeSet"
    )
}

fn item_has_unbounded_attr(item: &Value) -> bool {
    item.get("attrs")
        .and_then(Value::as_array)
        .is_some_and(|attrs| {
            attrs
                .iter()
                .filter_map(Value::as_str)
                .any(|attr| attr.contains("pallet::unbounded"))
        })
}

fn docs_mark_capacity_limited(item: &Value) -> bool {
    let Some(docs) = item.get("docs").and_then(Value::as_str) else {
        return false;
    };
    let lower = docs.to_ascii_lowercase();
    let admits_unbounded = lower.contains("no global maximum")
        || lower.contains("no global bound")
        || lower.contains("unbounded");
    if admits_unbounded {
        return false;
    }
    lower.contains("capacity")
        && (lower.contains("limited")
            || lower.contains("bounded")
            || lower.contains("maximum")
            || lower.contains("max "))
}

fn span_location(item: &Value, source_root: Option<&Path>) -> (PathBuf, usize, Option<usize>) {
    let filename = item
        .pointer("/span/filename")
        .and_then(Value::as_str)
        .unwrap_or("<rustdoc-json>");
    let mut file = PathBuf::from(filename);
    if file.is_relative() {
        if let Some(root) = source_root {
            file = root.join(file);
        }
    }

    let begin = item.pointer("/span/begin").and_then(Value::as_array);
    let line = begin
        .and_then(|values| values.first())
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    let column = begin
        .and_then(|values| values.get(1))
        .and_then(Value::as_u64)
        .map(|value| value as usize);

    (file, line, column)
}
