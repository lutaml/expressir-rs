//! A `(* *)` comment at the start of a line, before the USE FROM or
//! REFERENCE FROM clause that begins on the same line, leads that clause,
//! as an own-line comment above the clause already does. It kept no
//! placement and was written after the last clause.

const SOURCE: &str = "SCHEMA clause_block_probe;\n(* a *) USE FROM first_schema; -- one\nUSE FROM second_schema; -- two\n(* b *) REFERENCE FROM third_schema (c); -- three\n(* d *) USE FROM fourth_schema;\n(* e *) (* f *) USE FROM fifth_schema;\n(* g *)\nUSE FROM sixth_schema;\n-- h\nUSE FROM seventh_schema;\nENTITY part;\n  size : INTEGER;\nEND_ENTITY;\nEND_SCHEMA;\n";

const EXPECTED: &str = "SCHEMA clause_block_probe;\n\n(* a *)\nUSE FROM first_schema; -- one\n\nUSE FROM second_schema; -- two\n\n(* b *)\nREFERENCE FROM third_schema\n  (c); -- three\n\n(* d *)\nUSE FROM fourth_schema;\n\n(* e *)\n(* f *)\nUSE FROM fifth_schema;\n\n(* g *)\nUSE FROM sixth_schema;\n\n-- h\nUSE FROM seventh_schema;\n\nENTITY part;\n  size : INTEGER;\nEND_ENTITY;\n\nEND_SCHEMA;\n";

fn attached(source: &str) -> serde_json::Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

/// `(text, placement)` of every remark held by interface `n`.
fn held(wire: &serde_json::Value, n: usize) -> Vec<(String, Option<String>)> {
    let ptr = format!("/schemas/0/interfaces/{n}/untagged_remarks");
    let text = |r: &serde_json::Value, k: &str| r.get(k).and_then(|v| v.as_str()).map(String::from);
    wire.pointer(&ptr)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|r| {
                    (
                        text(r, "text").unwrap().trim().to_string(),
                        text(r, "placement"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn each_comment_is_written_before_the_clause_it_led() {
    let text = expressir::formatter::format(&attached(SOURCE), false);
    assert_eq!(text, EXPECTED);
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let once = expressir::formatter::format(&attached(SOURCE), false);
    assert_eq!(expressir::formatter::format(&attached(&once), false), once);
}

#[test]
fn the_stripped_form_has_no_comment() {
    let stripped = expressir::formatter::format(&attached(SOURCE), true);
    assert!(!stripped.contains("(*") && !stripped.contains("--"));
    assert!(stripped.contains("USE FROM first_schema;\n\nUSE FROM second_schema;"));
}

#[test]
fn the_clause_holds_the_comment_with_the_leading_placement() {
    let wire = attached(SOURCE);
    let l = |t: &str| (t.to_string(), Some("leading".to_string()));
    let i = |t: &str| (t.to_string(), Some("inline".to_string()));
    assert_eq!(held(&wire, 0), vec![l("a"), i("one")]);
    assert_eq!(held(&wire, 1), vec![i("two")]);
    assert_eq!(held(&wire, 2), vec![l("b"), i("three")]);
    assert_eq!(held(&wire, 3), vec![l("d")]);
    assert_eq!(held(&wire, 4), vec![l("e"), l("f")]);
    assert_eq!(held(&wire, 5), vec![l("g")]);
    assert_eq!(held(&wire, 6), vec![l("h")]);
    assert!(wire
        .pointer("/schemas/0/untagged_remarks")
        .is_none_or(|v| v.as_array().is_none_or(|a| a.is_empty())));
}

#[test]
fn a_block_comment_inside_the_clause_is_left_as_it_was() {
    // `(* c *)` follows the schema name, so it does not start the line.
    let source = "SCHEMA s;\nUSE FROM x (* c *) ( -- d\n  a);\nEND_SCHEMA;\n";
    let text = expressir::formatter::format(&attached(source), false);
    assert_eq!(
        text,
        "SCHEMA s;\n\nUSE FROM x (* c *) -- d\n  (a);\nEND_SCHEMA;\n"
    );
}
