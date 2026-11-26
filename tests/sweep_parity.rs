//! The parallel sweep's parity gate (#627, the parsanol-style gate for
//! the adopted portable/parallel layer): a sweep must be reproducible
//! — the same jobs sweep identically regardless of the worker count,
//! input order preserved, wire bytes identical, and the parallel sweep
//! equal to the serial one.

use expressir::batch::sweep;

fn corpus(dir: &std::path::Path, count: usize) -> Vec<(String, String)> {
    std::fs::create_dir_all(dir).unwrap();
    (0..count)
        .map(|i| {
            let name = format!("sweep_schema_{i:02}");
            let body = format!(
                "SCHEMA {name};\n\
TYPE kind_{i} = ENUMERATION OF (a, b, c); END_TYPE;\n\
ENTITY base_{i};\n  id : STRING;\n  kind : kind_{i};\nEND_ENTITY;\n\
ENTITY thing_{i} SUBTYPE OF (base_{i});\n  v : INTEGER;\n\
  WHERE wr1 : v > 0;\nEND_ENTITY;\n\
FUNCTION helper_{i}(x : INTEGER) : INTEGER;\n  RETURN (x + {i});\nEND_FUNCTION;\n\
END_SCHEMA;\n"
            );
            let path = dir.join(format!("{name}.exp"));
            std::fs::write(&path, body).unwrap();
            (path.to_string_lossy().into_owned(), name)
        })
        .collect()
}

fn fingerprint(outcomes: &[expressir::batch::BatchOutcome]) -> Vec<(String, String)> {
    outcomes
        .iter()
        .map(|o| {
            let rendered = match &o.result {
                Ok(wire) => serde_json::to_string(wire).unwrap(),
                Err(err) => format!("ERR {err}"),
            };
            (o.path.clone(), rendered)
        })
        .collect()
}

#[test]
fn parallel_sweep_equals_serial_sweep() {
    let dir = std::env::temp_dir().join(format!("sweep-parity-{}", std::process::id()));
    let jobs = corpus(&dir, 12);

    let serial = fingerprint(&sweep(jobs.clone(), 1));
    let parallel = fingerprint(&sweep(jobs.clone(), 0));
    assert_eq!(serial, parallel, "parallel sweep must equal the serial one");

    let wide = fingerprint(&sweep(jobs.clone(), 8));
    assert_eq!(serial, wide, "worker count must not change the sweep");

    let repeat = fingerprint(&sweep(jobs, 4));
    assert_eq!(serial, repeat, "repeated sweeps are identical");
}

#[test]
fn sweep_preserves_input_order() {
    let dir = std::env::temp_dir().join(format!("sweep-order-{}", std::process::id()));
    let jobs = corpus(&dir, 6);
    let outcomes = sweep(jobs.clone(), 4);
    let names: Vec<&str> = outcomes.iter().map(|o| o.path.as_str()).collect();
    let expected: Vec<&str> = jobs.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, expected, "outcomes ride the input order");
}

#[test]
fn sweep_reports_failures_in_place() {
    let dir = std::env::temp_dir().join(format!("sweep-fail-{}", std::process::id()));
    let mut jobs = corpus(&dir, 3);
    jobs.insert(2, ("/nonexistent/missing.exp".into(), "missing".into()));
    let outcomes = sweep(jobs, 4);
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes[2].path == "missing" && outcomes[2].result.is_err());
    assert!(outcomes[1].result.is_ok() && outcomes[3].result.is_ok());
}
