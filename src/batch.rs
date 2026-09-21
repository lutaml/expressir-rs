//! Async batch compile: a jobs queue feeding worker threads.
//!
//! Each task reads its file, parses on the shared static grammar,
//! emits the wire model (serde Value), and drops its arena — peak
//! memory is the largest in-flight parse plus the wire values queued
//! for the consumer. Outcomes arrive in completion order on the
//! returned channel; workers never touch Ruby state, so the consumer
//! can block freely while compilation proceeds in the background.

use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value as Wire;

use crate::ParsedTree;

/// One compiled file: the wire model on success, a message on failure.
pub struct BatchOutcome {
    /// The wire path (the logical file name the model carries).
    pub path: String,
    pub result: Result<Wire, String>,
}

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Compile `jobs` — `(read_path, wire_path)` pairs — on `workers`
/// threads (`0` = `available_parallelism`), returning a receiver that
/// yields one outcome per job in completion order and then closes.
pub fn parse_batch(jobs: Vec<(String, String)>, workers: usize) -> mpsc::Receiver<BatchOutcome> {
    let (job_tx, job_rx) = mpsc::channel::<(String, String)>();
    let (res_tx, res_rx) = mpsc::channel::<BatchOutcome>();
    let job_rx = Arc::new(Mutex::new(job_rx));

    let worker_count = if workers == 0 {
        thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1)
    } else {
        workers
    }
    .clamp(1, jobs.len().max(1));

    let handles: Vec<_> = (0..worker_count)
        .map(|_| {
            let job_rx = Arc::clone(&job_rx);
            let res_tx = res_tx.clone();
            thread::spawn(move || loop {
                let job = { job_rx.lock().expect("jobs queue").recv() };
                let Ok(job) = job else { break };
                if res_tx.send(compile_one(job)).is_err() {
                    break;
                }
            })
        })
        .collect();

    for job in jobs {
        if job_tx.send(job).is_err() {
            break;
        }
    }
    drop(job_tx);

    // Reap workers so the final res_tx clone drops exactly when the
    // queue drains, closing the results channel for the consumer.
    thread::spawn(move || {
        for handle in handles {
            let _ = handle.join();
        }
    });

    res_rx
}

fn compile_one(job: (String, String)) -> BatchOutcome {
    let (read_path, wire_path) = job;
    let outcome = try_compile(&read_path, &wire_path);
    BatchOutcome {
        path: wire_path,
        result: outcome,
    }
}

fn try_compile(read_path: &str, wire_path: &str) -> Result<Wire, String> {
    let raw = std::fs::read(read_path).map_err(|e| format!("read {read_path}: {e}"))?;
    // Mirror Expressir::Express::Parser.strip_bom: a UTF-8 BOM would
    // shift every offset and break the SCHEMA-at-byte-0 expectation.
    let bytes = raw.strip_prefix(BOM).unwrap_or(raw.as_slice());
    let source = std::str::from_utf8(bytes).map_err(|e| format!("utf8 {read_path}: {e}"))?;
    let tree = ParsedTree::parse(source).map_err(|e| format!("{e:?}"))?;
    tree.to_model_json(wire_path).map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_schema(name: &str, body: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("expressir-batch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.exp"));
        std::fs::write(&path, format!("SCHEMA {name};\n{body}\nEND_SCHEMA;\n")).unwrap();
        (path, name.to_string())
    }

    #[test]
    fn delivers_one_outcome_per_job() {
        let a = temp_schema("alpha", "ENTITY a; END_ENTITY;");
        let b = temp_schema("beta", "ENTITY b; END_ENTITY;");
        let jobs = vec![
            (a.0.to_string_lossy().into_owned(), a.1.clone()),
            (b.0.to_string_lossy().into_owned(), b.1.clone()),
        ];
        let rx = parse_batch(jobs, 2);
        let mut seen = std::collections::HashSet::new();
        for outcome in rx {
            assert!(outcome.result.is_ok(), "{:?}", outcome.result);
            let wire = outcome.result.unwrap();
            let id = wire["schemas"][0]["id"].as_str().unwrap().to_string();
            seen.insert(id);
        }
        assert_eq!(seen, ["alpha".into(), "beta".into()].into());
    }

    #[test]
    fn reports_failures_without_stopping_the_queue() {
        let good = temp_schema("good", "ENTITY g; END_ENTITY;");
        let jobs = vec![
            ("/nonexistent/missing.exp".into(), "missing".into()),
            (good.0.to_string_lossy().into_owned(), good.1.clone()),
        ];
        let rx = parse_batch(jobs, 2);
        let outcomes: Vec<_> = rx.iter().collect();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes
            .iter()
            .any(|o| o.path == "missing" && o.result.is_err()));
        assert!(outcomes
            .iter()
            .any(|o| o.path == "good" && o.result.is_ok()));
    }

    #[test]
    fn strips_a_utf8_bom() {
        let dir = std::env::temp_dir().join(format!("expressir-batch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bom.exp");
        std::fs::write(&path, "\u{FEFF}SCHEMA bom; END_SCHEMA;\n").unwrap();
        let rx = parse_batch(vec![(path.to_string_lossy().into_owned(), "bom".into())], 1);
        let outcome = rx.iter().next().unwrap();
        let wire = outcome.result.as_ref().expect("bom parses");
        assert_eq!(wire["schemas"][0]["id"], "bom");
    }
}
