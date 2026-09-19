# 07 — Flip the default: core path on

After 04+05: default `use_core` to true when the native ext is
available (NATIVE_AVAILABLE), keeping the Ruby path as fallback.
Expressir release notes + version bump.

- [ ] Flag default flip + changelog
- [ ] Full matrix CI green (all platforms)
- [ ] Release patch

## Acceptance
Released expressir where the default parse path is Rust-backed on
every platform with a native build.
