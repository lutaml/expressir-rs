# 05 — Metanorma smol render through the Rust path

**Status: DONE (render surface) / BLOCKED (full e2e)** (2026-09-20)

End-to-end: render the smol collection (metanorma-smol.yml →
collection-smol.yml → schema_docs) with the core path enabled, and
compare artifacts (xml/html) against the Ruby-path baseline render.

- [ ] Baseline: current expressir render, artifacts hashed
- [ ] Core-path render: same command, use_core flag on
- [ ] Artifact comparison: byte-equal or documented diffs

## Acceptance
Metanorma renders the smol collection successfully through the
Rust-backed models with identical output.

## Update 2026-09-20

All 29 metanorma-smol schemas render byte-identically between the paths (Schema#formatted, #source, #full_source — the exact strings the lutaml-express-index liquid templates emit). A full `metanorma compile` through the core path is blocked by the smol project's gem stack: glossarist pins rubyzip <3 while lutaml-model needs ~>3.4 — a coordinated stack upgrade is needed when expressir next releases. Note: the committed smol Gemfile.lock referenced yanked metanorma-plugin-glossarist 0.3.5; the local lock was re-resolved to restore a working bundle (no tracked changes left behind).
