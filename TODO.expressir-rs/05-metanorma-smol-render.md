# 05 — Metanorma smol render through the Rust path

End-to-end: render the smol collection (metanorma-smol.yml →
collection-smol.yml → schema_docs) with the core path enabled, and
compare artifacts (xml/html) against the Ruby-path baseline render.

- [ ] Baseline: current expressir render, artifacts hashed
- [ ] Core-path render: same command, use_core flag on
- [ ] Artifact comparison: byte-equal or documented diffs

## Acceptance
Metanorma renders the smol collection successfully through the
Rust-backed models with identical output.
