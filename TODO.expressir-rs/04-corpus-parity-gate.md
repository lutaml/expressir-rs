# 04 — Fixture corpus deepening to metanorma schemas

**Status: DONE** (2026-09-19)

Deepen the Rust serde through the real ISO 10303 schemas the smol
collection renders (certification, action, approval, ...): select and
enumeration types, function bodies with statements/expressions,
procedure parameters, rule expressions, constants, interface-less
headers. Extend fixtures + expected JSON per node type.

- [ ] Statements: if/case/assignment/repeat/return/procedure_call/null
- [ ] Expressions: binary/unary/query/interval/aggregate/entity
      constructor/refs with qualifiers/literals
- [ ] Types: select, enumeration, list/bag/set/array, generics
- [ ] Corpus parity spec: to_hash equality per schema

## Acceptance
Every smol-collection schema parses to a to_hash-equal model via the
core path.

## Update 2026-09-19

parser_core_parity_spec.rb in expressir PR #370 gates structural to_hash equality on every spec/syntax fixture plus failure-mode parity. Corpus result: 0 structural diffs; 12/17 byte-identical incl. remarks; 5 remark-attachment-only; 2 unparsable on both paths.
