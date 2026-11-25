//! The expansion family (lutaml/expressir#636): the concatenated
//! artifact (expressir#247, eeng's `--concat_schema`) — the interface
//! closure of a root schema and the banner writer. The SHTOLO longform
//! (flatten) builds on the closure.

use serde_json::Value;

/// The wire reference of an interface's target: a resolved
/// `References::SimpleReference` (`{"id": ..}`), or the pre-resolution
/// plain string.
fn interface_target_name(iface: &Value) -> Option<&str> {
    let schema = iface.get("schema")?;
    if let Some(name) = schema.as_str() {
        return Some(name);
    }
    if schema
        .get("_class")
        .and_then(Value::as_str)?
        .ends_with("::SimpleReference")
    {
        return schema.get("id").and_then(Value::as_str);
    }
    None
}

/// One concatenated section: the schema's name, file and version.
pub struct Section {
    pub id: String,
    pub file: Option<String>,
    pub version: Option<String>,
}

/// The interface closure of `root_id`, deduplicated by lowercased name
/// and sorted alphabetically — the exact set and order a concatenated
/// artifact contains. `repository` must hold every schema the closure
/// can reach (the wire of a resolved load).
pub fn closure(root_id: &str, repository: &Value) -> Vec<Section> {
    let lower = |s: &str| s.to_lowercase();
    let by_name: std::collections::HashMap<String, &Value> = repository
        .get("schemas")
        .and_then(Value::as_array)
        .map(|schemas| {
            schemas
                .iter()
                .filter_map(|s| s.get("id").and_then(Value::as_str).map(|id| (lower(id), s)))
                .collect()
        })
        .unwrap_or_default();

    let mut visited: Vec<&Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<&Value> = std::collections::VecDeque::new();
    if let Some(root) = by_name.get(root_id.to_lowercase().as_str()) {
        queue.push_back(*root);
    }
    while let Some(schema) = queue.pop_front() {
        let Some(id) = schema.get("id").and_then(Value::as_str) else {
            continue;
        };
        let key = lower(id);
        if !seen.insert(key) {
            continue;
        }
        visited.push(schema);
        if let Some(Value::Array(interfaces)) = schema.get("interfaces") {
            for iface in interfaces {
                if let Some(name) = interface_target_name(iface) {
                    if let Some(target) = by_name.get(name.to_lowercase().as_str()) {
                        queue.push_back(target);
                    }
                }
            }
        }
    }
    visited.sort_by_key(|s| {
        s.get("id")
            .and_then(Value::as_str)
            .map(|id| id.to_lowercase())
            .unwrap_or_default()
    });
    visited
        .into_iter()
        .map(|s| Section {
            id: s
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            file: s.get("file").and_then(Value::as_str).map(str::to_string),
            version: s
                .get("version")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect()
}

/// Writes the artifact: a provenance banner, the schema index (name,
/// file, version), then each source file verbatim behind a
/// `-- NAME (file)` separator. Errors name the file that failed.
pub fn write(schemas: &[Section]) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("(* Concatenated File produced by expressir\n\n");
    out.push_str(&format!(
        "{} Schemata for Concatenated File\n\n",
        schemas.len()
    ));
    for schema in schemas {
        out.push_str(&schema.id.to_lowercase());
        out.push('\n');
        out.push_str(&format!(
            "   Filename: {}\n",
            schema.file.as_deref().unwrap_or("")
        ));
        if let Some(version) = &schema.version {
            out.push_str(&format!("   Version:   {version}\n"));
        }
        out.push('\n');
    }
    out.push_str(" *)\n\n");
    for schema in schemas {
        out.push_str("--\n");
        out.push_str(&format!(
            "-- {} ({})\n",
            schema.id.to_uppercase(),
            schema.file.as_deref().unwrap_or("")
        ));
        out.push_str("--\n");
        let Some(file) = &schema.file else {
            return Err(format!("schema {} carries no file to embed", schema.id));
        };
        match std::fs::read_to_string(file) {
            Ok(text) => out.push_str(&text),
            Err(e) => return Err(format!("{file}: {e}")),
        }
        out.push('\n');
        out.push('\n');
    }
    Ok(out)
}

/// One-call convenience over a hydrated wire: the closure of `root_id`
/// written to `path`; the sections read the files the wire names.
pub fn concat_schema(root_id: &str, repository: &Value, path: &str) -> Result<Vec<String>, String> {
    let schemas = closure(root_id, repository);
    std::fs::write(path, write(&schemas)?).map_err(|e| format!("{path}: {e}"))?;
    Ok(schemas.into_iter().map(|s| s.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repository() -> Value {
        json!({
            "schemas": [
                { "id": "mim", "file": "/x/mim.exp", "interfaces": [
                    { "_class": "Expressir::Model::Declarations::Interface",
                      "schema": { "_class": "Expressir::Model::References::SimpleReference", "id": "ARM" } } ] },
                { "id": "arm", "file": "/x/arm.exp", "interfaces": [
                    { "schema": "support_resource" } ] },
                { "id": "support_resource", "file": "/x/sr.exp", "interfaces": [] },
                { "id": "unrelated", "file": "/x/u.exp", "interfaces": [] }
            ]
        })
    }

    #[test]
    fn closure_walks_and_sorts() {
        let ids: Vec<String> = closure("mim", &repository())
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["arm", "mim", "support_resource"]);
    }

    #[test]
    fn missing_root_yields_nothing() {
        assert!(closure("nope", &repository()).is_empty());
    }

    #[test]
    fn writer_embeds_sections() {
        let dir = std::env::temp_dir().join("concat_probe");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.exp");
        std::fs::write(&file, "SCHEMA a;\nEND_SCHEMA;\n").unwrap();
        let sections = vec![Section {
            id: "a".into(),
            file: Some(file.display().to_string()),
            version: Some("1".into()),
        }];
        let out = write(&sections).unwrap();
        assert!(out.contains("1 Schemata for Concatenated File"));
        assert!(out.contains("-- A ("));
        assert!(out.contains("SCHEMA a;\nEND_SCHEMA;\n"));
    }
}

// ---- the SHTOLO longform (G.1 stage 1) ----

/// The declaration collections a schema contributes to a longform, in
/// the artifact order the ruby Shtolo writes.
const DECL_COLLECTIONS: [&str; 7] = [
    "constants",
    "types",
    "entities",
    "subtype_constraints",
    "functions",
    "procedures",
    "rules",
];

/// All declarations of a schema wire, in the artifact order.
fn all_decls(schema: &Value) -> Vec<&Value> {
    DECL_COLLECTIONS
        .iter()
        .filter_map(|key| schema.get(*key).and_then(Value::as_array))
        .flatten()
        .collect()
}

fn decl_id(decl: &Value) -> Option<&str> {
    decl.get("id").and_then(Value::as_str)
}

/// G.NM.1 case-fold: the replacement carries the munge map's casing
/// unless the current id is SHOUTING (ruby preserve_case).
fn preserve_case(current: &str, new: &str) -> String {
    if current == current.to_uppercase() && current.chars().any(|c| c.is_alphabetic()) {
        new.to_uppercase()
    } else {
        new.to_string()
    }
}

/// Rewrites every SimpleReference id on `node` through `map`
/// (`rewrite_references` over the wire): the walk descends into every
/// object and array, and only reference nodes carry the replacement.
fn rewrite_references(node: &mut Value, map: &std::collections::HashMap<String, String>) {
    match node {
        Value::Object(map_obj) => {
            let is_ref = map_obj
                .get("_class")
                .and_then(Value::as_str)
                .is_some_and(|c| c.ends_with("::SimpleReference"));
            if is_ref {
                if let Some(id) = map_obj.get("id").and_then(Value::as_str) {
                    if let Some(to) = map.get(&id.to_lowercase()) {
                        let preserved = preserve_case(id, to);
                        map_obj.insert("id".into(), Value::from(preserved));
                    }
                }
            }
            for (_key, value) in map_obj.iter_mut() {
                rewrite_references(value, map);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_references(item, map);
            }
        }
        _ => {}
    }
}

/// The interface alias map of a schema wire: `USE FROM s (orig AS
/// alias)` items map alias → original (G.1.3).
fn renames_for(schema: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(Value::Array(interfaces)) = schema.get("interfaces") {
        for iface in interfaces {
            if let Some(Value::Array(items)) = iface.get("items") {
                for item in items {
                    let alias = item.get("id").and_then(Value::as_str);
                    let original = match item.get("ref") {
                        Some(Value::String(name)) => Some(name.as_str()),
                        Some(reference) => reference.get("id").and_then(Value::as_str),
                        None => None,
                    };
                    if let (Some(alias), Some(original)) = (alias, original) {
                        if !alias.eq_ignore_ascii_case(original) {
                            out.push((alias.to_lowercase(), original.to_string()));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Builds the G.1 stage-1 longform schema wire: the closure copied in
/// order [root, then the rest], every clashing declaration id munged
/// `<schema>_dot_<id>` (G.NM.1), interface aliases resolved, and every
/// reference rewritten through the schema's resolution order. The G.2
/// transformations (extensible folding, subtype-constraint
/// elimination, abstract-entity conversion, prune, provenance) are the
/// ruby Shtolo's later stages, not this slice.
pub fn flatten(root_id: &str, repository: &Value, longform_name: &str) -> Value {
    let lower = |s: &str| s.to_lowercase();

    // The closure (same walk as `closure`), ordered [root, then rest].
    let by_name: std::collections::HashMap<String, &Value> = repository
        .get("schemas")
        .and_then(Value::as_array)
        .map(|schemas| {
            schemas
                .iter()
                .filter_map(|s| s.get("id").and_then(Value::as_str).map(|id| (lower(id), s)))
                .collect()
        })
        .unwrap_or_default();
    let mut order: Vec<&Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<&Value> = std::collections::VecDeque::new();
    if let Some(root) = by_name.get(root_id.to_lowercase().as_str()) {
        queue.push_back(*root);
    }
    while let Some(schema) = queue.pop_front() {
        let Some(id) = schema.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !seen.insert(lower(id)) {
            continue;
        }
        order.push(schema);
        if let Some(Value::Array(interfaces)) = schema.get("interfaces") {
            for iface in interfaces {
                if let Some(name) = interface_target_name(iface) {
                    if let Some(target) = by_name.get(name.to_lowercase().as_str()) {
                        queue.push_back(target);
                    }
                }
            }
        }
    }

    // G.NM.1: the clash set over the whole closure, then the munge map.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for schema in &order {
        for decl in all_decls(schema) {
            if let Some(id) = decl_id(decl) {
                *counts.entry(lower(id)).or_insert(0) += 1;
            }
        }
    }
    let mut global: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut own_by_schema: std::collections::HashMap<
        String,
        std::collections::HashMap<String, String>,
    > = std::collections::HashMap::new();
    for schema in &order {
        let Some(schema_id) = schema.get("id").and_then(Value::as_str) else {
            continue;
        };
        let mut own = std::collections::HashMap::new();
        for decl in all_decls(schema) {
            let Some(id) = decl_id(decl) else {
                continue;
            };
            let key = lower(id);
            let final_id = if counts[&key] > 1 {
                let munged = format!("{}_dot_{}", lower(schema_id), id);
                global.insert(key.clone(), munged.clone());
                munged
            } else {
                id.to_string()
            };
            own.insert(key, final_id);
        }
        own_by_schema.insert(lower(schema_id), own);
    }

    // Copy each schema's declarations through its resolution order.
    let mut buckets: std::collections::HashMap<&str, Vec<Value>> = std::collections::HashMap::new();
    for schema in &order {
        let Some(schema_id) = schema.get("id").and_then(Value::as_str) else {
            continue;
        };
        let own = own_by_schema
            .get(&lower(schema_id))
            .cloned()
            .unwrap_or_default();
        let mut full = global.clone();
        for (key, final_id) in &own {
            full.insert(key.clone(), final_id.clone());
        }
        for (alias, original) in renames_for(schema) {
            let resolved = own
                .get(&original.to_lowercase())
                .cloned()
                .or_else(|| global.get(&original.to_lowercase()).cloned())
                .unwrap_or(original);
            full.insert(alias, resolved);
        }

        for key in DECL_COLLECTIONS {
            if let Some(Value::Array(decls)) = schema.get(key) {
                for decl in decls {
                    let mut copy = decl.clone();
                    rewrite_references(&mut copy, &full);
                    if let Some(id) = decl_id(decl) {
                        if let Some(final_id) = own.get(&lower(id)) {
                            if let Some(slot) = copy.get_mut("id") {
                                *slot = Value::from(preserve_case(id, final_id));
                            }
                        }
                    }
                    buckets.entry(key).or_default().push(copy);
                }
            }
        }
    }

    // The longform schema: interfaces dissolved (G.1.2).
    let mut out = serde_json::Map::new();
    out.insert(
        "_class".into(),
        Value::from("Expressir::Model::Declarations::Schema"),
    );
    out.insert("id".into(), Value::from(longform_name));
    out.insert("interfaces".into(), Value::Array(Vec::new()));
    for key in DECL_COLLECTIONS {
        let items = buckets.remove(key).unwrap_or_default();
        out.insert((*key).to_string(), Value::Array(items));
    }
    Value::Object(out)
}

#[cfg(test)]
mod flatten_tests {
    use super::*;
    use serde_json::json;

    fn repository() -> Value {
        json!({
            "schemas": [
                { "id": "farming", "interfaces": [
                    { "_class": "Expressir::Model::Declarations::Interface",
                      "schema": { "_class": "Expressir::Model::References::SimpleReference", "id": "pet_shows" },
                      "items": [ { "id": "dog", "ref": { "_class": "Expressir::Model::References::SimpleReference", "id": "dog" } } ] } ],
                  "entities": [
                    { "_class": "Expressir::Model::Declarations::Entity", "id": "dog",
                      "attributes": [] },
                    { "_class": "Expressir::Model::Declarations::Entity", "id": "dog_house",
                      "attributes": [ { "type": { "_class": "Expressir::Model::References::SimpleReference", "id": "dog" } } ] } ] },
                { "id": "pet_shows", "entities": [ { "_class": "Expressir::Model::Declarations::Entity", "id": "dog",
                                                     "attributes": [] } ] }
            ]
        })
    }

    #[test]
    fn clashing_ids_munge_with_the_schema_prefix() {
        // G.NM.1: both `dog`s clash, so both carry `<schema>_dot_<id>`.
        let lf = flatten("farming", &repository(), "longform");
        let entities = lf.get("entities").and_then(Value::as_array).unwrap();
        let ids: Vec<&str> = entities
            .iter()
            .filter_map(|e| e.get("id").and_then(Value::as_str))
            .collect();
        // G.NM.1: both `dog`s munge; the unique `dog_house` keeps its name.
        assert!(ids.contains(&"farming_dot_dog"));
        assert!(ids.contains(&"pet_shows_dot_dog"));
        assert!(ids.contains(&"dog_house"));
        // The aliased USE import resolves through the original's final id.
        // The attribute inside farming's dog_house references farming's
        // own dog — resolved through farming's own map, not the alias.
        let house = entities
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some("dog_house"))
            .unwrap();
        let attr_type = &house["attributes"][0]["type"];
        assert_eq!(
            attr_type.get("id").and_then(Value::as_str),
            Some("farming_dot_dog")
        );
    }

    #[test]
    fn interfaces_dissolve() {
        let lf = flatten("farming", &repository(), "longform");
        assert_eq!(
            lf.get("interfaces")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn unique_ids_keep_their_names() {
        let lf = flatten("pet_shows", &repository(), "longform");
        let entities = lf.get("entities").and_then(Value::as_array).unwrap();
        let ids: Vec<&str> = entities
            .iter()
            .filter_map(|e| e.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, ["dog"]);
    }
}
