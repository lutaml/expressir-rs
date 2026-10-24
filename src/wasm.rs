//! The WebAssembly surface (TODO.compat/08): the Part 21 structural
//! parse + validation behind wasm-bindgen, JSON in and out, for
//! schema + population checks in browser/Node. Compiled with
//! `wasm-pack build --target nodejs` (cfg wasm32 only).

use serde::Serialize;

use crate::p21;

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

fn population_json(population: &p21::Population) -> PopulationJson {
    PopulationJson {
        file_schemas: population.file_schemas.clone(),
        records: population
            .records
            .iter()
            .map(|record| RecordJson {
                id: record.id,
                keyword: if record.complex {
                    record.parts.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join("+")
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
    let population = p21::parse_population(&source)?;
    serde_json::to_string(&population_json(&population))
        .map_err(|e| format!("serialize: {e}"))
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
    let population = p21::parse_population(&source)?;
    let verdict = population.validate(&entities);
    serde_json::to_string(&VerdictJson {
        valid: verdict.valid,
        errors: verdict.errors,
    })
    .map_err(|e| format!("serialize: {e}"))
}

#[cfg(target_arch = "wasm32")]
mod bindings {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen]
    pub fn parse_population(source: String) -> Result<String, String> {
        super::parse_population_json(source)
    }

    #[wasm_bindgen]
    pub fn validate_population(entities_json: String, source: String) -> Result<String, String> {
        super::validate_population_json(entities_json, source)
    }

    #[wasm_bindgen]
    pub fn parse_schema(source: String) -> Result<String, String> {
        super::parse_schema_json(source)
    }
}
