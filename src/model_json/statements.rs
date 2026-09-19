//! Algorithmic statement emission — mirrors statement_builder.rb.

use serde_json::Value;

use parsanol::portable::{AstArena, AstNode};

use crate::walk::{hash_get, hash_pairs, nested_text};

use super::expressions::{actual_parameters_json, apply_qualifiers, build, expression_value};
use super::{
    attach_offset, children_of, node, obj, put, put_list, simple_ref, CLASS_SIMPLE_REFERENCE,
    REF_ID_KEYS,
};

const CLASS_ALIAS: &str = "Expressir::Model::Statements::Alias";
const CLASS_ASSIGNMENT: &str = "Expressir::Model::Statements::Assignment";
const CLASS_CASE: &str = "Expressir::Model::Statements::Case";
const CLASS_CASE_ACTION: &str = "Expressir::Model::Statements::CaseAction";
const CLASS_COMPOUND: &str = "Expressir::Model::Statements::Compound";
const CLASS_ESCAPE: &str = "Expressir::Model::Statements::Escape";
const CLASS_IF: &str = "Expressir::Model::Statements::If";
const CLASS_NULL: &str = "Expressir::Model::Statements::Null";
const CLASS_PROCEDURE_CALL: &str = "Expressir::Model::Statements::ProcedureCall";
const CLASS_REPEAT: &str = "Expressir::Model::Statements::Repeat";
const CLASS_RETURN: &str = "Expressir::Model::Statements::Return";
const CLASS_SKIP: &str = "Expressir::Model::Statements::Skip";

/// stmt dispatch over the first present statement alternative.
pub(crate) fn stmt_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    for (key, value) in hash_pairs(arena, n) {
        let v: Option<Value> = (match key.as_str() {
            "assignmentStmt" => assignment_json(arena, &value),
            "aliasStmt" => alias_json(arena, &value),
            "ifStmt" => if_json(arena, &value),
            "caseStmt" => case_json(arena, &value),
            "compoundStmt" => compound_json(arena, &value),
            "repeatStmt" => repeat_json(arena, &value),
            "returnStmt" => return_json(arena, &value),
            "escapeStmt" => Some(obj(node(CLASS_ESCAPE))),
            "skipStmt" => Some(obj(node(CLASS_SKIP))),
            "nullStmt" => Some(obj(node(CLASS_NULL))),
            "procedureCallStmt" => procedure_call_json(arena, &value),
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

/// A `stmt` repetition: [{stmt: {…}}, …] or {stmt: {…}}.
pub(crate) fn stmts_json(arena: &AstArena, host: &AstNode, key: &str) -> Vec<Value> {
    let Some(stmts) = hash_get(arena, host, key) else {
        return Vec::new();
    };
    children_of(arena, &stmts, "stmt")
        .into_iter()
        .filter_map(|inner| stmt_json(arena, &inner))
        .collect()
}

fn general_ref_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    for (key, value) in hash_pairs(arena, n) {
        if REF_KEYS.contains(&key.as_str()) {
            return simple_ref(arena, &value, REF_ID_KEYS);
        }
    }
    None
}

fn assignment_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_ASSIGNMENT);
    let mut r#ref = hash_get(arena, n, "generalRef").and_then(|g| general_ref_json(arena, &g))?;
    if let Some(quals) = hash_get(arena, n, "qualifier") {
        r#ref = apply_qualifiers(arena, r#ref, Some(quals))?;
    }
    m.insert("ref".into(), r#ref);
    put(
        &mut m,
        "expression",
        hash_get(arena, n, "expression").and_then(|e| expression_value(arena, &e)),
    );
    Some(obj(m))
}

fn alias_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_ALIAS);
    put(
        &mut m,
        "id",
        hash_get(arena, n, "variableId")
            .and_then(|v| nested_text(arena, &v))
            .map(Value::String),
    );
    let mut expression =
        hash_get(arena, n, "generalRef").and_then(|g| general_ref_json(arena, &g))?;
    if let Some(quals) = hash_get(arena, n, "qualifier") {
        expression = apply_qualifiers(arena, expression, Some(quals))?;
    }
    m.insert("expression".into(), expression);
    m.insert(
        "statements".into(),
        Value::Array(stmts_json(arena, n, "stmt")),
    );
    Some(obj(m))
}

fn if_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_IF);
    m.insert(
        "expression".into(),
        hash_get(arena, n, "logicalExpression").and_then(|le| build(arena, &le))?,
    );
    m.insert(
        "statements".into(),
        Value::Array(match hash_get(arena, n, "ifStmtStatements") {
            Some(then_block) => stmts_json(arena, &then_block, "stmt"),
            None => Vec::new(),
        }),
    );
    put(
        &mut m,
        "else_statements",
        hash_get(arena, n, "ifStmtElseStatements")
            .map(|else_block| Value::Array(stmts_json(arena, &else_block, "stmt"))),
    );
    Some(obj(m))
}

fn case_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_CASE);
    m.insert(
        "expression".into(),
        hash_get(arena, n, "selector")
            .and_then(|s| hash_get(arena, &s, "expression"))
            .and_then(|e| expression_value(arena, &e))?,
    );
    m.insert("actions".into(), Value::Array(case_actions_json(arena, n)));
    if hash_get(arena, n, "tOTHERWISE").is_some() {
        put(
            &mut m,
            "otherwise_statement",
            hash_get(arena, n, "stmt").and_then(|s| stmt_json(arena, &s)),
        );
    }
    Some(obj(m))
}

fn case_actions_json(arena: &AstArena, n: &AstNode) -> Vec<Value> {
    let Some(actions) = hash_get(arena, n, "caseAction") else {
        return Vec::new();
    };
    children_of(arena, &actions, "caseAction")
        .into_iter()
        .filter_map(|action| {
            let mut m = node(CLASS_CASE_ACTION);
            let mut labels = Vec::new();
            if let Some(list) = hash_get(arena, &action, "listOf_caseLabel") {
                for label in children_of(arena, &list, "caseLabel") {
                    if let Some(l) = hash_get(arena, &label, "expression")
                        .and_then(|e| expression_value(arena, &e))
                    {
                        labels.push(l);
                    }
                }
            }
            m.insert("labels".into(), Value::Array(labels));
            m.insert(
                "statement".into(),
                hash_get(arena, &action, "stmt").and_then(|s| stmt_json(arena, &s))?,
            );
            Some(obj(m))
        })
        .collect()
}

fn compound_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_COMPOUND);
    m.insert(
        "statements".into(),
        Value::Array(stmts_json(arena, n, "stmt")),
    );
    Some(obj(m))
}

fn repeat_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_REPEAT);
    if let Some(control) = hash_get(arena, n, "repeatControl") {
        if let Some(inc) = hash_get(arena, &control, "incrementControl") {
            put(
                &mut m,
                "id",
                hash_get(arena, &inc, "variableId")
                    .and_then(|v| nested_text(arena, &v))
                    .map(Value::String),
            );
            put(
                &mut m,
                "bound1",
                hash_get(arena, &inc, "bound1")
                    .and_then(|b| hash_get(arena, &b, "numericExpression"))
                    .and_then(|ne| expression_value(arena, &ne)),
            );
            put(
                &mut m,
                "bound2",
                hash_get(arena, &inc, "bound2")
                    .and_then(|b| hash_get(arena, &b, "numericExpression"))
                    .and_then(|ne| expression_value(arena, &ne)),
            );
            put(
                &mut m,
                "increment",
                hash_get(arena, &inc, "increment")
                    .and_then(|i| hash_get(arena, &i, "numericExpression"))
                    .and_then(|ne| expression_value(arena, &ne)),
            );
        }
        put(
            &mut m,
            "while_expression",
            hash_get(arena, &control, "whileControl")
                .and_then(|w| hash_get(arena, &w, "logicalExpression"))
                .and_then(|le| build(arena, &le)),
        );
        put(
            &mut m,
            "until_expression",
            hash_get(arena, &control, "untilControl")
                .and_then(|u| hash_get(arena, &u, "logicalExpression"))
                .and_then(|le| build(arena, &le)),
        );
    }
    m.insert(
        "statements".into(),
        Value::Array(stmts_json(arena, n, "stmt")),
    );
    Some(obj(m))
}

fn return_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_RETURN);
    put(
        &mut m,
        "expression",
        hash_get(arena, n, "expression").and_then(|e| expression_value(arena, &e)),
    );
    Some(obj(m))
}

fn procedure_call_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_PROCEDURE_CALL);
    let procedure = if let Some(pr) = hash_get(arena, n, "procedureRef") {
        simple_ref(arena, &pr, REF_ID_KEYS)
    } else if let Some(bip) = hash_get(arena, n, "builtInProcedure") {
        Some(keyword_ref_value(arena, &bip))
    } else {
        None
    }?;
    m.insert("procedure".into(), procedure);
    put_list(&mut m, "parameters", actual_parameters_json(arena, n));
    Some(obj(m))
}

fn keyword_ref_value(arena: &AstArena, n: &AstNode) -> Value {
    let mut m = node(CLASS_SIMPLE_REFERENCE);
    m.insert(
        "id".into(),
        Value::String(crate::walk::nested_text(arena, n).unwrap_or_default()),
    );
    obj(m)
}

const REF_KEYS: &[&str] = &[
    "constantRef",
    "functionRef",
    "generalRef",
    "parameterRef",
    "typeLabelRef",
    "variableRef",
    "attributeRef",
    "entityRef",
    "typeRef",
    "enumerationRef",
];
