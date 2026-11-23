//! The plain form writes no informal-proposition lines from tagged
//! descriptions (#723): a `(*"schema.entity.wr:IP1" ... *)` description
//! is TAGGED-channel content, and `keep_untagged_remarks` keeps UNTAGGED
//! comments only.

#[test]
fn plain_form_writes_no_ip_lines_for_an_entity_that_declared_none() {
    let source = r#"SCHEMA ip_probe;
ENTITY a;
  x : INTEGER;
WHERE
  wr1 : x > 0;
END_ENTITY;
ENTITY b;
  x : INTEGER;
WHERE
  wr1 : x > 0;
  --IP1: the source has this line
END_ENTITY;
END_SCHEMA;
(*"ip_probe.a.wr:IP1"
Text of the description of a.
*)
(*"ip_probe.b.wr:IP1"
Text of the description of b.
*)"#;

    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("ip_probe.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    let plain = expressir::formatter::format_with_keep(&wire, true, true);
    let entity_a = plain.split("END_ENTITY").next().unwrap_or("");
    assert!(
        !entity_a.contains("--IP1:"),
        "entity a must not gain an IP line: {entity_a}"
    );
    assert!(
        !plain.contains("Text of the description"),
        "tagged description text must stay out of the plain form"
    );
    let full = expressir::formatter::format(&wire, false);
    assert!(
        full.contains("--IP1: the source has this line"),
        "the source's own shorthand line keeps rendering on the full form"
    );
}
