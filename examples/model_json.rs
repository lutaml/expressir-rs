//! Emit the lutaml wire JSON for a fixture: the Rust side of the
//! parity check against `Expressir::Model::ExpFile#to_hash`.
//!
//! Usage: `cargo run --example model_json -- <file.exp> <relative.path> [out.json]`

use std::io::Write as _;

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("usage: model_json <file.exp> <rel.path> [out]");
    let rel_path = args.next().expect("relative path");
    let out_path = args.next();

    let source = std::fs::read_to_string(&file).expect("read input");
    let tree = expressir_rs::ParsedTree::parse(&source).expect("parse");
    let value = tree.to_model_json(&rel_path).expect("model json");
    let rendered = serde_json::to_string_pretty(&value).expect("render");

    match out_path {
        Some(path) => std::fs::write(path, rendered).expect("write"),
        None => println!("{rendered}"),
    }
}
