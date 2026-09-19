# 06 — lutaml-model needs for Rust-backed hydration

Any from_hash gaps found in 01/03/04 that require lutaml-model changes
(e.g. _class key handling, unknown-key leniency, nested hydration of
`liquid do` drops). Work in a lutaml-model worktree, PR to main.

- [ ] Audit from_hash over the full Expressir model surface
- [ ] Worktree + PR for any required change

## Acceptance
lutaml-model hydration covers the Expressir model without
expressir-side workarounds.
