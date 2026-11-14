//! #507 ground truth: dump the compiled entry region for the express
//! grammar — what follows the trivia block?

use parsanol::portable::bytecode::compiler::Compiler;

#[test]
fn dump_entry_region() {
    let program = Compiler::new(expressir_rs::grammar().clone())
        .compile()
        .unwrap();
    let entry = program.entry_point();
    println!("entry_point: {entry}; instructions: {}", program.instructions().len());
    let start = 0;
    for (i, ins) in program.instructions()[start..(start + 41970).min(program.instructions().len())]
        .iter()
        .enumerate()
    {
        println!("{:>5}: {:?}", start + i, ins);
    }
}
