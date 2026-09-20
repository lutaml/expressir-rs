**Status: BLOCKED by the metanorma stack's own dependency pins — upstream PRs opened** (2026-09-20)

## What is proven

- The expressir render surface metanorma consumes is byte-identical
  between the Ruby and core parse paths: all 29 smol schemas render
  identically via `Schema#formatted`/`#source`/`#full_source`.

## What blocks a full `metanorma compile` of the smol documents today

The metanorma dependency graph pins the rubyzip 2 line while current
lutaml-model (needed for the hydrate-only `source_offset` mapping)
requires rubyzip ~> 3.4. Pin holders found, with fixes:

| gem | pin | fix |
|---|---|---|
| glossarist | rubyzip < 3 | glossarist-ruby#235 |
| relaton-ieee | ~> 2.3.0 | relaton-ieee#49 |
| relaton-index | ~> 2.3.0 | relaton-index#22 |
| relaton-bipm | ~> 2.3.0 | relaton-bipm#73 |
| relaton-calconnect | ~> 2.3 | relaton-calconnect#26 |
| metanorma-plugin-lutaml | lutaml-uml ~> 0.5 (→ rubyzip 2 via lutaml-uml < 1.0) | metanorma-plugin-lutaml PR (branch `fix/lutaml-uml-1`) |

## Remaining after those merge

- `metanorma-plugin-lutaml` 0.7.51 + `lutaml` ↔ `lutaml-uml` 1.0 circular
  pin ("1.0.0 depends on lutaml") is an upstream design decision — the
  smol documents only need the express path, which does not require the
  UML stack.
- The smol project's committed `metanorma-cli` line is 0.0.3-era;
  compiling these docs requires a full metanorma stack upgrade, which
  is a project decision for the mn repo owners.

Note: the smol repo's committed Gemfile.lock referenced yanked
`metanorma-plugin-glossarist 0.3.5` — fresh installs must re-resolve
(0.3.6+ works).
