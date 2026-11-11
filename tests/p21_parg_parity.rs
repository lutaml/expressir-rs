//! The P21-via-PARG parity gate (expressir#625): the compiled
//! ISO-10303-21 artifact (`assets/p21/iso-10303-21.parg`) must
//! reproduce the ruby DSL grammar's trees byte-for-byte on the ISO
//! 10303-21 annex corpus. The expected trees are baked from the ruby
//! `Expressir::P21::Grammar` (the capture names are the Builder's
//! contract) and replayed here unchanged — the same fixture-replay
//! pattern as parsanol's `parg_bindings.rs`.

use parsanol::PargArtifact;
use serde_json::Value;

fn artifact() -> PargArtifact {
    PargArtifact::from_json(include_str!("../assets/p21/iso-10303-21.artifact.json"))
        .expect("the sealed ISO-10303-21 artifact loads and its checksum verifies")
}

const FIXTURES: &[(&str, &str)] = &[
    (
        "annex-e-0",
        include_str!("../assets/p21/fixtures/annex-e-0.p21"),
    ),
    (
        "annex-e-1",
        include_str!("../assets/p21/fixtures/annex-e-1.p21"),
    ),
    (
        "annex-h-0",
        include_str!("../assets/p21/fixtures/annex-h-0.p21"),
    ),
    (
        "annex-i-0",
        include_str!("../assets/p21/fixtures/annex-i-0.p21"),
    ),
    (
        "annex-i-1",
        include_str!("../assets/p21/fixtures/annex-i-1.p21"),
    ),
    (
        "annex-j-0",
        include_str!("../assets/p21/fixtures/annex-j-0.p21"),
    ),
];

#[test]
fn parg_artifact_replays_the_dsl_trees_on_the_annex_corpus() {
    let artifact = artifact();
    for (stem, source) in FIXTURES {
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(
                concat!(env!("CARGO_MANIFEST_DIR"), "/assets/p21/fixtures/",).to_owned()
                    + stem
                    + ".tree.json",
            )
            .expect("baked DSL tree"),
        )
        .expect("expected tree is valid JSON");
        let actual = artifact
            .parse_shape("exchange_structure", source)
            .unwrap_or_else(|err| panic!("{stem}: artifact parse failed: {err}"));
        assert_eq!(
            &actual, &expected,
            "{stem}: the artifact tree diverges from the ruby DSL tree"
        );
    }
}

#[test]
fn parg_artifact_rejects_a_truncated_exchange_structure() {
    let artifact = artifact();
    let truncated = "ISO-10303-21;\nHEADER;\nFILE_NAME('n');\nENDSEC;\nDATA;\n#1=E('v');\n";
    assert!(
        artifact
            .parse_shape("exchange_structure", truncated)
            .is_err(),
        "a missing END-ISO-10303-21 marker must not parse"
    );
}
