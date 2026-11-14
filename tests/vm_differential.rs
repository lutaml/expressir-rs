//! #507 differential: the bytecode VM against the interpreter over the
//! spec fixtures — the first divergence finder for the compiled program.

use std::collections::BTreeMap;

fn fixture_sources() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let roots = [
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../expressir-wt-654/spec/syntax"),
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
    ];
    for dir in roots {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("exp") {
                    let name = format!("{}::{}", dir.file_name().unwrap().to_string_lossy(), path.file_name().unwrap().to_string_lossy());
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        out.insert(name, source);
                    }
                }
            }
        }
    }
    out
}

fn wire(tree: &expressir_rs::ParsedTree) -> String {
    serde_json::to_string(&tree.to_model_json("fixture.exp").expect("wire")).expect("json")
}

#[test]
fn vm_matches_the_interpreter_on_small_fixtures() {
    let fixtures = fixture_sources();
    assert!(!fixtures.is_empty(), "no fixtures found");
    let mut diverged = Vec::new();
    for (name, source) in &fixtures {
        let interpreter = expressir_rs::ParsedTree::parse(source);
        let vm = expressir_rs::parse_vm_probe(source);
        match (interpreter, vm) {
            (Ok(a), Ok(b)) => {
                let (wa, wb) = (wire(&a), wire(&b));
                if wa != wb {
                    diverged.push(format!(
                        "{name}: wire differs (packrat {}B, vm {}B)",
                        wa.len(),
                        wb.len()
                    ));
                }
            }
            (Err(e), Ok(_)) => diverged.push(format!("{name}: packrat fails, vm parses ({e:?})")),
            (Ok(_), Err(e)) => diverged.push(format!("{name}: vm fails, packrat parses ({e:?})")),
            (Err(_), Err(_)) => {}
        }
    }
    assert!(
        diverged.is_empty(),
        "{} fixture(s) diverge:\n{}",
        diverged.len(),
        diverged.join("\n")
    );
}
