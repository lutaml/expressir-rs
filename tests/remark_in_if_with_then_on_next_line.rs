//! A comment on a line of its own before ELSE or END_IF stays after the
//! statement it followed when the THEN of the IF stands on a later line
//! than the IF. The opener table asked for THEN on the line of IF, so such
//! an IF never opened and the closing keyword found no IF to close.

const SOURCE: &str = "SCHEMA else_probe;\nFUNCTION f (x : INTEGER) : INTEGER;\n  IF x > 0\n    THEN RETURN (1);\n    -- G1 on its own line before ELSE with a statement\n    ELSE RETURN (2);\n  END_IF;\n  IF x > 1 THEN\n    RETURN (3);\n    (* G2 a block on its own line before ELSE with a statement *)\n  ELSE RETURN (4);\n  END_IF;\n  IF x > 2 THEN\n    RETURN (5);\n    -- G3 on its own line before a bare ELSE\n  ELSE\n    RETURN (6);\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n";

const EXPECTED: &str = "SCHEMA else_probe;\n\nFUNCTION f(x : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    RETURN (1);\n    -- G1 on its own line before ELSE with a statement\n  ELSE\n    RETURN (2);\n  END_IF;\n  IF x > 1 THEN\n    RETURN (3);\n    (* G2 a block on its own line before ELSE with a statement *)\n  ELSE\n    RETURN (4);\n  END_IF;\n  IF x > 2 THEN\n    RETURN (5);\n    -- G3 on its own line before a bare ELSE\n  ELSE\n    RETURN (6);\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\n\nEND_SCHEMA;\n";

fn attached(source: &str) -> serde_json::Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

fn wrap(body: &str) -> String {
    format!(
        "SCHEMA s;\nFUNCTION f (x : INTEGER) : INTEGER;\n{body}  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n"
    )
}

fn written(source: &str) -> String {
    expressir::formatter::format(&attached(source), false)
}

#[test]
fn each_comment_is_written_after_the_statement_it_followed() {
    assert_eq!(written(SOURCE), EXPECTED);
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let once = written(SOURCE);
    assert_eq!(written(&once), once);
}

#[test]
fn the_stripped_form_writes_none_of_them() {
    let text = expressir::formatter::format(&attached(SOURCE), true);
    assert!(!text.contains("--") && !text.contains("(*"), "{text}");
}

#[test]
fn the_remarks_close_the_then_body_of_their_if() {
    let wire = attached(SOURCE);
    for index in 0..3 {
        let ptr = format!("/schemas/0/functions/0/statements/{index}/untagged_remarks/0");
        let remark = wire
            .pointer(&ptr)
            .unwrap_or_else(|| panic!("no remark at {ptr}"));
        let field = |name: &str| remark.get(name).and_then(serde_json::Value::as_str);
        assert_eq!(field("placement"), Some("trailing"), "{ptr}");
        assert_eq!(field("region"), Some("statements"), "{ptr}");
    }
}

/// Every shape: the comment `C1` is written, in the place the table gives.
#[test]
fn the_shapes_of_an_if_keep_their_comment() {
    let shapes: &[(&str, &str, &str)] = &[
        // (name, body, text that must follow C1 on the next code line)
        ("then on next line, bare else", "  IF x > 0\n    THEN RETURN (1);\n    -- C1\n  ELSE\n    RETURN (2);\n  END_IF;\n", "ELSE"),
        ("then on next line, before end_if", "  IF x > 0\n    THEN RETURN (1);\n    -- C1\n  END_IF;\n", "END_IF;"),
        ("then on next line, after else", "  IF x > 0\n    THEN RETURN (1);\n  ELSE RETURN (2);\n    -- C1\n  END_IF;\n", "END_IF;"),
        ("multi-line condition", "  IF (x > 0)\n    AND (x < 5)\n    THEN RETURN (1);\n    -- C1\n    ELSE RETURN (2);\n  END_IF;\n", "ELSE"),
        ("outer then on next line", "  IF x > 0\n    THEN\n    IF x > 1 THEN RETURN (1); END_IF;\n    -- C1\n    ELSE RETURN (2);\n  END_IF;\n", "ELSE"),
        ("inner then on next line", "  IF x > 0 THEN\n    IF x > 1\n      THEN RETURN (1);\n      -- C1\n      ELSE RETURN (2);\n    END_IF;\n  ELSE RETURN (3);\n  END_IF;\n", "ELSE"),
        ("in a CASE action", "  CASE x OF\n    1 : IF x > 0\n      THEN RETURN (1);\n      -- C1\n      ELSE RETURN (2);\n      END_IF;\n    OTHERWISE : RETURN (3);\n  END_CASE;\n", "ELSE"),
        ("in a REPEAT", "  REPEAT i := 1 TO 3;\n    IF x > 0\n      THEN RETURN (1);\n      -- C1\n      ELSE RETURN (2);\n    END_IF;\n  END_REPEAT;\n", "ELSE"),
        ("lower case", "  if x > 0\n    then return (1);\n    -- C1\n    else return (2);\n  end_if;\n", "ELSE"),
    ];
    for (name, body, next) in shapes {
        let text = written(&wrap(body));
        let lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.trim() == "-- C1")
            .unwrap_or_else(|| panic!("{name}: C1 lost\n{text}"));
        assert!(
            lines[at + 1].trim().to_uppercase().starts_with(next),
            "{name}: C1 is followed by {:?}\n{text}",
            lines[at + 1]
        );
        assert!(
            lines[at - 1].trim_end().ends_with(';'),
            "{name}: C1 does not follow a statement\n{text}"
        );
        assert_eq!(written(&text), text, "{name}: second pass differs");
    }
}

/// The shapes that were right before are written as before.
#[test]
fn the_shapes_that_worked_are_unchanged() {
    let cases: &[(&str, &str)] = &[
        (
            "  IF x > 0 THEN\n    RETURN (1);\n    -- C1\n  ELSE\n    RETURN (2);\n  END_IF;\n",
            "  IF x > 0 THEN\n    RETURN (1);\n    -- C1\n  ELSE\n    RETURN (2);\n  END_IF;\n",
        ),
        (
            "  IF x > 0 THEN\n    RETURN (1);\n    -- C1\n  END_IF;\n",
            "  IF x > 0 THEN\n    RETURN (1);\n    -- C1\n  END_IF;\n",
        ),
        (
            "  IF x > 0 THEN\n    RETURN (1);\n  ELSE -- C1\n    RETURN (2);\n  END_IF;\n",
            "  IF x > 0 THEN\n    RETURN (1);\n  ELSE -- C1\n    RETURN (2);\n  END_IF;\n",
        ),
        (
            "  IF x > 0 THEN -- C1\n    RETURN (1);\n  END_IF;\n",
            "  IF x > 0 THEN -- C1\n    RETURN (1);\n  END_IF;\n",
        ),
        (
            "  IF x > 0 THEN\n    RETURN (1);\n  END_IF; -- C1\n",
            "  IF x > 0 THEN\n    RETURN (1);\n  END_IF; -- C1\n",
        ),
        (
            "  IF x > 0\n    THEN RETURN (1); -- C1\n  ELSE RETURN (2);\n  END_IF;\n",
            "  IF x > 0 THEN\n    RETURN (1); -- C1\n  ELSE\n    RETURN (2);\n  END_IF;\n",
        ),
        (
            "  IF x > 0 THEN\n    x := LENGTH('IF a THEN');\n    -- IF THEN ELSE in a comment\n  ELSE RETURN (2);\n  END_IF;\n",
            "  IF x > 0 THEN\n    x := LENGTH('IF a THEN');\n    -- IF THEN ELSE in a comment\n  ELSE\n    RETURN (2);\n  END_IF;\n",
        ),
        (
            "  LOCAL gift : INTEGER; then_x : INTEGER; else_value : INTEGER; END_LOCAL;\n  gift := 1;\n  -- C1\n  then_x := 2;\n  -- C2\n  else_value := 3;\n",
            "  LOCAL\n    gift : INTEGER;\n    then_x : INTEGER;\n    else_value : INTEGER;\n  END_LOCAL;\n  gift := 1;\n  -- C1\n  then_x := 2;\n  -- C2\n  else_value := 3;\n",
        ),
    ];
    for (body, expected_body) in cases {
        assert_eq!(
            written(&wrap(body)),
            wrap(expected_body)
                .replacen("SCHEMA s;\n", "SCHEMA s;\n\n", 1)
                .replacen(
                    "END_FUNCTION;\nEND_SCHEMA;\n",
                    "END_FUNCTION;\n\nEND_SCHEMA;\n",
                    1
                )
                .replacen("FUNCTION f (x : INTEGER)", "FUNCTION f(x : INTEGER)", 1),
            "{body}"
        );
    }
}

/// A comment inside a `(* ... *)` remark is not code for the scope map, and
/// of several IFs that start on one line the one still open is closed.
/// Each case: the source, then each `--` comment with the line before it.
#[test]
fn comments_in_prose_and_on_shared_lines_follow_the_statement_they_followed() {
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, &str, &[(&str, &str)])] = &[
        (
            "block_with_if_prose",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    RETURN (1);\n    (* note:\n IF this happens\n *)\n    -- c\n  ELSE\n    RETURN (2);\n    -- e\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("RETURN (2);", "-- e")],
        ),
        (
            "block_with_lower_if",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    RETURN (1);\n    (* if\n       if this\n    *)\n    -- c\n  ELSE\n    RETURN (2);\n  -- e\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("RETURN (2);", "-- e")],
        ),
        (
            "block_after_statement_one_line",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    y := 2; (* if big *)\n    -- c\n  ELSE\n    y := 3;\n    -- d\n  END_IF;\n  -- e\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("(* if big *)", "-- c"), ("y := 3;", "-- d"), ("END_IF;", "-- e")],
        ),
        (
            "block_opened_after_statement",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    RETURN (1); (* IF\n    still *)\n    -- c\n  ELSE\n    RETURN (2);\n  -- e\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("RETURN (2);", "-- e")],
        ),
        (
            "two_ifs_in_one_block",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    y := 1;\n    (*\n    IF a IF b\n    IF c\n    *)\n  -- d\n  ELSE\n    y := 2;\n  -- e\n  END_IF;\n  -- f\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- d"), ("y := 2;", "-- e"), ("END_IF;", "-- f")],
        ),
        (
            "prose_in_case",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  CASE x OF\n    1 : y := 1;\n    (* positive\n       if big *)\n    -- c\n    2 : y := 2;\n    -- d\n    OTHERWISE : y := 3;\n    -- e\n  END_CASE;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("y := 2;", "-- d"), ("y := 3;", "-- e")],
        ),
        (
            "prose_before_end_function",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  y := 1;\n  RETURN (0);\n  (* positive\n     if big *)\n  -- tail\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- tail")],
        ),
        (
            "prose_in_procedure",
            "SCHEMA s;\nPROCEDURE p (VAR x : INTEGER);\n  x := 1;\n  (* positive\n     if big *)\n  -- c\n  x := 2;\n  -- d\nEND_PROCEDURE;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("x := 2;", "-- d")],
        ),
        (
            "prose_in_rule",
            "SCHEMA s;\nENTITY e; a : INTEGER; END_ENTITY;\nRULE r FOR (e);\n  LOCAL z : INTEGER; END_LOCAL;\n  z := 1;\n  (* positive\n     if big *)\n  -- c\nWHERE\n  -- w\n  w1 : z > 0;\n  -- w2\nEND_RULE;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("w1: z > 0;", "-- w2")],
        ),
        (
            "nested_block_remark",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    y := 1;\n    (* a (* IF b *) IF c -- not an end\n       IF d *)\n    -- c\n  ELSE\n    y := 2;\n    -- e\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("*)", "-- c"), ("y := 2;", "-- e")],
        ),
        (
            "block_and_dashes_in_text",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    y := LENGTH('(* IF');\n    -- c (* IF\n  ELSE\n    y := 2;\n    -- e\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("y := LENGTH('(* IF');", "-- c (* IF"), ("y := 2;", "-- e")],
        ),
        (
            "prose_with_then_and_end_if",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN\n    y := 1;\n    (* IF a THEN b END_IF; REPEAT BEGIN ALIAS CASE y OF *)\n    -- c\n  ELSE\n    y := 2;\n    -- e\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("(* IF a THEN b END_IF; REPEAT BEGIN ALIAS CASE y OF *)", "-- c"), ("y := 2;", "-- e")],
        ),
        (
            "two_ifs_on_a_line_second_then_next",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN y := 1; END_IF; IF y > 0\n  THEN y := 2;\n  -- c\n  ELSE y := 3;\n  -- d\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("y := 2;", "-- c"), ("y := 3;", "-- d")],
        ),
        (
            "two_ifs_on_a_line_then_on_if_line",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN y := 1; END_IF; IF y > 0 THEN y := 2;\n  -- c\n  ELSE y := 3;\n  -- d\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("y := 2;", "-- c"), ("y := 3;", "-- d")],
        ),
        (
            "two_ifs_on_a_line_no_else",
            "SCHEMA s;\nFUNCTION f (x : INTEGER; y : INTEGER) : INTEGER;\n  IF x > 0 THEN y := 1; END_IF; IF y > 0\n  THEN y := 2;\n  -- c\n  END_IF;\n  RETURN (0);\nEND_FUNCTION;\nEND_SCHEMA;\n",
            &[("y := 2;", "-- c")],
        ),
    ];
    // The comments with the line before each; a second pass must keep them
    // (the indentation of the lines of a block remark may change).
    let comments_of = |text: &str| -> Vec<(String, String)> {
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("--"))
            .map(|(i, l)| (lines[i - 1].to_string(), l.to_string()))
            .collect()
    };
    for (name, source, comments) in cases {
        let text = written(source);
        let expected: Vec<(String, String)> = comments
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(comments_of(&text), expected, "{name}\n{text}");
        assert_eq!(
            comments_of(&written(&text)),
            expected,
            "{name}: second pass"
        );
    }
}
