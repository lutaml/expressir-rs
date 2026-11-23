//! The plain-form-visible same-line remarks of a CONSTANT and a UNIQUE
//! rule keep rendering (#735): the emitters write the placement-less
//! tail back after the `;`.

#[test]
fn constant_and_unique_same_line_remarks_render() {
    let source = "SCHEMA s;\nCONSTANT circle_constant : REAL := 3.14; -- the circle constant\nEND_CONSTANT;\nENTITY e;\n  id : STRING;\n  size : INTEGER;\n  UNIQUE\n    u1 : id; -- the identifier\n  WHERE\n    wr1 : size > 0;\n  END_ENTITY;\nEND_SCHEMA;\n";
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    let text = expressir::formatter::format(&wire, false);
    assert!(
        text.contains("-- the circle constant"),
        "the constant remark missing:\n{text}"
    );
    assert!(
        text.contains("-- the identifier"),
        "the unique remark missing:\n{text}"
    );
    let mut again = expressir::ParsedTree::parse(&text)
        .expect("the render reparses")
        .to_model_json("again.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut again, &text, false);
    expressir::attach::attach_untagged_remarks(&mut again, &text, false);
    let twice = expressir::formatter::format(&again, false);
    assert_eq!(twice, text, "the render is a fixed point");
}
