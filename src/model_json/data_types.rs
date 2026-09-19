//! Data type emission — mirrors type_builder.rb: simple, aggregation,
//! enumeration, select, and generic types behind the
//! concreteTypes/simpleTypes/… wrapper chain.

use serde_json::Value;

use parsanol::portable::{AstArena, AstNode};

use crate::walk::{hash_get, hash_pairs, nested_text};

use super::expressions::expression_value;
use super::{attach_offset, children_of, node, obj, put, simple_ref, REF_ID_KEYS};

const CLASS_AGGREGATE: &str = "Expressir::Model::DataTypes::Aggregate";
const CLASS_ARRAY: &str = "Expressir::Model::DataTypes::Array";
const CLASS_BAG: &str = "Expressir::Model::DataTypes::Bag";
const CLASS_BINARY: &str = "Expressir::Model::DataTypes::Binary";
const CLASS_BOOLEAN: &str = "Expressir::Model::DataTypes::Boolean";
const CLASS_ENUMERATION: &str = "Expressir::Model::DataTypes::Enumeration";
const CLASS_ENUMERATION_ITEM: &str = "Expressir::Model::DataTypes::EnumerationItem";
const CLASS_GENERIC: &str = "Expressir::Model::DataTypes::Generic";
const CLASS_GENERIC_ENTITY: &str = "Expressir::Model::DataTypes::GenericEntity";
const CLASS_INTEGER: &str = "Expressir::Model::DataTypes::Integer";
const CLASS_LIST: &str = "Expressir::Model::DataTypes::List";
const CLASS_LOGICAL: &str = "Expressir::Model::DataTypes::Logical";
const CLASS_NUMBER: &str = "Expressir::Model::DataTypes::Number";
const CLASS_REAL: &str = "Expressir::Model::DataTypes::Real";
const CLASS_SELECT: &str = "Expressir::Model::DataTypes::Select";
const CLASS_SET: &str = "Expressir::Model::DataTypes::Set";
const CLASS_STRING: &str = "Expressir::Model::DataTypes::String";

/// Wrapper alternatives the grammar nests concrete types behind, in
/// registry order (type_builder.build_type_wrapper iterates and
/// recurses into the first present).
const WRAPPERS: &[&str] = &[
    "concreteTypes",
    "simpleTypes",
    "aggregationTypes",
    "constructedTypes",
    "generalizedTypes",
    "instantiableType",
    "parameterType",
    "underlyingType",
    "namedTypes",
    "generalAggregationTypes",
];

/// parameterType / instantiableType / underlyingType dispatch: unwrap
/// wrapper chains, then emit the concrete type.
pub(crate) fn parameter_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    concrete_type_json(arena, n)
}

pub(crate) fn instantiable_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    concrete_type_json(arena, n)
}

pub(crate) fn underlying_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    concrete_type_json(arena, n)
}

fn concrete_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    for (key, value) in hash_pairs(arena, n) {
        let v = (match key.as_str() {
            "aggregateType" => Some(aggregate_type_json(
                arena,
                &value,
                CLASS_AGGREGATE,
                "parameterType",
            )),
            "arrayType" => Some(aggregation_type_json(arena, &value, CLASS_ARRAY)),
            "bagType" => Some(aggregation_type_json(arena, &value, CLASS_BAG)),
            "listType" => Some(aggregation_type_json(arena, &value, CLASS_LIST)),
            "setType" => Some(aggregation_type_json(arena, &value, CLASS_SET)),
            "generalArrayType" => Some(general_aggregation_json(arena, &value, CLASS_ARRAY)),
            "generalBagType" => Some(general_aggregation_json(arena, &value, CLASS_BAG)),
            "generalListType" => Some(general_aggregation_json(arena, &value, CLASS_LIST)),
            "generalSetType" => Some(general_aggregation_json(arena, &value, CLASS_SET)),
            "binaryType" => Some(width_type_json(arena, &value, CLASS_BINARY)),
            "stringType" => Some(width_type_json(arena, &value, CLASS_STRING)),
            "realType" => Some(real_type_json(arena, &value)),
            "booleanType" => Some(obj(node(CLASS_BOOLEAN))),
            "integerType" => Some(obj(node(CLASS_INTEGER))),
            "logicalType" => Some(obj(node(CLASS_LOGICAL))),
            "numberType" => Some(obj(node(CLASS_NUMBER))),
            "genericType" => Some(obj(node(CLASS_GENERIC))),
            "genericEntityType" => Some(obj(node(CLASS_GENERIC_ENTITY))),
            "enumerationType" => enumeration_type_json(arena, &value),
            "selectType" => select_type_json(arena, &value),
            "entityRef" => simple_ref(arena, &value, REF_ID_KEYS),
            "typeRef" => simple_ref(arena, &value, REF_ID_KEYS),
            k if WRAPPERS.contains(&k) => concrete_type_json(arena, &value),
            _ => None,
        })
        .map(|mut v| {
            attach_offset(arena, &value, &mut v);
            v
        });
        if v.is_some() {
            return v;
        }
    }
    None
}

/// widthSpec{width, tFIXED} for STRING/BINARY.
fn width_type_json(arena: &AstArena, n: &AstNode, class: &str) -> Value {
    let mut m = node(class);
    if let Some(spec) = hash_get(arena, n, "widthSpec") {
        put(
            &mut m,
            "width",
            hash_get(arena, &spec, "width")
                .and_then(|w| hash_get(arena, &w, "numericExpression"))
                .and_then(|ne| expression_value(arena, &ne)),
        );
        if hash_get(arena, &spec, "tFIXED").is_some() {
            m.insert("fixed".into(), Value::Bool(true));
        }
    }
    obj(m)
}

/// REAL(precision): the precisionSpec's numeric expression.
fn real_type_json(arena: &AstArena, n: &AstNode) -> Value {
    let mut m = node(CLASS_REAL);
    put(
        &mut m,
        "precision",
        hash_get(arena, n, "precisionSpec")
            .and_then(|ps| hash_get(arena, &ps, "numericExpression"))
            .and_then(|ne| expression_value(arena, &ne)),
    );
    obj(m)
}

/// LIST/SET/BAG/ARRAY [bounds] OF …, with OPTIONAL/UNIQUE flags.
///
/// build_aggregation_type always passes both flags, so Array renders
/// optional/unique (false included) and List renders unique — the
/// general_* variants (parameter-type position) omit them.
fn aggregation_type_json(arena: &AstArena, n: &AstNode, class: &str) -> Value {
    let mut m = node(class);
    put_bounds(arena, &mut m, n);
    let optional = hash_get(arena, n, "tOPTIONAL").is_some();
    let unique = hash_get(arena, n, "tUNIQUE").is_some();
    if class == CLASS_ARRAY {
        m.insert("optional".into(), Value::Bool(optional));
        m.insert("unique".into(), Value::Bool(unique));
    }
    if class == CLASS_LIST {
        m.insert("unique".into(), Value::Bool(unique));
    }
    put(
        &mut m,
        "base_type",
        hash_get(arena, n, "instantiableType")
            .or_else(|| hash_get(arena, n, "parameterType"))
            .and_then(|it| concrete_type_json(arena, &it)),
    );
    obj(m)
}

fn general_aggregation_json(arena: &AstArena, n: &AstNode, class: &str) -> Value {
    let mut m = node(class);
    put_bounds(arena, &mut m, n);
    put(
        &mut m,
        "base_type",
        hash_get(arena, n, "parameterType").and_then(|it| concrete_type_json(arena, &it)),
    );
    obj(m)
}

fn aggregate_type_json(arena: &AstArena, n: &AstNode, class: &str, inner_key: &str) -> Value {
    let mut m = node(class);
    put(
        &mut m,
        "base_type",
        hash_get(arena, n, inner_key).and_then(|it| concrete_type_json(arena, &it)),
    );
    obj(m)
}

fn put_bounds(arena: &AstArena, m: &mut serde_json::Map<String, Value>, n: &AstNode) {
    let Some(spec) = hash_get(arena, n, "boundSpec") else {
        return;
    };
    put(
        m,
        "bound1",
        hash_get(arena, &spec, "bound1")
            .and_then(|b| hash_get(arena, &b, "numericExpression"))
            .and_then(|ne| expression_value(arena, &ne)),
    );
    put(
        m,
        "bound2",
        hash_get(arena, &spec, "bound2")
            .and_then(|b| hash_get(arena, &b, "numericExpression"))
            .and_then(|ne| expression_value(arena, &ne)),
    );
}

/// ENUMERATION [EXTENSIBLE] OF (items) / BASED_ON with extensions.
fn enumeration_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_ENUMERATION);
    if hash_get(arena, n, "tEXTENSIBLE").is_some() {
        m.insert("extensible".into(), Value::Bool(true));
    }

    let (items_host, based_on) = if let Some(items) = hash_get(arena, n, "enumerationItems") {
        (Some(items), None)
    } else if let Some(ext) = hash_get(arena, n, "enumerationExtension") {
        (
            hash_get(arena, &ext, "enumerationItems"),
            hash_get(arena, &ext, "typeRef").and_then(|t| simple_ref(arena, &t, REF_ID_KEYS)),
        )
    } else {
        (None, None)
    };

    put(&mut m, "based_on", based_on);

    // type_builder always passes an array (items: []); Type#children
    // flat_maps enumeration_items, so nil here would leak into walks.
    let mut items = Vec::new();
    if let Some(host) = items_host {
        let items_raw = match hash_get(arena, &host, "listOf_enumerationItem") {
            Some(holder) => children_of(arena, &holder, "enumerationItem"),
            None => children_of(arena, &host, "enumerationItem"),
        };
        for item in items_raw {
            let id = hash_get(arena, &item, "enumerationId")
                .and_then(|e| nested_text(arena, &e))
                .or_else(|| nested_text(arena, &item))?;
            let mut em = node(CLASS_ENUMERATION_ITEM);
            em.insert("id".into(), Value::String(id));
            let mut ev = obj(em);
            attach_offset(arena, &item, &mut ev);
            items.push(ev);
        }
    }
    m.insert("items".into(), Value::Array(items));
    Some(obj(m))
}

/// SELECT [EXTENSIBLE] [GENERIC_ENTITY] (…) / BASED_ON with extensions.
fn select_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_SELECT);
    if hash_get(arena, n, "tEXTENSIBLE").is_some() {
        m.insert("extensible".into(), Value::Bool(true));
    }
    if hash_get(arena, n, "tGENERIC_ENTITY").is_some() {
        m.insert("generic_entity".into(), Value::Bool(true));
    }

    let (list_host, based_on) = if let Some(list) = hash_get(arena, n, "selectList") {
        (Some(list), None)
    } else if let Some(ext) = hash_get(arena, n, "selectExtension") {
        (
            hash_get(arena, &ext, "selectList"),
            hash_get(arena, &ext, "typeRef").and_then(|t| simple_ref(arena, &t, REF_ID_KEYS)),
        )
    } else {
        (None, None)
    };

    put(&mut m, "based_on", based_on);

    // Same array-always contract as enumeration items.
    let mut items = Vec::new();
    if let Some(host) = list_host {
        if let Some(named) = hash_get(arena, &host, "listOf_namedTypes") {
            for nt in children_of(arena, &named, "namedTypes") {
                let r = if let Some(er) = hash_get(arena, &nt, "entityRef") {
                    simple_ref(arena, &er, REF_ID_KEYS)
                } else {
                    hash_get(arena, &nt, "typeRef")
                        .and_then(|tr| simple_ref(arena, &tr, REF_ID_KEYS))
                };
                if let Some(r) = r {
                    items.push(r);
                }
            }
        }
    }
    m.insert("items".into(), Value::Array(items));
    Some(obj(m))
}
