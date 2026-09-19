# 06 — lutaml-model needs for Rust-backed hydration

**Status: DONE** (2026-09-20) (2026-09-19)

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

## Update 2026-09-20

lutaml-model PR #812 (feat/hydrate-only-mapping): `serialize: false` mapping option — hydrated by from_*, omitted by to_*. expressir maps ModelElement#source_offset with it. Full lutaml-model suite green (5962 examples). Also found+fixed aggregate repetition semantics: Ruby keeps [a : b, c : d] repetition (multi-occurrence elements are rule-key wrapped → build_element) but drops single [4:2] (merged → first-key dispatch); mirrored in expressir-rs.
