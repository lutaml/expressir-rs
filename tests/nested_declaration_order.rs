//! The declarations nested in a FUNCTION, PROCEDURE or RULE are written
//! back in the order of the source (lutaml/expressir#711 reached the
//! schema level only). The formatter wrote a callable's nested types,
//! entities, functions and procedures grouped by kind, so a PROCEDURE
//! written before a FUNCTION came out after it.

const SOURCE: &str = "SCHEMA order_probe;\nENTITY thing;\n  v : INTEGER;\nEND_ENTITY;\nFUNCTION outer_fn(a : INTEGER) : INTEGER;\n  PROCEDURE nested_p1(VAR x : INTEGER);\n    x := 1;\n  END_PROCEDURE;\n  FUNCTION nested_f1(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\n  TYPE nested_t = INTEGER;\n  END_TYPE;\n  PROCEDURE nested_p2(VAR x : INTEGER);\n    x := 2;\n  END_PROCEDURE;\n  RETURN (a);\nEND_FUNCTION;\nPROCEDURE outer_proc(VAR a : INTEGER);\n  FUNCTION inner_f(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\n  TYPE inner_t = INTEGER;\n  END_TYPE;\n  PROCEDURE inner_p(VAR x : INTEGER);\n    x := 3;\n  END_PROCEDURE;\n  a := 1;\nEND_PROCEDURE;\nRULE outer_rule FOR (thing);\n  PROCEDURE rule_p(VAR x : INTEGER);\n    x := 4;\n  END_PROCEDURE;\n  FUNCTION rule_f(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\nWHERE\n  wr1: TRUE;\nEND_RULE;\nEND_SCHEMA;\n";

const EXPECTED: &str = "SCHEMA order_probe;\n\nENTITY thing;\n  v : INTEGER;\nEND_ENTITY;\n\nFUNCTION outer_fn(a : INTEGER) : INTEGER;\n  PROCEDURE nested_p1(VAR x : INTEGER);\n    x := 1;\n  END_PROCEDURE;\n  FUNCTION nested_f1(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\n  TYPE nested_t = INTEGER;\n  END_TYPE;\n  PROCEDURE nested_p2(VAR x : INTEGER);\n    x := 2;\n  END_PROCEDURE;\n  RETURN (a);\nEND_FUNCTION;\n\nPROCEDURE outer_proc(VAR a : INTEGER);\n  FUNCTION inner_f(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\n  TYPE inner_t = INTEGER;\n  END_TYPE;\n  PROCEDURE inner_p(VAR x : INTEGER);\n    x := 3;\n  END_PROCEDURE;\n  a := 1;\nEND_PROCEDURE;\n\nRULE outer_rule FOR (thing);\n  PROCEDURE rule_p(VAR x : INTEGER);\n    x := 4;\n  END_PROCEDURE;\n  FUNCTION rule_f(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\nWHERE\n  wr1: TRUE;\nEND_RULE;\n\nEND_SCHEMA;\n";

fn attached(source: &str) -> serde_json::Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

#[test]
fn nested_declarations_keep_the_order_of_the_source() {
    let text = expressir::formatter::format(&attached(SOURCE), false);
    assert_eq!(text, EXPECTED);
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let once = expressir::formatter::format(&attached(SOURCE), false);
    assert_eq!(expressir::formatter::format(&attached(&once), false), once);
}

#[test]
fn the_stripped_form_keeps_the_order_and_has_no_comment() {
    let source = SOURCE.replace("x := 1;", "x := 1; -- kept");
    let text = expressir::formatter::format(&attached(&source), true);
    assert!(!text.contains("kept"));
    let names: Vec<&str> = text
        .lines()
        .filter_map(|l| {
            l.trim_start()
                .strip_prefix("PROCEDURE ")
                .or_else(|| l.trim_start().strip_prefix("FUNCTION "))
        })
        .collect();
    assert!(names[1].starts_with("nested_p1"), "{names:?}");
    assert!(names[2].starts_with("nested_f1"), "{names:?}");
}

#[test]
fn the_wire_records_the_order_on_the_callable_and_not_on_a_callable_without_nesting() {
    let wire = attached(SOURCE);
    let order = |ptr: &str| wire.pointer(ptr).cloned();
    assert_eq!(
        order("/schemas/0/functions/0/declaration_order"),
        Some(serde_json::json!([
            "nested_p1",
            "nested_f1",
            "nested_t",
            "nested_p2"
        ]))
    );
    assert_eq!(
        order("/schemas/0/procedures/0/declaration_order"),
        Some(serde_json::json!(["inner_f", "inner_t", "inner_p"]))
    );
    assert_eq!(
        order("/schemas/0/rules/0/declaration_order"),
        Some(serde_json::json!(["rule_p", "rule_f"]))
    );
    let plain =
        attached("SCHEMA s;\nFUNCTION f : INTEGER;\n  RETURN (1);\nEND_FUNCTION;\nEND_SCHEMA;\n");
    assert_eq!(
        plain.pointer("/schemas/0/functions/0/declaration_order"),
        None
    );
}

#[test]
fn a_wire_without_the_order_is_written_grouped_by_kind_as_before() {
    let mut wire = attached(SOURCE);
    for ptr in [
        "/schemas/0/functions/0",
        "/schemas/0/procedures/0",
        "/schemas/0/rules/0",
    ] {
        wire.pointer_mut(ptr)
            .and_then(serde_json::Value::as_object_mut)
            .expect("callable")
            .remove("declaration_order");
    }
    let text = expressir::formatter::format(&wire, false);
    let functions_first = "FUNCTION outer_fn(a : INTEGER) : INTEGER;\n  TYPE nested_t = INTEGER;\n  END_TYPE;\n  FUNCTION nested_f1(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\n  PROCEDURE nested_p1(VAR x : INTEGER);";
    assert!(text.contains(functions_first), "{text}");
}

// ---- every nested declaration is written once, whatever the record says ----

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

#[test]
fn two_nested_functions_of_one_id_are_each_written_once() {
    let source = "SCHEMA s;\nFUNCTION f(a : INTEGER) : INTEGER;\n  FUNCTION g(x : INTEGER) : INTEGER;\n    RETURN (x);\n  END_FUNCTION;\n  FUNCTION g(y : REAL) : REAL;\n    RETURN (y);\n  END_FUNCTION;\n  FUNCTION g(z : STRING) : STRING;\n    RETURN (z);\n  END_FUNCTION;\n  RETURN (a);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let text = expressir::formatter::format(&attached(source), false);
    assert_eq!(
        text,
        source
            .replace("SCHEMA s;\n", "SCHEMA s;\n\n")
            .replace("END_FUNCTION;\nEND_SCHEMA", "END_FUNCTION;\n\nEND_SCHEMA")
    );
}

#[test]
fn two_nested_procedures_and_a_type_and_function_of_one_id_are_each_written_once() {
    let source = "SCHEMA s;\nPROCEDURE p;\n  PROCEDURE q(VAR a : INTEGER);\n    a := 1;\n  END_PROCEDURE;\n  PROCEDURE q(VAR b : REAL);\n    b := 1.0;\n  END_PROCEDURE;\n  TYPE x = INTEGER;\n  END_TYPE;\n  FUNCTION x(y : INTEGER) : INTEGER;\n    RETURN (y);\n  END_FUNCTION;\nEND_PROCEDURE;\nRULE r FOR (thing);\n  FUNCTION g(x : INTEGER) : INTEGER;\n    RETURN (x);\n  END_FUNCTION;\n  FUNCTION g(y : REAL) : REAL;\n    RETURN (y);\n  END_FUNCTION;\nWHERE\n  wr1: TRUE;\nEND_RULE;\nEND_SCHEMA;\n";
    let text = expressir::formatter::format(&attached(source), false);
    assert_eq!(count(&text, "PROCEDURE q("), 2);
    assert_eq!(count(&text, "TYPE x"), 1);
    assert_eq!(count(&text, "FUNCTION x("), 1);
    assert_eq!(count(&text, "FUNCTION g("), 2);
    assert_eq!(count(&text, "(VAR b : REAL)"), 1);
    assert_eq!(count(&text, "g(y : REAL)"), 1);
    assert!(text.find("q(VAR a").unwrap() < text.find("q(VAR b").unwrap());
    assert!(text.find("TYPE x").unwrap() < text.find("FUNCTION x(").unwrap());
}

fn probe_with_record(record: serde_json::Value) -> String {
    let source = "SCHEMA s;\nFUNCTION f : INTEGER;\n  TYPE t1 = INTEGER;\n  END_TYPE;\n  FUNCTION g1 : INTEGER;\n    RETURN (1);\n  END_FUNCTION;\n  PROCEDURE p1;\n  END_PROCEDURE;\n  RETURN (1);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let mut wire = attached(source);
    let f = wire
        .pointer_mut("/schemas/0/functions/0")
        .unwrap()
        .as_object_mut()
        .unwrap();
    if record.is_null() {
        f.remove("declaration_order");
    } else {
        f.insert("declaration_order".into(), record);
    }
    expressir::formatter::format(&wire, false)
}

fn names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            ["TYPE ", "FUNCTION ", "PROCEDURE "]
                .iter()
                .find_map(|k| l.strip_prefix(k))
                .map(|r| {
                    r.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap()
                        .to_string()
                })
        })
        .collect()
}

#[test]
fn a_record_naming_only_some_still_writes_all_once() {
    let text = probe_with_record(serde_json::json!(["g1"]));
    assert_eq!(names(&text), ["f", "g1", "t1", "p1"]);
}

#[test]
fn a_record_with_unknown_ids_is_skipped_and_the_rest_written() {
    let text = probe_with_record(serde_json::json!(["nope", "p1", "p1", "other"]));
    assert_eq!(names(&text), ["f", "p1", "t1", "g1"]);
}

#[test]
fn an_empty_or_missing_record_writes_grouped_by_kind() {
    for record in [serde_json::json!([]), serde_json::Value::Null] {
        assert_eq!(names(&probe_with_record(record)), ["f", "t1", "g1", "p1"]);
    }
}

#[test]
fn a_declaration_added_after_parsing_is_written() {
    let source = "SCHEMA s;\nFUNCTION f : INTEGER;\n  TYPE t1 = INTEGER;\n  END_TYPE;\n  RETURN (1);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let mut wire = attached(source);
    let mut extra = wire
        .pointer("/schemas/0/functions/0/types/0")
        .unwrap()
        .clone();
    extra["id"] = serde_json::json!("newt");
    wire.pointer_mut("/schemas/0/functions/0/types")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(extra);
    let text = expressir::formatter::format(&wire, false);
    assert_eq!(names(&text), ["f", "t1", "newt"]);
}

// ---- the comparison ignores the record, as before ----

#[test]
fn schema_compare_ignores_the_order_record() {
    let a = "SCHEMA s;\nFUNCTION f : INTEGER;\n  TYPE t1 = INTEGER;\n  END_TYPE;\n  FUNCTION g1 : INTEGER;\n    RETURN (1);\n  END_FUNCTION;\n  RETURN (1);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let b = "SCHEMA s;\nFUNCTION f : INTEGER;\n  FUNCTION g1 : INTEGER;\n    RETURN (1);\n  END_FUNCTION;\n  TYPE t1 = INTEGER;\n  END_TYPE;\n  RETURN (1);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let (wa, wb) = (attached(a), attached(b));
    assert_ne!(
        wa.pointer("/schemas/0/functions/0/declaration_order"),
        wb.pointer("/schemas/0/functions/0/declaration_order")
    );
    let (sa, sb) = (&wa["schemas"][0], &wb["schemas"][0]);
    assert!(expressir::schema_compare::compare(sa, sb).is_empty());
    let mut older = sa.clone();
    older["functions"][0]
        .as_object_mut()
        .unwrap()
        .remove("declaration_order");
    assert!(expressir::schema_compare::compare(sa, &older).is_empty());
}
