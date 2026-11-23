//! The structural schema diff + patch (expressir#638) — a port of
//! `Expressir::Express::SchemaCompare` / `SchemaPatch`: declarations
//! match by canonical path `schema.item`; entity attributes get
//! sub-deltas at `schema.item.attribute`. `compare_json` is the
//! delegation entry (delta sequence, COLLECTIONS order, then added /
//! removed / changed per collection); `patch_json` applies deltas to
//! a deep copy of the base schema wire. Interfaces are out of scope
//! (the patch target is a flattened schema, which has none).

use serde_json::{json, Map, Value};

const SIGNATURE_IGNORE: &[&str] = &[
    "source",
    "include_source",
    "remarks",
    "remark_items",
    "ref",
    "base_path",
    "file",
    // The source order of nested declarations is no part of the signature.
    "declaration_order",
    // The ruby lane's to_hash carries none of these capture fields.
    "source_offset",
    "source_end_offset",
    "source_length",
];

const COLLECTIONS: &[&str] = &[
    "constants",
    "types",
    "entities",
    "subtype_constraints",
    "functions",
    "rules",
    "procedures",
];

fn schema_id(schema: &Value) -> String {
    schema
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase()
}

fn strip(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let out: Map<String, Value> = map
                .iter()
                .filter(|(key, _)| !SIGNATURE_IGNORE.contains(&key.as_str()))
                .map(|(key, inner)| (key.clone(), strip(inner)))
                .collect();
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(strip).collect()),
        other => other.clone(),
    }
}

fn signature(node: &Value) -> Value {
    strip(node)
}

/// The wire node class of a delta's node — the patch route (the ruby
/// NODE_COLLECTION).
fn node_collection(node: &Value) -> Option<&'static str> {
    let class = node.get("_class").and_then(Value::as_str)?;
    match class {
        "Expressir::Model::Declarations::Constant" => Some("constants"),
        "Expressir::Model::Declarations::Type" => Some("types"),
        "Expressir::Model::Declarations::Entity" => Some("entities"),
        "Expressir::Model::Declarations::SubtypeConstraint" => Some("subtype_constraints"),
        "Expressir::Model::Declarations::Function" => Some("functions"),
        "Expressir::Model::Declarations::Rule" => Some("rules"),
        "Expressir::Model::Declarations::Procedure" => Some("procedures"),
        _ => None,
    }
}

fn index_by_id(items: &[Value]) -> Vec<(String, usize)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let id = item.get("id").and_then(Value::as_str)?;
            if id.is_empty() {
                None
            } else {
                Some((id.to_lowercase(), i))
            }
        })
        .collect()
}

fn delta_json(op: &str, path: &str, node: Option<&Value>) -> Value {
    json!({
        "op": op,
        "path": path,
        "node": node.cloned().unwrap_or(Value::Null),
    })
}

/// The ruby `SchemaCompare.compare`: the delta sequence.
pub fn compare(base: &Value, revised: &Value) -> Vec<Value> {
    let mut deltas: Vec<Value> = Vec::new();
    for collection in COLLECTIONS {
        let base_items: Vec<Value> = base
            .get(collection)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let revised_items: Vec<Value> = revised
            .get(collection)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let prefix = format!("{}.", schema_id(base));
        let base_index = index_by_id(&base_items);
        let revised_index = index_by_id(&revised_items);
        for (key, i) in &revised_index {
            if !base_index.iter().any(|(k, _)| k == key) {
                deltas.push(delta_json(
                    "added",
                    &format!("{prefix}{key}"),
                    Some(&revised_items[*i]),
                ));
            }
        }
        for (key, _) in &base_index {
            if !revised_index.iter().any(|(k, _)| k == key) {
                deltas.push(delta_json("removed", &format!("{prefix}{key}"), None));
            }
        }
        for (key, old_i) in &base_index {
            let Some((_, new_i)) = revised_index.iter().find(|(k, _)| k == key) else {
                continue;
            };
            let old_item = &base_items[*old_i];
            let new_item = &revised_items[*new_i];
            if signature(old_item) == signature(new_item) {
                continue;
            }
            deltas.push(delta_json(
                "changed",
                &format!("{prefix}{key}"),
                Some(new_item),
            ));
            if new_item.get("_class").and_then(Value::as_str)
                == Some("Expressir::Model::Declarations::Entity")
            {
                deltas.extend(attribute_deltas(
                    old_item,
                    new_item,
                    &format!("{prefix}{key}"),
                ));
            }
        }
    }
    deltas
}

fn attribute_deltas(old_item: &Value, new_item: &Value, path: &str) -> Vec<Value> {
    let empty = Vec::new();
    let old_attrs = old_item
        .get("attributes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let new_attrs = new_item
        .get("attributes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let old_index = index_by_id(old_attrs);
    let new_index = index_by_id(new_attrs);
    let mut deltas: Vec<Value> = Vec::new();
    for (key, i) in &new_index {
        if !old_index.iter().any(|(k, _)| k == key) {
            deltas.push(delta_json(
                "added",
                &format!("{path}.{}", key),
                Some(&new_attrs[*i]),
            ));
        }
    }
    for (key, _) in &old_index {
        if !new_index.iter().any(|(k, _)| k == key) {
            deltas.push(delta_json("removed", &format!("{path}.{key}"), None));
        }
    }
    for (key, old_i) in &old_index {
        let Some((_, new_i)) = new_index.iter().find(|(k, _)| k == key) else {
            continue;
        };
        if signature(&old_attrs[*old_i]) == signature(&new_attrs[*new_i]) {
            continue;
        }
        deltas.push(delta_json(
            "changed",
            &format!("{path}.{key}"),
            Some(&new_attrs[*new_i]),
        ));
    }
    deltas
}

/// The delegation entry: two schema wires in, the delta sequence out.
pub fn compare_json(base_json: &str, revised_json: &str) -> Result<String, String> {
    let base: Value = serde_json::from_str(base_json).map_err(|e| format!("base json: {e}"))?;
    let revised: Value =
        serde_json::from_str(revised_json).map_err(|e| format!("revised json: {e}"))?;
    let deltas = compare(&base, &revised);
    serde_json::to_string(&json!({ "deltas": deltas })).map_err(|e| format!("result json: {e}"))
}

/// The ruby `SchemaPatch.apply`: deltas applied to a deep copy.
pub fn patch(base: &Value, deltas: &[Value]) -> Value {
    let mut revised = base.clone();
    let item_deltas: Vec<&Value> = deltas
        .iter()
        .filter(|d| {
            d.get("path")
                .and_then(Value::as_str)
                .map(|p| p.split('.').count())
                == Some(2)
        })
        .collect();
    for delta in &item_deltas {
        apply_item(&mut revised, delta);
    }
    let replaced: Vec<String> = item_deltas
        .iter()
        .filter(|d| {
            matches!(
                d.get("op").and_then(Value::as_str),
                Some("added") | Some("changed")
            )
        })
        .filter_map(|d| {
            d.get("path")
                .and_then(Value::as_str)
                .map(|p| p.split('.').take(2).collect::<Vec<_>>().join("."))
        })
        .collect();
    for delta in deltas {
        let path = delta.get("path").and_then(Value::as_str).unwrap_or("");
        if path.split('.').count() != 3 {
            continue;
        }
        let head = path.split('.').take(2).collect::<Vec<_>>().join(".");
        if replaced.contains(&head) {
            continue;
        }
        apply_attribute(&mut revised, delta);
    }
    revised
}

fn item_key(item: &Value) -> String {
    item.get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase()
}

fn live_collection<'a>(schema: &'a mut Value, wanted: &str) -> Option<&'a mut Vec<Value>> {
    for collection in COLLECTIONS {
        let found = schema
            .get(*collection)
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(|item| item_key(item) == wanted));
        if found {
            return schema.get_mut(*collection).and_then(Value::as_array_mut);
        }
    }
    None
}

fn apply_item(schema: &mut Value, delta: &Value) {
    let op = delta.get("op").and_then(Value::as_str).unwrap_or("");
    let path = delta.get("path").and_then(Value::as_str).unwrap_or("");
    let key = path.rsplit('.').next().unwrap_or("").to_lowercase();
    if op == "added" {
        let Some(node) = delta.get("node") else {
            return;
        };
        let Some(collection) = node_collection(node) else {
            return;
        };
        if let Some(items) = schema.get_mut(collection).and_then(Value::as_array_mut) {
            items.push(node.clone());
        }
        return;
    }
    let Some(collection) = live_collection(schema, &key) else {
        return;
    };
    match op {
        "removed" => collection.retain(|item| item_key(item) != key),
        "changed" => {
            let Some(node) = delta.get("node") else {
                return;
            };
            if let Some(index) = collection.iter().position(|item| item_key(item) == key) {
                collection[index] = node.clone();
            }
        }
        _ => {}
    }
}

fn apply_attribute(schema: &mut Value, delta: &Value) {
    let op = delta.get("op").and_then(Value::as_str).unwrap_or("");
    let path = delta.get("path").and_then(Value::as_str).unwrap_or("");
    let segments: Vec<&str> = path.split('.').collect();
    let (Some(wanted), Some(attr_key)) = (segments.get(1), segments.get(2)) else {
        return;
    };
    let wanted = wanted.to_lowercase();
    let attr_name = attr_key.to_lowercase();
    for collection in COLLECTIONS {
        let Some(items) = schema.get_mut(*collection).and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(index) = items.iter().position(|item| item_key(item) == wanted) else {
            continue;
        };
        let item = &mut items[index];
        let attrs: Vec<Value> = item
            .get("attributes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let next = match op {
            "added" => delta.get("node").map(|node| {
                let mut next = attrs;
                next.push(node.clone());
                next
            }),
            "removed" => Some(
                attrs
                    .into_iter()
                    .filter(|a| item_key(a) != attr_name)
                    .collect(),
            ),
            "changed" => delta.get("node").and_then(|node| {
                attrs
                    .iter()
                    .position(|a| item_key(a) == attr_name)
                    .map(|position| {
                        let mut next = attrs;
                        next[position] = node.clone();
                        next
                    })
            }),
            _ => None,
        };
        if let Some(next) = next {
            item["attributes"] = Value::Array(next);
            return;
        }
    }
}

/// The delegation entry: a base schema wire + the delta sequence in,
/// the revised schema wire out.
pub fn patch_json(base_json: &str, deltas_json: &str) -> Result<String, String> {
    let base: Value = serde_json::from_str(base_json).map_err(|e| format!("base json: {e}"))?;
    let deltas: Vec<Value> = serde_json::from_str::<Value>(deltas_json)
        .map_err(|e| format!("deltas json: {e}"))?
        .get("deltas")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let revised = patch(&base, &deltas);
    serde_json::to_string(&revised).map_err(|e| format!("result json: {e}"))
}
