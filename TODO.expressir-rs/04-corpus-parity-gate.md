# 04 — Fixture corpus deepening to metanorma schemas

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
