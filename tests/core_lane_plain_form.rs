//! The core lane plain form keeps the same-line comments (#733): the rs
//! inline emitters test the `no_remarks`/`keep_untagged_remarks` pair,
//! exactly like the ruby lane's #722.

#[test]
fn plain_form_through_the_core_emitters_keeps_inline_comments() {
    let source = "SCHEMA inline_probe;\nUSE FROM other_schema; -- ISO/TS 10303-1001\nENTITY point;\n  x : REAL; -- the abscissa\n  y : REAL;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("inline_probe.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    let plain = expressir::formatter::format_with_keep(&wire, true, true);
    assert!(
        plain.contains("-- ISO/TS 10303-1001"),
        "the interface line comment missing:\n{plain}"
    );
    assert!(
        plain.contains("-- the abscissa"),
        "the attribute line comment missing:\n{plain}"
    );
    let stripped = expressir::formatter::format_with_keep(&wire, true, false);
    assert!(
        !stripped.contains("-- the abscissa") && !stripped.contains("-- ISO/TS 10303-1001"),
        "no_remarks alone must still strip them:\n{stripped}"
    );
}
