# TODO.expressir-rs — campaign backlog

## 01-10: the Rust core path (SHIPPED)

Wire emitter, corpus parity, lutaml-model hydration needs, core-path
default flip, performance validation, Metanorma smol e2e — see
10-compiled-schema-set.md for the compiled-set design and its shipped
phases 1-2 (batch compiler, EXSCS1 artifact, warm start, graph
tables, remarks + refs overlays).

## 11-19: the performance campaign (status as of 2026-09-21)

Shipped: batch compile (288s serial → ~11s SRL cold), compiled set +
warm start, remarks overlay (`074b774`), reference re-application
overlay (`afb98be`), ItemGraph tables in the artifact (`539cac7`),
hydration fast path (lutaml-model `29c407c` + ext symbol keys
`280a5d1`).

Current numbers (SRL, 132 schemas / 8.1 MB): cold+write 11.3s, warm
4.1s — warm breakdown: hydrate+wire 3.26s (magnus walk + instantiate
object building; serialization measured at 0.064s), overlays 1.4s.

| # | task | status |
|---|---|---|
| 11 | parsanol engine enhancements (#100) | #106 (EOF scan fix) merged — `aarch64` fuse regression lifted; root cause confirmed in #107, fix in `parsanol-v0.8.2`. Named-separator list-pattern fix (multi-parameter / multi-attribute) on top of #83 splice. VM on SRL corpus: **1.05x, parity 135/135** — per-file backend selection not justified. Normalize interned-key perf: parsanol-rs#111 (−28% normalize) |
| 12 | skip to_parslet_compatible | **CLOSED — gated out.** Prototype built (walk_raw.rs + `raw-tree` feature, branch `spike/raw-tree-walker`): schema/id extraction, list patterns, and most of the corpus reached byte parity, but each remaining callsite needs per-site modeling of normalize's merge semantics. The gate closed first: after #111, normalize is 2.03s of a 26s Rust cold pipeline on SRL = **7.7%, below the ≥8% gate** (~6% of end-to-end cold). Revisit only if the parse side shrinks or the grammar's normalize share grows |
| 13 | serde→magnus fold | **CLOSED with evidence** — emit Value tree 0.43s + serde to_string 0.06s of ~30s ext cold (135 files): clears neither the −0.4s CPU gate nor justifies a second walker; instantiate floor untouched by the fold. Revisit only if transient RSS binds at SMRL scale |
| 14 | rkyv artifact v2 | CLOSED with evidence — serialization measured 0.064s; JSON tables serve graph queries at ms cost |
| 15 | graph tables + ItemGraph | SHIPPED (see 15-graph-tables… for remaining native-query notes) |
| 16 | lazy per-schema hydration | DE-SCOPED with evidence — per-document manifests already load few schemas |
| 17 | SMRL full-set artifact run | **SHIPPED** — 1307 schemas / 43MB: 0 failures, cold 288s, warm 105s byte-identical; graph edges 37.7k subtype / 5.5k interface |
| 18 | Metanorma collection e2e | v3 ladder **UNBLOCKED** — metanorma-iso `feat/model-validation-migration` renders ISO documents end-to-end (HTML, STS, PDF) through the unified pubid; `:flavor:`/collection-manifest declares the type, no `-t` needed. metanorma-core flavor table (their #18) still needs a release for CI to resolve it |
| 19 | benchmark discipline | STANDING PRACTICE — best-of-N stage bench now the default evidence tool |

SRL stage split (135 files / 9.2MB, best-of-3, 2026-09-21, parsanol
0.8.3): raw parse 18.8–26s (box-noisy), normalize 2.03s, emit Value
0.3s, serde 0.03s. **Parse is the wall** — the remaining lever lives
upstream in parsanol (selective VM memoization, #100 item 1).

Adopted: parsanol-rs 0.8.3 (named-separator list-pattern fix,
capture-tree #110, interned-key merge #111) and parsanol-ruby 1.3.45 —
both validated: expressir-rs suites + wire byte-parity (spec fixtures
+ SRL 135/135 normalized), expressir parser/parity/model suites green.
Oracle differential: the 5 non-parsing SRL resources files fail
identically on eeng 5.1.0 — invalid/auxiliary inputs, not regressions
(metanorma/iso-10303#758 filed for the schema's missing head `;`).
