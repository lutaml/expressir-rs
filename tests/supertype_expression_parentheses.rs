//! The parentheses of a supertype expression (lutaml/expressir#750).
//! `format` dropped the pairs the source wrote around a supertype
//! expression or one of its operands (the minimal-parentheses policy of
//! #651 applied to every operand). The source pairs are now recorded in
//! `parenthesized`, as for ordinary expressions (#707), and written back;
//! a model without the record keeps the minimal-parentheses text.

const SOURCE: &str = "SCHEMA paren_probe;\nENTITY a\n  ABSTRACT SUPERTYPE OF ((ONEOF (b, c)) ANDOR (d));\nEND_ENTITY;\nENTITY b SUBTYPE OF (a);\nEND_ENTITY;\nENTITY c SUBTYPE OF (a);\nEND_ENTITY;\nENTITY d\n  SUPERTYPE OF ((e))\n  SUBTYPE OF (a);\nEND_ENTITY;\nENTITY e SUBTYPE OF (d);\nEND_ENTITY;\nENTITY f\n  SUPERTYPE OF ((((e))));\nEND_ENTITY;\nENTITY g\n  SUPERTYPE OF ((b ANDOR c) AND d);\nEND_ENTITY;\nENTITY h\n  SUPERTYPE OF ((b AND c) ANDOR d);\nEND_ENTITY;\nENTITY i\n  SUPERTYPE OF (b AND (c ANDOR d));\nEND_ENTITY;\nENTITY j\n  SUPERTYPE OF (ONEOF ((ONEOF ((b), (c))), (d ANDOR e)));\nEND_ENTITY;\nENTITY k\n  SUPERTYPE OF (ONEOF (b, c) ANDOR d);\nEND_ENTITY;\nENTITY l\n  SUPERTYPE OF (b ANDOR (c ANDOR d));\nEND_ENTITY;\nSUBTYPE_CONSTRAINT sc_a FOR a;\n  (ONEOF (b, c));\nEND_SUBTYPE_CONSTRAINT;\nSUBTYPE_CONSTRAINT sc_b FOR a;\n  ONEOF (b, c);\nEND_SUBTYPE_CONSTRAINT;\nSUBTYPE_CONSTRAINT sc_c FOR a;\n  ABSTRACT SUPERTYPE;\n  TOTAL_OVER (b, c);\n  ((b) ANDOR (c));\nEND_SUBTYPE_CONSTRAINT;\nEND_SCHEMA;\n";

const EXPECTED: &str = "SCHEMA paren_probe;\n\nENTITY a\n  ABSTRACT SUPERTYPE OF ((ONEOF(b, c)) ANDOR (d));\nEND_ENTITY;\n\nENTITY b\n  SUBTYPE OF (a);\nEND_ENTITY;\n\nENTITY c\n  SUBTYPE OF (a);\nEND_ENTITY;\n\nENTITY d\n  SUPERTYPE OF ((e))\n  SUBTYPE OF (a);\nEND_ENTITY;\n\nENTITY e\n  SUBTYPE OF (d);\nEND_ENTITY;\n\nENTITY f\n  SUPERTYPE OF ((((e))));\nEND_ENTITY;\n\nENTITY g\n  SUPERTYPE OF ((b ANDOR c) AND d);\nEND_ENTITY;\n\nENTITY h\n  SUPERTYPE OF ((b AND c) ANDOR d);\nEND_ENTITY;\n\nENTITY i\n  SUPERTYPE OF (b AND (c ANDOR d));\nEND_ENTITY;\n\nENTITY j\n  SUPERTYPE OF (ONEOF((ONEOF((b), (c))), (d ANDOR e)));\nEND_ENTITY;\n\nENTITY k\n  SUPERTYPE OF (ONEOF(b, c) ANDOR d);\nEND_ENTITY;\n\nENTITY l\n  SUPERTYPE OF (b ANDOR (c ANDOR d));\nEND_ENTITY;\n\nSUBTYPE_CONSTRAINT sc_a FOR a;\n  (ONEOF(b, c));\nEND_SUBTYPE_CONSTRAINT;\n\nSUBTYPE_CONSTRAINT sc_b FOR a;\n  ONEOF(b, c);\nEND_SUBTYPE_CONSTRAINT;\n\nSUBTYPE_CONSTRAINT sc_c FOR a;\n  ABSTRACT SUPERTYPE;\n  TOTAL_OVER(b, c);\n  ((b) ANDOR (c));\nEND_SUBTYPE_CONSTRAINT;\n\nEND_SCHEMA;\n";

/// What 2.4.80 wrote, and what a model without the record still writes.
const MINIMAL: &str = "SCHEMA paren_probe;\n\nENTITY a\n  ABSTRACT SUPERTYPE OF (ONEOF(b, c) ANDOR d);\nEND_ENTITY;\n\nENTITY b\n  SUBTYPE OF (a);\nEND_ENTITY;\n\nENTITY c\n  SUBTYPE OF (a);\nEND_ENTITY;\n\nENTITY d\n  SUPERTYPE OF (e)\n  SUBTYPE OF (a);\nEND_ENTITY;\n\nENTITY e\n  SUBTYPE OF (d);\nEND_ENTITY;\n\nENTITY f\n  SUPERTYPE OF (e);\nEND_ENTITY;\n\nENTITY g\n  SUPERTYPE OF ((b ANDOR c) AND d);\nEND_ENTITY;\n\nENTITY h\n  SUPERTYPE OF (b AND c ANDOR d);\nEND_ENTITY;\n\nENTITY i\n  SUPERTYPE OF (b AND (c ANDOR d));\nEND_ENTITY;\n\nENTITY j\n  SUPERTYPE OF (ONEOF(ONEOF(b, c), d ANDOR e));\nEND_ENTITY;\n\nENTITY k\n  SUPERTYPE OF (ONEOF(b, c) ANDOR d);\nEND_ENTITY;\n\nENTITY l\n  SUPERTYPE OF (b ANDOR (c ANDOR d));\nEND_ENTITY;\n\nSUBTYPE_CONSTRAINT sc_a FOR a;\n  ONEOF(b, c);\nEND_SUBTYPE_CONSTRAINT;\n\nSUBTYPE_CONSTRAINT sc_b FOR a;\n  ONEOF(b, c);\nEND_SUBTYPE_CONSTRAINT;\n\nSUBTYPE_CONSTRAINT sc_c FOR a;\n  ABSTRACT SUPERTYPE;\n  TOTAL_OVER(b, c);\n  b ANDOR c;\nEND_SUBTYPE_CONSTRAINT;\n\nEND_SCHEMA;\n";

fn wire_of(source: &str) -> serde_json::Value {
    expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire")
}

fn format_text(wire: &serde_json::Value) -> String {
    expressir::formatter::format(wire, false)
}

fn strip_record(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.remove("parenthesized");
            m.values_mut().for_each(strip_record);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(strip_record),
        _ => {}
    }
}

fn find_entity<'a>(v: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    match v {
        serde_json::Value::Object(m) => {
            if m.get("_class").and_then(|c| c.as_str())
                == Some("Expressir::Model::Declarations::Entity")
                && m.get("id").and_then(|c| c.as_str()) == Some(id)
            {
                return Some(v);
            }
            m.values().find_map(|x| find_entity(x, id))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_entity(x, id)),
        _ => None,
    }
}

fn depth(v: &serde_json::Value) -> i64 {
    v.get("parenthesized").and_then(|d| d.as_i64()).unwrap_or(0)
}

#[test]
fn every_pair_of_the_source_is_written_back() {
    assert_eq!(format_text(&wire_of(SOURCE)), EXPECTED);
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let once = format_text(&wire_of(SOURCE));
    assert_eq!(format_text(&wire_of(&once)), once);
}

#[test]
fn the_wire_records_the_depth_of_each_pair() {
    let wire = wire_of(SOURCE);
    let se = |id: &str| find_entity(&wire, id).expect(id)["supertype_expression"].clone();
    // ((ONEOF (b, c)) ANDOR (d)): the outer pair is syntax of SUPERTYPE OF.
    let a = se("a");
    assert_eq!(depth(&a), 0);
    assert_eq!((depth(&a["operand1"]), depth(&a["operand2"])), (1, 1));
    // SUPERTYPE OF ((e)) and (((( e )))): the pairs inside the outer one.
    assert_eq!(depth(&se("d")), 1);
    assert_eq!(depth(&se("f")), 3);
    // (b ANDOR c) AND d: the pair that precedence needs is in the source.
    assert_eq!(depth(&se("g")["operand1"]), 1);
    // ONEOF (b, c) ANDOR d: no pair, no record.
    let k = se("k");
    assert_eq!((depth(&k["operand1"]), depth(&k["operand2"])), (0, 0));
}

#[test]
fn a_model_without_the_record_keeps_the_minimal_parentheses() {
    let mut wire = wire_of(SOURCE);
    strip_record(&mut wire);
    assert_eq!(format_text(&wire), MINIMAL);
}

#[test]
fn an_expression_without_extra_pairs_is_unchanged() {
    let source =
        "SCHEMA s;\nENTITY k\n  SUPERTYPE OF (ONEOF (b, c) ANDOR d);\nEND_ENTITY;\nEND_SCHEMA;\n";
    let text = format_text(&wire_of(source));
    assert!(
        text.contains("  SUPERTYPE OF (ONEOF(b, c) ANDOR d);\n"),
        "{text}"
    );
}

#[test]
fn a_pair_that_precedence_needs_is_written_once() {
    let source =
        "SCHEMA s;\nENTITY g\n  SUPERTYPE OF ((b ANDOR c) AND d);\nEND_ENTITY;\nEND_SCHEMA;\n";
    let text = format_text(&wire_of(source));
    assert!(
        text.contains("  SUPERTYPE OF ((b ANDOR c) AND d);\n"),
        "{text}"
    );
}
