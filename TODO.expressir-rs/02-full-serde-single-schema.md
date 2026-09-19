# 02 — Full-tree serde for single-schema surface

**Status: DONE** (2026-09-19)

Expand the Rust serde from the declaration skeleton to the full
`Expressir::Model::ExpFile#to_hash` shape: `_class` markers,
source_offset spans, entity attributes with supertype/optional/derived
kinds and underlying types, WHERE rule label+expression, schema
interface-less surface for `single.exp`/`multiple.exp`.

- [ ] Rust structs emit lutaml to_hash-shaped JSON (snake_case, _class)
- [ ] `Expressir::Core.parse_to_model_hash(source)` ext API returning
      the Ruby Hash
- [ ] Fixture parity extended: full-depth comparison vs Ruby to_hash on
      single/multiple fixtures

## Acceptance
`parse_to_model_hash` output deep-equals the Ruby `to_hash` for the
covered fixtures.

## Update 2026-09-19

Full-depth emitter landed on feat/model-json (PR lutaml/expressir-rs#2): model_json/ split into declarations/data_types/expressions/statements, mirroring the Ruby builder registry. Zero structural diffs vs the Ruby path across spec/syntax.
