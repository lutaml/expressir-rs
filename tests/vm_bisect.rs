//! #507 bisection: which construct breaks the VM?

use parsanol::portable::bytecode::compiler::Compiler;
use parsanol::portable::bytecode::vm::{BytecodeVM, VMConfig};
use parsanol::portable::{AstArena, PortableParser};

fn vm_end(grammar: &parsanol::portable::Grammar, source: &str) -> String {
    let program = match Compiler::new(grammar.clone()).compile() {
        Ok(p) => p,
        Err(e) => return format!("compile error {e:?}"),
    };
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.to_string());
    match BytecodeVM::new(&program, source, &mut arena, VMConfig::default()).run() {
        Ok(r) => {
            if r.end_pos == source.len() {
                "full".to_string()
            } else {
                format!("partial@{}", r.end_pos)
            }
        }
        Err(e) => format!("error {e:?}"),
    }
}

fn packrat_end(grammar: &parsanol::portable::Grammar, source: &str) -> String {
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.to_string());
    match PortableParser::new(grammar, source, &mut arena).parse_with_end_pos() {
        Ok(r) => {
            if r.end_pos == source.len() {
                "full".to_string()
            } else {
                format!("partial@{}", r.end_pos)
            }
        }
        Err(e) => format!("error {e:?}"),
    }
}

#[test]
fn bisect_the_vm_failure() {
    let grammar = expressir_rs::grammar();

    let base = "SCHEMA s;\nENTITY e;\n a : STRING;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let lead = "-- schema\nSCHEMA s;\nENTITY e;\n a : STRING;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let version =
        "SCHEMA s '{iso standard 10303 part(41) object(1) version(8)}';\nEND_SCHEMA;\n";

    for (label, source) in [
        ("base", base),
        ("lead-comment", lead),
        ("version-string", version),
    ] {
        println!(
            "{label:>15}: packrat={:>8} vm={:>8}",
            packrat_end(grammar, source),
            vm_end(grammar, source)
        );
    }

    // The real fixture, with features stripped one at a time.
    let full = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/syntax.exp"
    ))
    .unwrap();
    let no_lead = full.strip_prefix("-- schema\n").unwrap_or(&full).to_string();
    println!(
        "{:>15}: packrat={:>8} vm={:>8}",
        "full",
        packrat_end(grammar, &full),
        vm_end(grammar, &full)
    );
    println!(
        "{:>15}: packrat={:>8} vm={:>8}",
        "minus-lead",
        packrat_end(grammar, &no_lead),
        vm_end(grammar, &no_lead)
    );
}
