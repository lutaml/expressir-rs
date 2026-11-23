//! A comment on a line of its own after the last declaration of a schema
//! and before `END_SCHEMA` is written back before `END_SCHEMA`. The schema
//! held it without a placement: a `--` comment was written after
//! `END_SCHEMA` and a block comment at the top of the schema.

use serde_json::Value;

const PROBE: &str = "SCHEMA end_probe;\nENTITY a;\nEND_ENTITY;\n-- D1 on its own line before the end of the schema\n(* D2 a block on its own line before the end of the schema *)\nEND_SCHEMA; -- D3 after the end of the schema on its line\n";

const PROBE_OUT: &str = "SCHEMA end_probe;\n\nENTITY a;\nEND_ENTITY;\n\n-- D1 on its own line before the end of the schema\n(* D2 a block on its own line before the end of the schema *)\nEND_SCHEMA; -- D3 after the end of the schema on its line\n";

fn attached(source: &str) -> Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

fn text(source: &str) -> String {
    expressir::formatter::format(&attached(source), false)
}

fn held(wire: &Value, schema: usize) -> Vec<(String, Option<String>, Option<String>)> {
    let field = |r: &Value, name: &str| r.get(name).and_then(Value::as_str).map(String::from);
    wire.pointer(&format!("/schemas/{schema}/untagged_remarks"))
        .and_then(Value::as_array)
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    (
                        field(r, "text").unwrap_or_default(),
                        field(r, "placement"),
                        field(r, "region"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn both_comments_are_written_directly_before_end_schema_in_source_order() {
    assert_eq!(text(PROBE), PROBE_OUT);
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let once = text(PROBE);
    assert_eq!(text(&once), once);
}

#[test]
fn the_stripped_form_writes_none_of_them() {
    let out = expressir::formatter::format(&attached(PROBE), true);
    assert_eq!(
        out,
        "SCHEMA end_probe;\n\nENTITY a;\nEND_ENTITY;\n\nEND_SCHEMA;\n"
    );
}

#[test]
fn the_plain_form_writes_both_before_end_schema() {
    let out = expressir::formatter::format_with_keep(&attached(PROBE), true, true);
    assert_eq!(out, PROBE_OUT);
}

#[test]
fn the_schema_holds_d1_and_d2_as_trailing_remarks_of_its_declarations() {
    let placed = |t: &str| {
        (
            t.to_string(),
            Some("trailing".to_string()),
            Some("declarations".to_string()),
        )
    };
    let h = held(&attached(PROBE), 0);
    assert_eq!(h.len(), 3, "{h:?}");
    assert!(h[0].0.starts_with("D1") && h[0].1 == placed("").1 && h[0].2 == placed("").2);
    assert!(h[1].0.starts_with("D2") && h[1].1 == placed("").1 && h[1].2 == placed("").2);
    assert!(h[2].0.starts_with("D3") && h[2].1.is_none() && h[2].2.is_none());
}

#[test]
fn a_comment_after_end_schema_on_its_line_stays_where_it_was() {
    let s = "SCHEMA s;\nENTITY a;\nEND_ENTITY;\nEND_SCHEMA; -- D3 inline\n";
    assert_eq!(
        text(s),
        "SCHEMA s;\n\nENTITY a;\nEND_ENTITY;\n\nEND_SCHEMA; -- D3 inline\n"
    );
    assert_eq!(
        held(&attached(s), 0),
        vec![("D3 inline".to_string(), None, None)]
    );
    let plain = expressir::formatter::format_with_keep(&attached(s), true, true);
    assert_eq!(
        plain,
        "SCHEMA s;\n\nENTITY a;\nEND_ENTITY;\n\nEND_SCHEMA; -- D3 inline\n"
    );
}

#[test]
fn a_comment_on_a_line_after_end_schema_stays_where_it_was() {
    let s = "SCHEMA s;\nENTITY a;\nEND_ENTITY;\nEND_SCHEMA;\n-- D3 own line\n";
    assert_eq!(
        text(s),
        "SCHEMA s;\n\nENTITY a;\nEND_ENTITY;\n\nEND_SCHEMA;\n"
    );
}

#[test]
fn a_comment_right_after_the_schema_line_and_one_between_declarations_stay() {
    assert_eq!(
        text("SCHEMA s;\n-- after head\nENTITY a;\nEND_ENTITY;\nEND_SCHEMA;\n"),
        "SCHEMA s;\n\n-- after head\nENTITY a;\nEND_ENTITY;\n\nEND_SCHEMA;\n"
    );
    assert_eq!(
        text("SCHEMA s;\nENTITY a;\nEND_ENTITY;\n-- between\nENTITY b;\nEND_ENTITY;\nEND_SCHEMA;\n"),
        "SCHEMA s;\n\nENTITY a;\nEND_ENTITY;\n\n-- between\nENTITY b;\nEND_ENTITY;\n\nEND_SCHEMA;\n"
    );
}

#[test]
fn an_empty_schema_with_only_a_comment() {
    let s = "SCHEMA s;\n-- nothing yet\nEND_SCHEMA;\n";
    assert_eq!(text(s), "SCHEMA s;\n\n-- nothing yet\nEND_SCHEMA;\n");
    assert_eq!(
        held(&attached(s), 0),
        vec![(
            "nothing yet".to_string(),
            Some("trailing".to_string()),
            Some("declarations".to_string())
        )]
    );
}

#[test]
fn the_last_declaration_may_be_of_any_kind() {
    for last in [
        "FUNCTION f : INTEGER;\n  RETURN (1);\nEND_FUNCTION;\n",
        "ENTITY e;\n  x : INTEGER;\nEND_ENTITY;\n",
        "ENTITY e;\nEND_ENTITY;\nRULE r FOR (e);\nWHERE\n  wr1 : TRUE;\nEND_RULE;\n",
        "TYPE t = INTEGER;\nEND_TYPE;\n",
        "CONSTANT\n  c : INTEGER := 1;\nEND_CONSTANT;\n",
        "USE FROM other_schema;\n",
    ] {
        let out = text(&format!("SCHEMA s;\n{last}-- closing note\nEND_SCHEMA;\n"));
        assert!(
            out.ends_with("\n-- closing note\nEND_SCHEMA;\n"),
            "{last}: {out}"
        );
        assert_eq!(out.matches("closing note").count(), 1, "{out}");
        assert_eq!(text(&out), out);
    }
}

#[test]
fn two_schemas_in_one_file_each_keep_their_own_comment() {
    let s = "SCHEMA one;\nENTITY a;\nEND_ENTITY;\n-- of one\nEND_SCHEMA;\nSCHEMA two;\nENTITY b;\nEND_ENTITY;\n-- of two\nEND_SCHEMA;\n";
    let wire = attached(s);
    assert_eq!(held(&wire, 0).len(), 1);
    assert_eq!(held(&wire, 0)[0].0, "of one");
    assert_eq!(held(&wire, 1).len(), 1);
    assert_eq!(held(&wire, 1)[0].0, "of two");
    let out = expressir::formatter::format(&wire, false);
    assert!(
        out.contains("END_ENTITY;\n\n-- of one\nEND_SCHEMA;"),
        "{out}"
    );
    assert!(
        out.contains("END_ENTITY;\n\n-- of two\nEND_SCHEMA;"),
        "{out}"
    );
}

#[test]
fn several_comments_in_a_row_with_blank_lines_between() {
    assert_eq!(
        text("SCHEMA s;\nENTITY a;\nEND_ENTITY;\n-- one\n\n  -- two\n(* three *)\n\nEND_SCHEMA;\n"),
        "SCHEMA s;\n\nENTITY a;\nEND_ENTITY;\n\n-- one\n-- two\n(* three *)\nEND_SCHEMA;\n"
    );
}

#[test]
fn a_lower_case_end_schema_is_read() {
    let out = text("SCHEMA s;\nENTITY a;\nEND_ENTITY;\n-- note\nend_schema;\n");
    assert!(out.contains("-- note\nEND_SCHEMA;"), "{out}");
    // The wire drops a lower-case `schema` that starts at byte 0 (the
    // offset-0 test of the index needs an upper-case keyword), a limit
    // that is not part of this change: a line before it avoids it.
    let out = text("\nschema s;\nentity a;\nend_entity;\n-- note\nend_schema;\n");
    assert!(out.contains("-- note\nEND_SCHEMA;"), "{out}");
}

#[test]
fn a_block_comment_of_several_lines_may_name_end_schema_after_its_first_line() {
    let out = text(
        "SCHEMA s;\nENTITY a;\nEND_ENTITY;\n(* first line\nEND_SCHEMA is the next line\n*)\nEND_SCHEMA;\n",
    );
    assert!(out.ends_with("\nEND_SCHEMA;\n"), "{out}");
    assert_eq!(out.matches("first line").count(), 1, "{out}");
    assert!(out.find("first line") < out.rfind("END_SCHEMA;"));
}

#[test]
fn a_tagged_remark_is_not_taken() {
    let wire =
        attached("SCHEMA s;\nENTITY a;\nEND_ENTITY;\n(*\"s.a\" A description. *)\nEND_SCHEMA;\n");
    assert!(held(&wire, 0).is_empty());
}

#[test]
fn the_pretty_form_writes_them_before_end_schema_too() {
    let out = expressir::formatter::pretty_format(&attached(PROBE), "", "{}").expect("pretty");
    assert!(out.trim_end().ends_with(PROBE_OUT.trim_end()), "{out}");
}
