//! A comment whose own text names an END_ keyword is not the END line of
//! a scope. The attacher asked whether the LINE of a remark held an END_
//! keyword, text of the comment included, so `-- before END_FUNCTION` on a
//! line of its own was written after `END_FUNCTION;`. The question is now
//! asked of the code of the line only, in upper and lower case alike.

const KEYWORDS: [&str; 6] = [
    "END_SCHEMA",
    "END_ENTITY",
    "END_TYPE",
    "END_FUNCTION",
    "END_PROCEDURE",
    "END_RULE",
];

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

/// The comment on a line of its own, or at the end of a code line.
fn host(kind: &str, comment: &str, where_: &str) -> String {
    let (head, code, tail): (&str, &str, &str) = match kind {
        "END_ENTITY" => ("ENTITY e;\n  x : INTEGER;", "  y : INTEGER;", "END_ENTITY;"),
        "END_TYPE" => ("TYPE t = INTEGER;", "", "END_TYPE;"),
        "END_FUNCTION" => (
            "FUNCTION f (x : INTEGER) : INTEGER;\n  RETURN (x);",
            "  RETURN (x);",
            "END_FUNCTION;",
        ),
        "END_PROCEDURE" => (
            "PROCEDURE p (x : INTEGER);\n  RETURN;",
            "  RETURN;",
            "END_PROCEDURE;",
        ),
        "END_RULE" => ("RULE r FOR (e);\nWHERE\n  wr1 : TRUE;", "", "END_RULE;"),
        _ => ("ENTITY e;\n  x : INTEGER;\nEND_ENTITY;", "", ""),
    };
    let mut s = format!("SCHEMA s;\n{head}\n");
    match where_ {
        "before_end" => s.push_str(&format!("{comment}\n")),
        "middle" => s.push_str(&format!("{comment}\n{code}\n")),
        "inner" => {
            s = format!(
                "SCHEMA s;\nENTITY inner_e;\n  z : INTEGER;\n{comment}\nEND_ENTITY;\n{head}\n"
            )
        }
        "tail" => s.push_str(&format!("{code} {comment}\n")),
        _ => unreachable!(),
    }
    s.push_str(&format!("{tail}\nEND_SCHEMA;\n"));
    s
}

/// Expectation by construction: the same input with a neutral word in the
/// comment, formatted, with the keyword put back.
fn expected(kind: &str, style: &str, name: &str, where_: &str) -> String {
    let comment = |w: &str| match style {
        "dash" => format!("-- see {w}"),
        _ => format!("(* see {w} *)"),
    };
    formatted(&host(kind, &comment("WORD"), where_)).replace("WORD", name)
}

#[test]
fn a_comment_naming_an_end_keyword_goes_where_the_same_comment_without_it_goes() {
    let mut checked = 0;
    for kind in KEYWORDS {
        for lower in [false, true] {
            let name = if lower {
                kind.to_ascii_lowercase()
            } else {
                kind.to_string()
            };
            for style in ["dash", "block"] {
                for where_ in ["before_end", "middle", "inner", "tail"] {
                    if kind == "END_SCHEMA" && where_ != "before_end" && where_ != "inner" {
                        continue;
                    }
                    if matches!(kind, "END_TYPE" | "END_RULE") && where_ == "middle" {
                        continue;
                    }
                    let comment = match style {
                        "dash" => format!("-- see {name}"),
                        _ => format!("(* see {name} *)"),
                    };
                    if where_ == "tail" && matches!(kind, "END_TYPE" | "END_SCHEMA") {
                        continue;
                    }
                    let source = host(kind, &comment, where_);
                    assert_eq!(
                        formatted(&source),
                        expected(kind, style, &name, where_),
                        "{kind} {name} {style} {where_}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 60, "only {checked} cases ran");
}

#[test]
fn a_comment_before_end_function_naming_it_stays_before_it() {
    let source = "SCHEMA s;\nFUNCTION f (x : INTEGER) : INTEGER;\n  RETURN (x);\n  -- this is before END_FUNCTION\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let want = "SCHEMA s;\n\nFUNCTION f(x : INTEGER) : INTEGER;\n  RETURN (x);\n  -- this is before END_FUNCTION\nEND_FUNCTION;\n\nEND_SCHEMA;\n";
    assert_eq!(formatted(source), want);
}

#[test]
fn a_lower_case_mention_gives_the_same_text() {
    let source = "SCHEMA s;\nFUNCTION f (x : INTEGER) : INTEGER;\n  RETURN (x);\n  -- before end_function\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let want = "SCHEMA s;\n\nFUNCTION f(x : INTEGER) : INTEGER;\n  RETURN (x);\n  -- before end_function\nEND_FUNCTION;\n\nEND_SCHEMA;\n";
    assert_eq!(formatted(source), want);
}

#[test]
fn a_remark_that_follows_an_end_keyword_is_unchanged() {
    let source = "SCHEMA s;\nENTITY person;\n  x : INTEGER;\nEND_ENTITY; -- person\nENTITY dog;\n  y : INTEGER;\nend_entity; -- dog\nTYPE t = INTEGER;\nEND_TYPE; -- t\nEND_SCHEMA; -- s\n";
    let want = "SCHEMA s;\n\nENTITY person;\n  x : INTEGER;\nEND_ENTITY; -- person\n\nENTITY dog;\n  y : INTEGER;\nEND_ENTITY; -- dog\n\nTYPE t = INTEGER;\nEND_TYPE; -- t\n\nEND_SCHEMA; -- s\n";
    assert_eq!(formatted(source), want);
}

// The last line of a block comment of several lines may hold an apostrophe
// and further comments: none of that line is code.
#[test]
fn the_tail_of_a_block_comment_of_several_lines_is_not_code() {
    let source = "SCHEMA s;\nENTITY e;\n  a : INTEGER; (* x\n  it's *) (* a *) (* END_ENTITY *)\n  b : INTEGER;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let neutral = source.replace("(* END_ENTITY *)", "(* WORD *)");
    assert_eq!(
        formatted(source),
        formatted(&neutral).replace("WORD", "END_ENTITY")
    );
}

#[test]
fn a_block_comment_of_several_lines_is_read_at_its_first_line() {
    let source = "SCHEMA s;\nENTITY e;\n  x : INTEGER;\n  (* see\n END_SCHEMA for details *)\n  y : INTEGER;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let neutral = source.replace("END_SCHEMA for", "WORD for");
    assert_eq!(
        formatted(source),
        formatted(&neutral).replace("WORD", "END_SCHEMA")
    );
}

#[test]
fn the_same_text_again_from_its_own_output() {
    let source = "SCHEMA s;\nFUNCTION f (x : INTEGER) : INTEGER;\n  RETURN (x);\n  -- this is before END_FUNCTION\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let once = formatted(source);
    assert_eq!(formatted(&once), once);
}

#[test]
fn the_stripped_form_has_no_comment() {
    let source = "SCHEMA s;\nFUNCTION f (x : INTEGER) : INTEGER;\n  RETURN (x);\n  -- this is before END_FUNCTION\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let text = expressir::formatter::format(&attached(source), true);
    assert!(!text.contains("--"), "{text}");
}
