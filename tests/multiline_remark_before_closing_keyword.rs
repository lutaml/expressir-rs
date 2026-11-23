//! A comment of several lines on a line of its own before a closing keyword
//! is written back before that keyword. The lookup of the keyword started on
//! the second line of the comment, read the rest of the comment as code and
//! found no keyword: the comment was lost before ELSE, END_IF, OTHERWISE,
//! END_CASE, END, WHERE of a RULE, END_FUNCTION, END_PROCEDURE and END_RULE.

fn attached(source: &str) -> serde_json::Value {
    let mut wire = expressir::ParsedTree::parse(source)
        .expect("parses")
        .to_model_json("s.exp")
        .expect("wire");
    expressir::attach::attach_tagged_remarks(&mut wire, source, false);
    expressir::attach::attach_untagged_remarks(&mut wire, source, false);
    wire
}

fn function(body: &str) -> String {
    format!("FUNCTION f(p : INTEGER) : INTEGER;\n  LOCAL x : INTEGER; END_LOCAL;\n{body}  RETURN (1);\nEND_FUNCTION;\n")
}

/// The source of a schema with comment `c` before `keyword`.
fn source(keyword: &str, c: &str) -> String {
    let body = match keyword {
        "ELSE" => function(&format!(
            "  IF p > 0 THEN\n    x := 1;\n  {c}\n  ELSE\n    x := 2;\n  END_IF;\n"
        )),
        "END_IF" => function(&format!(
            "  IF p > 0 THEN\n    x := 1;\n  {c}\n  END_IF;\n"
        )),
        "OTHERWISE" => function(&format!(
            "  CASE p OF\n    1 : x := 1;\n  {c}\n  OTHERWISE : x := 2;\n  END_CASE;\n"
        )),
        "END_CASE" => function(&format!(
            "  CASE p OF\n    1 : x := 1;\n  {c}\n  END_CASE;\n"
        )),
        "END;" => function(&format!("  BEGIN\n    x := 1;\n  {c}\n  END;\n")),
        "WHERE" => format!(
            "RULE r FOR (a);\n  LOCAL y : INTEGER; END_LOCAL;\n  y := 1;\n  {c}\nWHERE\n  wr1 : TRUE;\nEND_RULE;\n"
        ),
        "END_RULE" => format!("RULE r FOR (a);\n  y := 1;\nWHERE\n  wr1 : TRUE;\n  {c}\nEND_RULE;\n"),
        "END_FUNCTION" => format!(
            "FUNCTION f(p : INTEGER) : INTEGER;\n  LOCAL x : INTEGER; END_LOCAL;\n  x := 1;\n  {c}\nEND_FUNCTION;\n"
        ),
        "END_PROCEDURE" => format!(
            "PROCEDURE pr(p : INTEGER);\n  LOCAL x : INTEGER; END_LOCAL;\n  x := 1;\n  {c}\nEND_PROCEDURE;\n"
        ),
        "END_REPEAT" => function(&format!(
            "  REPEAT i := 1 TO 3;\n    x := 1;\n  {c}\n  END_REPEAT;\n"
        )),
        "END_ALIAS" => function(&format!(
            "  ALIAS q FOR x;\n    x := 1;\n  {c}\n  END_ALIAS;\n"
        )),
        other => panic!("no construct for {other}"),
    };
    format!("SCHEMA s;\n{body}END_SCHEMA;\n")
}

const KEYWORDS: [&str; 9] = [
    "ELSE",
    "END_IF",
    "OTHERWISE",
    "END_CASE",
    "END;",
    "WHERE",
    "END_FUNCTION",
    "END_PROCEDURE",
    "END_RULE",
];

/// (name, comment with KW for the keyword, fragments in order)
fn forms() -> Vec<(&'static str, &'static str, Vec<&'static str>)> {
    vec![
        (
            "two lines",
            "(* a note\n     over two lines *)",
            vec!["a note", "over two lines"],
        ),
        (
            "three lines",
            "(* c1\n     c2\n     c3 *)",
            vec!["c1", "c2", "c3"],
        ),
        ("(* alone", "(*\n     c1\n     c2\n  *)", vec!["c1", "c2"]),
        (
            "nested",
            "(* c1 (* c2\n     c3 *) c4\n     c5 *)",
            vec!["c1", "c2", "c3", "c4", "c5"],
        ),
        (
            "dash then block",
            "-- c1\n  (* c2\n     c3 *)",
            vec!["c1", "c2", "c3"],
        ),
        (
            "block then dash",
            "(* c1\n     c2 *)\n  -- c3",
            vec!["c1", "c2", "c3"],
        ),
        (
            "names the keyword",
            "(* KW\n     note KW *)",
            vec!["note KW"],
        ),
        (
            "names another keyword",
            "(* END_IF ELSE\n     note OTHERWISE *)",
            vec!["note OTHERWISE"],
        ),
        (
            "blank lines after",
            "(* c1\n     c2 *)\n\n\n",
            vec!["c1", "c2"],
        ),
    ]
}

fn fill(s: &str, keyword: &str) -> String {
    s.replace("KW", &keyword.replace(';', ""))
}

/// Every fragment is written, in order, and nothing but the end of the
/// comment lies between the last one and the keyword.
fn in_place(out: &str, fragments: &[String], keyword: &str) -> bool {
    let mut from = 0;
    for f in fragments {
        match out[from..].find(f.as_str()) {
            Some(i) => from += i + f.len(),
            None => return false,
        }
    }
    let rest = out[from..].trim_start();
    let rest = rest.strip_prefix("*)").unwrap_or(rest).trim_start();
    rest.starts_with(keyword)
}

fn remarks(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::Array(a)) = m.get("untagged_remarks") {
                out.extend(a.iter().cloned());
            }
            for x in m.values() {
                remarks(x, out);
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| remarks(x, out)),
        _ => {}
    }
}

fn cases() -> Vec<(String, String, Vec<String>, String)> {
    let mut v = Vec::new();
    for keyword in KEYWORDS {
        for (name, comment, frags) in forms() {
            // A first line that names END_FUNCTION, END_PROCEDURE or END_RULE
            // is read as a comment on that END_ line, whatever follows: not
            // this case.
            if name == "names the keyword"
                && ["END_FUNCTION", "END_PROCEDURE", "END_RULE"].contains(&keyword)
            {
                continue;
            }
            v.push((
                format!("{keyword} / {name}"),
                source(keyword, &fill(comment, keyword)),
                frags.iter().map(|f| fill(f, keyword)).collect(),
                keyword.to_string(),
            ));
        }
    }
    v
}

#[test]
fn every_line_of_the_comment_is_written_before_the_keyword() {
    let mut bad = Vec::new();
    for (name, src, frags, keyword) in cases() {
        let out = expressir::formatter::format(&attached(&src), false);
        if !in_place(&out, &frags, &keyword) {
            bad.push(name);
        }
    }
    assert!(bad.is_empty(), "lost or misplaced: {bad:?}");
}

#[test]
fn the_same_comment_again_from_its_own_output() {
    let mut bad = Vec::new();
    for (name, src, frags, keyword) in cases() {
        let once = expressir::formatter::format(&attached(&src), false);
        let twice = expressir::formatter::format(&attached(&once), false);
        if !in_place(&twice, &frags, &keyword) {
            bad.push(name);
        }
    }
    assert!(
        bad.is_empty(),
        "lost or misplaced on the second pass: {bad:?}"
    );
}

#[test]
fn the_comment_is_held_as_trailing_and_absent_when_stripped() {
    let mut bad = Vec::new();
    for (name, src, _, _) in cases() {
        let wire = attached(&src);
        let mut found = Vec::new();
        remarks(&wire, &mut found);
        let all_trailing = !found.is_empty()
            && found
                .iter()
                .all(|r| r.get("placement").and_then(|p| p.as_str()) == Some("trailing"));
        let stripped = expressir::formatter::format(&wire, true);
        if !all_trailing || stripped.contains("(*") || stripped.contains("--") {
            bad.push(name);
        }
    }
    assert!(
        bad.is_empty(),
        "not trailing, or kept when stripped: {bad:?}"
    );
}

#[test]
fn the_comments_before_end_repeat_and_end_alias_are_unchanged() {
    for keyword in ["END_REPEAT", "END_ALIAS"] {
        let src = source(keyword, "(* c1\n     c2\n     c3 *)");
        let out = expressir::formatter::format(&attached(&src), false);
        let frags: Vec<String> = ["c1", "c2", "c3"].iter().map(|s| s.to_string()).collect();
        assert!(in_place(&out, &frags, keyword), "{keyword}:\n{out}");
    }
}

#[test]
fn a_comment_of_several_lines_between_statements_leads_the_next_one() {
    let src = "SCHEMA s;\nFUNCTION f(p : INTEGER) : INTEGER;\n  LOCAL x : INTEGER; END_LOCAL;\n  x := 1;\n  (* c1\n     c2 *)\n  x := 2;\n  RETURN (x);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let wire = attached(src);
    let mut found = Vec::new();
    remarks(&wire, &mut found);
    let placements: Vec<_> = found
        .iter()
        .map(|r| r.get("placement").and_then(|p| p.as_str()).unwrap_or(""))
        .collect();
    assert_eq!(placements, vec!["leading"]);
    let out = expressir::formatter::format(&wire, false);
    assert!(out.find("c2").unwrap() < out.find("x := 2;").unwrap());
}

#[test]
fn a_comment_of_several_lines_before_a_declaration_stays_with_it() {
    let src = "SCHEMA s;\n(* c1\n   c2 *)\nENTITY e;\n  a : INTEGER;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let out = expressir::formatter::format(&attached(src), false);
    assert!(out.find("c2").unwrap() < out.find("ENTITY e").unwrap());
}

// Code after the `*)` on the comment's last line: the comment is not on
// lines of its own before the keyword, and stays where it was written (the
// first two). Only comments after the `*)`: it is before the keyword.
#[test]
fn code_on_the_last_line_of_the_comment_keeps_it_before_that_code() {
    let head = "SCHEMA s;\nFUNCTION f(p : INTEGER) : INTEGER;\n  LOCAL\n    y : INTEGER;\n  END_LOCAL;\n  IF p > 0 THEN\n    y := 1;\n";
    let tail = "  ELSE\n    y := 3;\n  END_IF;\n  RETURN (y);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let cases = [
        (
            "    (* a\n       b *) y := 2;\n",
            "    y := 1;\n    (*\n    a\n           b\n    *)\n    y := 2;\n  ELSE\n",
        ),
        (
            "    (* a\n       b *) (* c *) y := 2;\n",
            "    y := 1;\n    (*\n    a\n           b\n    *)\n    y := 2;\n    (* c *)\n  ELSE\n",
        ),
        (
            "    (* a\n       b *) -- tail\n",
            "    y := 1;\n    (*\n    a\n           b\n    *)\n    -- tail\n  ELSE\n",
        ),
        (
            "    (* a\n       b *) (* c *)\n",
            "    y := 1;\n    (*\n    a\n           b\n    *)\n    (* c *)\n  ELSE\n",
        ),
    ];
    for (body, expected) in cases {
        let src = format!("{head}{body}{tail}");
        let out = expressir::formatter::format(&attached(&src), false);
        assert!(out.contains(expected), "{body}\n{out}");
    }
}

// After a comment of several lines, a block comment on the next line with
// code after it is a code line too: the comment before it leads that code
// and is not taken to the keyword. A comment of ONE line in that place is
// written as before (the last four).
#[test]
fn a_following_block_with_code_after_it_is_a_code_line() {
    let head = "SCHEMA s;\nFUNCTION f(p : INTEGER) : INTEGER;\n  LOCAL\n    y : INTEGER;\n  END_LOCAL;\n  IF p > 0 THEN\n    y := 1;\n";
    let tail = "  ELSE\n    y := 3;\n  END_IF;\n  RETURN (y);\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let cases = [
        ("    (* a\n       b *)\n    (* f *) y := 2;\n", "    y := 1;\n    (*\n    a\n           b\n    *)\n    y := 2;\n    (* f *)\n  ELSE\n"),
        ("    (* a\n       b *)\n    (* f\n       g *) y := 2;\n", "    y := 1;\n    (*\n    a\n           b\n    *)\n    (*\n    f\n           g\n    *)\n    y := 2;\n  ELSE\n"),
        ("    (* a *)\n    (* f *) y := 2;\n", "    y := 1;\n    y := 2;\n    (* a *)\n    (* f *)\n  ELSE\n"),
        ("    -- a\n    (* f *) y := 2;\n", "    y := 1;\n    y := 2;\n    -- a\n    (* f *)\n  ELSE\n"),
        ("    y := 2; (* r *)\n    (* f *) y := 6;\n", "    y := 1;\n    y := 2;\n    y := 6;\n    (* r *)\n    (* f *)\n  ELSE\n"),
        ("    (* a\n       b *) (* c *) y := 2;\n    (* f *) y := 6;\n", "    y := 1;\n    (*\n    a\n           b\n    *)\n    y := 2;\n    y := 6;\n    (* c *)\n    (* f *)\n  ELSE\n"),
    ];
    for (body, expected) in cases {
        let src = format!("{head}{body}{tail}");
        let out = expressir::formatter::format(&attached(&src), false);
        assert!(out.contains(expected), "{body}\n{out}");
    }
}
