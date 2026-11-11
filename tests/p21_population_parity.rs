//! The population-inventory retirement gate (expressir#625): the
//! artifact-backed builder must produce the EXACT inventory JSON the
//! retired hand-rolled parser (src/p21.rs, pre-#625) produced on the
//! ISO 10303-21 annex corpus. The expected JSONs were baked from the
//! old parser before its removal.

use serde_json::Value;

const CORPUS: &[(&str, &str)] = &[
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

fn inventory_json(source: &str) -> Value {
    let population = expressir_rs::population::parse_population(source)
        .unwrap_or_else(|err| panic!("artifact parse failed: {err}"));
    serde_json::json!({
        "file_schemas": population.file_schemas,
        "records": population.records.iter().map(|r| serde_json::json!({
            "id": r.id,
            "keyword": if r.complex { r.parts.join("+") } else { r.keyword.clone() },
            "complex": r.complex,
        })).collect::<Vec<_>>(),
        "anchors": population.anchors.iter()
            .map(|(name, target)| serde_json::json!([name, target]))
            .collect::<Vec<_>>(),
        "references": population.references.iter()
            .map(|(id, target)| format!("#{id} = <{target}>"))
            .collect::<Vec<_>>(),
    })
}

#[test]
fn the_artifact_builder_matches_the_retired_parser_byte_for_byte() {
    for (stem, source) in CORPUS {
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/p21_population/",
                )
                .to_owned()
                    + stem
                    + ".population.json",
            )
            .expect("baked expected inventory"),
        )
        .expect("expected inventory is valid JSON");
        assert_eq!(
            inventory_json(source),
            expected,
            "{stem}: the artifact-backed inventory diverges from the retired parser"
        );
    }
}
