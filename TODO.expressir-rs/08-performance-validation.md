**Status: MEASURED — negative result, core path is currently slower end-to-end** (2026-09-20)

## Method

Fork-isolated, best-of-3, CPU seconds (`Process.times`) on the 29-schema
metanorma-smol corpus, native ext built in the release profile
(`make RB_SYS_CARGO_PROFILE=release`):

```
ruby path: user 10.81  sys 0.26  total 11.07
core path: user 12.38  sys 0.26  total 12.64
speedup:   0.88x  (core path ~14% slower)
```

## Why

The core path trades the Ruby tree conversion for: Rust JSON string →
`JSON.parse` (Ruby) → `ExpFile.from_hash` (lutaml-model hydration:
per-node instance construction + polymorphic resolution) →
`wire_parents`. After parsanol 1.3.x made the native parse itself
several times faster, the remaining Ruby-builder cost is smaller than
the hydration cost. The ext must also be built with the release profile
(rb_sys defaults to dev locally) — a dev-profile ext is dramatically
worse.

## Follow-up options (in priority order)

1. Cut the JSON round-trip: build the model objects directly through
   magnus instead of `to_model_json` → `JSON.parse` → `from_hash`
   (largest win; removes one serialization and lutaml-model's
   polymorphic dispatch).
2. Profile `from_hash`/hydration itself; the polymorphic `_class`
   const-get path is hot.
3. Keep the core path opt-in until (1) lands — the parity gate
   (`parser_core_parity_spec`) holds either way, so the switch is
   one line.
