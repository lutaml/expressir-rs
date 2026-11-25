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
            .map(|id| lower(id))
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
