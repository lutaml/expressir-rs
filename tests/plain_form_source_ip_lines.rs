//! The plain form (`no_remarks` + `keep_untagged_remarks`) writes the
//! `--IPn:` comment lines the SOURCE has, with the source's own text, and
//! still writes nothing for a proposition that exists only through a tagged
//! `wr:IPn` description (#723). The full and the stripped form are unchanged.

use serde_json::Value;

fn wire(source: &str) -> Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

fn plain(source: &str) -> String {
    expressir::formatter::format_with_keep(&wire(source), true, true)
}

fn full(source: &str) -> String {
    expressir::formatter::format(&wire(source), false)
}

fn stripped(source: &str) -> String {
    expressir::formatter::format(&wire(source), true)
}

fn entity(wheres: &str, tail: &str) -> String {
    format!(
        "SCHEMA s;\nENTITY e;\n  x : INTEGER;\nWHERE\n  WR1: x > 0;\n{wheres}END_ENTITY;\nEND_SCHEMA;\n{tail}"
    )
}

const DESCRIPTION: &str = "(*\"s.e.wr:IP1\"\ndescription text\n*)\n";

#[test]
fn a_source_line_is_written_in_the_plain_form() {
    let source = entity("  --IP1: source text one\n", "");
    assert!(plain(&source).contains("\n  --IP1: source text one\nEND_ENTITY;"));
    assert!(full(&source).contains("\n  --IP1: source text one\nEND_ENTITY;"));
}

#[test]
fn a_tagged_description_alone_writes_nothing_in_the_plain_form() {
    let source = entity("", DESCRIPTION);
    assert!(!plain(&source).contains("--IP1"), "{}", plain(&source));
    assert!(full(&source).contains("--IP1: description text"));
}

#[test]
fn source_and_description_with_the_same_text_write_one_line_in_the_plain_form() {
    let source = entity("  --IP1: description text\n", DESCRIPTION);
    assert_eq!(plain(&source).matches("--IP1: description text").count(), 1);
    assert_eq!(full(&source).matches("--IP1: description text").count(), 2);
}

#[test]
fn source_and_description_with_different_text_write_the_source_text() {
    let source = entity("  --IP1: source text\n", DESCRIPTION);
    let plain = plain(&source);
    assert!(plain.contains("--IP1: source text"), "{plain}");
    assert!(!plain.contains("description text"), "{plain}");
    let full = full(&source);
    assert!(
        full.contains("  --IP1: description text\n  --IP1: source text\nEND_ENTITY;"),
        "{full}"
    );
}

#[test]
fn a_bare_marker_is_written_and_a_bare_marker_with_a_blank_after_the_dashes() {
    assert!(plain(&entity("  --IP1:\n", "")).contains("\n  --IP1:\nEND_ENTITY;"));
    assert!(plain(&entity("  -- IP1: blank after dashes\n", ""))
        .contains("\n  --IP1: blank after dashes\nEND_ENTITY;"));
}

#[test]
fn continuation_lines_and_several_lines_follow_the_full_form() {
    let source = entity(
        "  --IP1: first line\n  --      continuation one\n  --      continuation two\n  --IP2: second\n",
        "",
    );
    let expected = "WR1: x > 0;\n  -- continuation one\n  -- continuation two\n  --IP1: first line\n  --IP2: second\nEND_ENTITY;";
    assert!(full(&source).contains(expected), "{}", full(&source));
    assert!(plain(&source).contains(expected), "{}", plain(&source));
}

#[test]
fn lines_between_two_where_rules_are_written_where_the_full_form_writes_them() {
    let source = "SCHEMA s;\nENTITY e;\n  x : INTEGER;\nWHERE\n  WR1: x > 0;\n  --IP1: between\n  WR2: x < 10;\n  --IP2: after\nEND_ENTITY;\nEND_SCHEMA;\n";
    assert_eq!(plain(source), full(source));
    assert!(plain(source).contains("WR2: x < 10;\n  --IP1: between\n  --IP2: after\nEND_ENTITY;"));
}

#[test]
fn an_entity_without_formal_where_rules_keeps_its_lines() {
    let source = "SCHEMA s;\nENTITY e;\n  x : INTEGER;\n  --IP1: no formal rules\nEND_ENTITY;\nEND_SCHEMA;\n";
    assert_eq!(plain(source), full(source));
    assert!(plain(source).contains("--IP1: no formal rules"));
}

#[test]
fn the_stripped_form_has_no_comment() {
    let source = entity("  --IP1: source text\n", DESCRIPTION);
    assert!(!stripped(&source).contains("--"), "{}", stripped(&source));
}

#[test]
fn the_plain_form_is_stable_on_a_second_pass() {
    let source = entity(
        "  --IP1: first line\n  --      more\n  --IP2: second\n",
        DESCRIPTION,
    );
    let once = plain(&source);
    assert_eq!(plain(&once), once);
}

#[test]
fn the_source_texts_are_held_on_the_remark_item() {
    let held = |source: &str| {
        wire(source)
            .pointer("/schemas/0/entities/0/informal_propositions/0/remark_items/0/source_remarks")
            .cloned()
    };
    let d = held(&entity("  --IP1: source text\n", DESCRIPTION)).expect("held");
    assert_eq!(d, serde_json::json!(["source text"]));
    assert_eq!(
        held(&entity("  --IP1:\n", "")).expect("held"),
        serde_json::json!([""])
    );
    let b = held(&entity("", DESCRIPTION));
    assert!(
        b.as_ref().is_none_or(|v| *v == serde_json::json!([])),
        "{b:?}"
    );
}

#[test]
fn lines_in_a_type_or_rule_where_part_stay_as_the_full_form_has_them() {
    let source = "SCHEMA s;\nTYPE t = INTEGER;\nWHERE\n  WR1: SELF > 0;\n  --IP1: in a type\nEND_TYPE;\nEND_SCHEMA;\n";
    assert_eq!(plain(source), full(source));
}
