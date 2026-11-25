//! The expansion family (lutaml/expressir#636): the concatenated
//! artifact (expressir#247, eeng's `--concat_schema`) — the interface
//! closure of a root schema and the banner writer. The SHTOLO longform
//! (flatten) builds on the closure.

use crate::ParsedTree;
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
fn collect_called(
    node: &Value,
    invocable: &std::collections::HashSet<String>,
    out: &mut std::collections::HashSet<String>,
) {
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
            collect_called(value, invocable, out);
        }
    } else if let Value::Array(items) = node {
        for item in items {
            collect_called(item, invocable, out);
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

    let invocable: std::collections::HashSet<String> = ["functions", "procedures"]
        .iter()
        .filter_map(|key| longform.get(*key).and_then(Value::as_array))
        .flatten()
        .filter_map(decl_id)
        .map(|id| id.to_lowercase())
        .collect();
    loop {
        let mut called = std::collections::HashSet::new();
        collect_called(longform, &invocable, &mut called);
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
