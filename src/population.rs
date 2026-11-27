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
use serde::Serialize;
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
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EntityRecord {
    pub id: u64,
    pub keyword: String,
    /// For complex records: the per-part keywords in source order.
    pub parts: Vec<String>,
    pub complex: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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

/// One resync span: the junk the R2 rounds skipped over to keep the
/// spine parsing (expressir#627, the parsanol portable/dynamic layer
/// adopted for P21).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ResyncSpan {
    pub offset: usize,
    pub end: usize,
}

/// The recovery envelope's outcome. On well-formed input the
/// population builds exactly as the strict path does. On malformed
/// input the envelope is failure forensics — the failure wire, how
/// far the parse got, the spans the inferred/declared resync skipped,
/// and the records the deepest repetition prefix had already matched —
/// never a silently-repaired document.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RecoveredPopulation {
    pub ok: bool,
    pub population: Option<Population>,
    /// The failure wire: `offset`/`line`/`column`/`expected`.
    pub error: Option<Value>,
    /// The furthest input position any attempt matched.
    pub consumed_upto: Option<u64>,
    pub resyncs: Vec<ResyncSpan>,
    /// Records/references recovered from the partial spine prefix.
    pub partial_records: Vec<EntityRecord>,
    pub partial_references: Vec<(u64, String)>,
}

/// Parses through the recovery envelope: identical to
/// [`parse_population`] on well-formed input; on malformed input
/// returns the forensics instead of the hard error.
pub fn parse_population_recovering(input: &str) -> Result<RecoveredPopulation, String> {
    let artifact = artifact()?;
    let wire = artifact
        .parse_shape_recovering("exchange_structure", input)
        .map_err(|err| format!("p21 recovery: {err}"))?;

    if wire.get("ok").and_then(Value::as_bool) == Some(true) {
        let shape = wire
            .get("shape")
            .ok_or("recovery success wire missing its shape")?;
        let population = population_from_tree(shape)?;
        return Ok(RecoveredPopulation {
            ok: true,
            population: Some(population),
            error: None,
            consumed_upto: None,
            resyncs: Vec::new(),
            partial_records: Vec::new(),
            partial_references: Vec::new(),
        });
    }

    let mut partial_records = Vec::new();
    let mut partial_references = Vec::new();
    if let Some(partial) = wire.get("partial") {
        // The partial spine is shaped like the repetition it stopped
        // in: an array of spine iterations, possibly wrapped under the
        // spine rule's name. Each iteration walks like a strict-tree
        // instance; iterations the failed sequence left incomplete are
        // skipped rather than fatal — this is a best-effort prefix.
        let items: Vec<&Value> = match partial {
            Value::Array(items) => items.iter().collect(),
            Value::Object(map) => map.values().filter_map(Value::as_array).flatten().collect(),
            _ => Vec::new(),
        };
        for item in items {
            let mut sections = Vec::new();
            collect_partial_instances(item, &mut sections);
            for section in sections {
                let body = section.get("data_section").unwrap_or(section);
                if let Some(instances) = body.get("instance_list").and_then(Value::as_array) {
                    for instance in instances {
                        let _ =
                            push_instance(instance, &mut partial_records, &mut partial_references);
                    }
                }
                if let Some(instances) = section.get("instance_list").and_then(Value::as_array) {
                    for instance in instances {
                        let _ =
                            push_instance(instance, &mut partial_records, &mut partial_references);
                    }
                }
            }
        }
    }

    let resyncs = wire
        .get("recovered_spans")
        .and_then(Value::as_array)
        .map(|spans| {
            spans
                .iter()
                .filter_map(|s| {
                    Some(ResyncSpan {
                        offset: s.get("offset")?.as_u64()? as usize,
                        end: s.get("end")?.as_u64()? as usize,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(RecoveredPopulation {
        ok: false,
        population: None,
        error: wire.get("error").cloned(),
        consumed_upto: wire.get("consumed_upto").and_then(Value::as_u64),
        resyncs,
        partial_records,
        partial_references,
    })
}

/// Data-section-shaped subtrees under a partial spine item: either the
/// item IS a section iteration (`data_section` body under the same
/// name) or it wraps one. Best-effort — junk rounds leave odd shapes.
fn collect_partial_instances<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    match node {
        Value::Object(map) => {
            if map.contains_key("instance_list") {
                out.push(node);
                return;
            }
            if map.contains_key("data_section")
                && node.get("data_section").is_some_and(|b| b.is_object())
            {
                out.push(node);
                return;
            }
            for value in map.values() {
                collect_partial_instances(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_partial_instances(item, out);
            }
        }
        _ => {}
    }
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

/// One entity instance read into the domain inventories — the shared
/// walk of the strict tree and the recovered partial spine.
fn push_instance(
    instance: &Value,
    records: &mut Vec<EntityRecord>,
    references: &mut Vec<(u64, String)>,
) -> Result<(), String> {
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
        return Ok(());
    }
    if let Some(complex) = instance.get("complex_entity_instance") {
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
        return Ok(());
    }
    if let Some(external) = instance.get("external_reference_instance") {
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
    Ok(())
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
            push_instance(instance, &mut records, &mut references)?;
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

// ---- the full population model (#642 PR-1): parse → write round trip ----

/// A P21 parameter value, mirroring the ruby P21 value union.
#[derive(Debug, Clone, PartialEq)]
pub enum P21Value {
    Unset,        // $
    Derived,      // *
    Bool(bool),   // .T. / .F.
    Enum(String), // the name without the dots
    Binary(String),
    Str(String),
    /// A reference, emitted `#12`.
    Ref(u64),
    /// A number kept as its SOURCE text so re-emission is verbatim.
    Number(String),
    /// A constant/external reference emitted verbatim (anchors and
    /// attribute references).
    Constant(String),
    Typed(Typed),
    List(Vec<P21Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Typed {
    pub keyword: String,
    pub params: Vec<P21Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub keyword: String,
    pub params: Vec<P21Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instance {
    Simple(Record),
    Complex(Vec<Record>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub name: Option<String>,
    pub contexts: Vec<String>,
    /// The instance ids in source order.
    pub instance_ids: Vec<u64>,
}

/// The full exchange structure: header entities in insertion order,
/// instances in source order, named data sections, anchors and
/// references.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FullPopulation {
    pub header: Vec<(String, Vec<P21Value>)>,
    pub instances: Vec<(u64, Instance)>,
    pub sections: Vec<Section>,
    pub anchors: Vec<(String, P21Value)>,
    pub references: Vec<(u64, String)>,
}

/// Parses an exchange structure into the FULL population (typed
/// values, complex records, named sections) through the PARG artifact.
pub fn parse_full_population(input: &str) -> Result<FullPopulation, String> {
    let artifact = artifact()?;
    let tree = artifact
        .parse_shape("exchange_structure", input)
        .map_err(|err| format!("p21 parse: {err}"))?;
    full_population_from_tree(&tree)
}

/// Serializes a full population back to ISO 10303-21 text, mirroring
/// the ruby `P21::Writer` determinism: header entities in insertion
/// order, data instances in source order, anchors/references/named
/// sections re-emitted when present.
pub fn write_population(population: &FullPopulation) -> String {
    let mut lines = vec!["ISO-10303-21;".to_string(), "HEADER;".to_string()];
    for (keyword, params) in &population.header {
        lines.push(format!("{}({});", keyword, emit_params(params)));
    }
    lines.push("ENDSEC;".to_string());
    if !population.anchors.is_empty() {
        lines.push("ANCHOR;".to_string());
        for (label, value) in &population.anchors {
            lines.push(format!("<{label}> = {};", emit_value(value)));
        }
        lines.push("ENDSEC;".to_string());
    }
    if !population.references.is_empty() {
        lines.push("REFERENCE;".to_string());
        for (id, anchor) in &population.references {
            lines.push(format!("#{id} = <{anchor}>;"));
        }
        lines.push("ENDSEC;".to_string());
    }
    if population.sections.is_empty() {
        lines.push("DATA;".to_string());
        emit_instances(&population.instances, &mut lines);
        lines.push("ENDSEC;".to_string());
    } else {
        for section in &population.sections {
            lines.push(named_data_line(section));
            for id in &section.instance_ids {
                if let Some((_, instance)) = population.instances.iter().find(|(key, _)| key == id)
                {
                    lines.push(format!("#{id}={};", emit_instance(instance)));
                }
            }
            lines.push("ENDSEC;".to_string());
        }
    }
    lines.push("END-ISO-10303-21;".to_string());
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The parse + write round trip in one op (the delegation surface).
pub fn round_trip_population(input: &str) -> Result<String, String> {
    parse_full_population(input).map(|population| write_population(&population))
}

fn emit_instances(instances: &[(u64, Instance)], lines: &mut Vec<String>) {
    for (id, instance) in instances {
        lines.push(format!("#{id}={};", emit_instance(instance)));
    }
}

fn named_data_line(section: &Section) -> String {
    match &section.name {
        None => "DATA;".to_string(),
        Some(name) => {
            let contexts = section
                .contexts
                .iter()
                .map(|c| format!("'{c}'"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("DATA ('{name}', ({contexts}));")
        }
    }
}

fn emit_instance(instance: &Instance) -> String {
    match instance {
        Instance::Complex(records) => {
            let parts: Vec<String> = records.iter().map(emit_simple).collect();
            format!("({})", parts.join(""))
        }
        Instance::Simple(record) => emit_simple(record),
    }
}

fn emit_simple(record: &Record) -> String {
    format!("{}({})", record.keyword, emit_params(&record.params))
}

fn emit_params(params: &[P21Value]) -> String {
    params.iter().map(emit_value).collect::<Vec<_>>().join(",")
}

fn emit_value(value: &P21Value) -> String {
    match value {
        P21Value::Unset => "$".to_string(),
        P21Value::Derived => "*".to_string(),
        P21Value::Bool(true) => ".T.".to_string(),
        P21Value::Bool(false) => ".F.".to_string(),
        P21Value::Enum(name) => format!(".{name}."),
        P21Value::Binary(hex) => format!("\"{hex}\""),
        P21Value::Str(text) => format!("'{}'", text.replace('\'', "''")),
        P21Value::Ref(id) => format!("#{id}"),
        P21Value::Number(text) => text.clone(),
        P21Value::Constant(text) => text.clone(),
        P21Value::List(items) => {
            let inner: Vec<String> = items.iter().map(emit_value).collect();
            format!("({})", inner.join(","))
        }
        P21Value::Typed(typed) => format!("{}({})", typed.keyword, emit_params(&typed.params)),
    }
}

// ---- the full-model builder over the artifact capture tree ----

fn full_population_from_tree(tree: &Value) -> Result<FullPopulation, String> {
    let mut population = FullPopulation::default();

    if let Some(header_entities) = tree
        .get("header_section")
        .and_then(|h| h.get("header_entity"))
        .and_then(serde_json::Value::as_array)
    {
        for entity in header_entities {
            let Some(simple) = entity.get("simple_record") else {
                continue;
            };
            let keyword = leaf_text(simple, "keyword")
                .ok_or("header entity missing its keyword")?
                .trim_start_matches('!')
                .to_string();
            let params = record_params(simple)?;
            population.header.push((keyword, params));
        }
    }

    let sections = tree
        .get("data_section")
        .and_then(serde_json::Value::as_array)
        .ok_or("the exchange structure has no data section")?;
    for section in sections {
        let Some(body) = section.get("data_section") else {
            continue;
        };
        let mut instance_ids = Vec::new();
        if let Some(instances) = body
            .get("instance_list")
            .and_then(serde_json::Value::as_array)
        {
            for instance in instances {
                if let Some(simple) = instance.get("simple_entity_instance") {
                    let id = parse_id(simple)?;
                    let keyword = keyword_of(simple, "simple_record")
                        .ok_or("simple record missing its keyword")?
                        .trim_start_matches('!')
                        .to_string();
                    let params = record_params(simple.get("simple_record").ok_or("wrapper")?)?;
                    population
                        .instances
                        .push((id, Instance::Simple(Record { keyword, params })));
                    instance_ids.push(id);
                } else if let Some(complex) = instance.get("complex_entity_instance") {
                    let id = parse_id(complex)?;
                    let subs = complex
                        .get("subsuper_record")
                        .and_then(serde_json::Value::as_array)
                        .ok_or("complex record missing its subsuper record")?;
                    let mut records = Vec::new();
                    for sub in subs {
                        let record = sub
                            .get("simple_record")
                            .ok_or("part record wrapper missing")?;
                        let keyword = leaf_text(record, "keyword")
                            .ok_or("part record missing its keyword")?
                            .trim_start_matches('!')
                            .to_string();
                        let params = record_params(record)?;
                        records.push(Record { keyword, params });
                    }
                    population.instances.push((id, Instance::Complex(records)));
                    instance_ids.push(id);
                }
            }
        }
        let section_tag = body.get("section_tag");
        let (name, contexts) = match section_tag {
            Some(tag) => {
                // The ruby builder wraps a hash tag as {parameter: tag}
                // and passes an array tag through as-is.
                let entries: Vec<serde_json::Value> = match tag {
                    serde_json::Value::Object(_) => {
                        vec![serde_json::json!({"parameter": tag})]
                    }
                    serde_json::Value::Array(items) => items.clone(),
                    _ => Vec::new(),
                };
                let values = build_parameter_list(entries)?;
                let name = match values.first() {
                    Some(P21Value::Str(text)) => Some(text.clone()),
                    _ => None,
                };
                let contexts = match values.get(1) {
                    Some(P21Value::List(items)) => items
                        .iter()
                        .filter_map(|v| match v {
                            P21Value::Str(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                (name, contexts)
            }
            None => (None, Vec::new()),
        };
        population.sections.push(Section {
            name,
            contexts,
            instance_ids,
        });
    }

    if let Some(entries) = tree
        .get("anchor_section")
        .and_then(|a| a.get("anchor_entry"))
        .and_then(serde_json::Value::as_array)
    {
        for entry in entries {
            let Some(body) = entry.get("anchor_entry") else {
                continue;
            };
            let Some(label) = leaf_text(body, "anchor_label") else {
                continue;
            };
            if let Some(id) = leaf_text(body, "entity_instance_name") {
                let id = id.strip_prefix('#').unwrap_or(id);
                let id: u64 = id
                    .parse()
                    .map_err(|err| format!("bad anchor target #{id}: {err}"))?;
                population
                    .anchors
                    .push((label.to_string(), P21Value::Ref(id)));
            } else if body.get("unset").is_some() {
                population
                    .anchors
                    .push((label.to_string(), P21Value::Unset));
            }
        }
    }

    if let Some(instances) = tree
        .get("reference_section")
        .and_then(|r| r.get("instance_list"))
        .and_then(serde_json::Value::as_array)
    {
        for instance in instances {
            let Some(external) = instance.get("external_reference_instance") else {
                continue;
            };
            let id = parse_id(external)?;
            let target = leaf_text(external, "external_anchor")
                .ok_or("external reference missing its anchor")?;
            let target = target
                .strip_prefix('<')
                .and_then(|rest| rest.strip_suffix('>'))
                .ok_or("external anchor must be wrapped in <>")?
                .to_string();
            population.references.push((id, target));
        }
    }

    Ok(population)
}

/// A parameter_list capture: a hash when there is a single parameter,
/// an array of {parameter: ...} entries otherwise.
fn parameter_entries(capture: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    match capture {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|entry| entry.get("parameter"))
            .filter(|p| !p.is_null())
            .cloned()
            .collect(),
        Some(entry) => entry
            .get("parameter")
            .filter(|p| !p.is_null())
            .map(|p| vec![p.clone()])
            .unwrap_or_default(),
    }
}

fn record_params(record: &serde_json::Value) -> Result<Vec<P21Value>, String> {
    let entries = parameter_entries(record.get("parameter_list"));
    let mut out = Vec::with_capacity(entries.len());
    for param in &entries {
        out.push(build_value(param)?);
    }
    Ok(out)
}

fn build_parameter_list(entries: Vec<serde_json::Value>) -> Result<Vec<P21Value>, String> {
    // The entries arrive in the {parameter: ...} wrapper form.
    let mut out = Vec::with_capacity(entries.len());
    for entry in &entries {
        let param = entry.get("parameter").unwrap_or(entry);
        if param.is_null() {
            continue;
        }
        out.push(build_value(param)?);
    }
    Ok(out)
}

fn build_value(param: &serde_json::Value) -> Result<P21Value, String> {
    if let Some(typed) = param.get("typed_parameter") {
        let keyword = leaf_text(typed, "keyword")
            .ok_or("typed parameter missing its keyword")?
            .trim_start_matches('!')
            .to_string();
        let params = record_params(typed)?;
        return Ok(P21Value::Typed(Typed { keyword, params }));
    }
    if param.get("parameter_list").is_some() {
        let entries = parameter_entries(param.get("parameter_list"));
        let mut items = Vec::with_capacity(entries.len());
        for entry in &entries {
            items.push(build_value(entry)?);
        }
        return Ok(P21Value::List(items));
    }
    if let Some(text) = leaf_text(param, "entity_instance_name") {
        let id = text
            .strip_prefix('#')
            .ok_or("entity instance name must start with #")?;
        let id: u64 = id
            .parse()
            .map_err(|err| format!("bad reference #{id}: {err}"))?;
        return Ok(P21Value::Ref(id));
    }
    if let Some(text) = param
        .get("constant_reference")
        .and_then(serde_json::Value::as_str)
        .or_else(|| leaf_text(param, "constant_reference"))
    {
        return Ok(P21Value::Constant(text.to_string()));
    }
    if let Some(text) = leaf_text(param, "number_literal") {
        return Ok(P21Value::Number(text.to_string()));
    }
    if let Some(text) = leaf_text(param, "enumeration") {
        let name = text
            .trim_start_matches('.')
            .trim_end_matches('.')
            .to_string();
        return Ok(P21Value::Enum(name));
    }
    if let Some(text) = leaf_text(param, "binary_literal") {
        let hex = text
            .trim_start_matches('"')
            .trim_end_matches('"')
            .to_string();
        return Ok(P21Value::Binary(hex));
    }
    if let Some(quoted) = leaf_text(param, "string_literal") {
        return Ok(P21Value::Str(unescape_string(quoted)));
    }
    if param.get("unset").is_some() {
        return Ok(P21Value::Unset);
    }
    if param.get("derived").is_some() {
        return Ok(P21Value::Derived);
    }
    Err(format!("unrecognized parameter capture: {param}"))
}

// ---- the instance-level diff (#642 PR-2) ----

/// The instance-level comparison of two populations (the ruby
/// `P21::Diff`): ids added/removed, and shared ids whose record
/// keyword-or-parameter digest changed. Ids sort numerically.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PopulationDiff {
    pub added: Vec<u64>,
    pub removed: Vec<u64>,
    pub changed: Vec<u64>,
}

pub fn diff_populations(base: &FullPopulation, revised: &FullPopulation) -> PopulationDiff {
    fn find(population: &FullPopulation, id: u64) -> Option<&Instance> {
        population
            .instances
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, instance)| instance)
    }
    let mut added: Vec<u64> = revised
        .instances
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| find(base, *id).is_none())
        .collect();
    let mut removed: Vec<u64> = base
        .instances
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| find(revised, *id).is_none())
        .collect();
    let mut changed: Vec<u64> = base
        .instances
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| match (find(base, *id), find(revised, *id)) {
            (Some(b), Some(r)) => b != r,
            _ => false,
        })
        .collect();
    added.sort_unstable();
    removed.sort_unstable();
    changed.sort_unstable();
    PopulationDiff {
        added,
        removed,
        changed,
    }
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    fn rec(keyword: &str, params: Vec<P21Value>) -> Instance {
        Instance::Simple(Record {
            keyword: keyword.to_string(),
            params,
        })
    }

    #[test]
    fn diffs_added_removed_changed() {
        let base = FullPopulation {
            instances: vec![
                (1, rec("A", vec![])),
                (2, rec("B", vec![P21Value::Number("1".into())])),
                (3, rec("C", vec![])),
            ],
            ..Default::default()
        };
        let revised = FullPopulation {
            instances: vec![
                (2, rec("B", vec![P21Value::Number("2".into())])),
                (3, rec("C", vec![])),
                (4, rec("D", vec![])),
            ],
            ..Default::default()
        };
        let diff = diff_populations(&base, &revised);
        assert_eq!(diff.added, vec![4]);
        assert_eq!(diff.removed, vec![1]);
        assert_eq!(diff.changed, vec![2]);
    }
}

// ---- the JSON interchange (#642 PR-2): the tagged-value wire ----

/// Serializes a full population to the tagged-value JSON wire (the
/// ruby `P21::Json.to_value` shape): references, enumerations,
/// binaries, unset/derived and typed values stay tagged so the round
/// trip is value-identical. Numbers parse to JSON numbers — the ruby
/// lane stores Integers/Floats too, so "1.50" and "1.5" interchange
/// as the same value on both tiers.
pub fn population_to_json_value(population: &FullPopulation) -> serde_json::Value {
    serde_json::json!({
        "header": population
            .header
            .iter()
            .map(|(keyword, params)| {
                serde_json::json!({
                    "keyword": keyword,
                    "params": values_to_wire(params),
                })
            })
            .collect::<Vec<_>>(),
        "instances": population
            .instances
            .iter()
            .map(|(id, instance)| {
                (id.to_string(), instance_to_wire(id, instance))
            })
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "sections": population
            .sections
            .iter()
            .map(|section| {
                serde_json::json!({
                    "name": section.name,
                    "contexts": section.contexts,
                    "instance_ids": section
                        .instance_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
        "anchors": population
            .anchors
            .iter()
            .map(|(label, value)| {
                (label.clone(), anchor_to_wire(value))
            })
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "references": population
            .references
            .iter()
            .map(|(id, anchor)| (id.to_string(), serde_json::Value::String(format!("<{anchor}>"))))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
    })
}

/// Rebuilds a full population from the tagged-value JSON wire.
pub fn population_from_json_value(wire: &serde_json::Value) -> Result<FullPopulation, String> {
    let object = wire
        .as_object()
        .ok_or("the population wire must be an object")?;
    let mut population = FullPopulation::default();

    if let Some(header) = object.get("header").and_then(serde_json::Value::as_array) {
        for entity in header {
            let keyword = entity
                .get("keyword")
                .and_then(serde_json::Value::as_str)
                .ok_or("header entity missing its keyword")?
                .to_string();
            let params = wire_params(entity.get("params"))?;
            population.header.push((keyword, params));
        }
    }

    if let Some(instances) = object
        .get("instances")
        .and_then(serde_json::Value::as_object)
    {
        let mut parsed: Vec<(u64, Instance)> = instances
            .iter()
            .map(|(id, record)| {
                let id: u64 = id
                    .trim_start_matches('#')
                    .parse()
                    .map_err(|err| format!("bad instance id {id}: {err}"))?;
                Ok((id, instance_from_wire(id, record)?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        parsed.sort_by_key(|(id, _)| *id);
        population.instances = parsed;
    }

    if let Some(sections) = object.get("sections").and_then(serde_json::Value::as_array) {
        for section in sections {
            let name = section
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(String::from);
            let contexts = section
                .get("contexts")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let instance_ids = section
                .get("instance_ids")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| {
                            v.as_str()
                                .and_then(|text| text.trim_start_matches('#').parse().ok())
                        })
                        .collect()
                })
                .unwrap_or_default();
            population.sections.push(Section {
                name,
                contexts,
                instance_ids,
            });
        }
    }

    if let Some(anchors) = object.get("anchors").and_then(serde_json::Value::as_object) {
        for (label, anchor) in anchors {
            let value = match anchor {
                serde_json::Value::String(text) if text == "unset" => P21Value::Unset,
                serde_json::Value::String(text) if text.starts_with('#') => {
                    let id: u64 = text
                        .trim_start_matches('#')
                        .parse()
                        .map_err(|err| format!("bad anchor target {text}: {err}"))?;
                    P21Value::Ref(id)
                }
                serde_json::Value::Number(number) => P21Value::Number(number.to_string()),
                other => P21Value::Str(other.to_string()),
            };
            population.anchors.push((label.clone(), value));
        }
    }

    if let Some(references) = object
        .get("references")
        .and_then(serde_json::Value::as_object)
    {
        for (id, anchor) in references {
            let id: u64 = id
                .trim_start_matches('#')
                .parse()
                .map_err(|err| format!("bad reference id {id}: {err}"))?;
            let anchor = anchor
                .as_str()
                .ok_or("the reference anchor must be a string")?
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            population.references.push((id, anchor));
        }
    }

    Ok(population)
}

fn instance_to_wire(_id: &u64, instance: &Instance) -> serde_json::Value {
    match instance {
        Instance::Complex(records) => serde_json::json!({
            "complex": records.iter().map(simple_to_wire).collect::<Vec<_>>()
        }),
        Instance::Simple(record) => simple_to_wire(record),
    }
}

fn instance_from_wire(id: u64, wire: &serde_json::Value) -> Result<Instance, String> {
    if let Some(complex) = wire.get("complex").and_then(serde_json::Value::as_array) {
        let mut records = Vec::with_capacity(complex.len());
        for part in complex {
            records.push(simple_from_wire(part)?);
        }
        return Ok(Instance::Complex(records));
    }
    Ok(Instance::Simple(
        simple_from_wire(wire).map_err(|err| format!("#{id}: {err}"))?,
    ))
}

fn simple_to_wire(record: &Record) -> serde_json::Value {
    serde_json::json!({
        "keyword": record.keyword,
        "params": values_to_wire(&record.params),
    })
}

fn simple_from_wire(wire: &serde_json::Value) -> Result<Record, String> {
    let keyword = wire
        .get("keyword")
        .and_then(serde_json::Value::as_str)
        .ok_or("the record wire is missing its keyword")?
        .to_string();
    let params = wire_params(wire.get("params"))?;
    Ok(Record { keyword, params })
}

fn wire_params(params: Option<&serde_json::Value>) -> Result<Vec<P21Value>, String> {
    match params {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_from_wire(item)?);
            }
            Ok(out)
        }
        Some(other) => Ok(vec![value_from_wire(other)?]),
    }
}

fn values_to_wire(params: &[P21Value]) -> Vec<serde_json::Value> {
    params.iter().map(value_to_wire).collect()
}

fn value_to_wire(value: &P21Value) -> serde_json::Value {
    match value {
        P21Value::Unset => serde_json::json!({"unset": true}),
        P21Value::Derived => serde_json::json!({"derived": true}),
        P21Value::Bool(flag) => serde_json::json!({"bool": flag}),
        P21Value::Enum(name) => serde_json::json!({"enum": name}),
        P21Value::Binary(hex) => serde_json::json!({"binary": hex}),
        P21Value::Str(text) => serde_json::Value::String(text.clone()),
        P21Value::Ref(id) => serde_json::json!({"ref": format!("#{id}")}),
        P21Value::Number(text) => {
            if let Ok(int) = text.parse::<i64>() {
                serde_json::json!(int)
            } else {
                match text.parse::<f64>() {
                    Ok(float) => serde_json::json!(float),
                    Err(_) => serde_json::Value::String(text.clone()),
                }
            }
        }
        P21Value::Constant(text) => serde_json::json!({"constant": text}),
        P21Value::List(items) => serde_json::Value::Array(values_to_wire(items)),
        P21Value::Typed(typed) => serde_json::json!({
            "typed": {
                "keyword": typed.keyword,
                "value": values_to_wire(&typed.params),
            }
        }),
    }
}

fn value_from_wire(value: &serde_json::Value) -> Result<P21Value, String> {
    match value {
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_from_wire(item)?);
            }
            Ok(P21Value::List(out))
        }
        serde_json::Value::Object(map) => {
            if let Some(typed) = map.get("typed") {
                let keyword = typed
                    .get("keyword")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("the typed value is missing its keyword")?
                    .to_string();
                let params = wire_params(typed.get("value"))?;
                return Ok(P21Value::Typed(Typed { keyword, params }));
            }
            if let Some(name) = map.get("enum").and_then(serde_json::Value::as_str) {
                return Ok(P21Value::Enum(name.to_string()));
            }
            if let Some(hex) = map.get("binary").and_then(serde_json::Value::as_str) {
                return Ok(P21Value::Binary(hex.to_string()));
            }
            if map.get("unset").and_then(serde_json::Value::as_bool) == Some(true) {
                return Ok(P21Value::Unset);
            }
            if map.get("derived").and_then(serde_json::Value::as_bool) == Some(true) {
                return Ok(P21Value::Derived);
            }
            if let Some(flag) = map.get("bool").and_then(serde_json::Value::as_bool) {
                return Ok(P21Value::Bool(flag));
            }
            if let Some(text) = map.get("constant").and_then(serde_json::Value::as_str) {
                return Ok(P21Value::Constant(text.to_string()));
            }
            if let Some(text) = map.get("ref").and_then(serde_json::Value::as_str) {
                let id: u64 = text
                    .trim_start_matches('#')
                    .parse()
                    .map_err(|err| format!("bad ref {text}: {err}"))?;
                return Ok(P21Value::Ref(id));
            }
            Err(format!("unrecognized tagged value: {value}"))
        }
        serde_json::Value::Number(number) => Ok(P21Value::Number(number.to_string())),
        serde_json::Value::String(text) => Ok(P21Value::Str(text.clone())),
        other => Err(format!("unrecognized wire value: {other}")),
    }
}

fn anchor_to_wire(value: &P21Value) -> serde_json::Value {
    match value {
        P21Value::Unset => serde_json::Value::String("unset".to_string()),
        P21Value::Ref(id) => serde_json::Value::String(format!("#{id}")),
        other => value_to_wire(other),
    }
}
