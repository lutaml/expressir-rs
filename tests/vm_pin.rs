//! #507 final probes: pin the exact feature that breaks the VM.

use parsanol::portable::bytecode::compiler::Compiler;
use parsanol::portable::bytecode::vm::{BytecodeVM, VMConfig};
use parsanol::portable::{AstArena, PortableParser};

fn vm_end(grammar: &parsanol::portable::Grammar, source: &str) -> String {
    let program = Compiler::new(grammar.clone()).compile().unwrap();
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.to_string());
    match BytecodeVM::new(&program, source, &mut arena, VMConfig::default()).run() {
        Ok(r) => {
            if r.end_pos == source.len() { "full".into() } else { format!("partial@{}", r.end_pos) }
        }
        Err(e) => format!("error {e:?}"),
    }
}

fn pk_end(grammar: &parsanol::portable::Grammar, source: &str) -> String {
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.to_string());
    match PortableParser::new(grammar, source, &mut arena).parse_with_end_pos() {
        Ok(r) => {
            if r.end_pos == source.len() { "full".into() } else { format!("partial@{}", r.end_pos) }
        }
        Err(e) => format!("error {e:?}"),
    }
}

#[test]
fn pin_the_vm_breaking_feature() {
    let grammar = expressir_rs::grammar();
    let cases: Vec<(&str, String)> = vec![
        (
            "multi-line WHERE",
            "SCHEMA x;\nENTITY e;\nWHERE\nWR1 : TRUE;\nEND_WHERE;\nEND_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
        (
            "single-line WHERE",
            "SCHEMA x;\nENTITY e; WHERE WR1 : TRUE; END_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
        (
            "single-line entity, multi-line WHERE",
            "SCHEMA x;\nENTITY e;\nWHERE\nWR1 : TRUE;\nEND_WHERE;\nEND_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
        (
            "single-line WHERE, no label",
            "SCHEMA x;\nENTITY e; WHERE TRUE; END_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
        (
            "single-line WHERE, lowercase label",
            "SCHEMA x;\nENTITY e; WHERE wr1 : TRUE; END_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
        (
            "single-line WHERE with semicolon after expr",
            "SCHEMA x;\nENTITY e; WHERE WR1 : TRUE;\nEND_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
        (
            "one-line plain entity",
            "SCHEMA x;\nENTITY e; a : STRING; END_ENTITY;\nEND_SCHEMA;\n".into(),
        ),
    ];
    for (label, source) in cases {
        println!(
            "{label:>42}: packrat={:>8} vm={:>8}",
            pk_end(grammar, &source),
            vm_end(grammar, &source)
        );
    }
}
