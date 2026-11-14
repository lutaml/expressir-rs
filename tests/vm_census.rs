//! #507 table-index census: charset/regex table sizes against the
//! instructions that reference them, plus the full-instruction scan
//! for encoding anomalies.

use parsanol::portable::bytecode::compiler::Compiler;
use parsanol::portable::bytecode::instruction::Instruction;

#[test]
fn census() {
    let program = Compiler::new(expressir_rs::grammar().clone())
        .compile()
        .unwrap();
    let ins = program.instructions();
    println!("instructions: {}", ins.len());

    let mut charset_refs: std::collections::BTreeMap<usize, usize> = Default::default();
    let mut regex_refs: std::collections::BTreeMap<usize, usize> = Default::default();
    let mut calls: Vec<(usize, i64)> = Vec::new();
    for (i, instruction) in ins.iter().enumerate() {
        match instruction {
            Instruction::CharSet { set_idx } => {
                *charset_refs.entry(*set_idx as usize).or_default() += 1;
            }
            Instruction::Regex { regex_idx } => {
                *regex_refs.entry(*regex_idx as usize).or_default() += 1;
            }
            Instruction::Call { offset } => calls.push((i, i64::from(*offset))),
            _ => {}
        }
    }
    let cs_max = charset_refs.keys().last();
    let rx_max = regex_refs.keys().last();
    println!(
        "distinct charset refs: {}; max idx {:?}; distinct regex refs: {}; max idx {:?}",
        charset_refs.len(),
        cs_max,
        regex_refs.len(),
        rx_max
    );

    // Call targets outside the program = broken emission.
    let bad_calls: Vec<(usize, i64)> = calls
        .iter()
        .filter_map(|(i, o)| {
            let t = *i as i64 + 1 + *o as i64;
            (t < 0 || t as usize >= ins.len()).then(|| (*i, *o))
        })
        .collect();
    println!("calls: {}; out-of-range targets: {:?}", calls.len(), bad_calls);

    // The tables themselves.
    let n_sets = program.char_set_count();
    let n_regex = program.regex_count();
    println!("char_sets table: {n_sets}; regexes table: {n_regex}");
    if let Some(m) = cs_max {
        assert!(*m < n_sets, "charset ref {m} exceeds table {n_sets}");
    }
    if let Some(m) = rx_max {
        assert!(*m < n_regex, "regex ref {m} exceeds table {n_regex}");
    }
}
