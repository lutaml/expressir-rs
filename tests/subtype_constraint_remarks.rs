//! A comment inside the supertype expression of a SUBTYPE_CONSTRAINT is
//! written back where it stands: after the operand it follows on its
//! line, or on a line of its own before the operand it precedes. The wire
//! held these remarks on the SUBTYPE_CONSTRAINT, unplaced, and they were
//! written after `END_SUBTYPE_CONSTRAINT;`. The operands are handled as
//! the items of a SELECT are.

fn wrapped(body: &str) -> String {
    let mut text = String::from("SCHEMA s;\nENTITY t;\nEND_ENTITY;\n");
    for name in ["a", "b", "c", "d"] {
        text.push_str(&format!("ENTITY {name}\n  SUBTYPE OF (t);\nEND_ENTITY;\n"));
    }
    text.push_str(&format!(
        "SUBTYPE_CONSTRAINT k FOR t;\n{body}END_SUBTYPE_CONSTRAINT;\nEND_SCHEMA;\n"
    ));
    text
}

const LISTED: &str = "  ONEOF (a, -- from the first module\n         b, -- from the second module\n         c); -- from the third module\n";

fn attached(source: &str) -> serde_json::Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

fn formatted(source: &str) -> String {
    expressir::formatter::format(&attached(source), false)
}

fn written(body: &str) -> String {
    formatted(&wrapped(body))
}

#[test]
fn each_comment_is_written_where_it_stands_in_the_list() {
    assert!(
        written(LISTED).contains(
            "  ONEOF(a, -- from the first module\n        b, -- from the second module\n        c); -- from the third module\nEND_SUBTYPE_CONSTRAINT;"
        ),
        "{}",
        written(LISTED)
    );
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let once = written(LISTED);
    assert_eq!(formatted(&once), once);
}

#[test]
fn the_remarks_are_held_by_the_operands() {
    let wire = attached(&wrapped(LISTED));
    let base = "/schemas/0/subtype_constraints/0";
    let held = |i: usize| {
        let r = wire
            .pointer(&format!(
                "{base}/supertype_expression/operands/{i}/untagged_remarks/0"
            ))
            .expect("remark");
        (
            r["text"].as_str().unwrap_or("").to_string(),
            r["placement"].as_str().unwrap_or("").to_string(),
        )
    };
    assert_eq!(held(0), ("from the first module".into(), "inline".into()));
    assert_eq!(held(1), ("from the second module".into(), "inline".into()));
    assert_eq!(held(2), ("from the third module".into(), "inline".into()));
    assert!(wire
        .pointer(&format!("{base}/untagged_remarks"))
        .and_then(serde_json::Value::as_array)
        .is_none_or(Vec::is_empty));
}

#[test]
fn the_stripped_form_writes_none_of_them() {
    let text = expressir::formatter::format(&attached(&wrapped(LISTED)), true);
    assert!(
        text.contains("  ONEOF(a, b, c);\nEND_SUBTYPE_CONSTRAINT;"),
        "{text}"
    );
    assert!(!text.contains("--"), "{text}");
}

#[test]
fn a_comment_inside_a_nested_expression_and_one_after_the_semicolon() {
    let text = written("  (ONEOF (a, -- first\n          b) ANDOR c); -- last\n");
    assert!(
        text.contains("  ONEOF(a, -- first\n        b) ANDOR c; -- last\nEND_SUBTYPE_CONSTRAINT;"),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

#[test]
fn a_comment_on_a_line_of_its_own_before_an_operand() {
    let text = written("  ONEOF (\n    -- old_a,\n    a,\n    -- old_b,\n    b,\n    c);\n");
    assert!(
        text.contains(
            "  ONEOF(-- old_a,\n        a,\n        -- old_b,\n        b,\n        c);\n"
        ),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

#[test]
fn a_comment_after_two_operands_on_a_line_follows_the_second() {
    let text = written("  ONEOF (a, b, -- after b\n         c, d);\n");
    assert!(
        text.contains("  ONEOF(a,\n        b, -- after b\n        c,\n        d);"),
        "{text}"
    );
}

#[test]
fn a_comment_after_the_last_operand_is_written_after_the_semicolon() {
    let text = written("  ONEOF (a,\n         b -- after last\n        );\n");
    assert!(
        text.contains("  ONEOF(a,\n        b); -- after last\nEND_SUBTYPE_CONSTRAINT;"),
        "{text}"
    );
}

#[test]
fn an_expression_without_comments_is_written_as_before() {
    let text = written("  (ONEOF (a, b) ANDOR c) AND d;\n");
    assert!(
        text.contains("  (ONEOF(a, b) ANDOR c) AND d;\nEND_SUBTYPE_CONSTRAINT;"),
        "{text}"
    );
}

// Not changed: the comments of ABSTRACT SUPERTYPE and TOTAL_OVER, a
// comment after an operator, and one after an inner ONEOF.
#[test]
fn the_comments_of_the_other_lines_of_the_constraint_are_written_as_before() {
    let text = written(
        "  ABSTRACT SUPERTYPE; -- abstract\n  TOTAL_OVER (a, -- total a\n              b); -- total b\n  ONEOF (a, b);\n",
    );
    assert!(
        text.contains(
            "  ABSTRACT SUPERTYPE;\n  TOTAL_OVER(a, b);\n  ONEOF(a, b);\nEND_SUBTYPE_CONSTRAINT; -- abstract\n-- total a\n-- total b\n"
        ),
        "{text}"
    );
}

#[test]
fn a_comment_after_an_operator_is_written_as_before() {
    let text = written("  (ONEOF (a, b) ANDOR -- after operator\n   c);\n");
    assert!(
        text.contains("  ONEOF(a, b) ANDOR c;\nEND_SUBTYPE_CONSTRAINT; -- after operator"),
        "{text}"
    );
}

#[test]
fn a_comment_after_an_inner_oneof_is_written_as_before() {
    let text = written("  ONEOF (a, ONEOF (b, c) -- after inner\n         , d);\n");
    assert!(
        text.contains("  ONEOF(a, ONEOF(b, c), d);\nEND_SUBTYPE_CONSTRAINT; -- after inner"),
        "{text}"
    );
}

// An operand written in parentheses: the span the parser gives it may or
// may not include the parentheses, and the comments are placed the same.
#[test]
fn a_comment_after_a_parenthesized_operand_follows_the_operand() {
    let text = written("  ONEOF ((a), -- x\n b);\n");
    assert!(text.contains("  ONEOF(a, -- x\n        b);\n"), "{text}");
    let text = written("  ONEOF (a, ((b)), -- x\n c);\n");
    assert!(
        text.contains("  ONEOF(a,\n        b, -- x\n        c);\n"),
        "{text}"
    );
    let text = written("  ONEOF ((a), -- x\n (b), -- y\n c);\n");
    assert!(
        text.contains("  ONEOF(a, -- x\n        b, -- y\n        c);\n"),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

#[test]
fn a_comment_after_a_parenthesized_last_operand_is_written_after_the_semicolon() {
    let text = written("  (b ANDOR ONEOF ( d , b , c , (c) )) -- c1\n;\n");
    assert!(
        text.contains("  b ANDOR ONEOF(d,\n        b,\n        c,\n        c); -- c1\nEND_SUBTYPE_CONSTRAINT;"),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

// A block comment follows an operand as a line comment does.
#[test]
fn a_block_comment_after_the_last_operand_is_kept_by_the_second_pass() {
    for body in [
        "  ONEOF (c, b) (* b1 *) ; -- c2\n",
        "  ONEOF (c, b) (* b1 *); -- c2\n",
        "  ONEOF (a, b); (* x *) -- y\n",
        "  ONEOF (a, b); (* x *)\n",
    ] {
        let once = written(body);
        assert_eq!(formatted(&once), once, "{body}");
        assert!(once.contains("(*"), "{once}");
    }
    let text = written("  ONEOF (c, b) (* b1 *) ; -- c2\n");
    assert!(
        text.contains("  ONEOF(c,\n        b); (* b1 *)\nEND_SUBTYPE_CONSTRAINT; -- c2"),
        "{text}"
    );
}

#[test]
fn a_block_comment_between_two_operands_stays_after_the_comma() {
    let text = written("  ONEOF (a, (* x *) b -- y\n  , c);\n");
    assert!(
        text.contains("  ONEOF(a, (* x *)\n        b,\n        c);\nEND_SUBTYPE_CONSTRAINT; -- y"),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

// A comment before an operand leads it when the last code before the
// comment is a comma or the parenthesis that opens a ONEOF list, and only
// the white space and comments of the source lie between the comment and
// the operand's name.
#[test]
fn a_comment_before_an_operand_after_the_list_parenthesis_or_a_comma_leads_it() {
    let text = written("  ONEOF (\n -- l\n a,\n -- m\n b);\n");
    assert!(
        text.contains("  ONEOF(-- l\n        a,\n        -- m\n        b);\n"),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

#[test]
fn a_block_comment_after_a_comma_stays_with_the_operand_before_a_parenthesized_one() {
    let text = written("  ONEOF (a, (* l *)\n (b), c);\n");
    assert!(
        text.contains("  ONEOF(a, (* l *)\n        b,\n        c);\n"),
        "{text}"
    );
    assert_eq!(formatted(&text), text);
}

// A comment that stands inside the own parentheses of an operand is not
// taken: it stays where it was, after END_SUBTYPE_CONSTRAINT;, on every
// pass. So does one that follows the operator of an operand, whatever
// stands between the comment and the operand's name.
#[test]
fn a_comment_inside_the_parentheses_of_an_operand_is_not_taken() {
    for (body, expected) in [
        (
            "  ONEOF (a, (-- x\n b));\n",
            "  ONEOF(a, b);\nEND_SUBTYPE_CONSTRAINT; -- x\n",
        ),
        (
            "  ONEOF (a, (\n -- x\n b));\n",
            "  ONEOF(a, b);\nEND_SUBTYPE_CONSTRAINT; -- x\n",
        ),
        (
            "  ONEOF (a, ( -- x\n b)) ;\n",
            "  ONEOF(a, b);\nEND_SUBTYPE_CONSTRAINT; -- x\n",
        ),
        (
            "  ONEOF ((-- l\n a), b);\n",
            "  ONEOF(a, b);\nEND_SUBTYPE_CONSTRAINT; -- l\n",
        ),
        (
            "  ONEOF (\n -- l\n (a), b);\n",
            "  ONEOF(a, b);\nEND_SUBTYPE_CONSTRAINT; -- l\n",
        ),
        (
            "  ONEOF (a,\n -- l\n (b), c);\n",
            "  ONEOF(a, b, c);\nEND_SUBTYPE_CONSTRAINT; -- l\n",
        ),
        (
            "  ONEOF (a ANDOR (-- l\n b), c);\n",
            "  ONEOF(a ANDOR b, c);\nEND_SUBTYPE_CONSTRAINT; -- l\n",
        ),
        (
            "  ONEOF (b) ANDOR\n (-- c\n c);\n",
            "  ONEOF(b) ANDOR c;\nEND_SUBTYPE_CONSTRAINT; -- c\n",
        ),
        (
            "  ONEOF (b) ANDOR\n ((* c *)\n c);\n",
            "  ONEOF(b) ANDOR c;\nEND_SUBTYPE_CONSTRAINT;\n",
        ),
        (
            "  ONEOF (a, b) ANDOR -- c\n c;\n",
            "  ONEOF(a, b) ANDOR c;\nEND_SUBTYPE_CONSTRAINT; -- c\n",
        ),
        (
            "  ONEOF (a, b) AND\n -- c\n c;\n",
            "  ONEOF(a, b) AND c;\nEND_SUBTYPE_CONSTRAINT; -- c\n",
        ),
    ] {
        let text = written(body);
        assert!(
            text.contains(&format!("SUBTYPE_CONSTRAINT k FOR t;\n{expected}")),
            "{body}: {text}"
        );
        assert_eq!(formatted(&text), text, "{body}");
    }
}

// An operand that follows ANDOR or AND leads no comment: the written form
// has no place for it that is read back the same way.
#[test]
fn a_comment_inside_the_parentheses_after_andor_or_and_stays_after_the_constraint() {
    for body in [
        "  ONEOF (a, b) ANDOR (-- l\n c);\n",
        "  ONEOF (a, b) ANDOR (c ANDOR (-- l\n d));\n",
        "  ONEOF (a, b ANDOR (-- l\n (c)));\n",
        "  ONEOF (a, b AND (-- l\n c));\n",
        "  ONEOF (a, (b ANDOR ( -- l\n c)));\n",
        "  (a ANDOR (-- l\n b));\n",
        "  ONEOF (a, b) andor (-- l\n c);\n",
        "  ONEOF (a, b) ANDOR (\n -- l\n c);\n",
    ] {
        let text = written(body);
        assert!(
            text.contains("END_SUBTYPE_CONSTRAINT; -- l\n"),
            "{body}: {text}"
        );
        assert_eq!(formatted(&text), text, "{body}");
    }
}

// After the name of the last operand and inside its parentheses the comment
// follows the operand, as the parser spans it (the name).
#[test]
fn a_comment_after_the_name_of_a_parenthesized_last_operand_is_written_after_the_semicolon() {
    for (body, comment) in [
        ("  ONEOF (a, (b -- y\n ));\n", "-- y"),
        ("  ONEOF (a, (b (* y *)));\n", "(* y *)"),
    ] {
        let text = written(body);
        assert!(
            text.contains(&format!(
                "  ONEOF(a,\n        b); {comment}\nEND_SUBTYPE_CONSTRAINT;"
            )),
            "{body}: {text}"
        );
        assert_eq!(formatted(&text), text, "{body}");
    }
}

// A comment before the name of an operand, inside its parentheses, is held
// by no operand.
#[test]
fn no_operand_holds_a_comment_inside_its_parentheses() {
    let wire = attached(&wrapped("  ONEOF (a, (-- x\n b));\n"));
    for i in 0..2 {
        let pointer = format!(
            "/schemas/0/subtype_constraints/0/supertype_expression/operands/{i}/untagged_remarks"
        );
        assert!(
            wire.pointer(&pointer)
                .and_then(serde_json::Value::as_array)
                .is_none_or(Vec::is_empty),
            "operand {i}"
        );
    }
}

// A form feed after the comment is white space, as a blank is: the Ruby
// half reads it so too.
#[test]
fn a_form_feed_after_the_comment_is_white_space() {
    for space in ["\u{c}", " \u{c}"] {
        let text = written(&format!("  ONEOF (a,\n (* c *){space}\n b);\n"));
        assert!(
            text.contains("  ONEOF(a,\n        (* c *)\n        b);\n"),
            "{space:?}\n{text}"
        );
    }
}
