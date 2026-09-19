# expressir-rs

The Rust core for [expressir](https://github.com/lutaml/expressir): EXPRESS
(ISO 10303-11) schema parsing and lutaml wire-model emission on
[parsanol](https://github.com/parsanol/parsanol-rs).

[![License](https://img.shields.io/github/license/lutaml/expressir-rs.svg)](https://github.com/lutaml/expressir-rs/blob/main/LICENSE)
[![CI](https://github.com/lutaml/expressir-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/lutaml/expressir-rs/actions/workflows/ci.yml)

## Purpose

expressir's Ruby parser spends most of its time in tree conversion.
expressir-rs moves that work into Rust: it parses EXPRESS sources on the
parsanol packrat engine (grammar embedded from the Ruby tier) and emits
the sparse lutaml wire JSON — the exact shape
`Expressir::Model::ExpFile#to_hash` produces — so Ruby hydrates with
`ExpFile.from_hash` and gets a byte-identical model to the Ruby parse
path.

## Architecture

- `src/lib.rs` — `ParsedTree`: parse, extract, and `to_model_json`
- `src/extract.rs` — declaration-level extraction (Phase-1 skeleton)
- `src/model_json.rs` + `src/model_json/*` — the wire-JSON emitter,
  split to mirror the Ruby model hierarchy
  (`declarations`, `data_types`, `expressions`, `statements`)
- `src/walk.rs` — arena navigation helpers over the parslet-shaped tree
- `assets/express-grammar.json` — the serialized EXPRESS grammar
  (regenerate with `bundle exec rake expressir:grammar:dump`; CI fails
  on drift)

The Ruby builder quirks that are observable in the wire shape are
mirrored deliberately and documented in code — see the parity spec in
the expressir repository (`parser_core_parity_spec`).

## Quick start

```rust
use expressir_rs::ParsedTree;

let tree = ParsedTree::parse("SCHEMA s; END_SCHEMA;")?;
let wire = tree.to_model_json("s.exp")?;
```

## Development

```sh
cargo test            # fixture parity tests
cargo build --examples
bundle install && bundle exec rake expressir:grammar:dump  # regenerate the grammar asset
```

## Releasing

Releases are automated with [release-plz](https://release-plz.dev): every
push to `main` opens a release PR with the next version and changelog;
merging it tags the release. The crate is consumed through a git
dependency by expressir's native extension, so crates.io publishing
stays disabled.

## License

MIT.
