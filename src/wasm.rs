//! The WebAssembly surface: the same core operations the C ABI
//! exposes (expressir#620) — schema parse, remark attach, format,
//! check, plus the Part 21 structural parse + validation — JSON in
//! and out, for schema + population checks in browser/Node. Compiled
//! with `wasm-pack build --target nodejs` (cfg wasm32 only).

use serde::Serialize;

use crate::population;

#[derive(Serialize)]
struct PopulationJson {
    file_schemas: Vec<String>,
    records: Vec<RecordJson>,
    anchors: Vec<Vec<String>>,
    references: Vec<String>,
}

#[derive(Serialize)]
struct RecordJson {
    id: u64,
    keyword: String,
    complex: bool,
}

#[derive(Serialize)]
struct VerdictJson {
    valid: bool,
    errors: Vec<String>,
}

fn population_json(population: &population::Population) -> PopulationJson {
    PopulationJson {
        file_schemas: population.file_schemas.clone(),
        records: population
            .records
            .iter()
            .map(|record| RecordJson {
                id: record.id,
                keyword: if record.complex {
                    record.parts.join("+")
                } else {
                    record.keyword.clone()
                },
                complex: record.complex,
            })
            .collect(),
        anchors: population
            .anchors
            .iter()
            .map(|(name, target)| vec![name.clone(), target.clone()])
            .collect(),
        references: population
            .references
            .iter()
            .map(|(id, target)| format!("#{id} = <{target}>"))
            .collect(),
    }
}

/// Parses an ISO-10303-21 population; returns the structural
/// inventory as JSON, or an error message.
pub fn parse_population_json(source: String) -> Result<String, String> {
    let population = population::parse_population(&source)?;
    serde_json::to_string(&population_json(&population)).map_err(|e| format!("serialize: {e}"))
}

/// Parses an EXPRESS schema into the model-hash wire JSON — the same
/// shape `expr_parse_schema` (the C ABI) returns, and the input
/// attach/format/check consume. The declaration inventory below is
/// the browser-oriented summary view.
pub fn wire_schema_json(source: String) -> Result<String, String> {
    let tree = crate::ParsedTree::parse(&source).map_err(|e| format!("{e:?}"))?;
    let value = tree
        .to_model_json("source.exp")
        .map_err(|err| format!("model json: {err:?}"))?;
    serde_json::to_string(&value).map_err(|e| format!("serialize: {e}"))
}

/// Parses an EXPRESS schema; returns the declaration inventory
/// (schema name + entity names + per-entity attribute names) as JSON.
pub fn parse_schema_json(source: String) -> Result<String, String> {
    #[derive(Serialize)]
    struct SchemaJson {
        name: String,
        entities: Vec<EntityJson>,
    }
    #[derive(Serialize)]
    struct EntityJson {
        name: String,
        attributes: Vec<String>,
    }

    let tree = crate::ParsedTree::parse(&source).map_err(|e| format!("{e:?}"))?;
    let repository = tree.to_model();
    let schemas: Vec<SchemaJson> = repository
        .schemas
        .iter()
        .map(|schema| SchemaJson {
            name: schema.name.clone(),
            entities: schema
                .entities
                .iter()
                .map(|entity| EntityJson {
                    name: entity.name.clone(),
                    attributes: entity
                        .attributes
                        .iter()
                        .map(|attribute| attribute.name.clone())
                        .collect(),
                })
                .collect(),
        })
        .collect();
    serde_json::to_string(&schemas).map_err(|e| format!("serialize: {e}"))
}

/// Validates a population against a schema's entity inventory
/// (ISO 10303-21 clause 12 keyword resolution); returns the verdict
/// as JSON.
pub fn validate_population_json(entities_json: String, source: String) -> Result<String, String> {
    let entities: Vec<String> =
        serde_json::from_str(&entities_json).map_err(|e| format!("entities: {e}"))?;
    let population = population::parse_population(&source)?;
    let verdict = population.validate(&entities);
    serde_json::to_string(&VerdictJson {
        valid: verdict.valid,
        errors: verdict.errors,
    })
    .map_err(|e| format!("serialize: {e}"))
}

/// Attaches remarks to a wire (tagged + untagged); `annotations`
/// enables the EXPRESSdoc distinction. Returns the attached wire JSON.
pub fn attach_remarks_json(
    wire_json: String,
    source: String,
    annotations: bool,
) -> Result<String, String> {
    let mut wire: serde_json::Value =
        serde_json::from_str(&wire_json).map_err(|e| format!("wire json: {e}"))?;
    crate::attach::attach_tagged_remarks(&mut wire, &source, annotations);
    crate::attach::attach_untagged_remarks(&mut wire, &source, annotations);
    serde_json::to_string(&wire).map_err(|e| format!("serialize: {e}"))
}

/// Formats a wire; `no_remarks` strips remarks. Returns the EXPRESS
/// text.
pub fn format_json(wire_json: String, no_remarks: bool) -> Result<String, String> {
    let wire: serde_json::Value =
        serde_json::from_str(&wire_json).map_err(|e| format!("wire json: {e}"))?;
    Ok(crate::formatter::format(&wire, no_remarks))
}

/// Runs the checker over a wire; `dialect` is optional. Returns the
/// findings as a JSON array.
pub fn check_json(wire_json: String, dialect: Option<String>) -> Result<String, String> {
    let mut wire: serde_json::Value =
        serde_json::from_str(&wire_json).map_err(|e| format!("wire json: {e}"))?;
    let notes = crate::checker::check(&mut wire, dialect.as_deref());
    let findings: Vec<serde_json::Value> = notes.iter().map(|note| note.to_json()).collect();
    serde_json::to_string(&findings).map_err(|e| format!("serialize: {e}"))
}

/// Diffs two ISO-10303-21 exchange structures (#642).
pub fn diff_population_json(base: String, revised: String) -> Result<String, String> {
    let base = crate::population::parse_full_population(&base)?;
    let revised = crate::population::parse_full_population(&revised)?;
    let diff = crate::population::diff_populations(&base, &revised);
    serde_json::to_string(&diff).map_err(|e| format!("serialize: {e}"))
}

/// Serializes an exchange structure to the tagged-value JSON wire (#642).
pub fn population_wire_json(source: String) -> Result<String, String> {
    let population = crate::population::parse_full_population(&source)?;
    let wire = crate::population::population_to_json_value(&population);
    serde_json::to_string(&wire).map_err(|e| format!("serialize: {e}"))
}

/// Types a population's instances against a schema wire (9.2.5, #642).
pub fn type_population_json(
    schema_wire: String,
    schema_index: usize,
    source: String,
) -> Result<String, String> {
    let wire: serde_json::Value =
        serde_json::from_str(&schema_wire).map_err(|e| format!("schema wire: {e}"))?;
    let population = crate::population::parse_full_population(&source)?;
    let report = crate::p21_typing::type_population(&wire, schema_index, &population);
    serde_json::to_string(&report).map_err(|e| format!("serialize: {e}"))
}

/// Validates a population against a schema wire (#642).
pub fn validate_population_semantic_json(
    schema_wire: String,
    schema_index: usize,
    source: String,
) -> Result<String, String> {
    let wire: serde_json::Value =
        serde_json::from_str(&schema_wire).map_err(|e| format!("schema wire: {e}"))?;
    let population = crate::population::parse_full_population(&source)?;
    let report = crate::p21_validator::validate(&wire, schema_index, &population);
    serde_json::to_string(&report).map_err(|e| format!("serialize: {e}"))
}

/// Scans an EXPRESS source for non-ASCII violations (#643).
pub fn scan_ascii_json(source: String, check_remarks: bool) -> Result<String, String> {
    let violations = crate::validate_ascii::scan_source(&source, check_remarks);
    serde_json::to_string(&violations).map_err(|e| format!("serialize: {e}"))
}

/// Parses an ISO-10303-21 exchange structure and re-emits it (#642).
pub fn round_trip_population_json(source: String) -> Result<String, String> {
    crate::population::round_trip_population(&source)
}

/// Resolves a wire's references in place (#635); returns the resolved
/// wire JSON.
pub fn resolve_json(wire_json: String) -> Result<String, String> {
    let mut wire: serde_json::Value =
        serde_json::from_str(&wire_json).map_err(|e| format!("wire json: {e}"))?;
    crate::checker::resolve_references(&mut wire);
    serde_json::to_string(&wire).map_err(|e| format!("serialize: {e}"))
}

/// Computes documentation coverage over an attached wire (#644);
/// `skip_types_json` is a JSON array of skip expressions.
pub fn coverage_json(wire_json: String, skip_types_json: String) -> Result<String, String> {
    crate::coverage::coverage_model_json(&wire_json, &skip_types_json)
}

/// Emits the clause 9 uos document for a population + schema wire
/// pair (#640); options carry id/schema_ref/target_namespace/prefix.
pub fn uos_xml_json(
    population_json: String,
    schema_json: String,
    options_json: String,
) -> Result<String, String> {
    crate::p28::uos_xml_json(&population_json, &schema_json, &options_json)
}

/// Emits the XML Schema rendering of a schema wire (#641).
pub fn xsd_xml_json(schema_json: String) -> Result<String, String> {
    crate::xsd::xsd_xml_json(&schema_json)
}

/// The LSP document-symbol rows for one document's source (#646).
pub fn lsp_symbols_json(source: String) -> Result<String, String> {
    crate::lsp::symbols_json(&source)
}

/// The LSP navigation inventory for one document's source (#646) —
/// declarations + resolved references in one native pass.
pub fn lsp_inventory_json(source: String) -> Result<String, String> {
    crate::lsp::inventory_json(&source)
}

/// The LSP rename edit rows for one document's declaration path (#646).
pub fn lsp_rename_edits_json(source: String, path: String) -> Result<String, String> {
    crate::lsp::rename_edits_json(&source, &path)
}

#[cfg(target_arch = "wasm32")]
mod bindings {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen]
    pub fn coverage(wire_json: String, skip_types_json: String) -> Result<String, String> {
        super::coverage_json(wire_json, skip_types_json)
    }

    #[wasm_bindgen]
    pub fn uos_xml(
        population_json: String,
        schema_json: String,
        options_json: String,
    ) -> Result<String, String> {
        super::uos_xml_json(population_json, schema_json, options_json)
    }

    #[wasm_bindgen]
    pub fn xsd_xml(schema_json: String) -> Result<String, String> {
        super::xsd_xml_json(schema_json)
    }

    #[wasm_bindgen]
    pub fn lsp_symbols(source: String) -> Result<String, String> {
        super::lsp_symbols_json(source)
    }

    #[wasm_bindgen]
    pub fn lsp_inventory(source: String) -> Result<String, String> {
        super::lsp_inventory_json(source)
    }

    #[wasm_bindgen]
    pub fn lsp_rename_edits(source: String, path: String) -> Result<String, String> {
        super::lsp_rename_edits_json(source, path)
    }

    #[wasm_bindgen]
    pub fn concat_schema(repository_wire: String, root_id: String) -> Result<String, String> {
        super::concat_schema_json(repository_wire, root_id)
    }

    #[wasm_bindgen]
    pub fn flatten_schema(
        repository_wire: String,
        root_id: String,
        longform_name: String,
    ) -> Result<String, String> {
        super::flatten_schema_json(repository_wire, root_id, longform_name)
    }

    #[wasm_bindgen]
    pub fn parse_population(source: String) -> Result<String, String> {
        super::parse_population_json(source)
    }

    #[wasm_bindgen]
    pub fn parse_population_recovering(source: String) -> Result<String, String> {
        super::parse_population_recovering_json(source)
    }

    #[wasm_bindgen]
    pub fn validate_population(entities_json: String, source: String) -> Result<String, String> {
        super::validate_population_json(entities_json, source)
    }

    #[wasm_bindgen]
    pub fn parse_schema(source: String) -> Result<String, String> {
        super::parse_schema_json(source)
    }

    #[wasm_bindgen]
    pub fn wire_schema(source: String) -> Result<String, String> {
        super::wire_schema_json(source)
    }

    #[wasm_bindgen]
    pub fn attach_remarks(
        wire_json: String,
        source: String,
        annotations: bool,
    ) -> Result<String, String> {
        super::attach_remarks_json(wire_json, source, annotations)
    }

    #[wasm_bindgen]
    pub fn format(wire_json: String, no_remarks: bool) -> Result<String, String> {
        super::format_json(wire_json, no_remarks)
    }

    #[wasm_bindgen]
    pub fn check(wire_json: String, dialect: Option<String>) -> Result<String, String> {
        super::check_json(wire_json, dialect)
    }

    #[wasm_bindgen]
    pub fn resolve(wire_json: String) -> Result<String, String> {
        super::resolve_json(wire_json)
    }

    #[wasm_bindgen]
    pub fn round_trip_population(source: String) -> Result<String, String> {
        super::round_trip_population_json(source)
    }

    #[wasm_bindgen]
    pub fn scan_ascii(source: String, check_remarks: bool) -> Result<String, String> {
        super::scan_ascii_json(source, check_remarks)
    }

    #[wasm_bindgen]
    pub fn diff_population(base: String, revised: String) -> Result<String, String> {
        super::diff_population_json(base, revised)
    }

    #[wasm_bindgen]
    pub fn population_wire_json(source: String) -> Result<String, String> {
        super::population_wire_json(source)
    }

    #[wasm_bindgen]
    pub fn type_population(
        schema_wire: String,
        schema_index: usize,
        source: String,
    ) -> Result<String, String> {
        super::type_population_json(schema_wire, schema_index, source)
    }

    #[wasm_bindgen]
    pub fn validate_population_semantic(
        schema_wire: String,
        schema_index: usize,
        source: String,
    ) -> Result<String, String> {
        super::validate_population_semantic_json(schema_wire, schema_index, source)
    }
}

/// The interface closure of `root_id` over a resolved repository wire,
/// plus the concatenated-artifact text — the closure sections embed the
/// files the wire names, so this form is for in-memory wires whose
/// `file` entries are readable at the caller (#636). Returns the ids
/// and the artifact as JSON `{ "ids": [...], "artifact": "..." }`.
pub fn concat_schema_json(repository_wire: String, root_id: String) -> Result<String, String> {
    let wire: serde_json::Value =
        serde_json::from_str(&repository_wire).map_err(|e| format!("wire json: {e}"))?;
    let sections = crate::expand::closure(&root_id, &wire);
    let artifact = crate::expand::write(&sections)?;
    let ids: Vec<serde_json::Value> = sections
        .iter()
        .map(|s| serde_json::Value::from(s.id.as_str()))
        .collect();
    serde_json::to_string(&serde_json::json!({ "ids": ids, "artifact": artifact }))
        .map_err(|e| format!("serialize: {e}"))
}

/// The SHTOLO longform over a resolved repository wire (#636): the
/// full stage order (G.1 copy + munges, subtype-constraint
/// elimination, prune, reductions, conversions, extensible folding,
/// provenance). Returns the longform schema wire as JSON.
pub fn flatten_schema_json(
    repository_wire: String,
    root_id: String,
    longform_name: String,
) -> Result<String, String> {
    let wire: serde_json::Value =
        serde_json::from_str(&repository_wire).map_err(|e| format!("wire json: {e}"))?;
    let longform = crate::expand::shtolo_flatten(&root_id, &wire, &longform_name);
    serde_json::to_string(&longform).map_err(|e| format!("serialize: {e}"))
}

/// The P21 recovery envelope over raw exchange-structure text (#627):
/// the population on well-formed input, the failure forensics
/// (failure wire, consumed extent, resync spans, partial records) on
/// malformed input. Returns the envelope as JSON.
pub fn parse_population_recovering_json(input: String) -> Result<String, String> {
    let recovered = crate::population::parse_population_recovering(&input)?;
    serde_json::to_string(&recovered).map_err(|e| format!("serialize: {e}"))
}
