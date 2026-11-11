//! Part 21 structural inventory + validation: the ISO-10303-21
//! exchange structure parsed by the compiled PARG artifact (the
//! checksummed single source of truth, expressir#625) — the
//! hand-rolled lexer/parser that used to live here retired; this
//! module walks the artifact's capture tree into the population
//! inventory the wasm and C surfaces serve.
//!
//! Clause anchors:
//! - ISO 10303-21 clause 8/10: header section, FILE_SCHEMA.
//! - ISO 10303-21 clause 11: exchange structure of data sections.
//! - ISO 10303-21 clause 12: a record keyword shall name an entity
//!   (or complex part) of the file schema.

use std::sync::OnceLock;

use parsanol::PargArtifact;
use serde_json::Value;

const ARTIFACT_JSON: &str = include_str!("../assets/p21/iso-10303-21.artifact.json");

fn artifact() -> Result<&'static PargArtifact, String> {
    static ARTIFACT: OnceLock<Result<PargArtifact, String>> = OnceLock::new();
    ARTIFACT
        .get_or_init(|| {
            PargArtifact::from_json(ARTIFACT_JSON)
                .map_err(|err| format!("the P21 artifact failed to load: {err}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// A parsed entity record: `#12=KEYWORD(arg, ...)` or its complex
/// form `#12=(A(1)(2)B(3))` (the part keywords in `parts`).
#[derive(Debug, Clone, PartialEq)]
pub struct EntityRecord {
    pub id: u64,
    pub keyword: String,
    /// For complex records: the per-part keywords in source order.
    pub parts: Vec<String>,
    pub complex: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Population {
    pub file_schemas: Vec<String>,
    pub records: Vec<EntityRecord>,
    /// Annex I anchors: `<name> = target;` in source order
    pub anchors: Vec<(String, String)>,
    /// Annex I external references: `#id = <target>;` in source order
    pub references: Vec<(u64, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl Population {
    /// Structural validation against a schema's entity inventory:
    /// every record keyword (or complex part) shall resolve (clause
    /// 12), case-insensitively.
    pub fn validate(&self, entities: &[String]) -> Verdict {
        let known: Vec<String> = entities.iter().map(|e| e.to_lowercase()).collect();
        let mut errors = Vec::new();
        for record in &self.records {
            if record.complex {
                for part in &record.parts {
                    if !known.contains(&part.to_lowercase()) {
                        errors.push(format!(
                            "#{}: unknown complex record part {part}",
                            record.id
                        ));
                    }
                }
            } else if !known.contains(&record.keyword.to_lowercase()) {
                errors.push(format!(
                    "#{}: unknown entity keyword {}",
                    record.id, record.keyword
                ));
            }
        }
        Verdict {
            valid: errors.is_empty(),
            errors,
        }
    }
}

/// Parses an ISO-10303-21 exchange structure into its structural
/// inventory through the PARG artifact.
pub fn parse_population(input: &str) -> Result<Population, String> {
    let artifact = artifact()?;
    let tree = artifact
        .parse_shape("exchange_structure", input)
        .map_err(|err| format!("p21 parse: {err}"))?;
    population_from_tree(&tree)
}

fn leaf_text<'a>(node: &'a Value, field: &str) -> Option<&'a str> {
    node.get(field)?.get("value")?.as_str()
}

/// Collects every string literal under a subtree, in document order,
/// unescaped the way the scanner reads them: the captured value keeps
/// the surrounding quotes, `''` folds to `'`.
fn collect_strings(node: &Value, out: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            if map.contains_key("string_literal") {
                if let Some(quoted) = leaf_text(node, "string_literal") {
                    out.push(unescape_string(quoted));
                }
            }
            for value in map.values() {
                collect_strings(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_strings(item, out);
            }
        }
        _ => {}
    }
}

fn unescape_string(quoted: &str) -> String {
    let inner = quoted
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .unwrap_or(quoted);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                // A doubled quote inside the captured span folds to one.
                if chars.clone().next() == Some('\'') {
                    chars.next();
                }
                out.push('\'');
            }
            other => out.push(other),
        }
    }
    out
}

fn keyword_of<'a>(record: &'a Value, field: &str) -> Option<&'a str> {
    leaf_text(record.get(field)?, "keyword")
}

fn parse_id(name: &Value) -> Result<u64, String> {
    let text = leaf_text(name, "entity_instance_name")
        .ok_or("entity instance name capture missing")?
        .strip_prefix('#')
        .ok_or("entity instance name must start with #")?;
    text.parse::<u64>()
        .map_err(|err| format!("bad entity instance name #{text}: {err}"))
}

fn population_from_tree(tree: &Value) -> Result<Population, String> {
    let mut file_schemas = Vec::new();
    let mut records = Vec::new();
    let mut anchors = Vec::new();
    let mut references = Vec::new();

    if let Some(header_entities) = tree
        .get("header_section")
        .and_then(|h| h.get("header_entity"))
        .and_then(Value::as_array)
    {
        for entity in header_entities {
            let Some(simple) = entity.get("simple_record") else {
                continue;
            };
            let keyword = leaf_text(simple, "keyword")
                .ok_or("header entity missing its keyword")?
                .to_string();
            if keyword.eq_ignore_ascii_case("FILE_SCHEMA") {
                collect_strings(simple, &mut file_schemas);
            }
        }
    }

    let data_sections = tree
        .get("data_section")
        .and_then(Value::as_array)
        .ok_or("the exchange structure has no data section")?;
    for section in data_sections {
        let Some(body) = section.get("data_section") else {
            continue;
        };
        let Some(instances) = body.get("instance_list").and_then(Value::as_array) else {
            continue;
        };
        for instance in instances {
            if let Some(simple) = instance.get("simple_entity_instance") {
                let id = parse_id(simple)?;
                let keyword = keyword_of(simple, "simple_record")
                    .ok_or("simple record missing its keyword")?
                    .to_string();
                records.push(EntityRecord {
                    id,
                    keyword,
                    parts: Vec::new(),
                    complex: false,
                });
            } else if let Some(complex) = instance.get("complex_entity_instance") {
                let id = parse_id(complex)?;
                let parts: Vec<String> = complex
                    .get("subsuper_record")
                    .and_then(Value::as_array)
                    .ok_or("complex record missing its subsuper record")?
                    .iter()
                    .map(|part| {
                        let record = part
                            .get("simple_record")
                            .ok_or("part record wrapper missing")?;
                        leaf_text(record, "keyword")
                            .map(str::to_string)
                            .ok_or_else(|| "part record missing its keyword".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                records.push(EntityRecord {
                    id,
                    keyword: String::new(),
                    parts,
                    complex: true,
                });
            } else if let Some(external) = instance.get("external_reference_instance") {
                let id = parse_id(external)?;
                let target = leaf_text(external, "external_anchor")
                    .ok_or("external reference missing its anchor")?;
                references.push((
                    id,
                    target
                        .strip_prefix('<')
                        .and_then(|rest| rest.strip_suffix('>'))
                        .ok_or("external anchor must be wrapped in <>")?
                        .to_string(),
                ));
            }
        }
    }

    if let Some(entries) = tree
        .get("anchor_section")
        .and_then(|a| a.get("anchor_entry"))
        .and_then(Value::as_array)
    {
        for entry in entries {
            let Some(body) = entry.get("anchor_entry") else {
                continue;
            };
            let Some(label) = leaf_text(body, "anchor_label") else {
                continue;
            };
            let target = leaf_text(body, "entity_instance_name")
                .map(str::to_string)
                .or_else(|| leaf_text(body, "unset").map(str::to_string))
                .ok_or("anchor entry missing its target")?;
            anchors.push((label.to_string(), target));
        }
    }

    if let Some(instances) = tree
        .get("reference_section")
        .and_then(|r| r.get("instance_list"))
        .and_then(Value::as_array)
    {
        for instance in instances {
            let Some(external) = instance.get("external_reference_instance") else {
                continue;
            };
            let id = parse_id(external)?;
            let target = leaf_text(external, "external_anchor")
                .ok_or("external reference missing its anchor")?;
            references.push((
                id,
                target
                    .strip_prefix('<')
                    .and_then(|rest| rest.strip_suffix('>'))
                    .ok_or("external anchor must be wrapped in <>")?
                    .to_string(),
            ));
        }
    }

    Ok(Population {
        file_schemas,
        records,
        anchors,
        references,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_population;

    #[test]
    fn builds_the_inventory_from_the_artifact_tree() {
        let source = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('demo'));\nENDSEC;\n\
                      ANCHOR;\n<lbl>=#1;\n<prom>=$;\nENDSEC;\n\
                      REFERENCE;\n#2=<http://x/first#P>;\nENDSEC;\n\
                      DATA;\n#1=E('v');\n#2=(A(1)B(2));\nENDSEC;\n\
                      END-ISO-10303-21;\n";
        let population = parse_population(source).expect("parse");
        assert_eq!(population.file_schemas, vec!["demo"]);
        assert_eq!(population.records.len(), 2);
        assert_eq!(population.records[0].keyword, "E");
        assert!(!population.records[0].complex);
        assert_eq!(population.records[1].parts, vec!["A", "B"]);
        assert!(population.records[1].complex);
        assert_eq!(
            population.anchors,
            vec![
                ("lbl".to_string(), "#1".to_string()),
                ("prom".to_string(), "$".to_string()),
            ]
        );
        assert_eq!(
            population.references,
            vec![(2, "http://x/first#P".to_string())]
        );
    }

    #[test]
    fn validates_keywords_case_insensitively() {
        let source = "ISO-10303-21;\nHEADER;\nFILE_NAME('x');\nENDSEC;\n\
                      DATA;\n#1=point(1,2);\n#3=(A(1)B(2));\nENDSEC;\n\
                      END-ISO-10303-21;\n";
        let population = parse_population(source).expect("parse");
        let verdict = population.validate(&["Point".to_string(), "a".to_string(), "b".to_string()]);
        assert!(verdict.valid, "errors: {:?}", verdict.errors);
        let bad = population.validate(&["point".to_string()]);
        assert!(!bad.valid);
        // A and B, one unknown-part error per complex part.
        assert_eq!(bad.errors.len(), 2);
    }

    #[test]
    fn rejects_a_truncated_structure() {
        let truncated = "ISO-10303-21;\nHEADER;\nFILE_NAME('n');\nENDSEC;\nDATA;\n#1=E('v');\n";
        assert!(parse_population(truncated).is_err());
    }
}
