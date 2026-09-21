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
| 11 | parsanol engine enhancements (#100) | #106 (EOF scan fix) merged — `aarch64` fuse regression lifted; root cause confirmed in #107, fix in `parsanol-v0.8.2`. Named-separator list-pattern fix (multi-parameter / multi-attribute) on top of #83 splice. The 4 upstream asks now degrade to "selective VM memoization" + "raw-tree API stable" |
| 12 | skip to_parslet_compatible | MERGED into 13 — the fix patch lands the Ruby-faithful merge fold; raw-tree API is the next lever, not a blocker |
| 13 | serde→magnus fold | MERGED with 12: serial-vs-direct-arena cost measured (warm breakdown 3.26s hydrate+wire vs 0.064s serde); direct-arena walker now actionable end-to-end |
| 14 | rkyv artifact v2 | CLOSED with evidence — serialization measured 0.064s; JSON tables serve graph queries at ms cost |
| 15 | graph tables + ItemGraph | SHIPPED (see 15-graph-tables… for remaining native-query notes) |
| 16 | lazy per-schema hydration | DE-SCOPED with evidence — per-document manifests already load few schemas |
| 17 | SMRL full-set artifact run | IN PROGRESS (scale validation) |
| 18 | Metanorma collection e2e | IN PROGRESS on the 0.2.12 ladder; v3 ladder blocked on metanorma-iso#1644 |
| 19 | benchmark discipline | STANDING PRACTICE |

The one remaining big lever: **12+13 combined — a direct-arena walker
(no serde Value, no normalization) building Ruby objects on the main
thread.** Now actionable: parsanol-rs#107 (0.8.2) ships #106, and the
list-pattern fold fix means an untouched arena walker no longer
silently drops first-elements of lists. Profile first; if memory and
wall-clock benefits are non-trivial, the second walker is justified.
Everything else actionable without upstream is shipped and gated.
