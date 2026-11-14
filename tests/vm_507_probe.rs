//! #496 status on parsanol 0.20.20: does the VM still short-parse the
//! compiled EXPRESS grammar?

use parsanol::portable::bytecode::compiler::Compiler;
use parsanol::portable::bytecode::vm::{BytecodeVM, VMConfig};
use parsanol::portable::{AstArena, PortableParser};

#[test]
fn ground_truth_three_paths() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/syntax.exp"
    ))
    .unwrap();
    println!("bytes: {}", source.len());

    let grammar = expressir_rs::grammar();
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.clone());
    let packrat_end = PortableParser::new(grammar, &source, &mut arena)
        .parse_with_end_pos()
        .map(|r| r.end_pos);
    println!("packrat end_pos: {:?}", packrat_end);

    let program = Compiler::new(grammar.clone()).compile().unwrap();
    let mut arena2 = AstArena::for_input(source.len());
    arena2.set_input(source.clone());
    let vm_end = BytecodeVM::new(&program, &source, &mut arena2, VMConfig::default())
        .run()
        .map(|r| r.end_pos);
    println!("vm end_pos: {:?}", vm_end);
}
