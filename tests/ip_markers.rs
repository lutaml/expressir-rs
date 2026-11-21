#[test]
fn ip_markers_round_trip() {
    let source = "SCHEMA s;\nENTITY e;\n  a : INTEGER;\n  -- IP1:\n  -- IP2: the mirror axis text\nEND_ENTITY;\nEND_SCHEMA;\n";
    let mut wire = expressir::ParsedTree::parse(source).expect("parses").to_model_json("s.exp").expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    let text = expressir::formatter::format(&wire, false);
    assert!(text.contains("--IP1:"), "bare marker missing:\n{text}");
    assert!(text.contains("--IP2: the mirror axis text"), "text marker missing:\n{text}");
}
