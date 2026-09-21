# TODO.expressir-rs — campaign backlog

## 01-10: the Rust core path (SHIPPED)

Wire emitter, corpus parity, lutaml-model hydration needs, core-path
default flip, performance validation, Metanorma smol e2e — see
10-compiled-schema-set.md for the compiled-set design and its shipped
phases 1-2 (batch compiler, EXSCS1 artifact, warm start).

## 11-19: the performance campaign

Shipped so far: batch compile (288s serial → ~12s SRL cold), compiled
set + warm start, remarks overlay (`074b774`), reference
re-application overlay (`afb98be`), parsanol 0.7.6 adoption.

Current numbers (SRL, 132 schemas / 8.1 MB): cold+write 11.3s, warm
4.1s (hydration ≈ 2.5s of it), full metanorma render ~1m40s.

- 11 — parsanol engine enhancements (upstream #100)
- 12 — skip to_parslet_compatible (12-14% of parse)
- 13 — serde→magnus fold (direct arena emission)
- 14 — rkyv/mmap artifact v2 (EXSCS2, zero-copy)
- 15 — graph tables in the artifact + ItemGraph
- 16 — lazy per-schema hydration (Metanorma Tier 1)
- 17 — SMRL full-set artifact run (1307 schemas / 43 MB)
- 18 — Metanorma collection e2e on the compiled set
- 19 — benchmark discipline and harnesses

Recommended sequence: 17 (scale validation) → 13 (self-contained Rust
win) → 15 (foundation for Part 21), with 11 as the async upstream
track.
