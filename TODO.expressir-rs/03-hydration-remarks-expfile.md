# 03 — Ruby hydration pipeline: Hash → RemarkAttacher → ExpFile

**Status: DONE** (2026-09-20) (2026-09-19)

Ruby side: `Expressir::Express::Parser` gains the core path —
`parse_to_model_hash` Hash → `ExpFile.from_hash` → RemarkAttacher
(source-scanned remark attachment, unchanged from the Ruby parser) →
path/header wiring. Behind `use_core:` flag, default off.

- [ ] from_hash hydration incl. nested Serializables
- [ ] RemarkAttacher runs on hydrated models and attaches identically
- [ ] Parity spec: Ruby-path model == core-path model (to_hash equality)

## Acceptance
`from_file(use_core: true)` returns a model whose to_hash equals the
Ruby-path model's on all covered fixtures.

## Update 2026-09-19

Hydration pipeline complete in expressir PR #370 (from_file_core: from_hash + wire_parents + RemarkAttacher + transfer_header_to_schema + resolver + SchemaParseFailure wrapping). Remaining: position-dependent remark attachment differs (tail remarks) because source_offset cannot ride the wire.

## Update 2026-09-20

Full remark parity achieved: expressir-rs emits source_offset at every Builder#attach_source_info equivalent (feat/model-json), and ModelElement maps it with the hydrate-only wire mapping (lutaml-model PR #812, serialize: false). Also fixed en route: Entity#attributes typed ModelElement (polymorphic hydration of Derived/InverseAttribute), POLYMORPHIC_CLASS_MAP gained InverseAttribute, and attach_parent_to_children no longer reads :source during from_hash construction (it memoized incomplete renderings). Corpus: 15/15 parseable fixtures byte-identical incl. remarks.
