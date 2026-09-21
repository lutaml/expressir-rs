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
| 12 | skip to_parslet_compatible | Measured 12.6% of Rust cold on SRL (2.92s of ~26s); after #111 the prize is ~2.1s (~8%). Raw-tree walker remains OPEN but marginal — revisit after VM/selective-memoization upstream work |
| 13 | serde→magnus fold | **CLOSED with evidence** — emit Value tree 0.43s + serde to_string 0.06s of ~30s ext cold (135 files): clears neither the −0.4s CPU gate nor justifies a second walker; instantiate floor untouched by the fold. Revisit only if transient RSS binds at SMRL scale |
| 14 | rkyv artifact v2 | CLOSED with evidence — serialization measured 0.064s; JSON tables serve graph queries at ms cost |
| 15 | graph tables + ItemGraph | SHIPPED (see 15-graph-tables… for remaining native-query notes) |
| 16 | lazy per-schema hydration | DE-SCOPED with evidence — per-document manifests already load few schemas |
| 17 | SMRL full-set artifact run | **SHIPPED** — 1307 schemas / 43MB: 0 failures, cold 288s, warm 105s byte-identical; graph edges 37.7k subtype / 5.5k interface |
| 18 | Metanorma collection e2e | IN PROGRESS on the 0.2.12 ladder; v3 ladder blocked on metanorma-iso#1644 |
| 19 | benchmark discipline | STANDING PRACTICE — best-of-N stage bench now the default evidence tool |

SRL stage split (135 files / 9.2MB, best-of-3, 2026-09-21): raw parse
18.8–21.5s (box-noisy), normalize ~2.1s, emit Value 0.3s, serde
0.02s. **Parse is the wall** — the remaining lever lives upstream in
parsanol (selective VM memoization, #100 item 1).
