# 06 — lutaml-model needs for Rust-backed hydration

**Status: IN PROGRESS** (2026-09-19)

Any from_hash gaps found in 01/03/04 that require lutaml-model changes
(e.g. _class key handling, unknown-key leniency, nested hydration of
`liquid do` drops). Work in a lutaml-model worktree, PR to main.

- [ ] Audit from_hash over the full Expressir model surface
- [ ] Worktree + PR for any required change

## Acceptance
lutaml-model hydration covers the Expressir model without
expressir-side workarounds.

## Update 2026-09-19

Single remaining gap: ModelElement#source_offset (and raw source span) are not wire-mapped, so hydrated models cannot drive NodePositionIndex tail-remark attachment. Plan: emit source_offset from the Rust arena (InputRef offsets) + add a write-only (from_hash-only) mapping option in lutaml-model so to_hash stays unchanged. Requires the lutaml-model worktree PR.
