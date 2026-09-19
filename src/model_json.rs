//! Emits the sparse lutaml-model wire JSON for the surface the
//! extractor covers: the exact shape `Expressir::Model::ExpFile#to_hash`
//! produces on the Ruby side, so Ruby can hydrate the Rust-extracted
//! model with `ExpFile.from_hash` and get a model indistinguishable
//! from the Ruby parse path.
//!
//! lutaml-model omits nil/default attributes from to_hash, so the
//! emission is sparse: only keys whose value is present (non-nil,
//! non-empty) are written, and every node carries its `_class` marker.

use serde_json::{Map, Value};

use parsanol::portable::{AstArena, AstNode};

use crate::extract::{rule_id_str, rule_id_str_opt, unquote};
use crate::walk::{as_list, hash_get, text};

const CLASS_EXP_FILE: &str = "Expressir::Model::ExpFile";
const CLASS_SCHEMA: &str = "Expressir::Model::Declarations::Schema";
const CLASS_ENTITY: &str = "Expressir::Model::Declarations::Entity";
const CLASS_SCHEMA_VERSION: &str = "Expressir::Model::Declarations::SchemaVersion";
const CLASS_SIMPLE_REFERENCE: &str = "Expressir::Model::References::SimpleReference";

pub fn model_json(
    arena: &AstArena,
    root: &AstNode,
    path: &str,
) -> Result<Value, ModelJsonError> {
    let mut schemas = Vec::new();

    if let Some(syntax) = hash_get(arena, root, "syntax") {
        if let Some(schema_decls) = hash_get(arena, &syntax, "schemaDecl") {
            for wrapped in as_list(arena, &schema_decls) {
                if let Some(inner) = hash_get(arena, &wrapped, "schemaDecl") {
                    schemas.push(schema_json(arena, &inner, path)?);
                }
            }
        }
    }

    let mut root_map = Map::new();
    root_map.insert("_class".into(), Value::String(CLASS_EXP_FILE.into()));
    root_map.insert("path".into(), Value::String(path.into()));
    root_map.insert("schemas".into(), Value::Array(schemas));
    Ok(Value::Object(root_map))
}

#[derive(Debug)]
pub enum ModelJsonError {
    MissingNode,
}

fn schema_json(
    arena: &AstArena,
    decl: &AstNode,
    path: &str,
) -> Result<Value, ModelJsonError> {
    let id = rule_id_str(arena, hash_get(arena, decl, "schemaId").as_ref());

    let mut map = Map::new();
    map.insert("_class".into(), Value::String(CLASS_SCHEMA.into()));
    map.insert("id".into(), Value::String(id.clone()));
    map.insert("file".into(), Value::String(path.into()));

    let version = hash_get(arena, decl, "schemaVersionId")
        .and_then(|v| hash_get(arena, &v, "stringLiteral"))
        .and_then(|l| hash_get(arena, &l, "simpleStringLiteral"))
        .and_then(|s| hash_get(arena, &s, "str"))
        .and_then(|n| text(arena, &n))
        .map(unquote);
    if let Some(value) = version {
        let mut version_map = Map::new();
        version_map
            .insert("_class".into(), Value::String(CLASS_SCHEMA_VERSION.into()));
        version_map.insert("value".into(), Value::String(value));
        map.insert("version".into(), Value::Object(version_map));
    }

    let head = hash_get(arena, decl, "schemaBody");
    if let Some(body) = head.as_ref() {
        if let Some(entities) = entities_json(arena, body, path, &id) {
            map.insert("entities".into(), Value::Array(entities));
        }
    }

    Ok(Value::Object(map))
}

fn entities_json(
    arena: &AstArena,
    body: &AstNode,
    path: &str,
    schema_id: &str,
) -> Option<Vec<Value>> {
    let mut entities = Vec::new();

    // Entities arrive inside schemaBodyDeclaration wrappers, nested one
    // level deeper under "declaration" — the same layout extract_body
    // walks for the declaration dispatch.
    if let Some(declarations) = hash_get(arena, body, "schemaBodyDeclaration") {
        for wrapped in as_list(arena, &declarations) {
            let Some(outer) = hash_get(arena, &wrapped, "schemaBodyDeclaration") else {
                continue;
            };
            let Some(declaration) = hash_get(arena, &outer, "declaration") else {
                continue;
            };
            if let Some(inner) = hash_get(arena, &declaration, "entityDecl") {
                entities.push(entity_json(arena, &inner, path, schema_id));
            }
        }
    }

    if entities.is_empty() {
        None
    } else {
        Some(entities)
    }
}

fn entity_json(arena: &AstArena, node: &AstNode, path: &str, schema_id: &str) -> Value {
    let id = hash_get(arena, node, "entityHead")
        .and_then(|head| rule_id_str_opt(arena, hash_get(arena, &head, "entityId").as_ref()))
        .unwrap_or_default();

    let mut map = Map::new();
    map.insert("_class".into(), Value::String(CLASS_ENTITY.into()));
    map.insert("id".into(), Value::String(id.clone()));

    let subtype_of = hash_get(arena, node, "entityHead")
        .and_then(|head| hash_get(arena, &head, "subsuper"))
        .and_then(|s| hash_get(arena, &s, "subtypeDeclaration"))
        .and_then(|d| hash_get(arena, &d, "listOf_entityRef"))
        .map(|list| subtype_refs_json(arena, &list, path, schema_id));
    if let Some(refs) = subtype_of {
        map.insert("subtype_of".into(), Value::Array(refs));
    }

    Value::Object(map)
}

fn subtype_refs_json(
    arena: &AstArena,
    list: &AstNode,
    path: &str,
    schema_id: &str,
) -> Vec<Value> {
    // base_path points at the REFERENCED item: parent chain of the
    // declaring context + the reference id (Ruby builder semantics).
    as_list(arena, list)
        .into_iter()
        .filter_map(|item| {
            let entity_ref = hash_get(arena, &item, "entityRef")?;
            let id = rule_id_str(arena, hash_get(arena, &entity_ref, "entityId").as_ref());
            let base_path = format!("{path}.{schema_id}.{id}");
            let mut reference = Map::new();
            reference
                .insert("_class".into(), Value::String(CLASS_SIMPLE_REFERENCE.into()));
            reference.insert("id".into(), Value::String(id));
            reference.insert("base_path".into(), Value::String(base_path));
            Some(Value::Object(reference))
        })
        .collect()
}
