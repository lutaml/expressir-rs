# Contributing to expressir-rs

- Conventional commits keep the automated changelog meaningful
  (`feat:`, `fix:`, `docs:`, `chore:`, with an optional `!` for breaking
  changes).
- `cargo fmt` and `cargo clippy --all-targets` must be clean; CI denies
  warnings.
- The grammar asset is generated from the Ruby tier — never edit it by
  hand; run `bundle exec rake expressir:grammar:dump` and commit the
  result.
- Wire-shape changes must keep the expressir parity spec green; when
  mirroring a Ruby builder quirk, document it in code.
- All changes go through pull requests; releases are handled by
  release-plz.
