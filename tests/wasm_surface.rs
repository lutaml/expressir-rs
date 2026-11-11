//! The wasm surface's plain JSON functions (expressir#620's tier
//! contract): the same operations the wasm-bindgen wrappers export
//! run natively here — schema parse, remark attach, format, check,
//! and the Part 21 population — so a regression in the wire behavior
//! is caught without a wasm toolchain.

use expressir_rs::wasm;

const SCHEMA: &str = "SCHEMA demo;\nENTITY e;\n  a : STRING;\n  (*\"demo.e\" the note *)\nEND_ENTITY;\nEND_SCHEMA;\n";
const POPULATION: &str = "ISO-10303-21;\nHEADER;\nFILE_NAME('x');\nENDSEC;\nDATA;\n#1=E('v');\nENDSEC;\nEND-ISO-10303-21;\n";

#[test]
fn schema_parse_inventories_declarations() {
    let json = wasm::parse_schema_json(SCHEMA.to_string()).expect("schema parse");
    assert!(json.contains("\"name\":\"demo\""), "schemas: {json}");
    assert!(json.contains("\"a\""), "attributes: {json}");
}

#[test]
fn wire_schema_matches_the_c_abi_shape() {
    let json = wasm::wire_schema_json(SCHEMA.to_string()).expect("wire parse");
    assert!(
        json.contains("\"_class\":\"Expressir::Model::ExpFile\""),
        "model wire: {json}"
    );
}

#[test]
fn attach_format_check_round_trip() {
    // The model-hash wire — what attach/format/check consume — comes
    // from the wire_schema op (the C ABI's expr_parse_schema shape).
    let wire_json = wasm::wire_schema_json(SCHEMA.to_string()).expect("wire parse");

    let attached =
        wasm::attach_remarks_json(wire_json.clone(), SCHEMA.to_string(), false).expect("attach");
    assert!(attached.contains("the note"), "attached: {attached}");

    let text = wasm::format_json(attached.clone(), false).expect("format");
    assert!(text.contains("SCHEMA demo;"), "format: {text}");
    let stripped = wasm::format_json(attached, true).expect("format stripped");
    assert!(stripped.contains("END_ENTITY;"), "stripped: {stripped}");

    let findings = wasm::check_json(wire_json, None).expect("check");
    assert!(findings.starts_with('['), "findings: {findings}");
}

#[test]
fn population_parse_and_validate() {
    let json = wasm::parse_population_json(POPULATION.to_string()).expect("population");
    assert!(json.contains("E"), "population: {json}");
    let verdict = wasm::validate_population_json("[\"E\"]".to_string(), POPULATION.to_string())
        .expect("validate");
    assert!(verdict.contains("\"valid\""), "verdict: {verdict}");
}
