//! #507 bisection stage 2: line-ranged prefix schemas.

use parsanol::portable::bytecode::compiler::Compiler;
use parsanol::portable::bytecode::vm::{BytecodeVM, VMConfig};
use parsanol::portable::{AstArena, PortableParser};

fn vm_full(grammar: &parsanol::portable::Grammar, source: &str) -> bool {
    let program = Compiler::new(grammar.clone()).compile().unwrap();
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.to_string());
    match BytecodeVM::new(&program, source, &mut arena, VMConfig::default()).run() {
        Ok(r) => r.end_pos == source.len(),
        Err(_) => false,
    }
}

fn packrat_full(grammar: &parsanol::portable::Grammar, source: &str) -> bool {
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.to_string());
    match PortableParser::new(grammar, source, &mut arena).parse_with_end_pos() {
        Ok(r) => r.end_pos == source.len(),
        Err(_) => false,
    }
}

#[test]
fn bisect_to_the_smallest_failing_schema() {
    let grammar = expressir_rs::grammar();
    let full = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/syntax.exp"
    ))
    .unwrap();
    let lines: Vec<&str> = full.lines().collect();

    // Declaration starts inside the schema body.
    let decl_starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .skip(2)
        .filter(|(_, l)| {
            let t = l.trim_start();
            t.starts_with("ENTITY ")
                || t.starts_with("TYPE ")
                || t.starts_with("FUNCTION ")
                || t.starts_with("PROCEDURE ")
                || t.starts_with("RULE ")
                || t.starts_with("CONSTANT\n")
                || t.starts_with("CONSTANT ")
        })
        .map(|(i, _)| i)
        .collect();
    assert!(decl_starts.len() > 2, "expected several declarations");

    let end_schema_line = lines.iter().rposition(|l| l.trim() == "END_SCHEMA;").unwrap();
    let header = &lines[..decl_starts[0] - 1]; // up to the line before decl 1 (incl. blanks)

    let assemble = |n_decls: usize| -> String {
        let last_line = if n_decls == decl_starts.len() {
            end_schema_line
        } else {
            decl_starts[n_decls] // exclusive: up to the next decl start
        };
        let mut s = lines[..header.len()].join("\n");
        s.push('\n');
        s.push_str(&lines[decl_starts[0]..last_line].join("\n"));
        s.push_str("\nEND_SCHEMA;\n");
        s
    };

    let total = decl_starts.len();
    let mut first_fail = None;
    for n in 1..=total {
        let candidate = assemble(n);
        if !packrat_full(grammar, &candidate) {
            println!("prefix of {n} declarations: packrat cannot parse it (validity edge); stopping scan");
            break;
        }
        if !vm_full(grammar, &candidate) {
            first_fail = Some(n);
            break;
        }
    }
    let n = match first_fail {
        Some(n) => n,
        None => {
            println!("all {total} prefixes pass on the vm");
            return;
        }
    };
    println!("SMALLEST FAILING PREFIX: {n} declarations");
    let last_line = if n == decl_starts.len() { end_schema_line } else { decl_starts[n] };
    println!(
        "the newly added declaration (lines {}..{}):\n{}",
        decl_starts[n - 1] + 1,
        last_line,
        lines[decl_starts[n - 1]..last_line].join("\n")
    );
}
