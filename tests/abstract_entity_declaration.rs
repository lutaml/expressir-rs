//! ISO 10303-11:2004 has two forms: `abstract_entity_declaration =
//! ABSTRACT` and `abstract_supertype_declaration = ABSTRACT SUPERTYPE
//! [subtype_constraint]`. The wire kept one `abstract` flag for both and
//! the formatter wrote `ABSTRACT SUPERTYPE` for both, adding the code
//! token `SUPERTYPE` to the first. The wire now says `abstract_entity`
//! for the bare form (set only then), and the formatters write it back.

const PRETTY: &str = "{\"indent\":2,\"provenance\":false}";

fn wire(source: &str) -> serde_json::Value {
    expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire")
}

fn schema(entity: &str) -> String {
    format!("SCHEMA s;\n{entity}\nEND_SCHEMA;\n")
}

fn base(source: &str) -> String {
    expressir::formatter::format(&wire(source), false)
}

fn pretty(source: &str) -> String {
    expressir::formatter::pretty_format(&wire(source), "", PRETTY).expect("pretty")
}

#[test]
fn the_bare_abstract_form_is_written_without_supertype() {
    let cases = [
        (
            "ENTITY x ABSTRACT;\nEND_ENTITY;",
            "ENTITY x\n  ABSTRACT;\nEND_ENTITY;",
        ),
        (
            "ENTITY x ABSTRACT SUBTYPE OF (y);\nEND_ENTITY;",
            "ENTITY x\n  ABSTRACT\n  SUBTYPE OF (y);\nEND_ENTITY;",
        ),
        (
            "entity x abstract subtype of (y);\nend_entity;",
            "ENTITY x\n  ABSTRACT\n  SUBTYPE OF (y);\nEND_ENTITY;",
        ),
    ];
    for (src, want) in cases {
        let want = schema(want)
            .replace("SCHEMA s;\n", "SCHEMA s;\n\n")
            .replace("END_ENTITY;\nEND_SCHEMA", "END_ENTITY;\n\nEND_SCHEMA");
        assert_eq!(base(&schema(src)), want, "base form of {src}");
        assert_eq!(pretty(&schema(src)), want, "pretty form of {src}");
    }
}

#[test]
fn the_abstract_supertype_forms_keep_their_text() {
    let cases = [
        (
            "ENTITY x ABSTRACT SUPERTYPE;\nEND_ENTITY;",
            "ABSTRACT SUPERTYPE;",
        ),
        (
            "ENTITY x ABSTRACT SUPERTYPE OF (ONEOF (a, b));\nEND_ENTITY;",
            "ABSTRACT SUPERTYPE OF (ONEOF(a, b));",
        ),
        (
            "ENTITY x ABSTRACT SUPERTYPE SUBTYPE OF (y);\nEND_ENTITY;",
            "ABSTRACT SUPERTYPE\n  SUBTYPE OF (y);",
        ),
        (
            "entity x abstract supertype subtype of (y);\nend_entity;",
            "ABSTRACT SUPERTYPE\n  SUBTYPE OF (y);",
        ),
        ("ENTITY x SUBTYPE OF (y);\nEND_ENTITY;", "SUBTYPE OF (y);"),
    ];
    for (src, tail) in cases {
        let text = base(&schema(src));
        assert!(text.contains(tail), "{src} gave {text}");
        assert_eq!(text, pretty(&schema(src)), "base and pretty agree on {src}");
    }
}

#[test]
fn the_flag_is_set_only_for_the_bare_form() {
    let flag = |entity: &str| {
        let w = wire(&schema(entity));
        let e = &w["schemas"][0]["entities"][0];
        (
            e.get("abstract").cloned(),
            e.get("abstract_entity").cloned(),
        )
    };
    let t = Some(serde_json::Value::Bool(true));
    assert_eq!(
        flag("ENTITY x ABSTRACT;\nEND_ENTITY;"),
        (t.clone(), t.clone())
    );
    assert_eq!(
        flag("ENTITY x ABSTRACT SUBTYPE OF (y);\nEND_ENTITY;"),
        (t.clone(), t.clone())
    );
    assert_eq!(
        flag("ENTITY x ABSTRACT SUPERTYPE;\nEND_ENTITY;"),
        (t.clone(), None)
    );
    assert_eq!(
        flag("ENTITY x ABSTRACT SUPERTYPE OF (ONEOF (a, b));\nEND_ENTITY;"),
        (t, None)
    );
    assert_eq!(flag("ENTITY x;\nEND_ENTITY;"), (None, None));
}

#[test]
fn a_wire_without_the_field_is_written_as_before() {
    let mut w = wire(&schema("ENTITY x ABSTRACT;\nEND_ENTITY;"));
    w["schemas"][0]["entities"][0]
        .as_object_mut()
        .unwrap()
        .remove("abstract_entity");
    assert!(expressir::formatter::format(&w, false).contains("ABSTRACT SUPERTYPE;"));
    assert!(expressir::formatter::pretty_format(&w, "", PRETTY)
        .unwrap()
        .contains("ABSTRACT SUPERTYPE;"));
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/abstract_entity_declaration.exp"
    ))
    .expect("fixture");
    let once = base(&src);
    assert_eq!(base(&once), once);
    let once = pretty(&src);
    assert_eq!(pretty(&once), once);
}

#[test]
fn the_subtype_constraint_form_is_unchanged() {
    let src = "SCHEMA s;\nENTITY y;\nEND_ENTITY;\nSUBTYPE_CONSTRAINT sc FOR y;\n  ABSTRACT SUPERTYPE;\nEND_SUBTYPE_CONSTRAINT;\nEND_SCHEMA;\n";
    assert!(base(src).contains("ABSTRACT SUPERTYPE;"));
    assert!(pretty(src).contains("ABSTRACT SUPERTYPE;"));
}
