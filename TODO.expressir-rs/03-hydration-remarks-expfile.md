# 03 — Ruby hydration pipeline: Hash → RemarkAttacher → ExpFile

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
