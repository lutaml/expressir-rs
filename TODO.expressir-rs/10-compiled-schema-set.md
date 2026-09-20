# 10 — Compiled schema set: async batch compile, binary artifact, warm start

**Status: DESIGN (2026-09-20), approved direction**

## Premise

The cold path (1307 SMRL schemas, 43.3MB: 274s parse + 12s resolve +
1s indexes, 2.4GB hydrated) produces only deterministic data — a
function of (schema sources, grammar version, expressir version).
Therefore it is compilable and cacheable as a single artifact. Data
consumers (Part 21 validation, ARM/MIM graphing, Metanorma) load the
artifact in seconds and never parse EXPRESS again until schemas change.

Evidence driving the design (TODO 08, SMRL probes):
- packrat parse is linear; ~240KB/s worst files, interpreter constant
- bytecode VM regresses on long-forms (0.54x, 2.1GB arena) — engine fix
  is selective memoization (parsanol-rs#90), separate track
- fork from_files REGRESSES at scale (407s vs 288s): Marshal of
  hydrated models kills it → parallelism must happen below Ruby, at
  the wire/compiled level
- hydration via Serializable.instantiate ≈ 10MB/s → 43MB ≈ 4.5s

## Artifact: `CompiledSchemaSet`

Container (single file, versioned):

```
magic "EXSCS1" | format version
schema-set digest (SHA-256 over sorted (path, source-sha) pairs)
expressir-rs version | grammar digest | resolver ruleset version
per-file compiled model (model data, deterministic order)
resolved reference table (path -> base_path edges; resolver output)
entity/type path tables
graph adjacency: subtype / interface-item / attribute-type edges
source offsets (already stamped in the model data)
```

Encoding: v1 **bincode** (simple, fast: ~1s for 43MB in Rust);
v2 **rkyv** (zero-copy, mmap — Rust queries without deserializing at
all). The model data reuses the existing wire shape (serde) as the
single source of truth — compiled = the wire model + derived tables,
no new representation to keep in parity.

Consumers:
- **Rust native**: query path tables + graph directly (validation
  engine never builds Ruby objects)
- **Ruby full API**: hydrate per-file/per-schema lazily via
  Serializable.instantiate → formatters/Liquid keep working;
  graph-only workflows hydrate nothing

Invalidation: any input change (source, expressir, grammar) changes a
digest → regenerate. Same contract as the existing Ruby Marshal cache
(be70ecf, 29x) but language-portable, graph-aware, mmap-able.

## Phase 1 — async batch compile (L1)

expressir-rs `batch` module:
- work queue (file paths) + N worker threads (std::thread; arena per
  task, freed after emit), results channel
- per task: parse → to_model_json → encode compiled per-file blob
- backpressure: bounded results channel; workers never hold Ruby state

ext binding (magnus, GVL-released waits):
- `Expressir::Core.parse_batch(files, workers:)` returns an Enumerator;
  each `next` yields (path, compiled_blob) as workers complete; parse
  proceeds concurrently with Ruby-side consumption (pipeline overlap)

expressir Ruby `from_files` rework:
- workers compile ‖ main-thread hydration (instantiate) → resolver →
  merge → optionally write CompiledSchemaSet
- drop the fork/Marshal path for the core mode (keep for pure-Ruby
  fallback)

Expected: 288s → ~40s wall (274/8 ≈ 35s parse + 4.5s hydrate
overlapped + 12s resolve), peak RSS ≈ largest-file arena + models.

## Phase 2 — warm start from artifact (L3)

- `Expressir::Core::Set.from_artifact(path)`: read + verify digests;
  graph queries native in Rust; lazy per-schema hydration for Ruby
- CLI: `expressir compile <manifest> -o set.exscs` /
  `--from set.exscs`
- Expected: full-model warm start ≈ 5-10s; graph-only ≈ instant

## Phase 3 — engine (L2, upstream, parallel track)

parsanol-rs#90 selective memoization in the VM (append 0.54x/2.1GB
regression evidence). Cuts the *compile* cost itself (288→~100s),
which matters for CI and first-time users even though consumers use
the artifact.

## Phase 4 — memory (L4, incremental)

- worker arena pooling across tasks (AstArena::reset exists)
- artifact mmap (rkyv v2) → graph-only RSS ≈ artifact size
- lazy Ruby hydration → 2.4GB only when formatting everything

## Phase 5 — Part 21 + validation (the platform goal)

- `expressir-rs::part21`: streaming line-oriented parser (regular
  grammar; target 50-200MB/s) — NEVER the PEG schema grammar
- validation engine: instance stream ⋈ compiled set (entity vs
  subtype closure, attribute conformance, inverse attrs, WHERE rules
  via an expression interpreter — expressions already modeled)
- ARM/MIM graphing served entirely from the compiled graph
