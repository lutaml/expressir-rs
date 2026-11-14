//! #507: does the depth cap explain the prefix-34 failure?

use parsanol::portable::bytecode::compiler::Compiler;
use parsanol::portable::bytecode::vm::parse_with_vm_diag;
use parsanol::portable::{AstArena, PortableParser};

#[test]
fn depth_cap_probe() {
    let grammar = expressir_rs::grammar();
    let full = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/syntax.exp"
    ))
    .unwrap();
    let lines: Vec<&str> = full.lines().collect();
    let decl_starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            t.starts_with("ENTITY ")
                || t.starts_with("TYPE ")
                || t.starts_with("FUNCTION ")
                || t.starts_with("PROCEDURE ")
                || t.starts_with("RULE ")
        })
        .map(|(i, _)| i)
        .collect();
    let end_schema = lines.iter().rposition(|l| l.trim() == "END_SCHEMA;").unwrap();
    let n = 34;
    let last_line = if n == decl_starts.len() { end_schema } else { decl_starts[n] };
    let mut candidate = lines[..decl_starts[0]].join("\n");
    candidate.push('\n');
    candidate.push_str(&lines[decl_starts[0]..last_line].join("\n"));
    candidate.push_str("\nEND_SCHEMA;\n");

    let program = Compiler::new(grammar.clone()).compile().unwrap();
    for depth in [10_000usize, 100_000, 1_000_000] {
        let mut arena = AstArena::for_input(candidate.len());
        arena.set_input(candidate.clone());
        let config = VMConfig {
            max_recursion_depth: depth,
            ..VMConfig::default()
        };
        let (verdict, diag) = match parse_with_vm_diag(&program, &candidate, &mut arena) {
            Ok(r) if r.end_pos == candidate.len() => ("full".to_string(), None),
            Ok(r) => (format!("partial@{}", r.end_pos), None),
            Err(e) => (format!("error {e:?}"), None),
        };
        let _ = config;
        println!("depth {depth:>8}: {verdict}");
        if depth == 10_000 {
            println!("diag: {diag:?}");
            let line = candidate[..10].to_string();
            println!("pos10 context: {line:?}");
        }
    }
    // packrat cross-check
    let mut arena = AstArena::for_input(candidate.len());
    arena.set_input(candidate.clone());
    let pk = PortableParser::new(grammar, &candidate, &mut arena)
        .parse_with_end_pos()
        .map(|r| r.end_pos == candidate.len());
    println!("packrat full: {pk:?}");
}
