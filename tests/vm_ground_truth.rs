//! #507 ground truth: the failing bytes through all three paths.

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
    println!("lead: {:?}", &source[..24.min(source.len())]);

    // 1. the interpreter
    let grammar = expressir_rs::grammar();
    let mut arena = AstArena::for_input(source.len());
    arena.set_input(source.clone());
    let mut parser = PortableParser::new(grammar, &source, &mut arena);
    let packrat_end = parser.parse_with_end_pos().map(|r| r.end_pos);
    println!("packrat end_pos: {:?}", packrat_end);

    // 2. the VM with a fresh program
    let program = Compiler::new(grammar.clone()).compile().unwrap();
    let mut arena2 = AstArena::for_input(source.len());
    arena2.set_input(source.clone());
    let fresh_end = BytecodeVM::new(&program, &source, &mut arena2, VMConfig::default())
        .run()
        .map(|r| r.end_pos);
    println!("vm fresh-program end_pos: {:?}", fresh_end);

    // 3. the VM with a SHARED clone of the same program (the static
    // route clones into the OnceLock; exercise the borrow the same way)
    let shared = program.clone();
    let mut arena3 = AstArena::for_input(source.len());
    arena3.set_input(source.clone());
    let shared_end =
        BytecodeVM::new(&shared, &source, &mut arena3, VMConfig::default())
            .run()
            .map(|r| r.end_pos);
    println!("vm shared-clone end_pos: {:?}", shared_end);
}
