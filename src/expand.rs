//! The expansion family (lutaml/expressir#636): the concatenated
//! artifact (expressir#247, eeng's `--concat_schema`) — the interface
//! closure of a root schema and the banner writer. The SHTOLO longform
//! (flatten) builds on the closure.

use crate::ParsedTree;
use serde_json::Value;

// json! through the fully qualified macro path works on 2018+.
use serde_json::json;

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

// ---- the SHTOLO G.2: subtype-constraint elimination ----

/// Eliminates a longform's SUBTYPE_CONSTRAINTs (ruby
/// `eliminate_subtype_constraints!`): each constraint ANDOR-combines
/// its supertype expression into the `applies_to` entity's own,
/// propagates ABSTRACT, turns TOTAL_OVER into a `total_over_<id>`
/// rule (the annex snippet, parsed through the engine), and empties
/// the constraint list.
pub fn eliminate_subtype_constraints(longform: &mut Value) {
    let Some(Value::Array(constraints)) = longform.get_mut("subtype_constraints") else {
        return;
    };
    let constraints = std::mem::take(constraints);
    let mut extra_rules: Vec<Value> = Vec::new();

    for sc in &constraints {
        let Some(applies) = sc.get("applies_to") else {
            continue;
        };
        let Some(applies_id) = applies.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(Value::Array(entities)) = longform.get_mut("entities") else {
            continue;
        };
        let Some(entity) = entities.iter_mut().find(|e| {
            e.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.eq_ignore_ascii_case(applies_id))
        }) else {
            continue;
        };

        if let Some(expr) = sc.get("supertype_expression").filter(|v| !v.is_null()) {
            let combined = match entity.get_mut("supertype_expression") {
                Some(existing) if !existing.is_null() => {
                    let mut binary = serde_json::Map::new();
                    binary.insert(
                        "_class".into(),
                        Value::from(
                            "Expressir::Model::SupertypeExpressions::BinarySupertypeExpression",
                        ),
                    );
                    binary.insert("operator".into(), Value::from("ANDOR"));
                    binary.insert("operand1".into(), existing.clone());
                    binary.insert("operand2".into(), expr.clone());
                    Value::Object(binary)
                }
                _ => expr.clone(),
            };
            if let Some(obj) = entity.as_object_mut() {
                obj.insert("supertype_expression".into(), combined);
            }
        }
        if sc.get("abstract").and_then(Value::as_bool) == Some(true) {
            if let Some(obj) = entity.as_object_mut() {
                obj.insert("abstract".into(), Value::from(true));
            }
        }

        let total_over = sc
            .get("total_over")
            .and_then(Value::as_array)
            .map(|r| {
                r.iter()
                    .filter_map(|t| t.get("id").and_then(Value::as_str))
                    .map(|id| id.to_uppercase())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if total_over.is_empty() {
            continue;
        }
        let sc_id = sc.get("id").and_then(Value::as_str).unwrap_or("sc");
        let rule_name = format!("total_over_{sc_id}");
        let supertype = applies_id.to_uppercase();
        let supertype_lower = supertype.to_lowercase();
        let longform_upper = longform
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("LONGFORM")
            .to_uppercase();
        let target = total_over.first().cloned().unwrap_or_default();
        let snippet = format!(
            "SCHEMA __to__;\nRULE {rule_name} FOR ({supertype});\nWHERE\n  wr1: SIZEOF(QUERY({supertype_lower}_i <* {supertype} |\n    SIZEOF(['{longform_upper}.{target}'] * TYPEOF({supertype_lower}_i)) = 0)) = 0;\nEND_RULE;\nEND_SCHEMA;\n"
        );
        if let Ok(tree) = ParsedTree::parse(&snippet) {
            if let Ok(wire) = tree.to_model_json("snippet.exp") {
                if let Some(rule) = wire.pointer("/schemas/0/rules/0").cloned() {
                    let mut rule = rule;
                    if let Some(slot) = rule.get_mut("id") {
                        *slot = Value::from(rule_name.clone());
                    }
                    extra_rules.push(rule);
                }
            }
        }
    }

    if let Some(obj) = longform.as_object_mut() {
        if let Some(Value::Array(rules)) = obj.get_mut("rules") {
            rules.extend(extra_rules);
        } else if !extra_rules.is_empty() {
            obj.insert("rules".into(), Value::Array(extra_rules));
        }
        obj.insert("subtype_constraints".into(), Value::Array(Vec::new()));
    }
}

#[cfg(test)]
mod eliminate_tests {
    use super::*;
    use serde_json::json;

    fn longform() -> Value {
        json!({
            "_class": "Expressir::Model::Declarations::Schema",
            "id": "longform",
            "subtype_constraints": [
                { "_class": "Expressir::Model::Declarations::SubtypeConstraint",
                  "id": "sc_a", "applies_to": { "id": "piece" },
                  "abstract": true,
                  "supertype_expression": { "_class": "Expressir::Model::SupertypeExpressions::OneofSupertypeExpression",
                                            "operands": [ { "_class": "Expressir::Model::References::SimpleReference", "id": "a" } ] },
                  "total_over": [ { "id": "piece" } ] }
            ],
            "entities": [
                { "_class": "Expressir::Model::Declarations::Entity", "id": "piece" }
            ],
            "rules": []
        })
    }

    #[test]
    fn constraints_fold_into_the_entity() {
        let mut lf = longform();
        eliminate_subtype_constraints(&mut lf);
        assert_eq!(
            lf.get("subtype_constraints")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            0
        );
        let piece = &lf["entities"][0];
        assert_eq!(piece.get("abstract").and_then(Value::as_bool), Some(true));
        let expr = piece.get("supertype_expression").unwrap();
        assert!(expr
            .get("_class")
            .and_then(Value::as_str)
            .is_some_and(|c| c.ends_with("OneofSupertypeExpression")));
    }

    #[test]
    fn total_over_becomes_a_rule() {
        let mut lf = longform();
        eliminate_subtype_constraints(&mut lf);
        let rules = lf.get("rules").and_then(Value::as_array).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].get("id").and_then(Value::as_str),
            Some("total_over_sc_a")
        );
        // The wire's applies_to is an array of references.
        assert_eq!(
            rules[0]["applies_to"][0].get("id").and_then(Value::as_str),
            Some("PIECE")
        );
    }
}

// ---- the SHTOLO G.2: prune + the entity conversions ----

/// Every declaration id of the longform (`visible_declaration_ids`).
fn visible_ids(longform: &Value) -> std::collections::HashSet<String> {
    all_decls(longform)
        .into_iter()
        .filter_map(decl_id)
        .map(|id| id.to_lowercase())
        .collect()
}

/// The reference id of a wire node: a plain string or a SimpleReference.
fn reference_id(node: &Value) -> Option<&str> {
    if let Some(name) = node.as_str() {
        return Some(name);
    }
    // The ruby `ref.id` works on every node that carries one.
    node.get("id").and_then(Value::as_str)
}

/// Collects `out` with the ids a wire node names at call sites or as
/// references — FunctionCall's `function`, ProcedureCall's
/// `procedure`, and ANY SimpleReference (a zero-argument function used
/// bare parses as a plain reference; over-retaining is safe).
fn collect_called(node: &Value, out: &mut std::collections::HashSet<String>) {
    if let Value::Object(obj) = node {
        let class = obj.get("_class").and_then(Value::as_str).unwrap_or("");
        let id_key = if class.ends_with("::FunctionCall") {
            Some("function")
        } else if class.ends_with("::ProcedureCall") {
            Some("procedure")
        } else {
            None
        };
        if let Some(key) = id_key {
            if let Some(id) = reference_id(obj.get(key).unwrap_or(&Value::Null)) {
                out.insert(id.to_lowercase());
            }
        }
        if class.ends_with("::SimpleReference") {
            if let Some(id) = obj.get("id").and_then(Value::as_str) {
                out.insert(id.to_lowercase());
            }
        }
        for (_key, value) in obj.iter() {
            collect_called(value, out);
        }
    } else if let Value::Array(items) = node {
        for item in items {
            collect_called(item, out);
        }
    }
}

/// Prunes the longform (ruby `prune!`): rules whose parameter entities
/// are invisible, select items pointing at invisible declarations (an
/// emptied select dissolves the type's representation), and
/// functions/procedures nothing calls, to fixpoint.
pub fn prune(longform: &mut Value) {
    let visible = visible_ids(longform);

    if let Some(Value::Array(rules)) = longform.get_mut("rules") {
        rules.retain(|rule| match rule.get("applies_to") {
            Some(Value::Array(refs)) => refs
                .iter()
                .all(|r| reference_id(r).is_none_or(|id| visible.contains(&id.to_lowercase()))),
            _ => true,
        });
    }

    if let Some(Value::Array(types)) = longform.get_mut("types") {
        for t in types.iter_mut() {
            let Some(underlying) = t.get_mut("underlying_type") else {
                continue;
            };
            let is_select = underlying
                .get("_class")
                .and_then(Value::as_str)
                .is_some_and(|c| c.ends_with("::Select"));
            if !is_select {
                continue;
            }
            if let Some(Value::Array(items)) = underlying.get_mut("items") {
                let before = items.len();
                items.retain(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .is_none_or(|id| visible.contains(&id.to_lowercase()))
                });
                // An emptied select disappears from the representation
                // (annex G.1.9 s1/s2/s3 example).
                if before > 0 && items.is_empty() {
                    if let Some(obj) = t.as_object_mut() {
                        obj.insert("underlying_type".into(), Value::Null);
                    }
                }
            }
        }
        // An emptied select dissolves the type's representation; a type
        // without an underlying_type drops from the artifact.
        types.retain(|t| {
            let ut = t.get("underlying_type");
            !(ut.is_none_or(Value::is_null))
        });
    }

    loop {
        let mut called = std::collections::HashSet::new();
        collect_called(longform, &mut called);
        let mut dropped = false;
        for key in ["functions", "procedures"] {
            if let Some(Value::Array(list)) = longform.get_mut(key) {
                let before = list.len();
                list.retain(|item| {
                    decl_id(item).is_some_and(|id| called.contains(&id.to_lowercase()))
                });
                dropped |= list.len() != before;
            }
        }
        if !dropped {
            break;
        }
    }
}

/// Reduces a supertype expression over the visible ids (ruby
/// `reduce_supertype_expression`): invisible operands leave ONEOF,
/// a single-operand ONEOF collapses, a nil side drops from AND/OR,
/// and a vacuous expression returns None (the constraint deletes).
fn reduce_supertype_expression(
    expr: &Value,
    visible: &std::collections::HashSet<String>,
) -> Option<Value> {
    let class = expr.get("_class").and_then(Value::as_str).unwrap_or("");
    if class.ends_with("::OneofSupertypeExpression") {
        let mut reduced = Vec::new();
        for operand in expr
            .get("operands")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
        {
            if let Some(r) = reduce_supertype_expression(operand, visible) {
                reduced.push(r);
            }
        }
        return match reduced.len() {
            0 => None,
            1 => reduced.into_iter().next(),
            _ => {
                let mut copy = expr.clone();
                if let Some(obj) = copy.as_object_mut() {
                    obj.insert("operands".into(), Value::Array(reduced));
                }
                Some(copy)
            }
        };
    }
    if class.ends_with("::BinarySupertypeExpression") {
        let left = expr
            .get("operand1")
            .map(|o| reduce_supertype_expression(o, visible))
            .unwrap_or(None);
        let right = expr
            .get("operand2")
            .map(|o| reduce_supertype_expression(o, visible))
            .unwrap_or(None);
        return match (left, right) {
            (Some(l), Some(r)) => {
                let mut copy = expr.clone();
                if let Some(obj) = copy.as_object_mut() {
                    obj.insert("operand1".into(), l);
                    obj.insert("operand2".into(), r);
                }
                Some(copy)
            }
            (Some(l), None) => Some(l),
            (None, Some(r)) => Some(r),
            (None, None) => None,
        };
    }
    if class.ends_with("::SimpleReference") {
        if let Some(id) = expr.get("id").and_then(Value::as_str) {
            if !visible.contains(&id.to_lowercase()) {
                return None;
            }
        }
    }
    Some(expr.clone())
}

/// Prunes supertype expressions over the visible ids (ruby
/// `prune_supertype_expressions!`): a vacuous expression deletes its
/// constraint; total-over-only constraints stay for stage 2.
pub fn prune_supertype_expressions(longform: &mut Value) {
    let visible = visible_ids(longform);
    let Some(Value::Array(constraints)) = longform.get_mut("subtype_constraints") else {
        return;
    };
    let mut kept = Vec::new();
    for constraint in constraints.iter_mut() {
        match constraint.get("supertype_expression") {
            None | Some(Value::Null) => kept.push(constraint.clone()),
            Some(expr) => match reduce_supertype_expression(expr, &visible) {
                None => {}
                Some(reduced) => {
                    if let Some(obj) = constraint.as_object_mut() {
                        obj.insert("supertype_expression".into(), reduced);
                    }
                    kept.push(constraint.clone());
                }
            },
        }
    }
    *constraints = kept;
}

/// The entity conversions (ruby `convert_abstract_entities!` /
/// `convert_generic_entities!`): a bare ABSTRACT entity writes ABSTRACT
/// SUPERTYPE in the longform (`abstract_entity` drops), and every
/// GenericEntity in the tree becomes a plain Generic.
pub fn convert_entities(longform: &mut Value) {
    if let Some(Value::Array(entities)) = longform.get_mut("entities") {
        for entity in entities {
            if entity.get("abstract").and_then(Value::as_bool) == Some(true) {
                if let Some(obj) = entity.as_object_mut() {
                    obj.insert("abstract_entity".into(), Value::Null);
                }
            }
        }
    }
    convert_generics(longform);
}

fn convert_generics(node: &mut Value) {
    match node {
        Value::Object(obj) => {
            let is_generic_entity = obj
                .get("_class")
                .and_then(Value::as_str)
                .is_some_and(|c| c.ends_with("::GenericEntity"));
            if is_generic_entity {
                let mut generic = serde_json::Map::new();
                generic.insert(
                    "_class".into(),
                    Value::from("Expressir::Model::DataTypes::Generic"),
                );
                *node = Value::Object(generic);
                return;
            }
            for (_key, value) in obj.iter_mut() {
                convert_generics(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                convert_generics(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;
    use serde_json::json;

    fn longform() -> Value {
        json!({
            "_class": "Expressir::Model::Declarations::Schema",
            "id": "longform",
            "entities": [
                { "_class": "Expressir::Model::Declarations::Entity", "id": "piece",
                  "attributes": [ { "type": { "_class": "Expressir::Model::References::SimpleReference", "id": "kind" } } ] }
            ],
            "types": [
                { "_class": "Expressir::Model::Declarations::Type", "id": "kind",
                  "underlying_type": { "_class": "Expressir::Model::DataTypes::Select",
                                       "items": [ { "id": "piece" }, { "id": "gone" } ] } },
                { "_class": "Expressir::Model::Declarations::Type", "id": "doomed",
                  "underlying_type": { "_class": "Expressir::Model::DataTypes::Select",
                                       "items": [ { "id": "gone" } ] } }
            ],
            "functions": [
                { "_class": "Expressir::Model::Declarations::Function", "id": "used" },
                { "_class": "Expressir::Model::Declarations::Function", "id": "unused",
                  "statements": [ { "_class": "Expressir::Model::Statements::Call",
                                    "procedure": { "_class": "Expressir::Model::References::SimpleReference", "id": "also_unused" } } ] }
            ],
            "procedures": [
                { "_class": "Expressir::Model::Declarations::Procedure", "id": "also_unused" }
            ],
            "rules": [
                { "_class": "Expressir::Model::Declarations::Rule", "id": "kept",
                  "applies_to": [ { "id": "piece" } ],
                  "where_rules": [ { "expression": { "_class": "Expressir::Model::Expressions::FunctionCall",
                                                     "function": { "_class": "Expressir::Model::References::SimpleReference", "id": "used" } } } ] },
                { "_class": "Expressir::Model::Declarations::Rule", "id": "dropped",
                  "applies_to": [ { "id": "invisible" } ] }
            ],
            "subtype_constraints": []
        })
    }

    #[test]
    fn prune_drops_invisible_rules_and_select_items() {
        let mut lf = longform();
        prune(&mut lf);
        let rules = lf.get("rules").and_then(Value::as_array).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].get("id").and_then(Value::as_str), Some("kept"));
        let types = lf.get("types").and_then(Value::as_array).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].get("id").and_then(Value::as_str), Some("kind"));
        let items = types[0]["underlying_type"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].get("id").and_then(Value::as_str), Some("piece"));
    }

    #[test]
    fn prune_drops_uncalled_callables_to_fixpoint() {
        let mut lf = longform();
        prune(&mut lf);
        let functions = lf.get("functions").and_then(Value::as_array).unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].get("id").and_then(Value::as_str), Some("used"));
        let procedures = lf.get("procedures").and_then(Value::as_array).unwrap();
        assert!(procedures.is_empty());
    }

    #[test]
    fn vacuous_supertype_expressions_delete_their_constraint() {
        let mut lf = json!({
            "entities": [ { "_class": "Expressir::Model::Declarations::Entity", "id": "piece" } ],
            "subtype_constraints": [
                { "_class": "Expressir::Model::Declarations::SubtypeConstraint", "id": "kept",
                  "supertype_expression": { "_class": "Expressir::Model::SupertypeExpressions::OneofSupertypeExpression",
                                            "operands": [ { "_class": "Expressir::Model::References::SimpleReference", "id": "piece" },
                                                          { "_class": "Expressir::Model::References::SimpleReference", "id": "gone" } ] } },
                { "_class": "Expressir::Model::Declarations::SubtypeConstraint", "id": "vacuous",
                  "supertype_expression": { "_class": "Expressir::Model::SupertypeExpressions::OneofSupertypeExpression",
                                            "operands": [ { "_class": "Expressir::Model::References::SimpleReference", "id": "gone" } ] } }
            ]
        });
        prune_supertype_expressions(&mut lf);
        let constraints = lf
            .get("subtype_constraints")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(constraints.len(), 1);
        assert_eq!(
            constraints[0].get("id").and_then(Value::as_str),
            Some("kept")
        );
        // A single surviving operand collapses to the bare reference
        // (ruby: reduced.one? ? reduced.first : reset_oneof(...)).
        assert_eq!(
            constraints[0]["supertype_expression"]
                .get("id")
                .and_then(Value::as_str),
            Some("piece")
        );
    }

    #[test]
    fn abstract_entities_drop_the_bare_marker() {
        let mut lf = json!({
            "entities": [ { "_class": "Expressir::Model::Declarations::Entity", "id": "piece",
                            "abstract": true, "abstract_entity": true } ]
        });
        convert_entities(&mut lf);
        assert_eq!(lf["entities"][0].get("abstract_entity"), Some(&Value::Null));
    }
}

// ---- the SHTOLO G.2: extensible enumeration and select folding ----

/// Parses `SELF <> item;` inside a TYPE wrapper and returns the wire
/// WhereRule (the enumeration exclusion), re-id'd.
fn enumeration_exclusion_rule(id: &str, item: &str) -> Option<Value> {
    let snippet = format!(
        "SCHEMA __to__;\nTYPE __t__ = ENUMERATION OF (dummy);\nWHERE\n  {id}: SELF <> {item};\nEND_TYPE;\nEND_SCHEMA;\n"
    );
    let tree = ParsedTree::parse(&snippet).ok()?;
    let wire = tree.to_model_json("snippet.exp").ok()?;
    let mut rule = wire.pointer("/schemas/0/types/0/where_rules/0").cloned()?;
    if let Some(obj) = rule.as_object_mut() {
        obj.insert("id".into(), Value::from(id));
    }
    Some(rule)
}

/// Parses `NOT ('base.item' IN TYPEOF(SELF));` inside a TYPE wrapper
/// and returns the wire WhereRule (the select exclusion), re-id'd.
fn select_exclusion_rule(id: &str, base_ref: &str, item: &str) -> Option<Value> {
    let upper = item.to_uppercase();
    let snippet = format!(
        "SCHEMA __to__;\nTYPE __t__ = SELECT (dummy);\nWHERE\n  {id}: NOT ('{base_ref}.{upper}' IN TYPEOF(SELF));\nEND_TYPE;\nEND_SCHEMA;\n"
    );
    let tree = ParsedTree::parse(&snippet).ok()?;
    let wire = tree.to_model_json("snippet.exp").ok()?;
    let mut rule = wire.pointer("/schemas/0/types/0/where_rules/0").cloned()?;
    if let Some(obj) = rule.as_object_mut() {
        obj.insert("id".into(), Value::from(id));
    }
    Some(rule)
}

/// G.2.1: an extensible ENUMERATION folds its extensions' items in,
/// becomes non-extensible, and each extension type becomes a
/// non-extensible enumeration of the full domain with a `SELF <>
/// item` exclusion WHERE per item it added.
pub fn resolve_extensible_enumerations(longform: &mut Value) {
    let base_names: Vec<String> = longform
        .get("types")
        .and_then(Value::as_array)
        .map(|types| {
            types
                .iter()
                .filter(|t| {
                    let ut = t.get("underlying_type");
                    ut.map(|ut| {
                        ut.get("_class")
                            .and_then(Value::as_str)
                            .is_some_and(|c| c.ends_with("::Enumeration"))
                            && ut.get("extensible").and_then(Value::as_bool) == Some(true)
                    })
                    .unwrap_or(false)
                })
                .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    for base_name in base_names {
        // Collect first (immutable), then mutate: the base's merged
        // domain and each extension's index, then one write per type.
        let Some(types) = longform.get("types").and_then(Value::as_array) else {
            break;
        };
        let Some(base_index) = types
            .iter()
            .position(|t| t.get("id").and_then(Value::as_str) == Some(base_name.as_str()))
        else {
            continue;
        };
        let base_ut = &types[base_index]["underlying_type"];
        let mut merged: Vec<String> = base_ut
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut extensions: Vec<usize> = Vec::new();
        for (i, t) in types.iter().enumerate() {
            if i == base_index {
                continue;
            }
            let Some(ut) = t.get("underlying_type") else {
                continue;
            };
            let based_on_matches = ut.get("based_on").map(|b| {
                b.get("id")
                    .and_then(Value::as_str)
                    .map(|id| id.eq_ignore_ascii_case(&base_name))
                    .unwrap_or_else(|| {
                        b.as_str()
                            .is_some_and(|s| s.eq_ignore_ascii_case(&base_name))
                    })
            });
            let is_plain_extension = ut
                .get("_class")
                .and_then(Value::as_str)
                .is_some_and(|c| c.ends_with("::Enumeration"))
                && ut.get("extensible").and_then(Value::as_bool) != Some(true)
                && based_on_matches == Some(true);
            if is_plain_extension {
                extensions.push(i);
                let items = ut.get("items").and_then(Value::as_array).map(|items| {
                    items
                        .iter()
                        .filter_map(|i| i.get("id").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                });
                merged.extend(items.unwrap_or_default());
            }
        }
        merged.dedup();

        let mut plans: Vec<(usize, Vec<String>)> = Vec::new();
        for i in &extensions {
            let ext_items = types[*i]["underlying_type"]["items"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|i| i.get("id").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let exclusions: Vec<String> = merged
                .iter()
                .filter(|item| !ext_items.iter().any(|e| e.eq_ignore_ascii_case(item)))
                .cloned()
                .collect();
            plans.push((*i, exclusions));
        }

        // One write per type against a fresh mutable borrow.
        let Some(Value::Array(types)) = longform.get_mut("types") else {
            break;
        };
        if let Some(base) = types.get_mut(base_index) {
            if let Some(ut) = base.get_mut("underlying_type") {
                let items: Vec<Value> = merged
                    .iter()
                    .map(|id| {
                        serde_json::json!({ "_class": "Expressir::Model::DataTypes::EnumerationItem", "id": id })
                    })
                    .collect();
                if let Some(obj) = ut.as_object_mut() {
                    obj.insert("items".into(), Value::Array(items));
                    obj.insert("extensible".into(), Value::from(false));
                    obj.insert("based_on".into(), Value::Null);
                }
            }
        }
        for (i, exclusions) in plans {
            let Some(ext) = types.get_mut(i) else {
                continue;
            };
            if let Some(ut) = ext.get_mut("underlying_type") {
                let items: Vec<Value> = merged
                    .iter()
                    .map(|id| {
                        serde_json::json!({ "_class": "Expressir::Model::DataTypes::EnumerationItem", "id": id })
                    })
                    .collect();
                if let Some(obj) = ut.as_object_mut() {
                    obj.insert("items".into(), Value::Array(items));
                }
            }
            let rules: Vec<Value> = exclusions
                .iter()
                .enumerate()
                .filter_map(|(i, item)| enumeration_exclusion_rule(&format!("wr{}", i + 1), item))
                .collect();
            if let Some(obj) = ext.as_object_mut() {
                obj.insert("where_rules".into(), Value::Array(rules));
            }
        }
    }
}

/// G.2.2/G.2.4: every extensible/extending SELECT becomes a
/// non-extensible select whose items are the full domain in longform
/// scope; chains fold in dependency order, an extending type's
/// exclusions become `NOT ('base.item' IN TYPEOF(SELF))` WHERE
/// clauses, and
/// a GENERIC_ENTITY base folds in every entity the artifact carries.
pub fn resolve_extensible_selects(longform: &mut Value) {
    let select_ids: Vec<String> = longform
        .get("types")
        .and_then(Value::as_array)
        .map(|types| {
            types
                .iter()
                .filter(|t| {
                    t.get("underlying_type")
                        .and_then(|ut| ut.get("_class"))
                        .and_then(Value::as_str)
                        .is_some_and(|c| c.ends_with("::Select"))
                })
                .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let pending: Vec<String> = select_ids
        .iter()
        .filter(|id| select_pending(longform, id))
        .cloned()
        .collect();
    for id in pending {
        fold_select_node(longform, &id);
    }
}

fn select_pending(longform: &Value, id: &str) -> bool {
    longform
        .get("types")
        .and_then(Value::as_array)
        .and_then(|types| {
            types.iter().find_map(|t| {
                if t.get("id").and_then(Value::as_str) != Some(id) {
                    return None;
                }
                let ut = t.get("underlying_type")?;
                Some(
                    ut.get("extensible").and_then(Value::as_bool) == Some(true)
                        || ut.get("based_on").map(|b| !b.is_null()).unwrap_or(false),
                )
            })
        })
        .unwrap_or(false)
}

fn fold_select_node(longform: &mut Value, id: &str) {
    // All lookups go through `longform` by id: a held &mut into the
    // types array would pin the whole fold.
    let (extensible, based_on_name, generic_entity) = {
        let Some(t) = find_type(longform, id) else {
            return;
        };
        let ut = t.get("underlying_type");
        (
            ut.and_then(|u| u.get("extensible"))
                .and_then(Value::as_bool)
                == Some(true),
            ut.and_then(|u| u.get("based_on"))
                .map(|b| {
                    b.get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| b.as_str().map(str::to_string).unwrap_or_default())
                })
                .filter(|s| !s.is_empty()),
            ut.and_then(|u| u.get("generic_entity"))
                .and_then(Value::as_bool)
                == Some(true),
        )
    };
    if !extensible && !based_on_name.is_some() {
        return;
    }

    // GENERIC_ENTITY: the extenders are the entity types themselves
    // (WG12 requirement, #32); flags stay as declared.
    if generic_entity && based_on_name.is_none() {
        let entity_refs: Vec<Value> = longform
            .get("entities")
            .and_then(Value::as_array)
            .map(|entities| {
                entities
                    .iter()
                    .filter_map(|e| e.get("id").and_then(Value::as_str))
                    .map(|eid| {
                        json!({ "_class": "Expressir::Model::References::SimpleReference", "id": eid })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(t) = find_type_mut(longform, id) {
            if let Some(ut) = t.get_mut("underlying_type") {
                if let Some(obj) = ut.as_object_mut() {
                    obj.insert("items".into(), Value::Array(entity_refs));
                }
            }
        }
        return;
    }

    // Fold the BASE first: a chain resolves bottom-up, so an extending
    // type that is itself extensible resolves against its base's
    // folded domain. The folded domain is read back through find_type
    // below; the delta this type contributes is own + extenders.
    if let Some(base_name) = &based_on_name {
        if select_pending(longform, base_name) {
            fold_select_node(longform, base_name);
        }
        let _ = find_type(longform, base_name).map(|parent| {
            parent
                .get("underlying_type")
                .and_then(|ut| ut.get("items"))
                .and_then(Value::as_array)
                .map(|items| items.len())
        });
    }

    // Extenders: every OTHER select whose based_on names this type.
    let mut extenders: Vec<String> = Vec::new();
    if let Some(types) = longform.get("types").and_then(Value::as_array) {
        for t in types {
            let Some(t_id) = t.get("id").and_then(Value::as_str) else {
                continue;
            };
            if t_id.eq_ignore_ascii_case(id) {
                continue;
            }
            let based_on = t.get("underlying_type").and_then(|ut| ut.get("based_on"));
            let matches = based_on.map(|b| {
                b.get("id")
                    .and_then(Value::as_str)
                    .map(|i| i.eq_ignore_ascii_case(id))
                    .unwrap_or_else(|| b.as_str().is_some_and(|s| s.eq_ignore_ascii_case(id)))
            });
            if matches == Some(true) {
                extenders.push(t_id.to_string());
            }
        }
    }

    let mut merged: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let own_items: Vec<Value> = find_type(longform, id)
        .and_then(|t| {
            t.get("underlying_type")
                .and_then(|ut| ut.get("items"))
                .cloned()
        })
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let ext_items: Vec<Vec<Value>> = extenders
        .iter()
        .filter_map(|ext_id| {
            find_type(longform, ext_id).and_then(|t| {
                t.get("underlying_type")
                    .and_then(|ut| ut.get("items"))
                    .and_then(Value::as_array)
                    .cloned()
            })
        })
        .collect();
    for item in own_items.iter().chain(ext_items.iter().flatten()) {
        let key = item
            .get("id")
            .and_then(Value::as_str)
            .map(|i| i.to_lowercase())
            .unwrap_or_default();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        merged.push(item.clone());
    }

    let base_ref = format!(
        "{}.{}",
        longform
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("LONGFORM")
            .to_uppercase(),
        id.to_uppercase()
    );
    if let Some(t) = find_type_mut(longform, id) {
        if let Some(ut) = t.get_mut("underlying_type") {
            if let Some(obj) = ut.as_object_mut() {
                obj.insert("items".into(), Value::Array(merged.clone()));
                obj.insert("extensible".into(), Value::from(false));
                obj.insert("based_on".into(), Value::Null);
            }
        }
    }

    for ext_id in &extenders {
        let keep: Vec<String> = find_type(longform, ext_id)
            .and_then(|t| {
                t.get("underlying_type")
                    .and_then(|ut| ut.get("items"))
                    .cloned()
            })
            .and_then(|v| v.as_array().cloned())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.get("id").and_then(Value::as_str))
                    .map(|i| i.to_uppercase())
                    .collect()
            })
            .unwrap_or_default();
        let exclusions: Vec<String> = merged
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
            .filter(|i| !keep.contains(&i.to_uppercase()))
            .collect();
        let rules: Vec<Value> = exclusions
            .iter()
            .enumerate()
            .filter_map(|(i, item)| select_exclusion_rule(&format!("wr{}", i + 1), &base_ref, item))
            .collect();
        if let Some(ext) = find_type_mut(longform, ext_id) {
            if let Some(ut) = ext.get_mut("underlying_type") {
                let items: Vec<Value> = merged
                    .iter()
                    .map(|i| {
                        json!({ "_class": "Expressir::Model::References::SimpleReference",
                                "id": i.get("id").and_then(Value::as_str).unwrap_or_default() })
                    })
                    .collect();
                if let Some(obj) = ut.as_object_mut() {
                    obj.insert("items".into(), Value::Array(items));
                    obj.insert("extensible".into(), Value::from(false));
                    obj.insert("based_on".into(), Value::Null);
                }
            }
            if let Some(obj) = ext.as_object_mut() {
                obj.insert("where_rules".into(), Value::Array(rules));
            }
        }
    }
}

fn find_type<'a>(longform: &'a Value, id: &str) -> Option<&'a Value> {
    longform
        .get("types")
        .and_then(Value::as_array)?
        .iter()
        .find(|t| {
            t.get("id")
                .and_then(Value::as_str)
                .is_some_and(|i| i.eq_ignore_ascii_case(id))
        })
}

fn find_type_mut<'a>(longform: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    longform
        .get_mut("types")
        .and_then(Value::as_array_mut)?
        .iter_mut()
        .find(|t| {
            t.get("id")
                .and_then(Value::as_str)
                .is_some_and(|i| i.eq_ignore_ascii_case(id))
        })
}

#[cfg(test)]
mod extensible_tests {
    use super::*;
    use serde_json::json;

    fn select_type(id: &str, extensible: bool, based_on: Option<&str>, items: &[&str]) -> Value {
        let mut ut = json!({
            "_class": "Expressir::Model::DataTypes::Select",
            "items": items.iter().map(|i| {
                serde_json::json!({ "_class": "Expressir::Model::References::SimpleReference", "id": i })
            }).collect::<Vec<_>>()
        });
        if extensible {
            ut["extensible"] = json!(true);
        }
        if let Some(b) = based_on {
            ut["based_on"] = serde_json::json!(
                { "_class": "Expressir::Model::References::SimpleReference", "id": b }
            );
        }
        json!({ "_class": "Expressir::Model::Declarations::Type", "id": id, "underlying_type": ut })
    }

    #[test]
    fn select_chains_fold_with_exclusions() {
        let mut lf = json!({
            "_class": "Expressir::Model::Declarations::Schema", "id": "longform",
            "types": [
                select_type("base_select", true, None, &["a", "b"]),
                select_type("ext_select", false, Some("base_select"), &["b", "c"])
            ]
        });
        resolve_extensible_selects(&mut lf);
        let types = lf.get("types").and_then(Value::as_array).unwrap();
        let base = types.iter().find(|t| t["id"] == "base_select").unwrap();
        let items: Vec<&str> = base["underlying_type"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(items, ["a", "b", "c"]);
        assert_eq!(base["underlying_type"]["extensible"], json!(false));

        let ext = types.iter().find(|t| t["id"] == "ext_select").unwrap();
        let ext_items: Vec<&str> = ext["underlying_type"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(ext_items, ["a", "b", "c"]);
        // The extension keeps its added item, excludes the inherited ones.
        let rules = ext["where_rules"].as_array().unwrap();
        // The extension keeps its own b and c; only the inherited a
        // becomes an exclusion.
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn generic_entity_selects_fold_in_every_entity() {
        let mut lf = json!({
            "_class": "Expressir::Model::Declarations::Schema", "id": "longform",
            "entities": [
                { "_class": "Expressir::Model::Declarations::Entity", "id": "one" },
                { "_class": "Expressir::Model::Declarations::Entity", "id": "two" }
            ],
            "types": [ {
                "_class": "Expressir::Model::Declarations::Type", "id": "any_thing",
                "underlying_type": { "_class": "Expressir::Model::DataTypes::Select",
                                     "extensible": true, "generic_entity": true, "items": [] }
            } ]
        });
        resolve_extensible_selects(&mut lf);
        let t = &lf["types"][0];
        let items = t["underlying_type"]["items"].as_array().unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["one", "two"]);
    }

    #[test]
    fn extensible_enumerations_fold_with_exclusions() {
        let mut lf = json!({
            "_class": "Expressir::Model::Declarations::Schema", "id": "longform",
            "types": [
                { "_class": "Expressir::Model::Declarations::Type", "id": "colors",
                  "underlying_type": { "_class": "Expressir::Model::DataTypes::Enumeration",
                                       "extensible": true,
                                       "items": [ { "id": "red" }, { "id": "green" } ] } },
                { "_class": "Expressir::Model::Declarations::Type", "id": "colors_ext",
                  "underlying_type": { "_class": "Expressir::Model::DataTypes::Enumeration",
                                       "based_on": { "_class": "Expressir::Model::References::SimpleReference",
                                                     "id": "colors" },
                                       "items": [ { "id": "blue" } ] } }
            ]
        });
        resolve_extensible_enumerations(&mut lf);
        let types = lf.get("types").and_then(Value::as_array).unwrap();
        let base = &types[0];
        let items: Vec<&str> = base["underlying_type"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(items, ["red", "green", "blue"]);
        assert_eq!(base["underlying_type"]["extensible"], json!(false));
        let ext = &types[1];
        let rules = ext["where_rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["id"], json!("wr1"));
    }
}

// ---- the SHTOLO orchestrator (the full stage order) + expose ----

/// The provenance remark (ruby `add_provenance_remark!`, G.1.10): a
/// `provenance` remark item listing every contributing schema, in
/// alphabetical order, with each schema's version id when present.
pub fn add_provenance(longform: &mut Value, sections: &[Section]) {
    if sections.is_empty() {
        return;
    }
    let mut names: Vec<&Section> = sections.iter().collect();
    names.sort_by_key(|s| s.id.to_lowercase());
    let mut remarks = vec!["Original 2003 schemas:".to_string()];
    for section in names {
        let line = match &section.version {
            Some(version) => {
                format!("schema = {} schema_version_id = '{}';", section.id, version)
            }
            None => format!("schema = {};", section.id),
        };
        remarks.push(line);
    }
    let provenance = json!({
        "_class": "Expressir::Model::Declarations::RemarkItem",
        "id": "provenance",
        "remarks": remarks
    });
    let mut items = vec![provenance];
    if let Some(existing) = longform
        .get("remark_items")
        .and_then(Value::as_array)
        .cloned()
    {
        items.extend(existing);
    }
    if let Some(obj) = longform.as_object_mut() {
        obj.insert("remark_items".into(), Value::Array(items));
    }
}

/// G.2: redeclared (renamed) attributes become derived attributes
/// whose expression is `SELF\supertype.old_name` — the ruby
/// convert_renamed_attributes!. The ruby pass always emits the
/// qualified form, so a conversion with no resolvable supertype is
/// skipped rather than emitted.
pub fn convert_renamed_attributes(longform: &mut Value) {
    let Some(Value::Array(entities)) = longform.get_mut("entities") else {
        return;
    };
    for entity in entities {
        let Some(Value::Array(attributes)) = entity.get_mut("attributes") else {
            continue;
        };
        for attr in attributes.iter_mut() {
            let Some(supertype_ref) = attr
                .get("supertype_attribute")
                .filter(|v| !v.is_null())
                .cloned()
            else {
                continue;
            };
            let (old_name, supertype) = supertype_target(&supertype_ref);
            let (Some(old_name), Some(supertype)) = (old_name, supertype) else {
                continue;
            };
            let snippet = format!(
                "SCHEMA __x__;\nENTITY __e__;\nWHERE w1 : SELF\\{}.{};\nEND_ENTITY;\nEND_SCHEMA;\n",
                supertype, old_name
            );
            let Ok(tree) = ParsedTree::parse(&snippet) else {
                continue;
            };
            let Ok(wire) = tree.to_model_json("snippet.exp") else {
                continue;
            };
            let Some(expression) = wire
                .pointer("/schemas/0/entities/0/where_rules/0/expression")
                .cloned()
            else {
                continue;
            };
            let mut derived = serde_json::Map::new();
            derived.insert(
                "_class".into(),
                Value::from("Expressir::Model::Declarations::DerivedAttribute"),
            );
            let id = attr
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| Some(old_name.clone()));
            if let Some(id) = id {
                derived.insert("id".into(), Value::from(id));
            }
            if let Some(t) = attr.get("type").filter(|v| !v.is_null()) {
                derived.insert("type".into(), t.clone());
            }
            derived.insert("expression".into(), expression);
            *attr = Value::Object(derived);
        }
    }
}

/// `(old_name, supertype)` — a bare qualified attribute has only the
/// attribute name; the ruby supertype_target.
fn supertype_target(reference: &Value) -> (Option<String>, Option<String>) {
    let class = reference
        .get("_class")
        .and_then(Value::as_str)
        .unwrap_or("");
    if class.ends_with("::AttributeReference") {
        let old_name = reference
            .pointer("/attribute/id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let supertype = reference
            .get("ref")
            .filter(|g| {
                g.get("_class")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.ends_with("::GroupReference"))
            })
            .and_then(|g| g.pointer("/entity/id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        (old_name, supertype)
    } else {
        (
            reference
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            None,
        )
    }
}

/// G.NM.2: string literals carrying schema-qualified names (TYPEOF
/// arguments `'SCHEMA.TYPE'`) move to the longform name — the ruby
/// rewrite_qualified_strings! over every Literals::String `value`.
pub fn rewrite_qualified_strings(longform: &mut Value, prefixes: &[String], longform_name: &str) {
    match longform {
        Value::Object(obj) => {
            let is_string_literal = obj
                .get("_class")
                .and_then(Value::as_str)
                .is_some_and(|c| c.ends_with("Literals::String"));
            if is_string_literal {
                if let Some(Value::String(v)) = obj.get_mut("value") {
                    if let Some(rewritten) = strip_schema_prefix(v, prefixes, longform_name) {
                        *v = rewritten;
                    }
                }
            }
            for (_key, child) in obj.iter_mut() {
                rewrite_qualified_strings(child, prefixes, longform_name);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_qualified_strings(item, prefixes, longform_name);
            }
        }
        _ => {}
    }
}

/// `SCHEMA.`-prefixed heads over the closure names, case-insensitive.
fn strip_schema_prefix(value: &str, prefixes: &[String], longform_name: &str) -> Option<String> {
    let dot = value.find('.')?;
    let head = &value[..dot];
    if prefixes
        .iter()
        .any(|p| p.eq_ignore_ascii_case(head) && !head.is_empty())
    {
        Some(format!(
            "{}.{}",
            longform_name.to_uppercase(),
            &value[dot + 1..]
        ))
    } else {
        None
    }
}

/// The SHTOLO longform over a resolved repository wire — the full
/// stage order the ruby Shtolo runs: the G.1 closure copy with the
/// id munges, prune, the supertype reductions, the extensible
/// enumeration/select folding, subtype-constraint elimination, the
/// entity and renamed-attribute conversions, the qualified-string
/// rewrites, and the provenance remark.
pub fn shtolo_flatten(root_id: &str, repository: &Value, longform_name: &str) -> Value {
    let sections = closure(root_id, repository);
    let mut longform = flatten(root_id, repository, longform_name);
    // The ruby build_longform order: prune (the annex G.1.9 pass,
    // including the supertype reductions over the still-standing
    // constraints) runs BEFORE the stage-2 rewrites, so
    // subtype-constraint elimination moves already-reduced expressions
    // into the entities.
    prune(&mut longform);
    prune_supertype_expressions(&mut longform);
    resolve_extensible_enumerations(&mut longform);
    resolve_extensible_selects(&mut longform);
    eliminate_subtype_constraints(&mut longform);
    convert_entities(&mut longform);
    convert_renamed_attributes(&mut longform);
    let prefixes: Vec<String> = sections.iter().map(|s| s.id.to_uppercase()).collect();
    rewrite_qualified_strings(&mut longform, &prefixes, longform_name);
    add_provenance(&mut longform, &sections);
    longform
}

#[cfg(test)]
mod stage_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renamed_attributes_become_derived() {
        let mut lf = json!({
            "_class": "Expressir::Model::Declarations::Schema",
            "id": "longform",
            "entities": [
                { "_class": "Expressir::Model::Declarations::Entity", "id": "sub",
                  "supertype_expression": { "_class": "Expressir::Model::References::SimpleReference", "id": "base" },
                  "attributes": [
                    { "_class": "Expressir::Model::Declarations::Attribute", "id": "the_name",
                      "supertype_attribute": { "_class": "Expressir::Model::References::AttributeReference",
                                               "ref": { "_class": "Expressir::Model::References::GroupReference",
                                                        "entity": { "_class": "Expressir::Model::References::SimpleReference", "id": "base" } },
                                               "attribute": { "_class": "Expressir::Model::References::SimpleReference", "id": "name" } },
                      "type": { "_class": "Expressir::Model::DataTypes::String" } },
                    { "_class": "Expressir::Model::Declarations::Attribute", "id": "plain",
                      "type": { "_class": "Expressir::Model::DataTypes::String" } }
                  ] }
            ]
        });
        convert_renamed_attributes(&mut lf);
        let attrs = lf.pointer("/entities/0/attributes").unwrap();
        let derived = &attrs[0];
        assert_eq!(
            derived.get("_class").and_then(Value::as_str),
            Some("Expressir::Model::Declarations::DerivedAttribute")
        );
        assert_eq!(derived.get("id").and_then(Value::as_str), Some("the_name"));
        let text = derived.pointer("/expression").unwrap().to_string();
        assert!(
            text.contains("base"),
            "expression names the supertype: {text}"
        );
        assert!(
            text.contains("name"),
            "expression names the attribute: {text}"
        );
        assert!(derived.get("supertype_attribute").is_none());
        assert_eq!(
            attrs[1].get("_class").and_then(Value::as_str),
            Some("Expressir::Model::Declarations::Attribute"),
            "plain attributes stay"
        );
    }

    #[test]
    fn qualified_strings_move_to_the_longform_name() {
        let mut lf = json!({
            "_class": "Expressir::Model::Declarations::Schema",
            "id": "longform",
            "rules": [
                { "_class": "Expressir::Model::Declarations::Rule", "id": "r",
                  "where_rules": [
                    { "_class": "Expressir::Model::Declarations::WhereRule", "id": "wr1",
                      "expression": { "_class": "Expressir::Model::Expressions::FunctionCall",
                        "function": { "_class": "Expressir::Model::References::SimpleReference", "id": "TYPEOF" },
                        "parameters": [
                          { "_class": "Expressir::Model::Literals::String", "value": "ARM_ARM.APPLICATION_PROTOCOL" },
                          { "_class": "Expressir::Model::Literals::String", "value": "arm_arm.property" },
                          { "_class": "Expressir::Model::Literals::String", "value": "OTHER_SCHEMA.TYPE" },
                          { "_class": "Expressir::Model::Literals::String", "value": "unrelated string.no dot prefix" }
                        ] } }
                  ] }
            ]
        });
        rewrite_qualified_strings(&mut lf, &["ARM_ARM".to_string()], "longform");
        let params = lf
            .pointer("/rules/0/where_rules/0/expression/parameters")
            .unwrap();
        assert_eq!(
            params[0].get("value").and_then(Value::as_str),
            Some("LONGFORM.APPLICATION_PROTOCOL")
        );
        assert_eq!(
            params[1].get("value").and_then(Value::as_str),
            Some("LONGFORM.property"),
            "case-insensitive prefix"
        );
        assert_eq!(
            params[2].get("value").and_then(Value::as_str),
            Some("OTHER_SCHEMA.TYPE"),
            "non-closure prefixes stay"
        );
        assert_eq!(
            params[3].get("value").and_then(Value::as_str),
            Some("unrelated string.no dot prefix")
        );
    }

    #[test]
    fn orchestrator_runs_the_renamed_attribute_pass() {
        let repo = json!({
            "schemas": [
                { "_class": "Expressir::Model::Declarations::Schema", "id": "root",
                  "interfaces": [],
                  "entities": [
                    { "_class": "Expressir::Model::Declarations::Entity", "id": "base",
                      "attributes": [ { "_class": "Expressir::Model::Declarations::Attribute", "id": "name",
                                        "type": { "_class": "Expressir::Model::DataTypes::String" } } ] },
                    { "_class": "Expressir::Model::Declarations::Entity", "id": "sub",
                      "supertype_expression": { "_class": "Expressir::Model::References::SimpleReference", "id": "base" },
                      "attributes": [ { "_class": "Expressir::Model::Declarations::Attribute", "id": "renamed",
                                        "supertype_attribute": { "_class": "Expressir::Model::References::AttributeReference",
                                                                 "ref": { "_class": "Expressir::Model::References::GroupReference",
                                                                          "entity": { "_class": "Expressir::Model::References::SimpleReference", "id": "base" } },
                                                                 "attribute": { "_class": "Expressir::Model::References::SimpleReference", "id": "name" } },
                                        "type": { "_class": "Expressir::Model::DataTypes::String" } } ] }
                  ] }
            ]
        });
        let longform = shtolo_flatten("root", &repo, "longform");
        let attrs = longform.pointer("/entities/1/attributes").unwrap();
        assert_eq!(
            attrs[0].get("_class").and_then(Value::as_str),
            Some("Expressir::Model::Declarations::DerivedAttribute")
        );
        let provenance = longform
            .get("remark_items")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        assert!(provenance >= 1, "provenance rides the orchestrator");
    }
}
