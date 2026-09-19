# 01 — Hydration spike: Ruby to_hash → from_hash → formatted parity

Prove the Rust-path architecture in pure Ruby before writing more Rust:
take a Ruby-parsed Repository, `to_hash` it, hydrate via
`Expressir::Model::Repository.from_hash` (lutaml-model), run
`Schema#formatted` on both, and compare bytes. If formatter output is
byte-identical, the serialization boundary (Rust serde JSON ↔ lutaml
wire format) is viable and everything downstream is mechanical.

- [x] Round-trip `ExpFile → to_hash → from_hash` on `single.exp`
- [x] `schema.formatted` byte-comparison: original vs hydrated
- [x] Record gaps: missing source_offset handling, _class key needs,
      any node type from_hash fails on
- [x] Decided: hydrate via lutaml from_hash vs per-class fast hydrator

## Acceptance
formatted output identical, or a written list of exactly what
lutaml-model/Rust must change (feeds TODO 06/08).


## Result (2026-09-19)

`single.exp`: round_trip_hash_equal=true, formatted_equal=true,
SchemaDrop materializes on the hydrated model and drop.formatted is
byte-identical. lutaml-model from_hash handles the full shape as-is —
no lutaml-model change required for the hydration path (TODO 06 shrinks
to an audit of exotic node types as the serde deepens). Spike script
run via bundle exec in this repo; captured hash at
/tmp/lm_perf/spike_hash.json is the exact serde target for TODO 02.
