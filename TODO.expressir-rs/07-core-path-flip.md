# 07 — Flip the default: core path on

**Status: DONE (default flipped on branch)** (2026-09-20)

After 04+05: default `use_core` to true when the native ext is
available (NATIVE_AVAILABLE), keeping the Ruby path as fallback.
Expressir release notes + version bump.

- [ ] Flag default flip + changelog
- [ ] Full matrix CI green (all platforms)
- [ ] Release patch

## Acceptance
Released expressir where the default parse path is Rust-backed on
every platform with a native build.

## Update 2026-09-20

from_file now defaults to the core path when the extension is available (use_core: false forces the Ruby path). Gates: parser_core_parity_spec byte-identical on the corpus + 29 smol schemas; full suite 1557 examples green under the flipped default. All on expressir PR #370 (stacked on #366, depends on expressir-rs#2 + lutaml-model#812). Release/version decisions remain with the maintainers.
