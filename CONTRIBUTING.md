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

## Ribose Contributor License Agreement

Before your first pull request can be merged in any Ribose GitHub
organization, you need to be covered by the Ribose Contributor License
Agreement (CLA). Sign once; one signature covers every Ribose
organization.

- **Individual contributors** — sign the Individual CLA at
  <https://cla.ribose.com/icla>. The `cla/ribose` check on your pull
  request reports the state, and you can also sign from the pull request
  by commenting `@ribose-cla agree` and completing the one-click link
  the App replies with.
- **Corporate contributors** — if your employer has an executed
  Organization CLA, the employer's Point of Contact adds your GitHub
  handle to its Schedule A at
  <https://cla.ribose.com/organizations>, or you can request coverage
  there yourself. No individual signature is then needed.
- **The full guide** — the signing flows and the agreement text are at
  <https://github.com/riboseinc/cla/blob/main/docs/SIGNING.md>.
