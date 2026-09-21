# 10 — Compiled schema set: async batch compile, binary artifact, warm start

**Status: PHASES 1-2 SHIPPED (2026-09-20)** — batch compiler (expressir-rs
`batch`, ext `BatchStream`, `from_files` rework with input-ordered
progress, read-error raising, parse-failure nil-padding), EXSCS1
artifact (bincode envelope, wire-as-JSON, deterministic order,
SHA-256 set/grammar digests, staleness rejection), warm start with
byte-parity gate (`parser_compiled_set_spec`, SRL 132-schema corpus
12.0s cold vs 7.8s warm, identical to_hash). Scaled SRL numbers and
the plugin-level e2e (Metanorma plugin loader → from_files →
artifact → warm, models identical) verified. Parsanol bumped to
0.7.6. Remaining: full `metanorma compile` render blocked on
upstream release skew (metanorma-document 0.2.7+ double-defines
SourcecodeBody breaking lutaml-model map_all; 0.2.x needs unshipped
lutaml-model BasicObject type); remaining work moved to
TODO.expressir-rs/11-19 (see README)

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

## Render consumer: Metanorma Liquid over the compiled set

Liquid rendering is the third consumer of the artifact. The iso-10303
template surface is BOUNDED (~25 properties, inventoried 2026-09-20):
`thing.id/remarks/remark_items/where_rules/unique_rules/attributes/
variables/parameters/constants/types/items/source`,
`schema.{entities,types,functions,rules,procedures,constants,
subtype_constraints,interfaces,id}`,
`interface.{schema.id,source}`, `definition.{applies_to.{id,base_path},
underlying_type}`, `subtypes.size`, cross-schema `all_schemas | where:"id"`.
That surface = wire model + resolved paths + formatted sources +
graph lookups — all deterministic, all belong in the artifact.

Compile step therefore also runs (Ruby, compile-time only):
RemarkAttacher + SourceFormatter per node, storing formatted source
(and hyperlink-formatted variant) in the artifact. Render never
formats again.

Two tiers:

- **Tier 1 (compatible, cheap)**: artifact + lazy per-schema hydration.
  A document rendering N schemas pays N/1307 of hydration; graph
  queries (subtypes, referenced schemas, where:"id" lookups) served
  from the precomputed tables. No template or drop changes.
- **Tier 2 (full Rust serve)**: drop-compatible objects backed by
  magnus handles into the compiled set — no hydration at render at
  all; formatted sources read from the artifact. Gated by a parity
  spec: render the smol corpus through both tiers, diff byte-for-byte.

Parity gate for the artifact itself: to_hash of every hydrated model
== today's cold path (parser_core_parity_spec extended to warm load).
