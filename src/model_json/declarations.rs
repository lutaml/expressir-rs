//! Schema-level declaration emission — mirrors entity_decl_builder.rb,
//! type_decl_builder.rb, function/procedure/rule_decl_builder.rb,
//! constant_builder.rb, interface_builder.rb,
//! subtype_constraint_builder.rb, and the where/unique clause
//! builders.

use serde_json::{Map, Value};

use parsanol::portable::{AstArena, AstNode};

use crate::walk::{as_list, hash_get, hash_pairs, nested_text};

use super::data_types::{instantiable_type_json, parameter_type_json, underlying_type_json};
use super::expressions::{expression_value, qualified_attribute_json, supertype_expression_json};
use super::statements::stmts_json;
use super::{
    attach_offset, children_of, node, obj, put, put_list, simple_ref, unwrap_child, REF_ID_KEYS,
};

const CLASS_ENTITY: &str = "Expressir::Model::Declarations::Entity";
const CLASS_TYPE: &str = "Expressir::Model::Declarations::Type";
const CLASS_FUNCTION: &str = "Expressir::Model::Declarations::Function";
const CLASS_PROCEDURE: &str = "Expressir::Model::Declarations::Procedure";
const CLASS_RULE: &str = "Expressir::Model::Declarations::Rule";
const CLASS_SUBTYPE_CONSTRAINT: &str = "Expressir::Model::Declarations::SubtypeConstraint";
const CLASS_CONSTANT: &str = "Expressir::Model::Declarations::Constant";
const CLASS_VARIABLE: &str = "Expressir::Model::Declarations::Variable";
const CLASS_PARAMETER: &str = "Expressir::Model::Declarations::Parameter";
const CLASS_WHERE_RULE: &str = "Expressir::Model::Declarations::WhereRule";
const CLASS_UNIQUE_RULE: &str = "Expressir::Model::Declarations::UniqueRule";
const CLASS_ATTRIBUTE: &str = "Expressir::Model::Declarations::Attribute";
const CLASS_DERIVED_ATTRIBUTE: &str = "Expressir::Model::Declarations::DerivedAttribute";
const CLASS_INVERSE_ATTRIBUTE: &str = "Expressir::Model::Declarations::InverseAttribute";

/// The per-class collections schema_decl_builder sorts its children
/// into — the same buckets function/procedure/rule heads fill from
/// algorithmHead.
#[derive(Default)]
pub(crate) struct Declarations {
    types: Vec<Value>,
    entities: Vec<Value>,
    subtype_constraints: Vec<Value>,
    functions: Vec<Value>,
    rules: Vec<Value>,
    procedures: Vec<Value>,
}

pub(crate) fn apply(map: &mut Map<String, Value>, decls: &Declarations) {
    put_list(map, "types", decls.types.clone());
    put_list(map, "entities", decls.entities.clone());
    put_list(
        map,
        "subtype_constraints",
        decls.subtype_constraints.clone(),
    );
    put_list(map, "functions", decls.functions.clone());
    put_list(map, "rules", decls.rules.clone());
    put_list(map, "procedures", decls.procedures.clone());
}

/// schemaBody → interfaceSpecification / constantDecl /
/// schemaBodyDeclaration dispatch.
pub(crate) fn schema_declarations(arena: &AstArena, body: &AstNode, decls: &mut Declarations) {
    let Some(list) = hash_get(arena, body, "schemaBodyDeclaration") else {
        return;
    };
    for wrapped in as_list(arena, &list) {
        let Some(outer) = hash_get(arena, &wrapped, "schemaBodyDeclaration") else {
            continue;
        };
        // ruleDecl sits directly under the wrapper; the rest nest one
        // level deeper under "declaration".
        let declaration = hash_get(arena, &outer, "declaration").unwrap_or_else(|| outer.clone());
        dispatch_declaration(arena, &declaration, decls);
    }
}

fn dispatch_declaration(arena: &AstArena, declaration: &AstNode, decls: &mut Declarations) {
    for (key, value) in hash_pairs(arena, declaration) {
        let mut built = match key.as_str() {
            "entityDecl" => Some(entity_json(arena, &value)),
            "typeDecl" => Some(type_decl_json(arena, &value)),
            "functionDecl" => Some(function_json(arena, &value)),
            "procedureDecl" => Some(procedure_json(arena, &value)),
            "ruleDecl" => Some(rule_json(arena, &value)),
            "subtypeConstraintDecl" => Some(subtype_constraint_decl_json(arena, &value)),
            _ => None,
        };
        if let Some(v) = built.as_mut() {
            attach_offset(arena, &value, v);
            match key.as_str() {
                "entityDecl" => decls.entities.push(built.take().expect("built")),
                "typeDecl" => decls.types.push(built.take().expect("built")),
                "functionDecl" => decls.functions.push(built.take().expect("built")),
                "procedureDecl" => decls.procedures.push(built.take().expect("built")),
                "ruleDecl" => decls.rules.push(built.take().expect("built")),
                "subtypeConstraintDecl" => {
                    decls.subtype_constraints.push(built.take().expect("built"))
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------
// Interfaces
// ---------------------------------------------------------------------

pub(crate) fn interfaces_json(arena: &AstArena, body: &AstNode) -> Vec<Value> {
    let Some(list) = hash_get(arena, body, "interfaceSpecification") else {
        return Vec::new();
    };
    as_list(arena, &list)
        .into_iter()
        .filter_map(|wrapped| {
            let spec = hash_get(arena, &wrapped, "interfaceSpecification")?;
            let (kind, clause) = if let Some(rc) = hash_get(arena, &spec, "referenceClause") {
                ("REFERENCE", rc)
            } else {
                ("USE", hash_get(arena, &spec, "useClause")?)
            };
            let mut m = node(super::CLASS_INTERFACE);
            m.insert("kind".into(), Value::String(kind.into()));
            let mut schema = hash_get(arena, &clause, "schemaRef")
                .and_then(|sr| simple_ref(arena, &sr, REF_ID_KEYS))?;
            attach_offset(arena, &hash_get(arena, &clause, "schemaRef")?, &mut schema);
            m.insert("schema".into(), schema);
            let list_key = if kind == "REFERENCE" {
                "listOf_resourceOrRename"
            } else {
                "listOf_namedTypeOrRename"
            };
            let item_key = if kind == "REFERENCE" {
                "resourceOrRename"
            } else {
                "namedTypeOrRename"
            };
            // interface_builder always passes items: items.compact —
            // Schema#interfaced_items calls interface.items.empty?
            m.insert(
                "items".into(),
                Value::Array(interface_items(arena, &clause, list_key, item_key, kind)),
            );
            let mut v = obj(m);
            attach_offset(arena, &spec, &mut v);
            Some(v)
        })
        .collect()
}

fn interface_items(
    arena: &AstArena,
    clause: &AstNode,
    list_key: &str,
    item_key: &str,
    kind: &str,
) -> Vec<Value> {
    let Some(list) = hash_get(arena, clause, list_key) else {
        return Vec::new();
    };
    children_of(arena, &list, item_key)
        .into_iter()
        .filter_map(|item| {
            let mut m = node(super::CLASS_INTERFACE_ITEM);
            let mut r = if kind == "REFERENCE" {
                hash_get(arena, &item, "resourceRef").and_then(|rr| {
                    let (k, v) = hash_pairs(arena, &rr).first().cloned()?;
                    let mut r = simple_ref(arena, &v, REF_ID_KEYS)?;
                    attach_offset(arena, &v, &mut r);
                    if k == "entityRef" || k == "typeRef" {
                        // named resource refs attach at the innermost rule
                    }
                    Some(r)
                })
            } else {
                hash_get(arena, &item, "namedTypes").and_then(|nt| {
                    let inner = hash_get(arena, &nt, "entityRef")
                        .map(|er| (er, "entityRef"))
                        .or_else(|| hash_get(arena, &nt, "typeRef").map(|tr| (tr, "typeRef")))?;
                    let (node_val, _) = inner;
                    let mut r = simple_ref(arena, &node_val, REF_ID_KEYS)?;
                    attach_offset(arena, &node_val, &mut r);
                    Some(r)
                })
            };
            if let Some(r) = r.as_mut() {
                let _ = r;
            }
            put(&mut m, "ref", r);
            // renameId carries its own id kind (entityId/typeId/…); the
            // item id hydrates as a plain string.
            if let Some(rid) = hash_get(arena, &item, "renameId")
                .or_else(|| hash_get(arena, &item, "entityId"))
                .or_else(|| hash_get(arena, &item, "typeId"))
            {
                put(
                    &mut m,
                    "id",
                    hash_pairs(arena, &rid)
                        .first()
                        .and_then(|(_, v)| nested_text(arena, v))
                        .map(Value::String),
                );
            }
            let mut v = obj(m);
            attach_offset(arena, &item, &mut v);
            Some(v)
        })
        .collect()
}

// ---------------------------------------------------------------------
// Constants and local variables
// ---------------------------------------------------------------------

/// schemaBody.constantDecl → Constant list.
pub(crate) fn constants_json(arena: &AstArena, body: &AstNode) -> Vec<Value> {
    hash_get(arena, body, "constantDecl")
        .map(|decl| constant_body_json(arena, &decl))
        .unwrap_or_default()
}

fn constant_body_json(arena: &AstArena, decl: &AstNode) -> Vec<Value> {
    let Some(list) = hash_get(arena, decl, "constantBody") else {
        return Vec::new();
    };
    children_of(arena, &list, "constantBody")
        .into_iter()
        .filter_map(|body| {
            let mut m = node(CLASS_CONSTANT);
            m.insert(
                "id".into(),
                Value::String(wrapped_id_text(arena, &body, "constantId")?),
            );
            put(
                &mut m,
                "type",
                hash_get(arena, &body, "instantiableType")
                    .and_then(|it| instantiable_type_json(arena, &it)),
            );
            put(
                &mut m,
                "expression",
                hash_get(arena, &body, "expression").and_then(|e| expression_value(arena, &e)),
            );
            let mut v = obj(m);
            attach_offset(arena, &body, &mut v);
            Some(v)
        })
        .collect()
}

/// algorithmHead split: declaration children by class, constants, and
/// local variables — shared by function, procedure, and rule.
struct AlgorithmHead {
    decls: Declarations,
    constants: Vec<Value>,
    variables: Vec<Value>,
}

fn algorithm_head_json(arena: &AstArena, head: Option<AstNode>) -> AlgorithmHead {
    let mut out = AlgorithmHead {
        decls: Declarations::default(),
        constants: Vec::new(),
        variables: Vec::new(),
    };
    let Some(head) = head else { return out };
    for declaration in children_of(arena, &head, "declaration") {
        dispatch_declaration(arena, &declaration, &mut out.decls);
    }
    if let Some(cd) = hash_get(arena, &head, "constantDecl") {
        out.constants = constant_body_json(arena, &cd);
    }
    if let Some(ld) = hash_get(arena, &head, "localDecl") {
        out.variables = local_variables_json(arena, &ld);
    }
    out
}

fn local_variables_json(arena: &AstArena, decl: &AstNode) -> Vec<Value> {
    let Some(list) = hash_get(arena, decl, "localVariable") else {
        return Vec::new();
    };
    children_of(arena, &list, "localVariable")
        .into_iter()
        .flat_map(|var| {
            let mut ids = Vec::new();
            for id in children_of(arena, &var, "listOf_variableId") {
                for id in children_of(arena, &id, "variableId") {
                    if let Some(t) = nested_text(arena, &id) {
                        ids.push(t);
                    }
                }
            }
            let ty = hash_get(arena, &var, "parameterType")
                .and_then(|pt| parameter_type_json(arena, &pt));
            let expr =
                hash_get(arena, &var, "expression").and_then(|e| expression_value(arena, &e));
            ids.into_iter().map(move |id| {
                let mut m = node(CLASS_VARIABLE);
                m.insert("id".into(), Value::String(id));
                put(&mut m, "type", ty.clone());
                put(&mut m, "expression", expr.clone());
                obj(m)
            })
        })
        .collect()
}

/// formalParameter groups share one type across all ids.
fn formal_parameters_json(arena: &AstArena, host: &AstNode, list_key: &str) -> Vec<Value> {
    let Some(list) = hash_get(arena, host, list_key) else {
        return Vec::new();
    };
    children_of(arena, &list, "formalParameter")
        .into_iter()
        .flat_map(|param| {
            let param = unwrap_child(arena, &param, "procedureHeadParameter");
            let formal = unwrap_child(arena, &param, "formalParameter");
            let var = hash_get(arena, &param, "tVAR").is_some();
            let mut ids = Vec::new();
            for plist in children_of(arena, &formal, "listOf_parameterId") {
                for id in children_of(arena, &plist, "parameterId") {
                    if let Some(t) = nested_text(arena, &id) {
                        ids.push(t);
                    }
                }
            }
            let ty = hash_get(arena, &formal, "parameterType")
                .and_then(|pt| parameter_type_json(arena, &pt));
            ids.into_iter().map(move |id| {
                let mut m = node(CLASS_PARAMETER);
                m.insert("id".into(), Value::String(id));
                if var {
                    m.insert("var".into(), Value::Bool(true));
                }
                put(&mut m, "type", ty.clone());
                obj(m)
            })
        })
        .collect()
}

// ---------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------

fn entity_json(arena: &AstArena, n: &AstNode) -> Value {
    let head = hash_get(arena, n, "entityHead");
    let body = hash_get(arena, n, "entityBody");

    let mut m = node(CLASS_ENTITY);
    if let Some(head) = &head {
        put(
            &mut m,
            "id",
            hash_get(arena, head, "entityId")
                .and_then(|e| nested_text(arena, &e))
                .map(Value::String),
        );
        if let Some(subsuper) = hash_get(arena, head, "subsuper") {
            put_supersuper(arena, &mut m, &subsuper);
        }
    }

    if let Some(body) = &body {
        let mut attributes = Vec::new();
        if let Some(explicit) = hash_get(arena, body, "explicitAttr") {
            for group in children_of(arena, &explicit, "explicitAttr") {
                attributes.append(&mut explicit_attrs_json(arena, &group));
            }
        }
        if let Some(derive) = hash_get(arena, body, "deriveClause") {
            for attr in children_of(arena, &derive, "derivedAttr") {
                if let Some(v) = derived_attr_json(arena, &attr) {
                    attributes.push(v);
                }
            }
        }
        if let Some(inverse) = hash_get(arena, body, "inverseClause") {
            for attr in children_of(arena, &inverse, "inverseAttr") {
                if let Some(v) = inverse_attr_json(arena, &attr) {
                    attributes.push(v);
                }
            }
        }
        put_list(&mut m, "attributes", attributes);
        put_list(&mut m, "unique_rules", unique_rules_json(arena, body));
        put_list(&mut m, "where_rules", where_rules_json(arena, body));
    }
    obj(m)
}

/// SUBTYPE OF / SUPERTYPE OF / ABSTRACT handling for entity heads.
fn put_supersuper(arena: &AstArena, m: &mut Map<String, Value>, subsuper: &AstNode) {
    let mut subtype_of: Option<Value> = None;
    let mut supertype_expression: Option<Value> = None;
    let mut is_abstract = false;

    if let Some(constraint) = hash_get(arena, subsuper, "supertypeConstraint") {
        let abstract_decl = hash_get(arena, &constraint, "abstractEntityDeclaration");
        let abstract_supertype = hash_get(arena, &constraint, "abstractSupertypeDeclaration");
        is_abstract = abstract_decl.is_some() || abstract_supertype.is_some();

        if let Some(rule) = hash_get(arena, &constraint, "supertypeRule") {
            supertype_expression = hash_get(arena, &rule, "subtypeConstraint")
                .and_then(|sc| hash_get(arena, &sc, "supertypeExpression"))
                .and_then(|se| {
                    supertype_expression_json(arena, &se).map(|mut se_v| {
                        attach_offset(arena, &se, &mut se_v);
                        se_v
                    })
                });
        } else if let Some(decl) = abstract_supertype {
            if let Some(constraint) = hash_get(arena, &decl, "subtypeConstraint") {
                supertype_expression =
                    if let Some(se) = hash_get(arena, &constraint, "supertypeExpression") {
                        supertype_expression_json(arena, &se).map(|mut se_v| {
                            attach_offset(arena, &se, &mut se_v);
                            se_v
                        })
                    } else if let Some(list) = hash_get(arena, &constraint, "listOf_entityRef") {
                        // A single ref collapses to the ref itself.
                        let refs: Vec<Value> = children_of(arena, &list, "entityRef")
                            .into_iter()
                            .filter_map(|er| {
                                let mut r = simple_ref(arena, &er, REF_ID_KEYS)?;
                                attach_offset(arena, &er, &mut r);
                                Some(r)
                            })
                            .collect();
                        refs.first().cloned()
                    } else {
                        None
                    };
            }
        }
    }

    if let Some(decl) = hash_get(arena, subsuper, "subtypeDeclaration") {
        // Multi-supertype lists arrive as an array mixing tokens and
        // refs; entity_decl_builder reads only the single-hash form, so
        // those produce no subtype_of. Mirrored verbatim for parity —
        // diverging here would change observable Ruby output.
        let refs: Vec<Value> = match hash_get(arena, &decl, "listOf_entityRef") {
            Some(list) => match &list {
                parsanol::portable::AstNode::Array { .. } => Vec::new(),
                _ => children_of(arena, &list, "entityRef")
                    .into_iter()
                    .filter_map(|er| simple_ref(arena, &er, REF_ID_KEYS))
                    .collect(),
            },
            None => Vec::new(),
        };
        if !refs.is_empty() {
            subtype_of = Some(Value::Array(refs));
        }
    }

    if is_abstract {
        m.insert("abstract".into(), Value::Bool(true));
    }
    put(m, "supertype_expression", supertype_expression);
    put(m, "subtype_of", subtype_of);
}

/// One explicitAttr group: `a, b : T` contributes one attribute per
/// id, sharing the type and OPTIONAL flag.
fn explicit_attrs_json(arena: &AstArena, group: &AstNode) -> Vec<Value> {
    let ty = hash_get(arena, group, "parameterType").and_then(|pt| parameter_type_json(arena, &pt));
    let optional = hash_get(arena, group, "tOPTIONAL").is_some();

    let mut out = Vec::new();
    let decls = match hash_get(arena, group, "listOf_attributeDecl") {
        Some(holder) => children_of(arena, &holder, "attributeDecl"),
        None => children_of(arena, group, "attributeDecl"),
    };
    for decl in decls {
        let mut m = node(CLASS_ATTRIBUTE);
        m.insert("kind".into(), Value::String("EXPLICIT".into()));
        let supertype_attribute = hash_get(arena, &decl, "redeclaredAttribute").and_then(|re| {
            hash_get(arena, &re, "qualifiedAttribute").and_then(|qa| {
                qualified_attribute_json(arena, &qa).map(|mut qa_v| {
                    attach_offset(arena, &qa, &mut qa_v);
                    qa_v
                })
            })
        });
        let id = redeclared_id(arena, &decl, &supertype_attribute);
        put(&mut m, "id", id.map(Value::String));
        put(&mut m, "supertype_attribute", supertype_attribute);
        if optional {
            m.insert("optional".into(), Value::Bool(true));
        }
        put(&mut m, "type", ty.clone());
        let mut v = obj(m);
        attach_offset(arena, &decl, &mut v);
        out.push(v);
    }
    out
}

/// attribute_decl_builder id resolution: the plain attributeId, else
/// the qualified attribute ref id, overridden by an explicit rename
/// (`SELF\e.attr RENAMED name`).
fn redeclared_id(
    arena: &AstArena,
    decl: &AstNode,
    supertype_attribute: &Option<Value>,
) -> Option<String> {
    let id = hash_get(arena, decl, "attributeId")
        .and_then(|a| nested_text(arena, &a))
        .or_else(|| {
            supertype_attribute.as_ref().and_then(|sa| {
                sa.get("attribute")
                    .and_then(|a| a.get("id"))
                    .and_then(|i| i.as_str().map(|s| s.to_string()))
            })
        });
    hash_get(arena, decl, "redeclaredAttribute")
        .and_then(|re| hash_get(arena, &re, "attributeId"))
        .and_then(|a| nested_text(arena, &a))
        .or(id)
}

fn derived_attr_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_DERIVED_ATTRIBUTE);
    m.insert("kind".into(), Value::String("DERIVED".into()));
    let decl = hash_get(arena, n, "attributeDecl")?;
    let supertype_attribute = hash_get(arena, &decl, "redeclaredAttribute").and_then(|re| {
        hash_get(arena, &re, "qualifiedAttribute").and_then(|qa| {
            qualified_attribute_json(arena, &qa).map(|mut qa_v| {
                attach_offset(arena, &qa, &mut qa_v);
                qa_v
            })
        })
    });
    let id = redeclared_id(arena, &decl, &supertype_attribute);
    put(&mut m, "id", id.map(Value::String));
    put(&mut m, "supertype_attribute", supertype_attribute);
    put(
        &mut m,
        "type",
        hash_get(arena, n, "parameterType").and_then(|pt| parameter_type_json(arena, &pt)),
    );
    put(
        &mut m,
        "expression",
        hash_get(arena, n, "expression").and_then(|e| expression_value(arena, &e)),
    );
    let mut v = obj(m);
    attach_offset(arena, n, &mut v);
    Some(v)
}

fn inverse_attr_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_INVERSE_ATTRIBUTE);
    m.insert("kind".into(), Value::String("INVERSE".into()));
    let decl = hash_get(arena, n, "attributeDecl")?;
    let supertype_attribute = hash_get(arena, &decl, "redeclaredAttribute").and_then(|re| {
        hash_get(arena, &re, "qualifiedAttribute")
            .and_then(|qa| qualified_attribute_json(arena, &qa))
    });
    let id = redeclared_id(arena, &decl, &supertype_attribute);
    put(&mut m, "id", id.map(Value::String));
    put(&mut m, "supertype_attribute", supertype_attribute);
    put(
        &mut m,
        "type",
        hash_get(arena, n, "inverseAttrType").and_then(|it| inverse_attr_type_json(arena, &it)),
    );
    put(&mut m, "expression", inverse_expression_json(arena, n));
    let mut v = obj(m);
    attach_offset(arena, n, &mut v);
    Some(v)
}

/// inverseAttrType: SET/BAG [bounds] OF entity | bare entity ref.
fn inverse_attr_type_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let base = hash_get(arena, n, "entityRef").and_then(|er| simple_ref(arena, &er, REF_ID_KEYS));
    let (class, bounds) = if hash_get(arena, n, "tSET").is_some() {
        ("Expressir::Model::DataTypes::Set", true)
    } else if hash_get(arena, n, "tBAG").is_some() {
        ("Expressir::Model::DataTypes::Bag", true)
    } else {
        return base;
    };
    let mut m = node(class);
    if bounds {
        if let Some(spec) = hash_get(arena, n, "boundSpec") {
            put(
                &mut m,
                "bound1",
                hash_get(arena, &spec, "bound1")
                    .and_then(|b| hash_get(arena, &b, "numericExpression"))
                    .and_then(|ne| expression_value(arena, &ne)),
            );
            put(
                &mut m,
                "bound2",
                hash_get(arena, &spec, "bound2")
                    .and_then(|b| hash_get(arena, &b, "numericExpression"))
                    .and_then(|ne| expression_value(arena, &ne)),
            );
        }
    }
    put(&mut m, "base_type", base);
    Some(obj(m))
}

/// inverse expression: `SET OF (entity.attr)` | `attr`.
fn inverse_expression_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    if let Some(er) = hash_get(arena, n, "entityRef") {
        let mut r = simple_ref(arena, &er, REF_ID_KEYS)?;
        attach_offset(arena, &er, &mut r);
        let attr = hash_get(arena, n, "attributeRef").and_then(|ar| {
            let mut a = simple_ref(arena, &ar, REF_ID_KEYS)?;
            attach_offset(arena, &ar, &mut a);
            Some(a)
        });
        let mut m = node("Expressir::Model::References::AttributeReference");
        m.insert("ref".into(), r);
        m.insert("attribute".into(), attr?);
        return Some(obj(m));
    }
    hash_get(arena, n, "attributeRef").and_then(|ar| {
        let mut a = simple_ref(arena, &ar, REF_ID_KEYS)?;
        attach_offset(arena, &ar, &mut a);
        Some(a)
    })
}

fn where_rules_json(arena: &AstArena, host: &AstNode) -> Vec<Value> {
    let Some(clause) = hash_get(arena, host, "whereClause") else {
        return Vec::new();
    };
    let list = hash_get(arena, &clause, "listOf_domainRule")
        .or_else(|| hash_get(arena, &clause, "domainRule"));
    let Some(list) = list else {
        return Vec::new();
    };
    as_list(arena, &list)
        .into_iter()
        .filter_map(|raw| {
            // The Ruby dispatch attaches with the raw wrapper element —
            // with the depth-capped find_slice that can resolve to the
            // trailing op_delim rather than the expression.
            let inner = unwrap_child(arena, &raw, "domainRule");
            let mut m = node(CLASS_WHERE_RULE);
            let id = hash_get(arena, &inner, "ruleLabelId").and_then(|r| nested_text(arena, &r));
            put(&mut m, "id", id.map(Value::String));
            put(
                &mut m,
                "expression",
                hash_get(arena, &inner, "expression").and_then(|e| expression_value(arena, &e)),
            );
            let mut v = obj(m);
            attach_offset(arena, &raw, &mut v);
            Some(v)
        })
        .collect()
}

fn unique_rules_json(arena: &AstArena, host: &AstNode) -> Vec<Value> {
    let Some(clause) = hash_get(arena, host, "uniqueClause") else {
        return Vec::new();
    };
    let list = hash_get(arena, &clause, "listOf_uniqueRule")
        .or_else(|| hash_get(arena, &clause, "uniqueRule"));
    let Some(list) = list else {
        return Vec::new();
    };
    as_list(arena, &list)
        .into_iter()
        .filter_map(|raw| {
            let inner = unwrap_child(arena, &raw, "uniqueRule");
            let mut m = node(CLASS_UNIQUE_RULE);
            let id = hash_get(arena, &inner, "ruleLabelId").and_then(|r| nested_text(arena, &r));
            put(&mut m, "id", id.map(Value::String));
            let mut attributes = Vec::new();
            if let Some(list) = hash_get(arena, &inner, "listOf_referencedAttribute") {
                for attr in children_of(arena, &list, "referencedAttribute") {
                    let r = if let Some(ar) = hash_get(arena, &attr, "attributeRef") {
                        let mut r = simple_ref(arena, &ar, REF_ID_KEYS)?;
                        attach_offset(arena, &ar, &mut r);
                        Some(r)
                    } else if let Some(qa) = hash_get(arena, &attr, "qualifiedAttribute") {
                        qualified_attribute_json(arena, &qa)
                    } else {
                        None
                    }?;
                    attributes.push(r);
                }
            }
            put_list(&mut m, "attributes", attributes);
            let mut v = obj(m);
            attach_offset(arena, &raw, &mut v);
            Some(v)
        })
        .collect()
}

// ---------------------------------------------------------------------
// Types, functions, procedures, rules, subtype constraints
// ---------------------------------------------------------------------

fn type_decl_json(arena: &AstArena, n: &AstNode) -> Value {
    let mut m = node(CLASS_TYPE);
    put(
        &mut m,
        "id",
        hash_get(arena, n, "typeId")
            .and_then(|t| nested_text(arena, &t))
            .map(Value::String),
    );
    put(
        &mut m,
        "underlying_type",
        hash_get(arena, n, "underlyingType").and_then(|ut| underlying_type_json(arena, &ut)),
    );
    put_list(&mut m, "where_rules", where_rules_json(arena, n));
    obj(m)
}

fn function_json(arena: &AstArena, n: &AstNode) -> Value {
    let head = hash_get(arena, n, "functionHead");
    let mut m = node(CLASS_FUNCTION);
    if let Some(head) = &head {
        put(
            &mut m,
            "id",
            hash_get(arena, head, "functionId")
                .and_then(|f| nested_text(arena, &f))
                .map(Value::String),
        );
        put_list(
            &mut m,
            "parameters",
            formal_parameters_json(arena, head, "listOf_formalParameter"),
        );
        put(
            &mut m,
            "return_type",
            hash_get(arena, head, "parameterType").and_then(|pt| parameter_type_json(arena, &pt)),
        );
    }
    fill_algorithm(arena, &mut m, hash_get(arena, n, "algorithmHead"));
    put_list(&mut m, "statements", stmts_json(arena, n, "stmt"));
    obj(m)
}

fn procedure_json(arena: &AstArena, n: &AstNode) -> Value {
    let head = hash_get(arena, n, "procedureHead");
    let mut m = node(CLASS_PROCEDURE);
    if let Some(head) = &head {
        put(
            &mut m,
            "id",
            hash_get(arena, head, "procedureId")
                .and_then(|p| nested_text(arena, &p))
                .map(Value::String),
        );
        put_list(
            &mut m,
            "parameters",
            formal_parameters_json(arena, head, "listOf_procedureHeadParameter"),
        );
    }
    fill_algorithm(arena, &mut m, hash_get(arena, n, "algorithmHead"));
    put_list(&mut m, "statements", stmts_json(arena, n, "stmt"));
    obj(m)
}

fn rule_json(arena: &AstArena, n: &AstNode) -> Value {
    let head = hash_get(arena, n, "ruleHead");
    let mut m = node(CLASS_RULE);
    if let Some(head) = &head {
        put(
            &mut m,
            "id",
            hash_get(arena, head, "ruleId")
                .and_then(|r| nested_text(arena, &r))
                .map(Value::String),
        );
        let mut applies_to = Vec::new();
        if let Some(list) = hash_get(arena, head, "listOf_entityRef") {
            applies_to = children_of(arena, &list, "entityRef")
                .into_iter()
                .filter_map(|er| {
                    let mut r = simple_ref(arena, &er, REF_ID_KEYS)?;
                    attach_offset(arena, &er, &mut r);
                    Some(r)
                })
                .collect();
        }
        put_list(&mut m, "applies_to", applies_to);
    }
    fill_algorithm(arena, &mut m, hash_get(arena, n, "algorithmHead"));
    put_list(&mut m, "statements", stmts_json(arena, n, "stmt"));
    put_list(&mut m, "where_rules", where_rules_json(arena, n));
    obj(m)
}

fn subtype_constraint_decl_json(arena: &AstArena, n: &AstNode) -> Value {
    let head = hash_get(arena, n, "subtypeConstraintHead");
    let mut m = node(CLASS_SUBTYPE_CONSTRAINT);
    if let Some(head) = &head {
        put(
            &mut m,
            "id",
            hash_get(arena, head, "subtypeConstraintId")
                .and_then(|s| nested_text(arena, &s))
                .map(Value::String),
        );
        put(
            &mut m,
            "applies_to",
            hash_get(arena, head, "entityRef").and_then(|er| {
                let mut r = simple_ref(arena, &er, REF_ID_KEYS)?;
                attach_offset(arena, &er, &mut r);
                Some(r)
            }),
        );
    }
    if let Some(body) = hash_get(arena, n, "subtypeConstraintBody") {
        // subtype_constraint_builder passes abstract: false explicitly,
        // so the flag renders in to_hash either way.
        let is_abstract = hash_get(arena, &body, "abstractSupertype").is_some();
        m.insert("abstract".into(), Value::Bool(is_abstract));
        if let Some(total) = hash_get(arena, &body, "totalOver") {
            // subtype_constraint_builder reads totalOver.entityRef
            // directly, missing the listOf_entityRef level — so real
            // lists never surface. Mirrored for parity.
            let refs = match hash_get(arena, &total, "entityRef") {
                Some(list) => as_list(arena, &list)
                    .into_iter()
                    .filter_map(|er| {
                        let mut r = simple_ref(arena, &er, REF_ID_KEYS)?;
                        attach_offset(arena, &er, &mut r);
                        Some(r)
                    })
                    .collect::<Vec<_>>(),
                None => Vec::new(),
            };
            put_list(&mut m, "total_over", refs);
        }
        put(
            &mut m,
            "supertype_expression",
            hash_get(arena, &body, "supertypeExpression").and_then(|se| {
                let mut v = supertype_expression_json(arena, &se)?;
                attach_offset(arena, &se, &mut v);
                Some(v)
            }),
        );
    }
    obj(m)
}

fn fill_algorithm(arena: &AstArena, m: &mut Map<String, Value>, head: Option<AstNode>) {
    let parsed = algorithm_head_json(arena, head);
    apply(m, &parsed.decls);
    put_list(m, "constants", parsed.constants);
    put_list(m, "variables", parsed.variables);
}

fn wrapped_id_text(arena: &AstArena, host: &AstNode, key: &str) -> Option<String> {
    hash_get(arena, host, key).and_then(|v| nested_text(arena, &v))
}
