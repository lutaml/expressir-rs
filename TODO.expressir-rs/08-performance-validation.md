**Status: MEASURED — core path 1.33x faster end-to-end after direct construction** (2026-09-20)

## Method

Fork-isolated, best-of-3, CPU seconds (`Process.times`) on the 29-schema
metanorma-smol corpus, native ext built in the release profile
(`make RB_SYS_CARGO_PROFILE=release`):

```
ruby path: user 10.71  sys 0.22  total 10.93
core path: user  8.08  sys 0.14  total  8.23
speedup:   1.33x
```

History: 0.88x (2026-09-20, hash-path hydration) → 1.33x (direct
construction, below).

## Pipeline (after direct construction)

```
Rust:  parsanol packrat parse → wire JSON tree (serde Value)   ~4.3s CPU
Ext:   walk wire tree → RHash attrs → Klass.instantiate        ~0.6s
Ruby:  wire_parents 0.08s | RemarkAttacher ~0.9s | resolver 0.16s
```

Hydration dropped from 4.56s (`ExpFile.from_hash`, generic mapping
machinery) to ~0.2s (lutaml-model#819 `Serializable.instantiate`,
called from the ext: expressir#370 `parse_to_model`).

## Remaining bottleneck (honest ceiling)

The raw packrat parse (~4.3s CPU, ~52% of core-path total) is shared by
both paths and now dominates. expressir-side levers left:

- `to_parslet_compatible` normalization: 14% of the parse pipeline
  (`normalize_bench` example); skipping it means rewriting the walk
  layer against the raw tagged tree.
- serde `Value` intermediate: folding the walk into arena extraction
  saves the tree build (~0.5s CPU).
- RemarkAttacher's `node.source` formatting at remark_attacher.rb:300
  is ~0.9s shared by both paths.

Even a zero-cost post-parse pipeline caps expressir-side gains near
1.6x. **End-to-end 4x requires the parse itself to get ~3x faster —
parsanol engine work** (parsanol 0.7.3 ships only the interpreter-style
`PortableParser`; no bytecode/VM backend yet).

## Benchmark entry points

- `examples/corpus_bench.rs` — parse-only vs +wire Value vs +JSON string
- `examples/normalize_bench.rs` — normalization share of the pipeline
- expressir `/tmp/core_bench.rb` pattern — fork-isolated Ruby-side totals

The ext must be built with the release profile (rb_sys defaults to dev
locally) — a dev-profile ext is dramatically worse.
