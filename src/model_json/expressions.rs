//! Expression-chain, literal, reference, qualifier, and supertype
//! expression emission — mirrors expression_builder.rb,
//! literal_builder.rb, qualifier_builder.rb, and the supertype half of
//! subtype_constraint_builder.rb.

use serde_json::Value;

use parsanol::portable::{AstArena, AstNode};

use crate::walk::{hash_get, hash_pairs, nested_text};

use super::{
    attach_offset, children_of, node, obj, put, simple_ref, CLASS_SIMPLE_REFERENCE, REF_ID_KEYS,
};

pub(crate) const CLASS_BINARY_EXPRESSION: &str = "Expressir::Model::Expressions::BinaryExpression";
pub(crate) const CLASS_UNARY_EXPRESSION: &str = "Expressir::Model::Expressions::UnaryExpression";
pub(crate) const CLASS_FUNCTION_CALL: &str = "Expressir::Model::Expressions::FunctionCall";
pub(crate) const CLASS_INTERVAL: &str = "Expressir::Model::Expressions::Interval";
pub(crate) const CLASS_QUERY_EXPRESSION: &str = "Expressir::Model::Expressions::QueryExpression";
pub(crate) const CLASS_AGGREGATE_INITIALIZER: &str =
    "Expressir::Model::Expressions::AggregateInitializer";
pub(crate) const CLASS_ATTRIBUTE_REFERENCE: &str =
    "Expressir::Model::References::AttributeReference";
pub(crate) const CLASS_GROUP_REFERENCE: &str = "Expressir::Model::References::GroupReference";
pub(crate) const CLASS_INDEX_REFERENCE: &str = "Expressir::Model::References::IndexReference";
pub(crate) const CLASS_BINARY_SUPERTYPE: &str =
    "Expressir::Model::SupertypeExpressions::BinarySupertypeExpression";
pub(crate) const CLASS_ONEOF_SUPERTYPE: &str =
    "Expressir::Model::SupertypeExpressions::OneofSupertypeExpression";

/// Builder.build of an expression-bearing node: dispatches on the
/// first present alternative, mirroring the registry. Token nodes
/// produce nil and are skipped.
pub(crate) fn build(arena: &AstArena, n: &AstNode) -> Option<Value> {
    match n {
        AstNode::Hash { .. } => {
            for (key, value) in hash_pairs(arena, n) {
                if let Some(mut v) = build_key(arena, &key, &value) {
                    attach_offset(arena, &value, &mut v);
                    return Some(v);
                }
            }
            None
        }
        _ => None,
    }
}

/// Builder.build({expression: e}) equivalent: expression_json plus the
/// source-offset stamp the Ruby dispatch attaches with the expression
/// subtree.
pub(crate) fn expression_value(arena: &AstArena, e: &AstNode) -> Option<Value> {
    let mut v = expression_json(arena, e)?;
    attach_offset(arena, e, &mut v);
    Some(v)
}

/// The registry dispatch for one node name; `value` is the node as it
/// appears under that name (the name key is already consumed).
fn build_key(arena: &AstArena, key: &str, value: &AstNode) -> Option<Value> {
    match key {
        "expression" => expression_json(arena, value),
        "logicalExpression" | "numericExpression" => hash_get(arena, value, "simpleExpression")
            .and_then(|se| simple_expression_json(arena, &se)),
        "simpleExpression" => simple_expression_json(arena, value),
        "term" => term_json(arena, value),
        "factor" => {
            hash_get(arena, value, "simpleFactor").and_then(|sf| simple_factor_json(arena, &sf))
        }
        "simpleFactor" => simple_factor_json(arena, value),
        "primary" => primary_json(arena, value),
        "literal" => literal_json(arena, value),
        "integerLiteral" | "realLiteral" | "binaryLiteral" => literal_value_json(arena, key, value),
        "logicalLiteral" => logical_literal_json(arena, value),
        "stringLiteral" => string_literal_json(arena, value),
        "builtInConstant" | "builtInFunction" | "builtInProcedure" => {
            Some(keyword_ref_value(arena, value))
        }
        "attributeRef" | "constantRef" | "entityRef" | "enumerationRef" | "functionRef"
        | "parameterRef" | "procedureRef" | "schemaRef" | "typeRef" | "variableRef" => {
            simple_ref(arena, value, REF_ID_KEYS)
        }
        "generalRef" => general_ref_json(arena, value),
        "typeLabelRef" => {
            let id = nested_text(arena, value)?;
            let mut m = node(CLASS_SIMPLE_REFERENCE);
            m.insert("id".into(), Value::String(id));
            Some(obj(m))
        }
        "functionCall" => function_call_json(arena, value),
        "entityConstructor" => entity_constructor_json(arena, value),
        "aggregateInitializer" => aggregate_initializer_json(arena, value),
        "queryExpression" => query_expression_json(arena, value),
        "interval" => interval_json(arena, value),
        "enumerationReference" => enumeration_reference_json(arena, value),
        "qualifiedAttribute" => qualified_attribute_json(arena, value),
        _ => None,
    }
}

/// expression = [simpleExpression | logicalExpression] [relOpExtended rhs]
pub(crate) fn expression_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let left = if let Some(se) = hash_get(arena, n, "simpleExpression") {
        simple_expression_json(arena, &se)
    } else if let Some(le) = hash_get(arena, n, "logicalExpression") {
        hash_get(arena, &le, "simpleExpression").and_then(|se| simple_expression_json(arena, &se))
    } else {
        None
    }?;

    let rel = hash_get(arena, n, "relOpExtended");
    let rhs = hash_get(arena, n, "rhs");
    if let (Some(rel), Some(rhs)) = (rel, rhs) {
        let operator = rel_op_string(arena, &rel)?;
        let operand2 = build(arena, &rhs)
            .or_else(|| single_child(arena, &rhs).and_then(|c| build(arena, &c)))?;
        let mut m = node(CLASS_BINARY_EXPRESSION);
        m.insert("operator".into(), Value::String(operator));
        m.insert("operand1".into(), left);
        m.insert("operand2".into(), operand2);
        return Some(obj(m));
    }
    Some(left)
}

/// simpleExpression = term (addLikeOp term)* — left-folded
/// BinaryExpressions; term folds its multiplication chain first.
pub(crate) fn simple_expression_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let first = hash_get(arena, n, "term").and_then(|t| term_json(arena, &t))?;
    Some(fold_rhs(arena, hash_get(arena, n, "rhs"), first, "term"))
}

fn term_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let first = hash_get(arena, n, "factor")
        .and_then(|f| hash_get(arena, &f, "simpleFactor"))
        .and_then(|sf| simple_factor_json(arena, &sf))?;
    Some(fold_rhs(arena, hash_get(arena, n, "rhs"), first, "factor"))
}

/// Folds a `rhs` repetition (items of {operator, <kind>}) into
/// left-nested BinaryExpressions.
fn fold_rhs(arena: &AstArena, rhs: Option<AstNode>, first: Value, kind: &str) -> Value {
    let Some(rhs) = rhs else { return first };
    let mut result = first;
    for item in children_of(arena, &rhs, "item") {
        // simpleExpression items nest {operator: {addLikeOp}}, term
        // items carry multiplicationLikeOp directly (expression_builder
        // reads both shapes).
        let operator = hash_get(arena, &item, "operator")
            .and_then(|op| single_child(arena, &op))
            .and_then(|inner| op_string(arena, &inner, OP_TABLE))
            .or_else(|| {
                hash_get(arena, &item, "multiplicationLikeOp")
                    .and_then(|op| op_string(arena, &op, OP_TABLE))
            });
        let operand = match kind {
            "term" => hash_get(arena, &item, "term").and_then(|t| term_json(arena, &t)),
            _ => hash_get(arena, &item, "factor")
                .and_then(|f| hash_get(arena, &f, "simpleFactor"))
                .and_then(|sf| simple_factor_json(arena, &sf)),
        };
        match (operator, operand) {
            (Some(op), Some(operand)) => {
                result = binary(CLASS_BINARY_EXPRESSION, &op, result, operand);
            }
            _ => break,
        }
    }
    result
}

/// The first child of a single-key hash, used to descend the operator
/// nesting ({operator: {addLikeOp: {...}}} → {op_plus: {...}}).
fn single_child(arena: &AstArena, n: &AstNode) -> Option<AstNode> {
    match n {
        AstNode::Hash { .. } => match hash_pairs(arena, n).as_slice() {
            [(_, v)] => Some(v.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn binary(class: &str, operator: &str, operand1: Value, operand2: Value) -> Value {
    let mut m = node(class);
    m.insert("operator".into(), Value::String(operator.to_string()));
    m.insert("operand1".into(), operand1);
    m.insert("operand2".into(), operand2);
    obj(m)
}

fn simple_factor_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    if let Some(p) = hash_get(arena, n, "primary") {
        return primary_json(arena, &p);
    }
    if let Some(sfe) = hash_get(arena, n, "simpleFactorExpression") {
        if let Some(p) = hash_get(arena, &sfe, "primary") {
            return primary_json(arena, &p);
        }
        return hash_get(arena, &sfe, "expression").and_then(|e| expression_json(arena, &e));
    }
    if let Some(sfue) = hash_get(arena, n, "simpleFactorUnaryExpression") {
        return simple_factor_unary_json(arena, &sfue);
    }
    if let Some(cf) = hash_get(arena, n, "constantFactor") {
        return build(arena, &cf);
    }
    if let Some(ai) = hash_get(arena, n, "aggregateInitializer") {
        return aggregate_initializer_json(arena, &ai);
    }
    if let Some(qe) = hash_get(arena, n, "queryExpression") {
        return query_expression_json(arena, &qe);
    }
    if let Some(ec) = hash_get(arena, n, "entityConstructor") {
        return entity_constructor_json(arena, &ec);
    }
    if let Some(iv) = hash_get(arena, n, "interval") {
        return interval_json(arena, &iv);
    }
    if let Some(sl) = hash_get(arena, n, "stringLiteral") {
        return string_literal_json(arena, &sl);
    }
    if let Some(er) = hash_get(arena, n, "enumerationReference") {
        return enumeration_reference_json(arena, &er);
    }
    None
}

fn simple_factor_unary_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let operator = hash_get(arena, n, "unaryOp").and_then(|op| op_string(arena, &op, UNARY_OPS));
    let operand = if let Some(sf) = hash_get(arena, n, "simpleFactor") {
        simple_factor_json(arena, &sf)
    } else if let Some(sfe) = hash_get(arena, n, "simpleFactorExpression") {
        if let Some(p) = hash_get(arena, &sfe, "primary") {
            primary_json(arena, &p)
        } else {
            hash_get(arena, &sfe, "expression").and_then(|e| expression_json(arena, &e))
        }
    } else {
        hash_get(arena, n, "primary").and_then(|p| primary_json(arena, &p))
    };

    match operator {
        Some(op) => {
            let mut m = node(CLASS_UNARY_EXPRESSION);
            m.insert("operator".into(), Value::String(op));
            m.insert("operand".into(), operand?);
            Some(obj(m))
        }
        None => operand,
    }
}

fn primary_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    if let Some(l) = hash_get(arena, n, "literal") {
        return literal_json(arena, &l);
    }
    if let Some(qf) = hash_get(arena, n, "qualifiableFactor") {
        return qualifiable_factor_json(arena, &qf, hash_get(arena, n, "qualifier"));
    }
    if let Some(pop) = hash_get(arena, n, "population") {
        let factor = hash_get(arena, &pop, "entityRef")
            .and_then(|er| simple_ref(arena, &er, REF_ID_KEYS))?;
        return apply_qualifiers(arena, factor, hash_get(arena, n, "qualifier"));
    }
    if let Some(e) = hash_get(arena, n, "expression") {
        return expression_json(arena, &e);
    }
    None
}

/// qualifiableFactor + qualifier repetition → SimpleReference wrapped
/// left-to-right in Attribute/Group/IndexReference. Qualifiers reach
/// here only through `primary`; the bare qualifiable_factor registry
/// entry passes none.
fn qualifiable_factor_json(
    arena: &AstArena,
    n: &AstNode,
    qualifiers: Option<AstNode>,
) -> Option<Value> {
    let base = if let Some(cf) = hash_get(arena, n, "constantFactor") {
        build(arena, &cf)
    } else if let Some(fc) = hash_get(arena, n, "functionCall") {
        function_call_json(arena, &fc)
    } else {
        REF_KEYS.iter().find_map(|key| {
            hash_get(arena, n, key).and_then(|v| match *key {
                "generalRef" => general_ref_json(arena, &v),
                _ => simple_ref(arena, &v, REF_ID_KEYS),
            })
        })
    }?;
    apply_qualifiers(arena, base, qualifiers)
}

/// Applies qualifier nodes left-to-right, wrapping the reference.
pub(crate) fn apply_qualifiers(
    arena: &AstArena,
    base: Value,
    qualifiers: Option<AstNode>,
) -> Option<Value> {
    let Some(qualifiers) = qualifiers else {
        return Some(base);
    };
    let mut result = base;
    for q in children_of(arena, &qualifiers, "qualifier") {
        if let Some(gq) = hash_get(arena, &q, "groupQualifier") {
            let mut m = node(CLASS_GROUP_REFERENCE);
            m.insert("ref".into(), result);
            m.insert(
                "entity".into(),
                hash_get(arena, &gq, "entityRef")
                    .and_then(|er| simple_ref(arena, &er, REF_ID_KEYS))?,
            );
            result = obj(m);
        } else if let Some(aq) = hash_get(arena, &q, "attributeQualifier") {
            let mut m = node(CLASS_ATTRIBUTE_REFERENCE);
            m.insert("ref".into(), result);
            m.insert(
                "attribute".into(),
                hash_get(arena, &aq, "attributeRef")
                    .and_then(|ar| simple_ref(arena, &ar, REF_ID_KEYS))?,
            );
            result = obj(m);
        } else if let Some(iq) = hash_get(arena, &q, "indexQualifier") {
            let mut m = node(CLASS_INDEX_REFERENCE);
            m.insert("ref".into(), result);
            m.insert(
                "index1".into(),
                hash_get(arena, &iq, "index1").and_then(|i| index_json(arena, &i))?,
            );
            put(
                &mut m,
                "index2",
                hash_get(arena, &iq, "index2").and_then(|i| index_json(arena, &i)),
            );
            result = obj(m);
        }
    }
    Some(result)
}

/// index1/index2 nodes nest `index` → numericExpression.
fn index_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    hash_get(arena, n, "index")
        .or(Some(n.clone()))
        .and_then(|i| hash_get(arena, &i, "numericExpression"))
        .and_then(|ne| expression_json(arena, &ne))
}

fn general_ref_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    for (key, value) in hash_pairs(arena, n) {
        if REF_KEYS.contains(&key.as_str()) {
            return simple_ref(arena, &value, REF_ID_KEYS);
        }
    }
    None
}

fn function_call_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let func = if let Some(bif) = hash_get(arena, n, "builtInFunction") {
        Some(keyword_ref_value(arena, &bif))
    } else {
        hash_get(arena, n, "functionRef").and_then(|fr| simple_ref(arena, &fr, REF_ID_KEYS))
    }?;
    let params = actual_parameters_json(arena, n);
    if params.is_empty() {
        return Some(func);
    }
    let mut m = node(CLASS_FUNCTION_CALL);
    m.insert("function".into(), func);
    m.insert("parameters".into(), Value::Array(params));
    Some(obj(m))
}

fn entity_constructor_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let func =
        hash_get(arena, n, "entityRef").and_then(|er| simple_ref(arena, &er, REF_ID_KEYS))?;
    let params = actual_parameters_json(arena, n);
    let mut m = node(CLASS_FUNCTION_CALL);
    m.insert("function".into(), func);
    if !params.is_empty() {
        m.insert("parameters".into(), Value::Array(params));
    }
    Some(obj(m))
}

/// actualParameterList.listOf_parameter[].parameter → expression.
pub(crate) fn actual_parameters_json(arena: &AstArena, n: &AstNode) -> Vec<Value> {
    let Some(list) = hash_get(arena, n, "actualParameterList") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for param in children_of(arena, &list, "listOf_parameter") {
        for param in children_of(arena, &param, "parameter") {
            if let Some(expr) =
                hash_get(arena, &param, "expression").and_then(|e| expression_json(arena, &e))
            {
                out.push(expr);
            }
        }
    }
    out
}

fn query_expression_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut m = node(CLASS_QUERY_EXPRESSION);
    put(
        &mut m,
        "id",
        hash_get(arena, n, "variableId")
            .and_then(|v| nested_text(arena, &v))
            .map(Value::String),
    );
    put(
        &mut m,
        "aggregate_source",
        hash_get(arena, n, "aggregateSource")
            .and_then(|a| hash_get(arena, &a, "simpleExpression"))
            .and_then(|se| simple_expression_json(arena, &se)),
    );
    // logicalExpression content dispatches generically ({expression: …}
    // or {simpleExpression: …}) — Builder.build_optional semantics.
    m.insert(
        "expression".into(),
        hash_get(arena, n, "logicalExpression").and_then(|le| build(arena, &le))?,
    );
    Some(obj(m))
}

fn aggregate_initializer_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let mut items = Vec::new();
    if let Some(list) = hash_get(arena, n, "listOf_element") {
        // The Ruby-side tree wraps multi-occurrence elements in
        // rule-key hashes, routing them through build_element (which
        // keeps `[a : b, c : d]` repetition); a single occurrence merges
        // into the holder and dispatches on its first key, dropping the
        // repetition (`[4:2]` yields the bare expression). Both shapes
        // are mirrored here.
        match &list {
            AstNode::Array { .. } => {
                for element in children_of(arena, &list, "element") {
                    let expression = hash_get(arena, &element, "expression")
                        .and_then(|e| expression_value(arena, &e))?;
                    if let Some(rep) = hash_get(arena, &element, "repetition") {
                        let mut m = node("Expressir::Model::Expressions::AggregateInitializerItem");
                        m.insert("expression".into(), expression);
                        put(
                            &mut m,
                            "repetition",
                            hash_get(arena, &rep, "numericExpression")
                                .and_then(|ne| expression_value(arena, &ne)),
                        );
                        items.push(obj(m));
                    } else {
                        items.push(expression);
                    }
                }
            }
            _ => {
                for element in children_of(arena, &list, "element") {
                    items.push(build(arena, &element)?);
                }
            }
        }
    }
    let mut m = node(CLASS_AGGREGATE_INITIALIZER);
    m.insert("items".into(), Value::Array(items));
    Some(obj(m))
}

fn interval_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let simple = |key: &str| -> Option<Value> {
        hash_get(arena, n, key)
            .and_then(|v| hash_get(arena, &v, "simpleExpression"))
            .and_then(|se| simple_expression_json(arena, &se))
    };
    let mut m = node(CLASS_INTERVAL);
    m.insert("low".into(), simple("intervalLow")?);
    m.insert(
        "operator1".into(),
        Value::String(interval_op_string(
            arena,
            &hash_get(arena, n, "intervalOp")?,
        )?),
    );
    m.insert("item".into(), simple("intervalItem")?);
    put(
        &mut m,
        "operator2",
        hash_get(arena, n, "intervalOp2")
            .and_then(|op| interval_op_string(arena, &op))
            .map(Value::String),
    );
    m.insert("high".into(), simple("intervalHigh")?);
    Some(obj(m))
}

// ---------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------

fn literal_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    for (key, value) in hash_pairs(arena, n) {
        let v = match key.as_str() {
            "integerLiteral" | "realLiteral" | "binaryLiteral" => {
                literal_value_json(arena, &key, &value)
            }
            "logicalLiteral" => logical_literal_json(arena, &value),
            "stringLiteral" => string_literal_json(arena, &value),
            _ => None,
        };
        if v.is_some() {
            return v;
        }
    }
    None
}

fn literal_value_json(arena: &AstArena, key: &str, n: &AstNode) -> Option<Value> {
    let value = nested_text(arena, n)?;
    let class = match key {
        "integerLiteral" => "Expressir::Model::Literals::Integer",
        "realLiteral" => "Expressir::Model::Literals::Real",
        _ => "Expressir::Model::Literals::Binary",
    };
    let mut m = node(class);
    m.insert("value".into(), Value::String(value));
    Some(obj(m))
}

fn logical_literal_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let value = if hash_get(arena, n, "tTRUE").is_some() {
        "TRUE"
    } else if hash_get(arena, n, "tFALSE").is_some() {
        "FALSE"
    } else if hash_get(arena, n, "tUNKNOWN").is_some() {
        "UNKNOWN"
    } else {
        return None;
    };
    let mut m = node("Expressir::Model::Literals::Logical");
    m.insert("value".into(), Value::String(value.into()));
    Some(obj(m))
}

fn string_literal_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let (raw, encoded) = if let Some(s) = hash_get(arena, n, "simpleStringLiteral") {
        (nested_text(arena, &s)?, false)
    } else if let Some(s) = hash_get(arena, n, "encodedStringLiteral") {
        (nested_text(arena, &s)?, true)
    } else {
        return None;
    };
    let mut m = node("Expressir::Model::Literals::String");
    // literal_builder strips one surrounding pair of either quote kind
    let stripped = if raw.len() >= 2
        && ((raw.starts_with('\'') && raw.ends_with('\''))
            || (raw.starts_with('"') && raw.ends_with('"')))
    {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw
    };
    m.insert("value".into(), Value::String(stripped));
    if encoded {
        m.insert("encoded".into(), Value::Bool(true));
    }
    Some(obj(m))
}

// ---------------------------------------------------------------------
// Supertype expressions
// ---------------------------------------------------------------------

pub(crate) fn supertype_expression_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let base = hash_get(arena, n, "supertypeFactor")?;
    let mut factors = vec![supertype_factor_json(arena, &base)?];
    if let Some(rhs) = hash_get(arena, n, "rhs") {
        for item in children_of(arena, &rhs, "item") {
            if let Some(f) = hash_get(arena, &item, "supertypeFactor") {
                factors.push(supertype_factor_json(arena, &f)?);
            }
        }
    }

    if factors.len() == 1 {
        return factors.pop();
    }
    let mut result = factors[0].clone();
    for next in &factors[1..] {
        result = binary(CLASS_BINARY_SUPERTYPE, "ANDOR", result, next.clone());
    }
    Some(result)
}

fn supertype_factor_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let base = hash_get(arena, n, "supertypeTerm")?;
    let mut terms = vec![supertype_term_json(arena, &base)?];
    if let Some(rhs) = hash_get(arena, n, "rhs") {
        for item in children_of(arena, &rhs, "item") {
            if let Some(t) = hash_get(arena, &item, "supertypeTerm") {
                terms.push(supertype_term_json(arena, &t)?);
            }
        }
    }

    if terms.len() == 1 {
        return terms.pop();
    }
    let mut result = terms[0].clone();
    for next in &terms[1..] {
        result = binary(CLASS_BINARY_SUPERTYPE, "AND", result, next.clone());
    }
    Some(result)
}

fn supertype_term_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    if let Some(er) = hash_get(arena, n, "entityRef") {
        return simple_ref(arena, &er, REF_ID_KEYS);
    }
    if let Some(one) = hash_get(arena, n, "oneOf") {
        let mut operands = Vec::new();
        if let Some(list) = hash_get(arena, &one, "listOf_supertypeExpression") {
            for expr in children_of(arena, &list, "supertypeExpression") {
                if let Some(v) = supertype_expression_json(arena, &expr) {
                    operands.push(v);
                }
            }
        }
        let mut m = node(CLASS_ONEOF_SUPERTYPE);
        m.insert("operands".into(), Value::Array(operands));
        return Some(obj(m));
    }
    if let Some(expr) = hash_get(arena, n, "supertypeExpression") {
        return supertype_expression_json(arena, &expr);
    }
    None
}

// ---------------------------------------------------------------------
// Operators and qualified attribute
// ---------------------------------------------------------------------

const OP_TABLE: &[(&str, &str)] = &[
    ("op_plus", "ADDITION"),
    ("op_minus", "SUBTRACTION"),
    ("tOR", "OR"),
    ("tXOR", "XOR"),
    ("op_asterisk", "MULTIPLICATION"),
    ("op_slash", "REAL_DIVISION"),
    ("tDIV", "INTEGER_DIVISION"),
    ("tMOD", "MODULO"),
    ("tAND", "AND"),
    ("op_double_pipe", "COMBINE"),
];

const UNARY_OPS: &[(&str, &str)] = &[("op_plus", "PLUS"), ("op_minus", "MINUS"), ("tNOT", "NOT")];

const REL_OPS: &[(&str, &str)] = &[
    ("op_equals", "EQUAL"),
    ("t_equal", "EQUAL"),
    ("op_less_greater", "NOT_EQUAL"),
    ("t_not_equal", "NOT_EQUAL"),
    ("op_less_than", "LESS_THAN"),
    ("t_less_than", "LESS_THAN"),
    ("op_greater_than", "GREATER_THAN"),
    ("t_greater_than", "GREATER_THAN"),
    ("op_less_equal", "LESS_THAN_OR_EQUAL"),
    ("t_less_than_or_equal", "LESS_THAN_OR_EQUAL"),
    ("op_greater_equal", "GREATER_THAN_OR_EQUAL"),
    ("t_greater_than_or_equal", "GREATER_THAN_OR_EQUAL"),
    ("op_colon_equals_colon", "INSTANCE_EQUAL"),
    ("t_instance_equal", "INSTANCE_EQUAL"),
    ("op_colon_less_greater_colon", "INSTANCE_NOT_EQUAL"),
    ("t_instance_not_equal", "INSTANCE_NOT_EQUAL"),
];

const INTERVAL_OPS: &[(&str, &str)] = &[
    ("op_less_than", "LESS_THAN"),
    ("op_less_equal", "LESS_THAN_OR_EQUAL"),
];

fn op_string(arena: &AstArena, n: &AstNode, table: &[(&str, &str)]) -> Option<String> {
    for (key, name) in table {
        if hash_get(arena, n, key).is_some() {
            return Some((*name).to_string());
        }
    }
    nested_text(arena, n).map(|t| t.trim().to_uppercase())
}

fn rel_op_string(arena: &AstArena, rel_extended: &AstNode) -> Option<String> {
    // relOpExtended = rel_op | t_in | t_like; the relOp value is the
    // concrete operator node ({op_equals: …}) already.
    match hash_get(arena, rel_extended, "relOp") {
        Some(rel) => op_string(arena, &rel, REL_OPS),
        None => op_string(arena, rel_extended, REL_OPS),
    }
}

fn interval_op_string(arena: &AstArena, op_node: &AstNode) -> Option<String> {
    // intervalOp nodes nest one {intervalOp: …} wrapper or arrive raw.
    let inner = hash_get(arena, op_node, "intervalOp").unwrap_or_else(|| op_node.clone());
    op_string(arena, &inner, INTERVAL_OPS)
}

/// SimpleReference over a keyword node (built-ins): id = nested text.
fn keyword_ref_value(arena: &AstArena, n: &AstNode) -> Value {
    let mut m = node(CLASS_SIMPLE_REFERENCE);
    m.insert(
        "id".into(),
        Value::String(nested_text(arena, n).unwrap_or_default()),
    );
    obj(m)
}

fn enumeration_reference_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let type_ref = hash_get(arena, n, "typeRef").and_then(|t| simple_ref(arena, &t, REF_ID_KEYS));
    let enum_ref =
        hash_get(arena, n, "enumerationRef").and_then(|e| simple_ref(arena, &e, REF_ID_KEYS));
    match (type_ref, enum_ref) {
        (Some(t), Some(e)) => {
            let mut m = node(CLASS_ATTRIBUTE_REFERENCE);
            m.insert("ref".into(), t);
            m.insert("attribute".into(), e);
            Some(obj(m))
        }
        (None, Some(e)) => Some(e),
        _ => None,
    }
}

/// SELF\entity.attr redeclared attribute target
/// (qualifier_builder.build_qualified_attribute).
pub(crate) fn qualified_attribute_json(arena: &AstArena, n: &AstNode) -> Option<Value> {
    let attr = hash_get(arena, n, "attributeQualifier").and_then(|aq| {
        hash_get(arena, &aq, "attributeRef").and_then(|ar| simple_ref(arena, &ar, REF_ID_KEYS))
    });
    let group = hash_get(arena, n, "groupQualifier").and_then(|gq| {
        let mut self_ref = node(CLASS_SIMPLE_REFERENCE);
        self_ref.insert("id".into(), Value::String("SELF".into()));
        let mut m = node(CLASS_GROUP_REFERENCE);
        m.insert("ref".into(), obj(self_ref));
        m.insert(
            "entity".into(),
            hash_get(arena, &gq, "entityRef").and_then(|er| simple_ref(arena, &er, REF_ID_KEYS))?,
        );
        Some(obj(m))
    });

    match (group, attr) {
        (Some(g), Some(a)) => {
            let mut m = node(CLASS_ATTRIBUTE_REFERENCE);
            m.insert("ref".into(), g);
            m.insert("attribute".into(), a);
            Some(obj(m))
        }
        (None, Some(a)) => Some(a),
        (Some(g), None) => Some(g),
        _ => None,
    }
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
