# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.5.0] - 2026-11-27

### Changed

- The crate joins the family's 2.5.0 cut-over line: the gem shipped
  the deliberate remark-surface cut-over as 2.5.0, and the engine
  version follows (the ext pin v0.1.66 + ENGINE-COMPAT.json map the
  native bridge; the ext tag line stays v0.1.x by design).

### Other

- The deterministic parallel corpus sweep ([#627](https://github.com/lutaml/expressir-rs/pull/627)) by @[object]

### Fixed

- The plain form ends at END_SCHEMA; the pair test follows expressir main (lutaml/expressir#758) by @[object]
- The plain face keeps the comment after END_SCHEMA; (lutaml/expressir#758) by @[object]
- The supertype expression holds its operand remarks (expressir#757 mirror, attachment half) by @[object]
- Format writes comments in a supertype expression (lutaml/expressir#757) by @[object]
- A comment of several lines before a closing keyword is kept (lutaml/expressir#756) by @[object]
- Ask the code of a line, not a comment's text, for END_ lines (lutaml/expressir#755) by @[object]
- The plain form writes the source's own --IPn: comment lines (lutaml/expressir#753) by @[object]
- Format keeps the bare ABSTRACT of an entity (lutaml/expressir#752) by @[object]
- Format writes nested declarations in the order of the source (lutaml/expressir#751) by @[object]
- Format keeps the parentheses of a supertype expression (lutaml/expressir#750) by @[object]
- Format keeps a comment on its own line before END_SCHEMA in place (lutaml/expressir#749) by @[object]
- A block comment before a USE FROM clause leads the clause (lutaml/expressir#748) by @[object]
- Format keeps a comment on its own line before an entity section keyword (lutaml/expressir#747) by @[object]
- Format writes a block comment after the label of a WHERE rule (lutaml/expressir#746) by @[object]
- Format writes a comment between two CASE actions above the next label (lutaml/expressir#744) by @[object]
- Format keeps a comment before LOCAL or a nested declaration in place (lutaml/expressir#743) by @[object]
- Format keeps a comment in an IF whose THEN is on a later line (lutaml/expressir#745) by @[object]
- Format writes the comments in the item list of a SELECT where they stand (lutaml/expressir#741) by @[object]
- A comment after the opening parenthesis of an interface item list is written after it (lutaml/expressir#728) by @[object]
- A comment on its own line before END_ENTITY or END_TYPE is written before the keyword (lutaml/expressir#726) by @[object]
- Format writes the comments of an enumeration item and of a WHERE rule (lutaml/expressir#724) by @[object]

### Other

- The crate stays on the 2.4.x line by @[object]
- The tracked Cargo.lock leaves the gitignore by @[object]
- The P21 recovery envelope (expressir#627, first slice) by @[object]
- Lift the parsanol pin to =1.5.22 by @[object]
- The G.1.6 dependent-instantiability rule and function (lutaml/expressir#636) by @[object]
- The SHTOLO stage order rides the ruby build_longform (lutaml/expressir#636) by @[object]
- The provenance remark, the shtolo orchestrator, the C ABI and wasm expose (lutaml/expressir#636) by @[object]
- The lock rides parsanol 1.5.22 (lutaml/expressir#636) by @[object]
- Clippy + spell: the fold passes deny-warnings and the typos gate by @[object]
- The extensible enumeration and select folding (lutaml/expressir#636) by @[object]
- Prune, the supertype reductions and the entity conversions (lutaml/expressir#636) by @[object]
- The lock rides parsanol 1.5.21 (lutaml/expressir#636) by @[object]
- Subtype-constraint elimination (lutaml/expressir#636) + parsanol 1.5.21 by @[object]
- The SHTOLO G.1 stage-1 longform (lutaml/expressir#636) by @[object]
- Track Cargo.lock: parsanol floats to a VM-regressed release (the #496 short-parse is back in 1.5.20) by @[object]
- Bind concat_schema (lutaml/expressir#636) by @[object]
- The closure sort key folds inline by @[object]
- The concat core rides the C ABI and the wasm surface (lutaml/expressir#636) by @[object]
- The concatenated artifact over the resolved wire (lutaml/expressir#636) by @[object]
- The paren expectations ride expressir main's contract; fmt; the no-drift job reads main by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- Say the informal-proposition lines in words (the spell check reads IPn as On) by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- Fresh parsanol everywhere by @[object]
- CONSTANT and UNIQUE same-line remarks keep rendering ([#735](https://github.com/lutaml/expressir-rs/pull/735)) by @[object]
- The core lane plain form keeps the same-line comments ([#733](https://github.com/lutaml/expressir-rs/pull/733)) by @[object]
- The spell gate reads IPn as On; say it in words by @[object]
- The plain form writes no informal-proposition lines from tagged descriptions ([#723](https://github.com/lutaml/expressir-rs/pull/723)) by @[object]
- The no-drift differential reads expressir main by @[object]
- The no-drift job reads the expressir branch of the same change by @[object]
- The no-drift differential rides expressir main again ([#133](https://github.com/lutaml/expressir-rs/pull/133)) by @[object]
- The printer keeps the parentheses of the source ([#707](https://github.com/lutaml/expressir-rs/pull/707)); the declarations follow the order of the source ([#711](https://github.com/lutaml/expressir-rs/pull/711)) ([#131](https://github.com/lutaml/expressir-rs/pull/131)) by @[object]
- Keep_untagged_remarks on the core lane ([#130](https://github.com/lutaml/expressir-rs/pull/130)) by @[object]
- Cargo fmt by @[object]
- The tail-append skips every placed remark; the first end-scope remark keeps its space by @[object]
- Every legacy scope remark is written; informal propositions render by @[object]
- The parity lane rides expressir main again by @[object]
- Revert "the parg copy tracks expressir's grammar-parg source" by @[object]
- The parg copy tracks expressir's grammar-parg source by @[object]
- Cargo fmt by @[object]
- String literals: the doubled apostrophe escape, and bodies that start with a blank by @[object]
- The stripped formatter drops remark blocks again; parsanol tracks the 1.5 line by @[object]
- Cargo fmt by @[object]
- Owners render their tagged descriptions; bare -- comments survive (#694, #696, #691) by @[object]
- Restore the vendored lane's own ext build by @[object]
- The LSP walk does not span through attached remarks by @[object]
- The vendored lane's ext rides this PR's engine for the parity gates by @[object]
- Clippy, fmt, and the PR-scoped ruby-lane checkout by @[object]
- Every tail remark is written next to the token it followed ([#693](https://github.com/lutaml/expressir-rs/pull/693)) by @[object]
- The LSP symbols gate goes required; both LSP gates wired into CI (rs#567) by @[object]
- The schema's tagged description survives a source round trip by @[object]
- The inventory gate goes required: byte-exact against the ruby walk (rs#567) ([#122](https://github.com/lutaml/expressir-rs/pull/122)) by @[object]
- The INSERT keyword byte + the generalized assignment-target rule (rs#567) ([#121](https://github.com/lutaml/expressir-rs/pull/121)) by @[object]
- Add the Ribose CLA section by @[object]
- Release v2.4.69 ([#111](https://github.com/lutaml/expressir-rs/pull/111)) by @[object]
- The supertype stamp narrows to the bare factor; the LHS/RHS qualifier convention settles (rs#567) ([#119](https://github.com/lutaml/expressir-rs/pull/119)) by @[object]
- Untagged remarks differentiate from renderable remarks ([#679](https://github.com/lutaml/expressir-rs/pull/679)) by @[object]
- The inventory capture-START conventions: attributes, UNIQUE qualifiers, the supertype keyword (rs#567) ([#116](https://github.com/lutaml/expressir-rs/pull/116)) by @[object]
- Release-plz rides LUTAML_CI_PAT_TOKEN by @[object]

### Fixed

- The wasm population wrapper follows the renamed serializer by @[object]

### Other

- The crate carries the shared version line (2.4.69) by @[object]
# Changelog

Generated by [release-plz](https://release-plz.dev) on every release.
