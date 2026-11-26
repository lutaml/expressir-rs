//! The P21 recovery envelope's parity gate (expressir#627, the first
//! #627 slice): the recovering path agrees with the strict path on
//! well-formed input, and its forensics are pinned on malformed input
//! — the failure wire, the consumed extent, the resync spans, and the
//! partial records the spine had already matched. The strict path
//! keeps refusing loudly; recovery is opt-in.

use expressir::population::{parse_population, parse_population_recovering};

const CORPUS: [&str; 6] = [
    "annex-e-0",
    "annex-e-1",
    "annex-h-0",
    "annex-i-0",
    "annex-i-1",
    "annex-j-0",
];

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("assets/p21/fixtures/{name}.p21"))
        .unwrap_or_else(|err| panic!("{name}: {err}"))
}

fn junked_tail(source: &str) -> String {
    // Junk before the final ENDSEC: a keyword the schema can never
    // name, with characters no ISO-10303-21 token starts with.
    let mut out = source.to_string();
    let pos = out.rfind("ENDSEC;").expect("fixture carries ENDSEC;");
    out.insert_str(pos, "\n#9999=JUNK(%%%);\n");
    out
}

#[test]
fn recovering_matches_the_strict_path_on_the_whole_corpus() {
    for name in CORPUS {
        let source = fixture(name);
        let strict = parse_population(&source).unwrap_or_else(|err| panic!("{name} strict: {err}"));
        let recovered = parse_population_recovering(&source)
            .unwrap_or_else(|err| panic!("{name} recovering: {err}"));
        assert!(recovered.ok, "{name}: valid input must recover as ok");
        let population = recovered.population.expect("{name}: population present");
        assert_eq!(population, strict, "{name}: populations identical");
        assert!(
            recovered.error.is_none(),
            "{name}: no error wire on valid input"
        );
        assert!(
            recovered.resyncs.is_empty(),
            "{name}: no resyncs on valid input"
        );
        assert!(
            recovered.partial_records.is_empty(),
            "{name}: no partial records on valid input"
        );
    }
}

#[test]
fn strict_path_still_refuses_the_junk_loudly() {
    let junked = junked_tail(&fixture("annex-e-0"));
    assert!(parse_population(&junked).is_err(), "strict stays strict");
}

#[test]
fn junk_mid_data_yields_failure_forensics_with_a_record_prefix() {
    let source = fixture("annex-e-0");
    let strict = parse_population(&source).unwrap();
    let junked = junked_tail(&source);
    let recovered = parse_population_recovering(&junked).unwrap();

    assert!(!recovered.ok);
    let error = recovered.error.as_ref().expect("failure wire present");
    let offset = error["offset"].as_u64().expect("error offset");
    let line = error["line"].as_u64().expect("error line");
    let column = error["column"].as_u64().expect("error column");
    assert!(
        (offset as usize) < junked.len(),
        "error offset inside the input: {offset}"
    );
    assert!(line >= 1 && column >= 1, "line/column are one-based");
    let expected = error["expected"].as_array().expect("expected set");
    assert!(
        !expected.is_empty(),
        "the expected set names the alternatives"
    );
    assert_eq!(
        recovered.consumed_upto,
        Some(offset),
        "the consumed extent anchors the failure for this case"
    );

    assert!(
        !recovered.partial_records.is_empty(),
        "the spine prefix carries the records before the junk"
    );
    let strict_ids: Vec<u64> = strict.records.iter().map(|r| r.id).collect();
    let mut last = 0;
    for record in &recovered.partial_records {
        assert!(
            strict_ids.contains(&record.id),
            "partial record #{} exists in the strict set",
            record.id
        );
        assert!(record.id > last, "partial records stay in document order");
        last = record.id;
    }
}

#[test]
fn truncated_tail_resyncs_to_the_whole_prefix() {
    let source = fixture("annex-e-0");
    let strict = parse_population(&source).unwrap();
    let truncated = &source[..source.len() - 12];
    let recovered = parse_population_recovering(truncated).unwrap();

    assert!(!recovered.ok);
    let error_offset = recovered.error.as_ref().and_then(|e| e["offset"].as_u64());
    assert_eq!(
        recovered.consumed_upto, error_offset,
        "the consumed extent reaches the failure"
    );
    assert!(
        !recovered.resyncs.is_empty(),
        "the skipped tail is documented as resync spans"
    );
    let last_end = recovered.resyncs.last().map(|s| s.end).expect("spans");
    if let Some(offset) = error_offset {
        assert_eq!(
            last_end as u64, offset,
            "the final resync span ends at the failure offset"
        );
    }
    assert!(
        !recovered.partial_records.is_empty(),
        "records recovered despite the missing tail"
    );
    let strict_ids: Vec<u64> = strict.records.iter().map(|r| r.id).collect();
    for record in &recovered.partial_records {
        assert!(
            strict_ids.contains(&record.id),
            "partial record #{} exists in the strict set",
            record.id
        );
    }
}
