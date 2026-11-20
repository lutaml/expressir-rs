//! The #646 LSP symbols differential: the rs engine's rows (converted
//! through the ruby Document's own position math) must EQUAL the ruby
//! `Document#symbols` walk — name, LSP kind, span range, selection
//! range — over the corpus documents. Gated on EXPRESSIR_DIR.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const RUBY_HARNESS: &str = r##"
require "json"
ENV["EXPRESSIR_DISABLE_NATIVE"] = "1"
root = ARGV[0]
$LOAD_PATH.unshift(File.join(root, "lib"))
require "expressir"

out = {}
request = JSON.parse($stdin.read)
request["documents"].each do |key, source|
  begin
    document = Expressir::Lsp::Document.new(uri: "file:///#{key}.exp", text: source)
    rows = document.symbols.map do |symbol|
      {
        "name" => symbol["name"],
        "kind" => symbol["kind"],
        "range" => symbol["range"],
        "selectionRange" => symbol["selectionRange"],
      }
    end
    out[key] = { "symbols" => rows }
  rescue StandardError => e
    out[key] = { "error" => "#{e.class}: #{e.message}" }
  end
end
puts JSON.generate(out)
"##;

const CORPUS_CAP: usize = 40;

fn expressir_dir() -> Option<PathBuf> {
    std::env::var_os("EXPRESSIR_DIR").map(PathBuf::from)
}

fn collect_exp_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_exp_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "exp") {
            out.push(path);
        }
    }
}

fn corpus_files(expressir_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_exp_files(expressir_dir, &mut files);
    files.sort();
    files.dedup();
    files.truncate(CORPUS_CAP);
    files
}

fn ruby_symbols(
    expressir_dir: &Path,
    documents: &serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let request = json!({ "documents": documents });
    let mut child = Command::new("bundle")
        .arg("exec")
        .arg("ruby")
        .arg("-e")
        .arg(RUBY_HARNESS)
        .arg(expressir_dir)
        .current_dir(expressir_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("bundle/ruby unavailable: {e}"))?;
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(request.to_string().as_bytes())
            .map_err(|e| format!("write: {e}"))?;
    }
    let output = child.wait_with_output().map_err(|e| format!("wait: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "ruby harness failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("json: {e}"))
}

/// The corpus symbols walk is byte-exact against the ruby builder
/// (40/40 docs): the capture-START conventions (rs#116/#121/#122)
/// closed the span families, and the last two drifting rows were the
/// ruby builder indexing the source with character addressing on
/// byte offsets — fixed in lutaml/expressir#695.
#[test]
fn lsp_symbols_match_the_ruby_walk_over_the_corpus() {
    let Some(expressir_dir) = expressir_dir() else {
        eprintln!("EXPRESSIR_DIR not set; skipping");
        return;
    };
    let files = corpus_files(&expressir_dir);
    if files.is_empty() {
        eprintln!("no corpus files under EXPRESSIR_DIR; skipping");
        return;
    }

    let mut documents = serde_json::Map::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let key = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("doc")
            .to_string();
        documents.insert(key.clone(), json!(source));
        sources.push((key, source));
    }
    if sources.is_empty() {
        eprintln!("no readable corpus documents; skipping");
        return;
    }

    let ruby = ruby_symbols(&expressir_dir, &documents).expect("ruby symbols");

    let mut failures = Vec::new();
    let mut compared = 0usize;
    for (key, source) in &sources {
        let Some(entry) = ruby.get(key) else {
            continue;
        };
        if let Some(error) = entry.get("error").and_then(Value::as_str) {
            failures.push(format!("{key}: ruby lane errored: {error}"));
            continue;
        }
        let rs_rows: Vec<Value> =
            match serde_json::from_str(&expressir::lsp::symbols_json(source).expect("rs rows")) {
                Ok(rows) => rows,
                Err(e) => {
                    failures.push(format!("{key}: rs errored: {e}"));
                    continue;
                }
            };
        let ruby_rows = entry["symbols"].as_array().cloned().unwrap_or_default();
        compared += 1;

        // The rs rows are byte terms; the ruby side's own position math
        // converts them into the LSP shape for the comparison — the
        // exact conversion the delegation performs.
        let mut converted: Vec<Value> = Vec::new();
        let document = build_document(source);
        for row in &rs_rows {
            converted.push(json!({
                "name": row["name"],
                "kind": row["kind"],
                "range": line_range(&document, row["span_line"].as_u64().unwrap_or(1) as usize, row["span_end_line"].as_u64().unwrap_or(1) as usize),
                "selectionRange": byte_range(&document, row["sel_start"].as_u64().unwrap_or(0) as usize, row["sel_len"].as_u64().unwrap_or(1) as usize),
            }));
        }

        if converted.len() != ruby_rows.len() {
            failures.push(format!(
                "{key}: symbol count ruby={} rs={}",
                ruby_rows.len(),
                converted.len()
            ));
            continue;
        }
        for (i, (rs, vr)) in converted.iter().zip(&ruby_rows).enumerate() {
            if rs != vr {
                failures.push(format!("{key}: symbol {i} drift\n  ruby={vr}\n  rs={rs}"));
            }
        }
    }

    assert!(
        compared > 0,
        "no documents compared — the corpus produced no paired symbol sets"
    );
    assert!(
        failures.is_empty(),
        "lsp symbols drift over {compared} documents:\n{}",
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ---------------------------------------------------------------------
// The conversion helpers, replicated from the ruby Document (line
// starts over bytes, UTF-16 columns) — the same math the delegation
// runs.
// ---------------------------------------------------------------------

fn line_starts(bytes: &[u8]) -> Vec<usize> {
    let mut starts = vec![0usize];
    let mut index = 0usize;
    while let Some(found) = bytes[index..].iter().position(|b| *b == b'\n') {
        index += found + 1;
        starts.push(index);
    }
    starts
}

fn utf16_code_units(ch: char) -> usize {
    if (ch as u32) > 0xFFFF {
        2
    } else {
        1
    }
}

fn position(bytes: &[u8], starts: &[usize], byte: usize) -> Value {
    let byte = byte.min(bytes.len());
    let line = match starts.binary_search(&byte) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let prefix = String::from_utf8_lossy(&bytes[starts[line]..byte]).to_string();
    let character = prefix.chars().map(utf16_code_units).sum::<usize>();
    json!({ "line": line, "character": character })
}

fn byte_range(document: &Document, start: usize, length: usize) -> Value {
    json!({ "start": position(&document.bytes, &document.starts, start),
            "end": position(&document.bytes, &document.starts, start + length) })
}

fn line_range(document: &Document, start_line: usize, end_line: usize) -> Value {
    let start_byte = document
        .starts
        .get(start_line - 1)
        .copied()
        .unwrap_or(document.bytes.len());
    let end_byte = document
        .starts
        .get(end_line)
        .map(|s| s - 1)
        .unwrap_or(document.bytes.len());
    json!({ "start": position(&document.bytes, &document.starts, start_byte),
            "end": position(&document.bytes, &document.starts, end_byte) })
}

struct Document {
    bytes: Vec<u8>,
    starts: Vec<usize>,
}

fn build_document(source: &str) -> Document {
    Document {
        bytes: source.as_bytes().to_vec(),
        starts: line_starts(source.as_bytes()),
    }
}
